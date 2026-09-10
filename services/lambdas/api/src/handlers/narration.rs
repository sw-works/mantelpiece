//! Narration (F-709, research R10): one day's story through `Speech` into an MP3 under
//! `narration/`, behind the household's flag. `POST` is the `synthesize_narration` tool;
//! `GET` presigns the audio for the TV.

use memorylane_core::events::EventKind;
use memorylane_core::ids::TripId;
use serde::Deserialize;
use serde_json::json;

use crate::app::App;
use crate::error::ApiError;
use crate::req::{Req, Resp};

pub const VOICE: &str = "Joanna";

fn key(household: &memorylane_core::ids::HouseholdId, trip: &TripId, day: u32) -> String {
    format!("narration/{household}/{trip}/day-{day}.mp3")
}

#[derive(Deserialize)]
struct Body {
    day: u32,
}

pub async fn synthesize(app: &App, req: &Req, trip: &str) -> Result<Resp, ApiError> {
    let trip = TripId::new(trip);
    let access = app.owner_or_co_editor(req, &trip).await?;
    let household = access.owner_household().clone();
    let enabled = app
        .storage
        .get_household(&household)
        .await?
        .is_some_and(|h| h.narration_enabled);
    if !enabled {
        return Err(ApiError::conflict("narration is off for this household"));
    }
    let body: Body = req.json()?;
    let record = app
        .storage
        .get_trip(&household, &trip)
        .await?
        .ok_or_else(|| ApiError::not_found("no such trip"))?;
    let day = record
        .days
        .iter()
        .find(|d| d.index == body.day)
        .ok_or_else(|| ApiError::not_found("no such day"))?;
    let story = day
        .story
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ApiError::conflict("that day has no story yet"))?;
    let audio = app.speech.synthesize(story, VOICE).await?;
    let audio_key = key(&household, &trip, body.day);
    app.blobs.put(&audio_key, audio, "audio/mpeg").await?;
    app.emit(
        &household,
        EventKind::NarrationReady {
            trip: trip.clone(),
            day: body.day,
            audio_key: audio_key.clone(),
        },
    )
    .await?;
    Resp::created(json!({ "trip": trip, "day": body.day, "audio_key": audio_key }))
}

/// `GET /trips/{trip}/narration?day=`: presigned audio, 404 until synthesised.
pub async fn url(app: &App, req: &Req, trip: &str) -> Result<Resp, ApiError> {
    let trip = TripId::new(trip);
    let access = app.trip_access(req, &trip).await?;
    let day: u32 = req
        .query_param("day")?
        .parse()
        .map_err(|_| ApiError::bad_request("day must be an integer"))?;
    let audio_key = key(access.owner_household(), &trip, day);
    if app.blobs.get(&audio_key).await.is_err() {
        return Err(ApiError::not_found("no narration for that day yet"));
    }
    Resp::ok(json!({ "url": app.blobs.presign_get(&audio_key).await?, "day": day }))
}
