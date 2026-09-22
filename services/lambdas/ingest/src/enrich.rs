//! Per-keeper task: labels, faces and text through `Vision`, a checked description for
//! keepers whose landmark is unknown (research R8), and the embedding for Ask (F-303).
//! Non-keepers inherit from their keeper in `organize`.

use memorylane_core::events::{Event, EventKind};
use memorylane_core::ids::{HouseholdId, MediaId};
use memorylane_core::media::{
    cleanup_reason, Description, Enrichment, MediaItem, ProcessingState, Tag,
};
use memorylane_core::narrative_check::{check, Allowed};
use memorylane_core::person::Person;
use memorylane_ingest::embed;
use memorylane_ingest::renditions::Rendition;
use memorylane_store::places::Place;
use serde::Serialize;
use std::fmt::Write as _;

use crate::landmarks::{ensure_facts, grounded, sights_near};
use crate::{Ingest, IngestError, Result};

/// Landmarks below this confidence get a description with a GPS hint.
pub const LANDMARK_CONFIDENCE: f32 = 0.7;
/// The model picked one of the sights known to be near the photo.
pub const GROUNDED_CONFIDENCE: f32 = 0.85;
/// A free answer, but the encyclopedia has that landmark where the photo was taken.
pub const PLACED_CONFIDENCE: f32 = 0.75;
/// A free answer nothing backs up.
pub const GUESSED_CONFIDENCE: f32 = 0.5;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Output {
    pub media_id: MediaId,
    pub state: ProcessingState,
    pub skipped: Option<String>,
    pub described: bool,
    pub cleanup_reason: Option<String>,
}

/// The question put to the multimodal model. The answer's first line names the landmark
/// or says "none"; the rest is the description.
pub fn describe_prompt(media: &MediaItem, enrichment: &Enrichment, sights: &[Place]) -> String {
    let mut p = String::from(
        "Describe this family photo in two short sentences for a photo album. \
         First line: 'Landmark: <name>' if a known landmark is clearly visible, else 'Landmark: none'. \
         Then the description. Do not guess dates or people's names.",
    );
    p.push_str(&sights_hint(sights));
    if let Some(gps) = media.gps {
        let _ = write!(
            p,
            "\nTaken near latitude {:.4}, longitude {:.4}.",
            gps.lat, gps.lon
        );
    }
    if !enrichment.scene_tags.is_empty() {
        let labels: Vec<&str> = enrichment
            .scene_tags
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        let _ = write!(p, "\nLabels: {}.", labels.join(", "));
    }
    p
}

/// Grounded recognition: the model chooses among sights that are really there.
pub fn sights_hint(sights: &[Place]) -> String {
    if sights.is_empty() {
        return String::new();
    }
    let names: Vec<&str> = sights.iter().map(|s| s.name.as_str()).collect();
    format!(
        "\nSights near where this was taken: {}. If one of them is what the photo shows, \
         answer with that name exactly as written. Name a landmark outside this list only \
         if it is unmistakable.",
        names.join("; ")
    )
}

/// Split the model's answer into landmark and text.
pub fn parse_description(raw: &str) -> (Option<String>, String) {
    let mut lines = raw.trim().lines();
    let first = lines.next().unwrap_or("").trim();
    let rest: String = lines.collect::<Vec<_>>().join(" ").trim().to_owned();
    match first.strip_prefix("Landmark:") {
        Some(name) => {
            let name = name.trim().trim_end_matches('.');
            let landmark =
                (!name.is_empty() && !name.eq_ignore_ascii_case("none")).then(|| name.to_owned());
            (landmark, rest)
        }
        None => (None, raw.trim().to_owned()),
    }
}

#[allow(clippy::too_many_lines)]
pub async fn run(ingest: &Ingest, household: &HouseholdId, media_id: &MediaId) -> Result<Output> {
    let mut media = ingest
        .storage
        .get_media(household, media_id)
        .await?
        .ok_or_else(|| IngestError::Input(format!("no media {media_id}")))?;
    if media.state == ProcessingState::Enriched {
        return Ok(Output {
            media_id: media.id,
            state: media.state,
            skipped: Some("already enriched".into()),
            described: false,
            cleanup_reason: None,
        });
    }
    if !media.is_keeper {
        return Ok(Output {
            media_id: media.id,
            state: media.state,
            skipped: Some("not a keeper".into()),
            described: false,
            cleanup_reason: None,
        });
    }
    let key = Rendition::Tv.key(household.as_str(), media_id.as_str());
    let jpeg = ingest.blobs.get(&key).await?;

    let mut enrichment = ingest.vision.analyze(&jpeg).await?;
    enrichment.inherited_from = None;
    let lines = ingest.vision.detect_text(&jpeg).await?;
    let text = lines.join(" ").trim().to_owned();
    media.text = (!text.is_empty()).then_some(text);

    // F-705: quick cleanup candidates, deterministic, announced to the Concierge only.
    media.cleanup_reason = cleanup_reason(
        media.captured.is_some(),
        media.signals.width,
        media.signals.height,
        media.signals.sharpness,
        &lines,
    );
    if let Some(reason) = media.cleanup_reason {
        ingest
            .bus
            .publish(&Event {
                household: household.clone(),
                seq: 0,
                at: ingest.now(),
                kind: EventKind::CleanupCandidates {
                    trip: media.trip.clone(),
                    media: vec![media.id.clone()],
                    reasons: vec![reason.as_str().to_owned()],
                },
            })
            .await?;
    }

    // Description when the landmark is unknown or weak and there is something to go on.
    let mut described = false;
    let weak_landmark = enrichment
        .landmark
        .as_ref()
        .is_none_or(|l| l.confidence < LANDMARK_CONFIDENCE);
    if weak_landmark && (media.gps.is_some() || !enrichment.scene_tags.is_empty()) {
        let sights = match media.gps {
            Some(at) => sights_near(ingest, at).await,
            None => vec![],
        };
        let raw = ingest
            .vision
            .describe(&jpeg, &describe_prompt(&media, &enrichment, &sights))
            .await?;
        let (landmark, text) = parse_description(&raw);
        if !text.is_empty() {
            // Constitution IV: a description naming a date the photo does not carry is
            // withheld. The landmark's weight depends on what backs it: a sight known to
            // be there, an encyclopedia article at that position, or nothing.
            let mut allowed = Allowed::new();
            if let Some(c) = media.captured {
                let local = c.local();
                allowed.date(local.year(), u8::from(local.month()));
            }
            for t in &enrichment.scene_tags {
                allowed.name(&t.name);
            }
            if let Some(l) = &landmark {
                allowed.name(l);
            }
            let problems: Vec<String> = check(&text, &allowed)
                .into_iter()
                .filter(|p| p.starts_with("year") || p.starts_with("month"))
                .collect();
            if problems.is_empty() {
                media.description = Some(Description {
                    text,
                    landmark: landmark.clone(),
                    model_id: "vision.describe".into(),
                    generated_at: ingest.now(),
                });
                described = true;
                if let (Some(l), None) = (landmark, &enrichment.landmark) {
                    let sight = grounded(&l, &sights);
                    let near = sight.map(|s| s.at).or(media.gps);
                    // The sight's own spelling wins, so photos and routes agree on a name.
                    let name = sight.map_or(l, |s| s.name.clone());
                    let placed = ensure_facts(ingest, &name, near).await.is_some();
                    let confidence = match (sight.is_some(), placed && media.gps.is_some()) {
                        (true, _) => GROUNDED_CONFIDENCE,
                        (false, true) => PLACED_CONFIDENCE,
                        (false, false) => GUESSED_CONFIDENCE,
                    };
                    enrichment.landmark = Some(Tag { name, confidence });
                }
            }
        }
    }

    // A landmark Vision itself was sure of has no description pass; it still gets its facts.
    if let Some(l) = enrichment.landmark.as_ref().filter(|_| !described) {
        ensure_facts(ingest, &l.name, media.gps).await;
    }

    // F-704: opt-in face grouping on keepers only (research R9).
    let people_enabled = ingest
        .storage
        .get_household(household)
        .await?
        .is_some_and(|h| h.people_enabled);
    if people_enabled && enrichment.face_count > 0 {
        media.people = group_faces(ingest, household, &media.id, &jpeg).await?;
    }

    let vector = embed::embed_media(
        &enrichment.scene_tags,
        enrichment.landmark.as_ref().map(|t| t.name.as_str()),
        media.text.as_deref(),
        media.description.as_ref().map(|d| d.text.as_str()),
    );
    ingest
        .vectors
        .upsert(household, media.trip.as_ref(), &media.id, &vector)
        .await?;

    media.enrichment = enrichment;
    media.state = ProcessingState::Enriched;
    ingest.storage.put_media(&media).await?;
    Ok(Output {
        media_id: media.id,
        state: media.state,
        skipped: None,
        described,
        cleanup_reason: media.cleanup_reason.map(|r| r.as_str().to_owned()),
    })
}

/// Index the faces on this keeper, then ask about each new face on its own: a known face
/// joins its person, an unknown one opens a new unnamed person. One person per photo at
/// most once, so two lookalikes in a frame never double-count.
async fn group_faces(
    ingest: &Ingest,
    household: &HouseholdId,
    media: &MediaId,
    jpeg: &[u8],
) -> Result<Vec<memorylane_core::ids::PersonId>> {
    let indexed = ingest.faces.index(household, media, jpeg).await?;
    let mut people = ingest.storage.list_people(household).await?;
    let mut out = Vec::new();
    for face_id in indexed {
        let matches = ingest.faces.search(household, &face_id).await?;
        let known = matches.iter().find_map(|m| {
            people.iter().position(|p| {
                p.face_ids.contains(&m.face_id) && !p.is_merged() && !out.contains(&p.id)
            })
        });
        if let Some(pos) = known {
            let person = &mut people[pos];
            person.add_face(face_id, media.clone());
            ingest.storage.put_person(person).await?;
            out.push(person.id.clone());
        } else {
            let person = Person::new(household.clone(), face_id, media.clone(), ingest.now());
            ingest.storage.put_person(&person).await?;
            out.push(person.id.clone());
            people.push(person);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_landmark_line() {
        assert_eq!(
            parse_description("Landmark: Todai-ji\nThe Great Buddha hall at dusk."),
            (
                Some("Todai-ji".into()),
                "The Great Buddha hall at dusk.".into()
            )
        );
        assert_eq!(
            parse_description("Landmark: none\nA quiet street."),
            (None, "A quiet street.".into())
        );
        assert_eq!(
            parse_description("Just prose."),
            (None, "Just prose.".into())
        );
    }

    #[test]
    fn the_prompt_offers_the_sights_that_are_really_there() {
        use memorylane_core::geo::GeoPoint;
        let sight = |name: &str| Place {
            name: name.into(),
            at: GeoPoint::new(63.53, -19.51),
            distance_m: 120.0,
            source: "memory".into(),
            kind: Some("landmark".into()),
            dim_m: None,
            country: None,
        };
        let with = sights_hint(&[sight("Skógafoss"), sight("Skógar Museum")]);
        assert!(with.contains("Sights near where this was taken: Skógafoss; Skógar Museum."));
        assert!(with.contains("exactly as written"));
        assert_eq!(sights_hint(&[]), "");
    }
}
