//! The family channel: phones send photos to the TV, and the TV answers. The TV loves an
//! arrival with one button; the sender's phone gets a web push, and the note waits in the
//! phone's inbox either way.

use memorylane_core::contributor::{Contributor, InboxNote, PushSubscription};
use memorylane_core::ids::{ContributorId, HouseholdId, MediaId};
use memorylane_store::push::{PushMessage, PushOutcome};
use serde::Deserialize;
use serde_json::json;

use crate::app::App;
use crate::error::ApiError;
use crate::req::{Req, Resp};
use crate::tokens::Session;

/// The phone behind a request and the contributor it acts as: a paired phone (its
/// subject is its contributor id, or the household id for an Amazon-signed owner) or a
/// guest holding a trip code. The TV and a phone still waiting for approval are refused.
pub async fn phone(app: &App, req: &Req) -> Result<(HouseholdId, Contributor), ApiError> {
    let (household, id) = match app.session(req)? {
        Some(Session::Phone { household, subject }) => {
            let id = ContributorId::new(subject.as_str());
            (household, id)
        }
        Some(Session::Upload { .. }) => match app.caller(req)? {
            crate::app::Caller::Contributor(Session::Upload {
                household,
                contributor,
                ..
            }) => (household, contributor),
            _ => return Err(ApiError::forbidden("not a phone session")),
        },
        Some(Session::Tv { .. }) => return Err(ApiError::forbidden("only a phone can do this")),
        Some(Session::Pairing { .. }) => {
            return Err(ApiError::forbidden("wait for the TV to let this phone in"))
        }
        None => return Err(ApiError::unauthorized("pair this phone with the TV first")),
    };
    let contributor = app
        .storage
        .get_contributor(&household, &id)
        .await?
        .filter(Contributor::is_active)
        .ok_or_else(|| ApiError::forbidden("this phone is no longer paired"))?;
    Ok((household, contributor))
}

/// `GET /push/key`: the VAPID public key the browser subscribes with.
pub fn key(app: &App) -> Result<Resp, ApiError> {
    Resp::ok(json!({ "key": app.push.public_key() }))
}

/// `PushSubscription.toJSON()` as the browser sends it.
#[derive(Deserialize)]
struct SubscriptionBody {
    endpoint: String,
    keys: Keys,
}

#[derive(Deserialize)]
struct Keys {
    p256dh: String,
    auth: String,
}

/// `PUT /push/subscription`: this phone wants to hear when the TV reacts.
pub async fn subscribe(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let (_, mut contributor) = phone(app, req).await?;
    let body: SubscriptionBody = req.json()?;
    if !body.endpoint.starts_with("https://") {
        return Err(ApiError::bad_request("push endpoint must be https"));
    }
    contributor.push = Some(PushSubscription {
        endpoint: body.endpoint,
        p256dh: body.keys.p256dh,
        auth: body.keys.auth,
    });
    app.storage.put_contributor(&contributor).await?;
    Resp::ok(json!({ "push": true }))
}

/// `DELETE /push/subscription`.
pub async fn unsubscribe(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let (_, mut contributor) = phone(app, req).await?;
    contributor.push = None;
    app.storage.put_contributor(&contributor).await?;
    Resp::no_content()
}

/// `GET /inbox`: what the TV said about this phone's photos, newest first.
pub async fn inbox(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let (_, contributor) = phone(app, req).await?;
    Resp::ok(json!({ "notes": contributor.inbox, "push": contributor.push.is_some() }))
}

#[derive(Deserialize)]
struct LoveBody {
    contributor: String,
    count: u32,
    cover: Option<String>,
}

/// `POST /arrivals/love`: the TV loved a phone's photos. Writes the note, then pushes it
/// when the phone subscribed; a subscription the push service no longer knows is dropped.
pub async fn love(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = match app.session(req)? {
        Some(Session::Tv { household, .. }) => household,
        Some(_) => return Err(ApiError::forbidden("only the TV can do this")),
        None => return Err(ApiError::unauthorized("TV session required")),
    };
    let body: LoveBody = req.json()?;
    let mut contributor = app
        .storage
        .get_contributor(&household, &ContributorId::new(body.contributor))
        .await?
        .ok_or_else(|| ApiError::not_found("no such contributor"))?;
    let from = app
        .storage
        .get_household(&household)
        .await?
        .map_or_else(|| "The TV".to_owned(), |h| h.name);
    let count = body.count.max(1);
    contributor.note(InboxNote {
        at: app.now(),
        from: from.clone(),
        kind: "love".into(),
        count,
        cover: body.cover.map(MediaId::new),
    });
    let mut pushed = false;
    if let Some(sub) = contributor.push.clone() {
        let noun = if count == 1 { "photo" } else { "photos" };
        let message = PushMessage {
            title: from,
            body: format!("loved your {count} {noun}"),
            url: format!("{}/?inbox=1", app.config.upload_base),
        };
        match app.push.send(&sub, &message).await {
            Ok(PushOutcome::Sent) => pushed = true,
            Ok(PushOutcome::Gone) => contributor.push = None,
            // A push service hiccup still leaves the note in the inbox.
            Err(e) => tracing::warn!(error = %e, "push not delivered"),
        }
    }
    app.storage.put_contributor(&contributor).await?;
    Resp::ok(json!({ "pushed": pushed, "noted": true }))
}
