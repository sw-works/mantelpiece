//! Confirmation tokens minted by the TV's Review Changes modal (F-410, F-504).

use memorylane_core::confirmation::{ConfirmationToken, ConfirmedAction};
use memorylane_core::ids::HouseholdId;
use serde_json::json;

use crate::app::App;
use crate::error::ApiError;
use crate::req::{Req, Resp};

pub async fn mint(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let action: ConfirmedAction = req.json()?;
    let token = ConfirmationToken::mint(household, action, app.now());
    app.storage.put_confirmation(&token).await?;
    Resp::created(
        json!({ "token": token.token, "tool": token.action.tool_name(), "expires_at": token.expires_at.unix_timestamp() }),
    )
}

/// Redeem the `X-Confirmation` value carried in the body as `confirmation`, or in the
/// query string, for exactly `action`.
pub async fn redeem(
    app: &App,
    req: &Req,
    household: &HouseholdId,
    action: &ConfirmedAction,
) -> Result<(), ApiError> {
    let from_body = serde_json::from_slice::<serde_json::Value>(&req.body)
        .ok()
        .and_then(|v| v["confirmation"].as_str().map(str::to_owned));
    let token = from_body
        .or_else(|| req.query.get("confirmation").cloned())
        .ok_or_else(|| ApiError::forbidden("confirmation required"))?;
    let mut t = app
        .storage
        .get_confirmation(&token)
        .await?
        .ok_or_else(|| ApiError::forbidden("unknown confirmation"))?;
    t.redeem(household, action, app.now())
        .map_err(|e| ApiError::forbidden(e.to_string()))?;
    app.storage.put_confirmation(&t).await?;
    Ok(())
}
