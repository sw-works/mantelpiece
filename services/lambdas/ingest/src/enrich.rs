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
use serde::Serialize;
use std::fmt::Write as _;

use crate::{Ingest, IngestError, Result};

/// Landmarks below this confidence get a description with a GPS hint.
pub const LANDMARK_CONFIDENCE: f32 = 0.7;

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
pub fn describe_prompt(media: &MediaItem, enrichment: &Enrichment) -> String {
    let mut p = String::from(
        "Describe this family photo in two short sentences for a photo album. \
         First line: 'Landmark: <name>' if a known landmark is clearly visible, else 'Landmark: none'. \
         Then the description. Do not guess dates or people's names.",
    );
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
        let raw = ingest
            .vision
            .describe(&jpeg, &describe_prompt(&media, &enrichment))
            .await?;
        let (landmark, text) = parse_description(&raw);
        if !text.is_empty() {
            // Constitution IV: a description naming a date the photo does not carry is
            // withheld. Places cannot be checked without a gazetteer, so the landmark is
            // kept as a low-confidence tag.
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
                    enrichment.landmark = Some(Tag {
                        name: l,
                        confidence: 0.6,
                    });
                }
            }
        }
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
}
