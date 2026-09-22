//! Likes and comments (F-710) from contributors, member households and the read-only
//! web view. One like per actor per target; comments at most 200 characters. Every
//! reaction reaches the owner's ticker and, through fan-out, the other member TVs.

use mantelpiece_core::events::EventKind;
use mantelpiece_core::ids::{HouseholdId, TripId};
use mantelpiece_core::reaction::{Reaction, ReactionKind, ReactionTarget};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::app::{App, Caller, TripAccess};
use crate::error::ApiError;
use crate::req::{Req, Resp};
use crate::tokens::Session;

#[derive(Deserialize)]
struct Body {
    #[serde(flatten)]
    target: ReactionTarget,
    kind: String,
    text: Option<String>,
    /// The read-only viewer's name (no session).
    name: Option<String>,
}

/// Who is reacting: (owner household holding the trip, actor id, actor name).
async fn actor(
    app: &App,
    req: &Req,
    trip: &TripId,
    name: Option<&str>,
) -> Result<(HouseholdId, String, String), ApiError> {
    match app.caller(req) {
        Ok(Caller::Contributor(Session::Upload {
            household,
            contributor,
            trip: bound,
            ..
        })) => {
            if bound.as_ref().is_some_and(|t| t != trip) {
                return Err(ApiError::forbidden("not your trip"));
            }
            let c = app
                .storage
                .get_contributor(&household, &contributor)
                .await?
                .filter(mantelpiece_core::contributor::Contributor::is_active)
                .ok_or_else(|| ApiError::forbidden("contributor removed"))?;
            Ok((household, contributor.to_string(), c.display_name))
        }
        Ok(Caller::Contributor(_)) => Err(ApiError::forbidden("sign in required")),
        Ok(Caller::Owner { .. }) => {
            let access = app.trip_access(req, trip).await?;
            let viewer = access.viewing_household().clone();
            let name = app
                .storage
                .get_household(&viewer)
                .await?
                .map_or_else(|| "A household".to_owned(), |h| h.name);
            let owner = access.owner_household().clone();
            let label = if matches!(access, TripAccess::Owner { .. }) {
                "You".to_owned()
            } else {
                name
            };
            Ok((owner, viewer.to_string(), label))
        }
        Err(e) if e.status == 401 && req.query.contains_key("code") => {
            let (household, opened) = app.viewer(req).await?;
            if &opened != trip {
                return Err(ApiError::forbidden("that code opens another trip"));
            }
            let name = name
                .map(str::trim)
                .filter(|n| (2..=40).contains(&n.chars().count()))
                .ok_or_else(|| ApiError::bad_request("name must be 2 to 40 characters"))?;
            let slug: String = name
                .to_lowercase()
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect();
            Ok((household, format!("viewer:{slug}"), name.to_owned()))
        }
        Err(e) => Err(e),
    }
}

pub async fn create(app: &App, req: &Req, trip: &str) -> Result<Resp, ApiError> {
    let trip = TripId::new(trip);
    let body: Body = req.json()?;
    let (household, actor_id, actor_name) = actor(app, req, &trip, body.name.as_deref()).await?;
    if app.storage.get_trip(&household, &trip).await?.is_none() {
        return Err(ApiError::not_found("no such trip"));
    }
    let now = app.now();
    let reaction = match body.kind.as_str() {
        "like" => Reaction::like(
            household.clone(),
            trip.clone(),
            body.target,
            actor_id,
            actor_name,
            now,
        ),
        "comment" => Reaction::comment(
            household.clone(),
            trip.clone(),
            body.target,
            actor_id,
            actor_name,
            body.text.as_deref().unwrap_or(""),
            now,
        )
        .map_err(|e| ApiError::bad_request(e.to_string()))?,
        _ => return Err(ApiError::bad_request("kind must be like or comment")),
    };
    app.storage.put_reaction(&reaction).await?;
    let text = match &reaction.kind {
        ReactionKind::Comment { text } => Some(text.clone()),
        ReactionKind::Like => None,
    };
    app.emit(
        &household,
        EventKind::ReactionReceived {
            trip: trip.clone(),
            target: reaction.target.key(),
            actor_name: reaction.actor_name.clone(),
            kind: body.kind.clone(),
            text,
        },
    )
    .await?;
    Resp::created(reaction)
}

/// All reactions on the trip plus counts and the latest comment per target.
pub async fn list(app: &App, req: &Req, trip: &str) -> Result<Resp, ApiError> {
    let trip = TripId::new(trip);
    let household = match app.caller(req) {
        Ok(Caller::Contributor(Session::Upload {
            household,
            trip: bound,
            ..
        })) => {
            if bound.as_ref().is_some_and(|t| t != &trip) {
                return Err(ApiError::forbidden("not your trip"));
            }
            household
        }
        Ok(Caller::Contributor(_)) => return Err(ApiError::forbidden("sign in required")),
        Ok(Caller::Owner { .. }) => app.trip_access(req, &trip).await?.owner_household().clone(),
        Err(e) if e.status == 401 && req.query.contains_key("code") => {
            let (household, opened) = app.viewer(req).await?;
            if opened != trip {
                return Err(ApiError::forbidden("that code opens another trip"));
            }
            household
        }
        Err(e) => return Err(e),
    };
    let reactions = app.storage.list_reactions(&household, &trip).await?;
    let mut summary: serde_json::Map<String, Value> = serde_json::Map::new();
    for r in &reactions {
        let entry = summary
            .entry(r.target.key())
            .or_insert_with(|| json!({ "likes": 0, "comments": 0, "latest_comment": null }));
        match &r.kind {
            ReactionKind::Like => {
                entry["likes"] = json!(entry["likes"].as_u64().unwrap_or(0) + 1);
            }
            ReactionKind::Comment { text } => {
                entry["comments"] = json!(entry["comments"].as_u64().unwrap_or(0) + 1);
                entry["latest_comment"] = json!({ "by": r.actor_name, "text": text });
            }
        }
    }
    Resp::ok(json!({ "reactions": reactions, "summary": summary }))
}
