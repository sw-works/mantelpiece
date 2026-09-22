//! Live from the event. A paired phone asks to go live; the TV grants or declines. A grant
//! ends whoever is live now, makes an IVS channel for this one broadcast and hands the
//! stream key to the asking phone alone; the TV learns where to watch from the feed.
//! Stopping cuts the stream and deletes the channel. IVS refuses to delete a channel whose
//! stream is still winding down, so those wait in `stale_channels` and go on the next
//! start or stop.

use memorylane_core::events::EventKind;
use memorylane_core::household::{
    Household, LiveBroadcast, LiveGrant, LiveRequest, LiveRequestStatus,
};
use memorylane_core::ids::{ContributorId, HouseholdId};
use serde_json::{json, Value};
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

use crate::app::App;
use crate::error::ApiError;
use crate::handlers::family;
use crate::req::{Req, Resp};
use crate::tokens::Session;

/// A broadcast older than this is over, whatever the phone said last.
pub const MAX_LIVE: Duration = Duration::hours(4);
/// How long the TV shows a request before it lapses.
pub const REQUEST_TTL: Duration = Duration::seconds(60);
/// Answered requests stay this long so the phone can read the answer.
const REQUEST_KEEP: Duration = Duration::minutes(10);

async fn household(app: &App, id: &HouseholdId) -> Result<Household, ApiError> {
    app.storage
        .get_household(id)
        .await?
        .ok_or_else(|| ApiError::not_found("no such household"))
}

fn rfc3339(t: OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

/// Waiting and still inside its time on the TV.
fn open(r: &LiveRequest, now: OffsetDateTime) -> bool {
    r.status == LiveRequestStatus::Waiting && now - r.requested_at < REQUEST_TTL
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

/// Forget answers nobody will read any more, and a grant whose broadcast is over.
fn prune(h: &mut Household, now: OffsetDateTime) {
    let live = h.live.as_ref().map(|l| l.id.clone());
    h.live_requests.retain(|r| match r.status {
        LiveRequestStatus::Waiting => now - r.requested_at < REQUEST_KEEP,
        LiveRequestStatus::Granted => {
            r.grant.as_ref().map(|g| &g.broadcast) == live.as_ref()
                || now - r.requested_at < REQUEST_KEEP
        }
        LiveRequestStatus::Declined | LiveRequestStatus::Cancelled => {
            now - r.requested_at < REQUEST_KEEP
        }
    });
    // The stream key goes with the broadcast.
    for r in &mut h.live_requests {
        if r.grant.as_ref().map(|g| &g.broadcast) != live.as_ref() {
            r.grant = None;
        }
    }
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

fn tv_household(app: &App, req: &Req) -> Result<HouseholdId, ApiError> {
    match app.session(req)? {
        Some(Session::Tv { household, .. }) => Ok(household),
        Some(_) => Err(ApiError::forbidden("only the TV can answer")),
        None => Err(ApiError::unauthorized("TV session required")),
    }
}

/// `POST /live`: this phone asks the TV to show its camera. Replaces its earlier request.
pub async fn request(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let (household_id, contributor) = family::phone(app, req).await?;
    let mut h = household(app, &household_id).await?;
    let now = app.now();
    if h.live
        .as_ref()
        .is_some_and(|l| l.contributor == contributor.id && now - l.started_at < MAX_LIVE)
    {
        return Err(ApiError::conflict("this phone is already live"));
    }
    prune(&mut h, now);
    let mut closed = Vec::new();
    h.live_requests.retain(|r| {
        let mine = r.contributor == contributor.id;
        if mine && r.status == LiveRequestStatus::Waiting {
            closed.push(r.id.clone());
        }
        !mine
    });
    let id = ContributorId::random().to_string();
    h.live_requests.push(LiveRequest {
        id: id.clone(),
        contributor: contributor.id.clone(),
        display_name: contributor.display_name.clone(),
        requested_at: now,
        status: LiveRequestStatus::Waiting,
        grant: None,
    });
    let replacing = h
        .live
        .as_ref()
        .filter(|l| now - l.started_at < MAX_LIVE)
        .map(|l| l.display_name.clone());
    app.storage.put_household(&h).await?;
    for request in closed {
        app.emit(&household_id, EventKind::LiveRequestClosed { request })
            .await?;
    }
    app.emit(
        &household_id,
        EventKind::LiveRequested {
            request: id.clone(),
            display_name: contributor.display_name.clone(),
            expires_at: now + REQUEST_TTL,
            replacing: replacing.clone(),
        },
    )
    .await?;
    Resp::accepted(json!({
            "request": id,
            "status": "waiting",
            "expires_at": rfc3339(now + REQUEST_TTL),
            "replacing": replacing,
            "tv": h.name,
    }))
}

/// `GET /live/request`: the answer to this phone's latest request. A grant carries where
/// to stream and the key.
pub async fn my_request(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let (household_id, contributor) = family::phone(app, req).await?;
    let h = household(app, &household_id).await?;
    let now = app.now();
    let Some(r) = h
        .live_requests
        .iter()
        .rev()
        .find(|r| r.contributor == contributor.id)
    else {
        return Err(ApiError::not_found("no request from this phone"));
    };
    let status = match r.status {
        LiveRequestStatus::Waiting if !open(r, now) => "expired",
        LiveRequestStatus::Waiting => "waiting",
        LiveRequestStatus::Granted if r.grant.is_none() => "ended",
        LiveRequestStatus::Granted => "granted",
        LiveRequestStatus::Declined => "declined",
        LiveRequestStatus::Cancelled => "cancelled",
    };
    let mut body = json!({ "request": r.id, "status": status, "tv": h.name });
    if let (Some(g), "granted") = (&r.grant, status) {
        body["broadcast"] = Value::from(g.broadcast.clone());
        body["ingest_endpoint"] = Value::from(g.ingest_endpoint.clone());
        body["stream_key"] = Value::from(g.stream_key.clone());
        body["playback_url"] = Value::from(g.playback_url.clone());
    }
    Resp::ok(body)
}

/// `DELETE /live/request`: this phone stops asking.
pub async fn cancel(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let (household_id, contributor) = family::phone(app, req).await?;
    let mut h = household(app, &household_id).await?;
    let mut closed = Vec::new();
    for r in &mut h.live_requests {
        if r.contributor == contributor.id && r.status == LiveRequestStatus::Waiting {
            r.status = LiveRequestStatus::Cancelled;
            closed.push(r.id.clone());
        }
    }
    app.storage.put_household(&h).await?;
    for request in closed {
        app.emit(&household_id, EventKind::LiveRequestClosed { request })
            .await?;
    }
    Resp::no_content()
}

/// `POST /live/requests/{id}/grant`: the TV lets this phone in. Whoever is live now is cut
/// off; a channel is made for the new broadcast.
pub async fn grant(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let household_id = tv_household(app, req)?;
    let mut h = household(app, &household_id).await?;
    let now = app.now();
    let Some(i) = h.live_requests.iter().position(|r| r.id == id) else {
        return Err(ApiError::not_found("no such request"));
    };
    if !open(&h.live_requests[i], now) {
        return Err(ApiError::conflict("the phone is no longer asking"));
    }
    let asking = h.live_requests[i].clone();
    // The phone may have been unpaired while it waited.
    let still_paired = app
        .storage
        .get_contributor(&household_id, &asking.contributor)
        .await?
        .is_some_and(|c| c.is_active());
    if !still_paired {
        h.live_requests[i].status = LiveRequestStatus::Cancelled;
        app.storage.put_household(&h).await?;
        app.emit(
            &household_id,
            EventKind::LiveRequestClosed { request: asking.id },
        )
        .await?;
        return Err(ApiError::conflict("that phone is no longer paired"));
    }
    // Leftovers first: a channel just parked by `end` would only be refused again.
    sweep(app, &mut h).await;
    end(app, &mut h).await?;
    let broadcast = ContributorId::random().to_string();
    let name: String = format!("ml-{broadcast}")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(128)
        .collect();
    let channel = app.live.create(&name, household_id.as_str()).await?;
    h.live = Some(LiveBroadcast {
        id: broadcast.clone(),
        channel_arn: channel.arn,
        playback_url: channel.playback_url.clone(),
        contributor: asking.contributor.clone(),
        display_name: asking.display_name.clone(),
        started_at: now,
    });
    let r = &mut h.live_requests[i];
    r.status = LiveRequestStatus::Granted;
    r.grant = Some(LiveGrant {
        broadcast: broadcast.clone(),
        ingest_endpoint: channel.ingest_endpoint,
        stream_key: channel.stream_key,
        playback_url: channel.playback_url.clone(),
    });
    prune(&mut h, now);
    app.storage.put_household(&h).await?;
    app.emit(
        &household_id,
        EventKind::LiveRequestClosed {
            request: asking.id.clone(),
        },
    )
    .await?;
    app.emit(
        &household_id,
        EventKind::LiveStarted {
            broadcast: broadcast.clone(),
            display_name: asking.display_name,
            playback_url: channel.playback_url,
        },
    )
    .await?;
    Resp::ok(json!({ "broadcast": broadcast }))
}

/// `POST /live/requests/{id}/decline`: the TV says not now.
pub async fn decline(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let household_id = tv_household(app, req)?;
    let mut h = household(app, &household_id).await?;
    let Some(r) = h.live_requests.iter_mut().find(|r| r.id == id) else {
        return Err(ApiError::not_found("no such request"));
    };
    if r.status == LiveRequestStatus::Waiting {
        r.status = LiveRequestStatus::Declined;
    }
    app.storage.put_household(&h).await?;
    app.emit(
        &household_id,
        EventKind::LiveRequestClosed {
            request: id.to_owned(),
        },
    )
    .await?;
    Resp::no_content()
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
    prune(&mut h, app.now());
    app.storage.put_household(&h).await?;
    Resp::ok(json!({ "ended": ended }))
}

/// `GET /live`: what the TV should be showing, and the requests it has not answered, for
/// a cold start that missed the events.
pub async fn current(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let session = app.session(req)?;
    let household_id = match &session {
        Some(Session::Pairing { .. }) | None => {
            return Err(ApiError::unauthorized("session required"))
        }
        Some(s) => s.household().clone(),
    };
    let h = household(app, &household_id).await?;
    let now = app.now();
    let live = h
        .live
        .as_ref()
        .filter(|l| now - l.started_at < MAX_LIVE)
        .map(|l| {
            json!({
                "id": l.id,
                "display_name": l.display_name,
                "playback_url": l.playback_url,
                "started_at": rfc3339(l.started_at),
            })
        });
    let mut body = json!({ "live": live });
    if matches!(session, Some(Session::Tv { .. })) {
        let requests: Vec<Value> = h
            .live_requests
            .iter()
            .filter(|r| open(r, now))
            .map(|r| {
                json!({
                    "request": r.id,
                    "display_name": r.display_name,
                    "expires_at": rfc3339(r.requested_at + REQUEST_TTL),
                })
            })
            .collect();
        body["requests"] = Value::from(requests);
    }
    Resp::ok(body)
}
