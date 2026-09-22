//! Landmarks a viewer can learn about (F-210). Two jobs: list the sights near a photo so
//! the vision model picks from real candidates instead of naming one freely, and make
//! sure a recognised landmark has its encyclopedia facts stored. Neither ever fails a
//! task: a lookup that errors leaves the landmark a plain name.

use mantelpiece_core::geo::GeoPoint;
use mantelpiece_core::landmark::{self, LandmarkFacts};
use mantelpiece_store::places::Place;

use crate::Ingest;

/// How far from the photo a sight can be and still be in the frame.
pub const IN_FRAME_M: u32 = 2_500;
pub const MAX_CANDIDATES: usize = 8;

/// Named sights around `at`, nearest first; towns are not something a photo shows.
pub async fn sights_near(ingest: &Ingest, at: GeoPoint) -> Vec<Place> {
    match ingest
        .places
        .nearby(at, IN_FRAME_M, MAX_CANDIDATES * 2)
        .await
    {
        Ok(places) => places
            .into_iter()
            .filter(|p| p.kind.as_deref() != Some("city"))
            .take(MAX_CANDIDATES)
            .collect(),
        Err(e) => {
            tracing::warn!(error = %e, "sights lookup failed; recognising without candidates");
            vec![]
        }
    }
}

/// The candidate the model meant, when its answer is one of them.
pub fn grounded<'a>(name: &str, candidates: &'a [Place]) -> Option<&'a Place> {
    let wanted = landmark::key(name);
    candidates.iter().find(|c| landmark::key(&c.name) == wanted)
}

/// Stored facts for `name`, fetched and stored on first sight. `near` is where the
/// landmark (or the photo of it) is; the encyclopedia's answer must be about that place.
pub async fn ensure_facts(
    ingest: &Ingest,
    name: &str,
    near: Option<GeoPoint>,
) -> Option<LandmarkFacts> {
    let key = landmark::key(name);
    if key.is_empty() {
        return None;
    }
    match ingest.storage.get_landmark_facts(&key).await {
        Ok(Some(facts)) => return Some(facts),
        Ok(None) => {}
        Err(e) => {
            tracing::warn!(error = %e, landmark = name, "landmark facts read failed");
            return None;
        }
    }
    let article = match ingest.encyclopedia.lookup(name, near).await {
        Ok(Some(article)) => article,
        Ok(None) => return None,
        Err(e) => {
            tracing::warn!(error = %e, landmark = name, "encyclopedia lookup failed");
            return None;
        }
    };
    let facts = LandmarkFacts {
        key,
        name: name.to_owned(),
        title: article.title,
        tagline: article.tagline,
        summary: article.summary,
        url: article.url,
        credit: article.credit,
        at: article.at.or(near),
        trivia: vec![],
        fetched_at: ingest.now(),
    };
    if let Err(e) = ingest.storage.put_landmark_facts(&facts).await {
        tracing::warn!(error = %e, landmark = name, "landmark facts write failed");
    }
    Some(facts)
}
