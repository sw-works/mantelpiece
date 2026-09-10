//! `POST /ask` (F-405): a question from the TV, the phone's voice page or Alexa+. With a
//! fresh now-showing it is about that photo and is answered from what the pipeline stored
//! (research R8, no model call on this path); otherwise it is a search that becomes a
//! playlist. Both answers reach the TV as feed events.

use memorylane_core::events::EventKind;
use memorylane_core::ids::{ContributorId, HouseholdId, MediaId, PlaylistId};
use memorylane_core::media::MediaItem;
use memorylane_core::presence::{freshest, Presence};
use memorylane_core::trip::Playlist;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fmt::Write as _;

use crate::app::App;
use crate::error::ApiError;
use crate::handlers::search;
use crate::req::{Req, Resp};

#[derive(Deserialize)]
struct AskBody {
    question: String,
    /// Device id of the TV the question is about; defaults to the freshest.
    tv: Option<String>,
}

/// The stored answer for one photo: description first, metadata otherwise.
pub async fn describe(
    app: &App,
    household: &HouseholdId,
    media: &MediaItem,
) -> Result<Value, ApiError> {
    let taken_by = app
        .storage
        .get_contributor(household, &media.contributor)
        .await?
        .map_or_else(|| media.contributor.to_string(), |c| c.display_name);
    let when = media.captured.map(|c| c.local().date().to_string());
    let landmark = media
        .description
        .as_ref()
        .and_then(|d| d.landmark.clone())
        .or_else(|| media.enrichment.landmark.as_ref().map(|t| t.name.clone()));
    let answer = match &media.description {
        Some(d) if !d.text.is_empty() => d.text.clone(),
        _ => {
            let labels: Vec<&str> = media
                .enrichment
                .scene_tags
                .iter()
                .take(4)
                .map(|t| t.name.as_str())
                .collect();
            let mut s = format!("Taken by {taken_by}");
            if let Some(w) = &when {
                let _ = write!(s, " on {w}");
            }
            if let Some(l) = &landmark {
                let _ = write!(s, " at {l}");
            }
            if !labels.is_empty() {
                let _ = write!(s, ". I can see {}", labels.join(", ").to_lowercase());
            }
            s.push('.');
            s
        }
    };
    let related: Vec<MediaId> = match &media.trip {
        Some(t) => app
            .storage
            .get_trip(household, t)
            .await?
            .and_then(|trip| {
                trip.days
                    .iter()
                    .find(|d| d.media.contains(&media.id))
                    .map(|d| {
                        d.media
                            .iter()
                            .filter(|m| *m != &media.id)
                            .take(5)
                            .cloned()
                            .collect()
                    })
            })
            .unwrap_or_default(),
        None => vec![],
    };
    Ok(json!({
        "answer": answer,
        "landmark": landmark,
        "taken_by": taken_by,
        "when": when,
        "related": related,
        "media": media.id,
    }))
}

fn target<'a>(
    records: &'a [Presence],
    tv: Option<&str>,
    now: time::OffsetDateTime,
) -> Option<&'a Presence> {
    match tv {
        Some(d) => records
            .iter()
            .find(|p| p.device_id.as_str() == d && p.is_fresh(now)),
        None => freshest(records, now),
    }
}

pub async fn ask(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let body: AskBody = req.json()?;
    let question = body.question.trim().to_owned();
    if question.is_empty() {
        return Err(ApiError::bad_request("question required"));
    }
    let now = app.now();
    let records = app.storage.list_presence(&household).await?;
    if let Some(p) = target(&records, body.tv.as_deref(), now) {
        let media = app
            .storage
            .get_media(&household, &p.media)
            .await?
            .filter(|m| !m.is_removed());
        if let Some(media) = media {
            let card = describe(app, &household, &media).await?;
            app.storage
                .append_event(
                    &household,
                    now,
                    EventKind::AnswerCard {
                        question: question.clone(),
                        answer: card["answer"].as_str().unwrap_or_default().to_owned(),
                        media: Some(media.id.clone()),
                    },
                )
                .await?;
            return Resp::ok(
                json!({ "mode": "describe", "tv": p.device_id, "question": question, "card": card }),
            );
        }
    }

    // Nothing on screen: the question is a search.
    let hits = search::run(app, &household, &question, None, search::DEFAULT_LIMIT).await?;
    if hits.is_empty() {
        return Resp::ok(
            json!({ "mode": "nothing", "question": question, "hint": "Try a place, a person or a day." }),
        );
    }
    let playlist = Playlist {
        id: PlaylistId::random(),
        household: household.clone(),
        name: question.chars().take(60).collect(),
        source: format!("ask:{question}"),
        media: hits.iter().map(|h| h.media.clone()).collect(),
        created_at: now,
    };
    app.storage.put_playlist(&playlist).await?;
    app.storage
        .append_event(
            &household,
            now,
            EventKind::PlaylistCreated {
                playlist: playlist.id.clone(),
                name: playlist.name.clone(),
            },
        )
        .await?;
    let by: Vec<ContributorId> = hits.iter().map(|h| h.contributor.clone()).collect();
    Resp::ok(
        json!({ "mode": "search", "question": question, "playlist": playlist.id, "count": hits.len(), "contributors": by }),
    )
}
