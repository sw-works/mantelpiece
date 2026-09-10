//! Read trips. Owners see all; contributors see the trip their code names (F-159); the
//! read-only web view opens one trip by its code with no session.

use memorylane_core::events::EventKind;
use memorylane_core::ids::{HouseholdId, TripId};
use memorylane_core::narrative_check::{check, Allowed};
use memorylane_core::trip::{Narrative, NarrativeCheck, Trip};
use serde::Deserialize;
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

/// `GET /trips/{trip}/map?day=` : a presigned URL for the trip's route map, or one day's.
pub async fn map(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let id = TripId::new(id);
    let access = app.trip_access(req, &id).await?;
    let trip = app
        .storage
        .get_trip(access.owner_household(), &id)
        .await?
        .ok_or_else(|| ApiError::not_found("no such trip"))?;
    let key = match req.query.get("day") {
        None => trip.route.map_key.clone(),
        Some(d) => {
            let index: u32 = d
                .parse()
                .map_err(|_| ApiError::bad_request("day must be an integer"))?;
            trip.days
                .iter()
                .find(|day| day.index == index)
                .ok_or_else(|| ApiError::not_found("no such day"))?
                .route
                .map_key
                .clone()
        }
    };
    let key = key.ok_or_else(|| ApiError::not_found("no map yet"))?;
    Resp::ok(json!({ "url": app.blobs.presign_get(&key).await?, "inferred": trip.route.inferred }))
}

/// Everything the metadata lets a story mention (constitution IV).
async fn allowed_for(app: &App, household: &HouseholdId, trip: &Trip) -> Result<Allowed, ApiError> {
    let mut allowed = Allowed::new();
    for day in &trip.days {
        allowed.date(day.date.year(), u8::from(day.date.month()));
        if let Some(t) = &day.title {
            allowed.name(t);
            if let Some((_, place)) = t.split_once(": ") {
                allowed.name(place);
            }
        }
    }
    if trip.days.is_empty() {
        allowed.date(trip.start.year(), u8::from(trip.start.month()));
        allowed.date(trip.end.year(), u8::from(trip.end.month()));
    }
    allowed.name(&trip.name);
    if let Some((place, _)) = trip.name.split_once(',') {
        allowed.name(place);
    }
    for c in app.storage.list_contributors(household).await? {
        if trip.contributors.contains(&c.id) {
            allowed.name(&c.display_name);
        }
    }
    for m in app.storage.list_media(household).await? {
        if m.trip.as_ref() == Some(&trip.id) {
            if let Some(l) = &m.enrichment.landmark {
                allowed.name(&l.name);
            }
            if let Some(d) = &m.description {
                if let Some(l) = &d.landmark {
                    allowed.name(l);
                }
            }
        }
    }
    Ok(allowed)
}

#[derive(Deserialize)]
struct NarrativeBody {
    #[serde(default)]
    text: String,
    model_id: String,
    /// One paragraph per day for narration (F-709).
    #[serde(default)]
    stories: Option<Vec<String>>,
    /// The agent gave up after the retry; store a withheld narrative.
    #[serde(default)]
    withhold: bool,
}

/// `POST /trips/{trip}/narrative`: the `save_narrative` tool. The story passes the
/// place-and-date check here too (F-301); a failing story is stored as withheld and the
/// call answers 422 so the agent knows.
pub async fn save_narrative(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let id = TripId::new(id);
    let access = app.owner_or_co_editor(req, &id).await?;
    let household = access.owner_household().clone();
    let body: NarrativeBody = req.json()?;
    let mut trip = app
        .storage
        .get_trip(&household, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("no such trip"))?;
    let now = app.now();
    let problems = if body.withhold || body.text.trim().is_empty() {
        vec!["withheld by the agent".to_owned()]
    } else {
        check(&body.text, &allowed_for(app, &household, &trip).await?)
    };
    if problems.is_empty() {
        trip.narrative = Some(Narrative {
            text: body.text.trim().to_owned(),
            model_id: body.model_id,
            generated_at: now,
            check: NarrativeCheck::Passed,
        });
        if let Some(stories) = body.stories.filter(|s| s.len() == trip.days.len()) {
            for (day, story) in trip.days.iter_mut().zip(stories) {
                day.story = Some(story);
            }
        }
        app.storage.put_trip(&trip).await?;
        return Resp::ok(json!({ "check": "passed" }));
    }
    trip.narrative = Some(Narrative::withheld(body.model_id, now));
    for day in &mut trip.days {
        day.story = None;
    }
    app.storage.put_trip(&trip).await?;
    Ok(crate::req::Resp {
        status: 422,
        body: json!({ "check": "withheld", "problems": problems }),
    })
}

/// `POST /trips/{trip}/narrative/regenerate` (F-302): clear the story and re-run the Trip
/// Curator through the bus. The TV shows "being written" until the new one lands.
pub async fn regenerate(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let id = TripId::new(id);
    let access = app.owner_or_co_editor(req, &id).await?;
    let household = access.owner_household().clone();
    let mut trip = app
        .storage
        .get_trip(&household, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("no such trip"))?;
    trip.narrative = None;
    for day in &mut trip.days {
        day.story = None;
    }
    app.storage.put_trip(&trip).await?;
    let event = app
        .storage
        .append_event(
            &household,
            app.now(),
            EventKind::TripDetected {
                trip: id,
                new_media: 0,
            },
        )
        .await?;
    app.bus.publish(&event).await?;
    Resp::ok(json!({ "requested": true }))
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
