//! Opt-in People (F-704, research R9): a face collection per household that exists only
//! while the household says so, unnamed clusters named from the phone, merges, and a
//! search filter. Turning it off deletes the collection and every person record.

use memorylane_core::ids::PersonId;
use memorylane_core::person::Person;
use serde::Deserialize;
use serde_json::json;

use crate::app::App;
use crate::error::ApiError;
use crate::req::{Req, Resp};

/// Whether faces are grouped is the household's call, and the household is the TV: a
/// paired phone names and merges people but cannot switch the feature.
fn switch_holder(app: &App, req: &Req) -> Result<memorylane_core::ids::HouseholdId, ApiError> {
    let caller = app.caller(req)?;
    if caller.paired_phone().is_some() {
        return Err(ApiError::forbidden("only the TV can turn People on or off"));
    }
    app.owner(req)
}

pub async fn enable(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = switch_holder(app, req)?;
    let mut record = app
        .storage
        .get_household(&household)
        .await?
        .unwrap_or_else(|| {
            memorylane_core::household::Household::new(household.clone(), "My TV", app.now())
        });
    if !record.people_enabled {
        app.faces.create_collection(&household).await?;
        record.people_enabled = true;
        app.storage.put_household(&record).await?;
    }
    Resp::ok(json!({ "people_enabled": true }))
}

/// Everything learned about faces goes: the collection, the people, the links on media.
pub async fn disable(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = switch_holder(app, req)?;
    forget_people(app, &household).await?;
    if let Some(mut record) = app.storage.get_household(&household).await? {
        record.people_enabled = false;
        app.storage.put_household(&record).await?;
    }
    Resp::ok(json!({ "people_enabled": false }))
}

pub async fn forget_people(
    app: &App,
    household: &memorylane_core::ids::HouseholdId,
) -> Result<(), ApiError> {
    app.faces.delete_collection(household).await?;
    app.storage.delete_people(household).await?;
    for mut m in app.storage.list_media(household).await? {
        if !m.people.is_empty() {
            m.people.clear();
            app.storage.put_media(&m).await?;
        }
    }
    Ok(())
}

fn view(p: &Person) -> serde_json::Value {
    json!({
        "id": p.id,
        "name": p.name,
        "media_count": p.media_count,
        "sample_media": p.sample_media,
        "faces": p.face_ids.len(),
    })
}

/// Clusters, most photos first; merged clusters are folded away.
pub async fn list(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let enabled = app
        .storage
        .get_household(&household)
        .await?
        .is_some_and(|h| h.people_enabled);
    let people: Vec<serde_json::Value> = app
        .storage
        .list_people(&household)
        .await?
        .iter()
        .filter(|p| !p.is_merged())
        .map(view)
        .collect();
    Resp::ok(json!({ "people_enabled": enabled, "people": people }))
}

#[derive(Deserialize)]
struct UpdateBody {
    name: Option<String>,
    merge_into: Option<PersonId>,
}

/// Name a cluster, or fold it into another (`merge_into`).
pub async fn update(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let id = PersonId::new(id);
    let body: UpdateBody = req.json()?;
    let mut person = app
        .storage
        .get_person(&household, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown person"))?;
    if let Some(name) = body.name {
        let name = name.trim();
        person.name = (!name.is_empty()).then(|| name.chars().take(40).collect());
    }
    if let Some(target_id) = body.merge_into {
        if target_id == id {
            return Err(ApiError::bad_request(
                "cannot merge a person into themselves",
            ));
        }
        let mut target = app
            .storage
            .get_person(&household, &target_id)
            .await?
            .ok_or_else(|| ApiError::not_found("unknown merge target"))?;
        target.merge(&mut person);
        app.storage.put_person(&target).await?;
        // Media that pointed at the merged cluster now point at the target.
        for mut m in app.storage.list_media(&household).await? {
            if m.people.contains(&id) {
                m.people.retain(|p| p != &id);
                if !m.people.contains(&target_id) {
                    m.people.push(target_id.clone());
                }
                app.storage.put_media(&m).await?;
            }
        }
        app.storage.put_person(&person).await?;
        return Resp::ok(view(&target));
    }
    app.storage.put_person(&person).await?;
    Resp::ok(view(&person))
}

/// Resolve names to person ids for the search filter, following merges.
pub async fn resolve_names(
    app: &App,
    household: &memorylane_core::ids::HouseholdId,
    names: &[String],
) -> Result<Vec<PersonId>, ApiError> {
    let people = app.storage.list_people(household).await?;
    let mut ids = Vec::new();
    for name in names {
        let wanted = name.trim().to_lowercase();
        if wanted.is_empty() {
            continue;
        }
        let found = people
            .iter()
            .find(|p| p.name.as_ref().is_some_and(|n| n.to_lowercase() == wanted))
            .map(|p| p.merged_into.clone().unwrap_or_else(|| p.id.clone()));
        match found {
            Some(id) => ids.push(id),
            None => return Err(ApiError::not_found(format!("no person named {name}"))),
        }
    }
    Ok(ids)
}
