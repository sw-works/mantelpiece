//! Audit log (F-605): written centrally from `App::handle`, read by the owner.

use mantelpiece_core::audit::{action_for, AuditEntry};

use crate::app::{App, Caller};
use crate::error::ApiError;
use crate::req::{Req, Resp};
use crate::tokens::Session;

fn actor(c: &Caller) -> String {
    match c {
        Caller::Owner { subject, .. } => subject.clone(),
        Caller::Contributor(Session::Upload { contributor, .. }) => contributor.to_string(),
        Caller::Contributor(_) => "session".into(),
    }
}

/// One entry when the route is audited and the caller's household is known. Anonymous
/// requests (a guest joining) have no partition to land in and are skipped.
pub async fn record(app: &App, req: &Req, resp: &Resp) -> Result<(), ApiError> {
    let Some(method) = req.method else {
        return Ok(());
    };
    let segs = req.segments();
    let Some(action) = action_for(method.as_str(), &segs) else {
        return Ok(());
    };
    let Ok(caller) = app.caller(req) else {
        return Ok(());
    };
    let confirmation = serde_json::from_slice::<serde_json::Value>(&req.body)
        .ok()
        .and_then(|v| v["confirmation"].as_str().map(str::to_owned))
        .or_else(|| req.query.get("confirmation").cloned());
    let mut entry = AuditEntry::new(
        caller.household().clone(),
        actor(&caller),
        action,
        req.path.clone(),
        app.now(),
    );
    entry.confirmation = confirmation;
    entry.outcome = resp.status;
    app.storage.put_audit(&entry).await?;
    Ok(())
}

/// `GET /audit`: the owner's log, newest first.
pub async fn list(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    Resp::ok(app.storage.list_audit(&household).await?)
}
