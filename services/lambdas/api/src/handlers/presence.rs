//! What each TV is showing (research R3): `PUT /presence` on every slide change and
//! pause, `GET /presence` for the phone's Ask page.

use mantelpiece_core::ids::{DeviceId, MediaId, TripId};
use mantelpiece_core::presence::Presence;
use serde::Deserialize;

use crate::app::App;
use crate::error::ApiError;
use crate::req::{Req, Resp};

#[derive(Deserialize)]
struct PresenceBody {
    device_id: String,
    media: MediaId,
    trip: Option<TripId>,
    day: Option<u32>,
    #[serde(default)]
    paused: bool,
}

pub async fn put(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let body: PresenceBody = req.json()?;
    if body.device_id.is_empty() {
        return Err(ApiError::bad_request("device_id required"));
    }
    let presence = Presence {
        household,
        device_id: DeviceId::new(body.device_id),
        media: body.media,
        trip: body.trip,
        day: body.day,
        paused: body.paused,
        at: app.now(),
    };
    app.storage.put_presence(&presence).await?;
    Resp::no_content()
}

/// Fresh records only; a TV that went quiet ten minutes ago shows nothing.
pub async fn list(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let now = app.now();
    let fresh: Vec<Presence> = app
        .storage
        .list_presence(&household)
        .await?
        .into_iter()
        .filter(|p| p.is_fresh(now))
        .collect();
    Resp::ok(fresh)
}
