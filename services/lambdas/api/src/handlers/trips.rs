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

/// Own trips, plus trips joined from another TV with `shared_by` (F-162).
pub async fn list(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let caller = app.caller(req)?;
    let trips = app.storage.list_trips(caller.household()).await?;
    let mut visible: Vec<Value> = match &caller {
        Caller::Owner { .. } => trips,
        Caller::Contributor(Session::Upload { trip: Some(t), .. }) => {
            trips.into_iter().filter(|x| &x.id == t).collect()
        }
        Caller::Contributor(_) => vec![],
    }
    .into_iter()
    .map(|t| serde_json::to_value(t).unwrap_or(Value::Null))
    .collect();
    if let Caller::Owner { household, .. } = &caller {
        for m in app.storage.list_memberships(household).await? {
            if !m.is_active() {
                continue;
            }
            if let Some(t) = app.storage.get_trip(&m.owner_household, &m.trip).await? {
                let mut v = serde_json::to_value(&t).unwrap_or(Value::Null);
                let owner_name = app
                    .storage
                    .get_household(&m.owner_household)
                    .await?
                    .map_or_else(|| m.owner_household.to_string(), |h| h.name);
                v["shared_by"] = Value::String(owner_name);
                v["owner_household"] = Value::String(m.owner_household.to_string());
                visible.push(v);
            }
        }
    }
    Resp::ok(visible)
}

#[derive(Deserialize, Default)]
struct PlayBody {
    #[serde(default)]
    source: Option<String>,
}

/// Ask the household's TV to play this trip as a slideshow (F-405 by voice): the TV picks
/// the `play.requested` event off its feed and opens the show. Owner sessions only.
pub async fn play(app: &App, req: &Req, trip: &str) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let trip_id = TripId::new(trip);
    let found = app
        .storage
        .get_trip(&household, &trip_id)
        .await?
        .ok_or_else(|| ApiError::not_found("no such trip"))?;
    let body: PlayBody = if req.body.is_empty() {
        PlayBody::default()
    } else {
        req.json()?
    };
    app.emit(
        &household,
        EventKind::PlayRequested {
            trip: Some(trip_id),
            playlist: None,
            name: found.name.clone(),
            source: body.source.unwrap_or_else(|| "phone".into()),
        },
    )
    .await?;
    Resp::ok(json!({ "requested": true, "trip": found.id, "name": found.name }))
}

/// Per-household view of the highlights (F-163): this household's pins first, its
/// hidden items gone, the owner's own flags untouched.
fn personalise(trip: &mut Trip, viewer: &HouseholdId, media: &[memorylane_core::media::MediaItem]) {
    let by_id = |id: &memorylane_core::ids::MediaId| media.iter().find(|m| &m.id == id);
    let mut pinned: Vec<_> = trip
        .highlights
        .iter()
        .filter(|id| by_id(id).is_some_and(|m| m.pinned_for(viewer)))
        .cloned()
        .collect();
    let rest: Vec<_> = trip
        .highlights
        .iter()
        .filter(|id| !pinned.contains(id) && by_id(id).is_none_or(|m| !m.hidden_for(viewer)))
        .cloned()
        .collect();
    pinned.extend(rest);
    trip.highlights = pinned;
    for day in &mut trip.days {
        day.media
            .retain(|id| by_id(id).is_none_or(|m| !m.hidden_for(viewer)));
    }
}

/// The trip plus who took what: `contributor_names` (id to display name) and
/// `media_credits` (media id to contributor id) so every thumbnail can carry a name
/// (F-154). `clock_offsets` on the trip already say whose clock was corrected (F-155).
pub async fn trip_json(app: &App, household: &HouseholdId, id: &TripId) -> Result<Value, ApiError> {
    trip_json_for(app, household, id, household).await
}

/// Like [`trip_json`] but with `viewer`'s pins and hides applied (F-163) and
/// `contributor_roles` for the owner's Promote action (F-164).
pub async fn trip_json_for(
    app: &App,
    household: &HouseholdId,
    id: &TripId,
    viewer: &HouseholdId,
) -> Result<Value, ApiError> {
    let mut trip = app
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
    let roles: serde_json::Map<String, Value> = contributors
        .iter()
        .filter(|c| trip.contributors.contains(&c.id))
        .map(|c| {
            (
                c.id.to_string(),
                json!(
                    if matches!(c.kind, memorylane_core::contributor::ContributorKind::Owner) {
                        "owner"
                    } else if c.can_edit() {
                        "co_editor"
                    } else {
                        "contributor"
                    }
                ),
            )
        })
        .collect();
    let media: Vec<memorylane_core::media::MediaItem> = app
        .storage
        .list_media(household)
        .await?
        .into_iter()
        .filter(|m| m.trip.as_ref() == Some(id) && !m.is_removed())
        .collect();
    let credits: serde_json::Map<String, Value> = media
        .iter()
        .map(|m| (m.id.to_string(), Value::String(m.contributor.to_string())))
        .collect();
    personalise(&mut trip, viewer, &media);
    let mut v = serde_json::to_value(&trip).map_err(|e| ApiError::internal(e.to_string()))?;
    // Places for the story and for Alexa+: route landmarks plus what Vision recognised.
    let mut places: Vec<String> = trip.landmarks().iter().map(|l| l.name.clone()).collect();
    for m in &media {
        if let Some(l) = &m.enrichment.landmark {
            if !places.contains(&l.name) {
                places.push(l.name.clone());
            }
        }
    }
    v["landmarks"] = json!(places);
    v["contributor_names"] = Value::Object(names);
    v["contributor_roles"] = Value::Object(roles);
    v["media_credits"] = Value::Object(credits);
    if viewer != household {
        v["shared_by"] = json!(app.storage.get_household(household).await?.map(|h| h.name));
        v["owner_household"] = Value::String(household.to_string());
    }
    Ok(v)
}

pub async fn get(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let id = TripId::new(id);
    let access = app.trip_access(req, &id).await?;
    Resp::ok(
        trip_json_for(
            app,
            access.owner_household(),
            &id,
            access.viewing_household(),
        )
        .await?,
    )
}

#[derive(Deserialize)]
struct RenameBody {
    name: String,
}

/// `PATCH /trips/{trip}` (F-164): owner or co-editor renames the trip.
pub async fn rename(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let id = TripId::new(id);
    let access = app.owner_or_co_editor(req, &id).await?;
    let body: RenameBody = req.json()?;
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(ApiError::bad_request("name must be 1 to 80 characters"));
    }
    let mut trip = app
        .storage
        .get_trip(access.owner_household(), &id)
        .await?
        .ok_or_else(|| ApiError::not_found("no such trip"))?;
    trip.name = name.to_owned();
    app.storage.put_trip(&trip).await?;
    Resp::ok(json!({ "id": trip.id, "name": trip.name }))
}

#[derive(Deserialize)]
struct RoleBody {
    contributor: memorylane_core::ids::ContributorId,
    role: memorylane_core::contributor::Role,
}

/// `POST /trips/{trip}/roles` (F-164): the owner promotes or demotes a contributor.
/// Guests can never be co-editors (F-165).
pub async fn set_role(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let id = TripId::new(id);
    let trip = app
        .storage
        .get_trip(&household, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("no such trip"))?;
    let body: RoleBody = req.json()?;
    let _ = trip;
    let mut c = app
        .storage
        .get_contributor(&household, &body.contributor)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown contributor"))?;
    c.set_role(body.role)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    app.storage.put_contributor(&c).await?;
    Resp::ok(json!({ "contributor": c.id, "role": c.role }))
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
    for l in trip.landmarks() {
        allowed.name(&l.name);
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
        .emit(
            &household,
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
