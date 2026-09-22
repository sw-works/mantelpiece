//! Live from the event. A paired phone asks to go live; the API makes an IVS channel for
//! that one broadcast, hands the stream key to the phone alone, and tells the TV where to
//! watch. Stopping cuts the stream and deletes the channel. IVS refuses to delete a
//! channel whose stream is still winding down, so those wait in `stale_channels` and go on
//! the next start or stop.

use memorylane_core::events::EventKind;
use memorylane_core::household::{Household, LiveBroadcast};
use memorylane_core::ids::{ContributorId, HouseholdId};
use serde_json::json;
use time::format_description::well_known::Rfc3339;
use time::Duration;

use crate::app::App;
use crate::error::ApiError;
use crate::handlers::family;
use crate::req::{Req, Resp};
use crate::tokens::Session;

/// A broadcast older than this is over, whatever the phone said last.
pub const MAX_LIVE: Duration = Duration::hours(4);

async fn household(app: &App, id: &HouseholdId) -> Result<Household, ApiError> {
    app.storage
        .get_household(id)
        .await?
        .ok_or_else(|| ApiError::not_found("no such household"))
}

/// Delete what IVS refused before; keep what it still refuses.
async fn sweep(app: &App, h: &mut Household) {
    let mut left = Vec::new();
    for arn in std::mem::take(&mut h.stale_channels) {
        match app.live.delete(&arn).await {
            Ok(true) => {}
            Ok(false) => left.push(arn),
            Err(e) => {
                tracing::warn!(error = %e, arn = %arn, "live channel not deleted");
                left.push(arn);
            }
        }
    }
    h.stale_channels = left;
}

/// End the household's broadcast, if any: cut, delete or park, tell the TV.
async fn end(app: &App, h: &mut Household) -> Result<bool, ApiError> {
    let Some(live) = h.live.take() else {
        return Ok(false);
    };
    if let Err(e) = app.live.stop(&live.channel_arn).await {
        tracing::warn!(error = %e, "live stream not stopped");
    }
    match app.live.delete(&live.channel_arn).await {
        Ok(true) => {}
        Ok(false) | Err(_) => h.stale_channels.push(live.channel_arn.clone()),
    }
    app.emit(&h.id, EventKind::LiveEnded { broadcast: live.id })
        .await?;
    Ok(true)
}

/// `POST /live`: this phone goes live on the household's TV. Any earlier broadcast ends.
pub async fn start(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let (household_id, contributor) = family::phone(app, req).await?;
    let mut h = household(app, &household_id).await?;
    // Leftovers first: a channel just parked by `end` would only be refused again.
    sweep(app, &mut h).await;
    end(app, &mut h).await?;
    let id = ContributorId::random().to_string();
    let name: String = format!("ml-{id}")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(128)
        .collect();
    let channel = app.live.create(&name, household_id.as_str()).await?;
    let now = app.now();
    h.live = Some(LiveBroadcast {
        id: id.clone(),
        channel_arn: channel.arn,
        playback_url: channel.playback_url.clone(),
        contributor: contributor.id.clone(),
        display_name: contributor.display_name.clone(),
        started_at: now,
    });
    app.storage.put_household(&h).await?;
    app.emit(
        &household_id,
        EventKind::LiveStarted {
            broadcast: id.clone(),
            display_name: contributor.display_name.clone(),
            playback_url: channel.playback_url.clone(),
        },
    )
    .await?;
    Resp::created(json!({
        "id": id,
        "ingest_endpoint": channel.ingest_endpoint,
        "stream_key": channel.stream_key,
        "playback_url": channel.playback_url,
        "tv": h.name,
    }))
}

/// `POST /live/stop`: the broadcasting phone or the TV ends the broadcast.
pub async fn stop(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let (household_id, phone) = match app.session(req)? {
        Some(Session::Tv { household, .. }) => (household, None),
        Some(_) => {
            let (h, c) = family::phone(app, req).await?;
            (h, Some(c.id))
        }
        None => return Err(ApiError::unauthorized("session required")),
    };
    let mut h = household(app, &household_id).await?;
    if let (Some(phone), Some(live)) = (&phone, &h.live) {
        if &live.contributor != phone {
            return Err(ApiError::forbidden("another phone is live"));
        }
    }
    sweep(app, &mut h).await;
    let ended = end(app, &mut h).await?;
    app.storage.put_household(&h).await?;
    Resp::ok(json!({ "ended": ended }))
}

/// `GET /live`: what the TV should be showing, for a cold start that missed the event.
pub async fn current(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household_id = match app.session(req)? {
        Some(Session::Pairing { .. }) | None => {
            return Err(ApiError::unauthorized("session required"))
        }
        Some(s) => s.household().clone(),
    };
    let h = household(app, &household_id).await?;
    let live = h
        .live
        .filter(|l| app.now() - l.started_at < MAX_LIVE)
        .map(|l| {
            json!({
                "id": l.id,
                "display_name": l.display_name,
                "playback_url": l.playback_url,
                "started_at": l.started_at.format(&Rfc3339).unwrap_or_default(),
            })
        });
    Resp::ok(json!({ "live": live }))
}
