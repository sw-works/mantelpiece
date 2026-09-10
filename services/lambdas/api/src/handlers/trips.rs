//! Read trips. Owners see all; contributors see the trip their code names (F-159); the
//! read-only web view opens one trip by its code with no session.

use memorylane_core::ids::{HouseholdId, TripId};
use serde_json::{json, Value};

use crate::app::{App, Caller};
use crate::error::ApiError;
use crate::req::{Req, Resp};
use crate::tokens::Session;

pub async fn list(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let caller = app.caller(req)?;
    let trips = app.storage.list_trips(caller.household()).await?;
    let visible: Vec<_> = match &caller {
        Caller::Owner { .. } => trips,
        Caller::Contributor(Session::Upload { trip: Some(t), .. }) => {
            trips.into_iter().filter(|x| &x.id == t).collect()
        }
        Caller::Contributor(_) => vec![],
    };
    Resp::ok(visible)
}

/// The trip plus who took what: `contributor_names` (id to display name) and
/// `media_credits` (media id to contributor id) so every thumbnail can carry a name
/// (F-154). `clock_offsets` on the trip already say whose clock was corrected (F-155).
pub async fn trip_json(app: &App, household: &HouseholdId, id: &TripId) -> Result<Value, ApiError> {
    let trip = app
        .storage
        .get_trip(household, id)
        .await?
        .ok_or_else(|| ApiError::not_found("no such trip"))?;
    let contributors = app.storage.list_contributors(household).await?;
    let names: serde_json::Map<String, Value> = contributors
        .iter()
        .filter(|c| trip.contributors.contains(&c.id))
        .map(|c| (c.id.to_string(), Value::String(c.display_name.clone())))
        .collect();
    let credits: serde_json::Map<String, Value> = app
        .storage
        .list_media(household)
        .await?
        .into_iter()
        .filter(|m| m.trip.as_ref() == Some(id) && !m.is_removed())
        .map(|m| (m.id.to_string(), Value::String(m.contributor.to_string())))
        .collect();
    let mut v = serde_json::to_value(&trip).map_err(|e| ApiError::internal(e.to_string()))?;
    v["contributor_names"] = Value::Object(names);
    v["media_credits"] = Value::Object(credits);
    Ok(v)
}

pub async fn get(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let caller = app.caller(req)?;
    let id = TripId::new(id);
    if let Caller::Contributor(Session::Upload { trip, .. }) = &caller {
        if trip.as_ref() != Some(&id) {
            return Err(ApiError::forbidden("not your trip"));
        }
    }
    Resp::ok(trip_json(app, caller.household(), &id).await?)
}

/// `GET /view?code=` for the phone's read-only page (F-159): the trip with credits and the
/// household name, and nothing that needs a session.
pub async fn view(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let (household, trip) = app.viewer(req).await?;
    let record = app.storage.get_household(&household).await?;
    Resp::ok(json!({
        "household_name": record.map(|h| h.name),
        "trip": trip_json(app, &household, &trip).await?,
    }))
}
