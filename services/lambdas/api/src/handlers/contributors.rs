//! Owner's view of who is in the household, and removal (F-158).

use memorylane_core::confirmation::ConfirmedAction;
use memorylane_core::ids::ContributorId;
use serde_json::json;

use crate::app::App;
use crate::error::ApiError;
use crate::handlers::confirmations::redeem;
use crate::req::{Req, Resp};

pub async fn list(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    Resp::ok(app.storage.list_contributors(&household).await?)
}

/// Requires a confirmation token minted by the TV (F-410). Marks the contributor
/// removed; their media is detached and deleted by the grace-period job.
pub async fn remove(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let id = ContributorId::new(id);
    if id.as_str() == household.as_str() {
        return Err(ApiError::bad_request("the owner cannot be removed"));
    }
    redeem(
        app,
        req,
        &household,
        &ConfirmedAction::RemoveContributor {
            contributor_id: id.to_string(),
        },
    )
    .await?;
    let mut c = app
        .storage
        .get_contributor(&household, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown contributor"))?;
    c.removed_at.get_or_insert(app.now());
    app.storage.put_contributor(&c).await?;
    let media = app
        .storage
        .list_media_by_contributor(&household, &id)
        .await?;
    Resp::ok(json!({ "removed": c.id, "detached_media": media.len() }))
}
