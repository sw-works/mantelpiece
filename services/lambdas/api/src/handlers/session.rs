use serde_json::json;

use crate::app::{App, Caller};
use crate::error::ApiError;
use crate::req::{Req, Resp};
use crate::tokens::Session;

pub async fn me(app: &App, req: &Req) -> Result<Resp, ApiError> {
    match app.caller(req)? {
        Caller::Owner { household, subject } => {
            let record = app.storage.get_household(&household).await?;
            Resp::ok(json!({
                "role": "owner",
                "household": household,
                "subject": subject,
                "household_name": record.as_ref().map(|h| h.name.clone()),
                "people_enabled": record.as_ref().is_some_and(|h| h.people_enabled),
                "narration_enabled": record.as_ref().is_some_and(|h| h.narration_enabled),
            }))
        }
        Caller::Contributor(Session::Upload {
            household,
            contributor,
            trip,
            code,
            ..
        }) => {
            let c = app
                .storage
                .get_contributor(&household, &contributor)
                .await?;
            Resp::ok(json!({
                "role": "contributor",
                "household": household,
                "contributor": contributor,
                "display_name": c.map(|c| c.display_name),
                "trip": trip,
                "code": code,
            }))
        }
        Caller::Contributor(Session::Tv { .. } | Session::Phone { .. }) => {
            Err(ApiError::internal("unreachable session shape"))
        }
    }
}

/// `DELETE /me` (F-604): the household is marked deleting first so uploads answer 410,
/// then originals, renditions, embeddings, faces, people and every record go. The
/// household row stays with `deleting_since` as the tombstone.
pub async fn delete_account(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let now = app.now();
    let mut record = app
        .storage
        .get_household(&household)
        .await?
        .unwrap_or_else(|| {
            memorylane_core::household::Household::new(household.clone(), "My TV", now)
        });
    record.deleting_since = Some(now);
    record.people_enabled = false;
    app.storage.put_household(&record).await?;

    crate::handlers::people::forget_people(app, &household).await?;
    let media = app.storage.list_media(&household).await?;
    let mut keys = Vec::new();
    for m in &media {
        keys.push(m.original_key.clone());
        for r in ["thumb", "tv", "poster"] {
            keys.push(format!("renditions/{}/{}/{r}.jpg", m.household, m.id));
        }
        app.vectors.delete(&household, &m.id).await?;
    }
    if !keys.is_empty() {
        app.blobs.delete(&keys).await?;
    }
    app.storage.purge_household(&household).await?;
    // The tombstone survives the purge.
    app.storage.put_household(&record).await?;
    Resp::ok(json!({ "deleted": true, "media": media.len(), "household": household }))
}

#[derive(serde::Deserialize)]
struct SettingsBody {
    narration_enabled: Option<bool>,
}

/// `PATCH /me`: household flags the TV or phone can flip. People has its own routes
/// because turning it off deletes data.
pub async fn settings(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let body: SettingsBody = req.json()?;
    let mut record = app
        .storage
        .get_household(&household)
        .await?
        .unwrap_or_else(|| {
            memorylane_core::household::Household::new(household.clone(), "My TV", app.now())
        });
    if let Some(v) = body.narration_enabled {
        record.narration_enabled = v;
    }
    app.storage.put_household(&record).await?;
    Resp::ok(
        json!({ "narration_enabled": record.narration_enabled, "people_enabled": record.people_enabled }),
    )
}
