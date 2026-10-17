//! Trip reels and day films (F-713, F-714, specs/004-trip-reels/contracts/api.md).
//!
//! `GET /trips/{trip}/reel?day=` plans the reel on demand for the viewing household (its
//! pins and hides apply) and presigns the music; nothing is stored. `GET /trips/{trip}/film`
//! presigns a stored day film; `POST` queues one.

use mantelpiece_core::film::FilmKeys;
use mantelpiece_core::ids::TripId;
use mantelpiece_reels::plan::{self, Frame, PlanError, Request};
use serde::Deserialize;
use serde_json::json;

use crate::app::App;
use crate::error::ApiError;
use crate::req::{Req, Resp};

fn day_param(req: &Req) -> Result<Option<u32>, ApiError> {
    req.query
        .get("day")
        .map(|d| {
            d.parse()
                .map_err(|_| ApiError::bad_request("day must be an integer"))
        })
        .transpose()
}

/// `GET /trips/{trip}/reel?day=`: the plan, with the music presigned.
pub async fn reel(app: &App, req: &Req, trip: &str) -> Result<Resp, ApiError> {
    let trip = TripId::new(trip);
    let access = app.trip_access(req, &trip).await?;
    let owner = access.owner_household().clone();
    let record = app
        .storage
        .get_trip(&owner, &trip)
        .await?
        .ok_or_else(|| ApiError::not_found("no such trip"))?;
    let media: Vec<_> = app
        .storage
        .list_media(&owner)
        .await?
        .into_iter()
        .filter(|m| m.trip.as_ref() == Some(&trip))
        .collect();
    let mut reel = plan::plan(&Request {
        trip: &record,
        day: day_param(req)?,
        media: &media,
        viewer: access.viewing_household(),
        frame: Frame::TV,
    })
    .map_err(|e| match e {
        PlanError::NoSuchDay => ApiError::not_found("no such day"),
        PlanError::TooFewPhotos => ApiError::new(422, "too few photos"),
    })?;
    reel.track.url = app.blobs.presign_get(&reel.track.url).await?;
    Resp::ok(reel)
}

/// `GET /trips/{trip}/film?day=`: a stored day film, 404 until one is made.
pub async fn film(app: &App, req: &Req, trip: &str) -> Result<Resp, ApiError> {
    let trip = TripId::new(trip);
    let access = app.trip_access(req, &trip).await?;
    let day = day_param(req)?.ok_or_else(|| ApiError::bad_request("day is required"))?;
    let keys = FilmKeys::new(access.owner_household(), &trip);
    let Ok(marker) = app.blobs.get(&keys.marker(day)).await else {
        return Err(ApiError::not_found("no film yet"));
    };
    let (_, duration_ms, made_at) = FilmKeys::read_marker(&marker);
    Resp::ok(json!({
        "day": day,
        "url": app.blobs.presign_get(&keys.film(day)).await?,
        "duration_ms": duration_ms,
        "made_at": made_at,
    }))
}

#[derive(Deserialize)]
struct Body {
    day: u32,
}

/// `POST /trips/{trip}/film`: queue a render of one day now (owner or co-editor).
pub async fn make(app: &App, req: &Req, trip: &str) -> Result<Resp, ApiError> {
    let trip = TripId::new(trip);
    let access = app.owner_or_co_editor(req, &trip).await?;
    let owner = access.owner_household().clone();
    let body: Body = req.json()?;
    let record = app
        .storage
        .get_trip(&owner, &trip)
        .await?
        .ok_or_else(|| ApiError::not_found("no such trip"))?;
    if !record.days.iter().any(|d| d.index == body.day) {
        return Err(ApiError::not_found("no such day"));
    }
    app.films
        .queue(&owner, &trip, body.day)
        .await
        .map_err(|e| ApiError::new(503, e.to_string()))?;
    Resp::accepted(json!({ "queued": true }))
}
