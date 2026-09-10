//! Owner's view of who is in the household, and removal (F-158).

use memorylane_core::confirmation::ConfirmedAction;
use memorylane_core::events::EventKind;
use memorylane_core::ids::ContributorId;
use serde_json::json;
use time::Duration;

use crate::app::{App, Caller};
use crate::error::ApiError;
use crate::handlers::confirmations::redeem;
use crate::handlers::media::cascade;
use crate::req::{Req, Resp};
use crate::tokens::Session;

/// How long a removed contributor can still export their originals before they are
/// purged (F-158).
pub const GRACE: Duration = Duration::days(30);

pub async fn list(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    Resp::ok(app.storage.list_contributors(&household).await?)
}

/// Requires a confirmation token minted by the TV (F-410). Marks the contributor removed,
/// takes their media out of every view, and keeps the originals for the grace period so
/// they can export them; `purge_expired` deletes them after.
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
    let now = app.now();
    let grace_until = now + GRACE;
    let mut c = app
        .storage
        .get_contributor(&household, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown contributor"))?;
    c.removed_at.get_or_insert(now);
    c.grace_until = Some(grace_until);
    app.storage.put_contributor(&c).await?;

    let mut detached = Vec::new();
    for mut m in app
        .storage
        .list_media_by_contributor(&household, &id)
        .await?
    {
        if m.is_removed() {
            continue;
        }
        m.remove(now);
        m.restore_until = Some(grace_until);
        app.storage.put_media(&m).await?;
        cascade(app, &household, &m).await?;
        detached.push(m.id);
    }
    if !detached.is_empty() {
        app.emit(
            &household,
            EventKind::MediaRemoved {
                media: detached.clone(),
            },
        )
        .await?;
    }
    Resp::ok(json!({
        "removed": c.id,
        "detached_media": detached.len(),
        "grace_until": grace_until.format(&time::format_description::well_known::Rfc3339).unwrap_or_default(),
        "export": format!("/contributors/{}/export", c.id),
    }))
}

/// Presigned links to a removed contributor's originals while the grace period runs,
/// for the contributor themselves (their old session) or the owner.
pub async fn export(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let caller = app.caller(req)?;
    let id = ContributorId::new(id);
    let allowed = match &caller {
        Caller::Owner { .. } => true,
        Caller::Contributor(Session::Upload { contributor, .. }) => contributor == &id,
        Caller::Contributor(_) => false,
    };
    if !allowed {
        return Err(ApiError::forbidden("not your export"));
    }
    let c = app
        .storage
        .get_contributor(caller.household(), &id)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown contributor"))?;
    if c.grace_until.is_none_or(|g| app.now() >= g) {
        return Err(ApiError::gone("the export window has closed"));
    }
    let mut files = Vec::new();
    for m in app
        .storage
        .list_media_by_contributor(caller.household(), &id)
        .await?
    {
        files.push(json!({
            "media_id": m.id,
            "filename": m.original_filename,
            "url": app.blobs.presign_get(&m.original_key).await?,
        }));
    }
    Resp::ok(json!({
        "grace_until": c.grace_until.and_then(|g| g.format(&time::format_description::well_known::Rfc3339).ok()),
        "files": files,
    }))
}
