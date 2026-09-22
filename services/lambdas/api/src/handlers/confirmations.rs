//! Confirmation tokens minted by the TV's Review Changes modal (F-410, F-504).

use mantelpiece_core::confirmation::{ConfirmationToken, ConfirmedAction};
use mantelpiece_core::contributor::Contributor;
use mantelpiece_core::ids::HouseholdId;
use serde_json::json;

use crate::app::{App, Caller};
use crate::error::ApiError;
use crate::req::{Req, Resp};
use crate::tokens::Session;

/// The owner mints any token; a co-editor mints tokens for the actions the role allows
/// (F-164), recorded as the token's subject so the Cedar rule can check it.
pub async fn mint(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let action: ConfirmedAction = req.json()?;
    // Optional proposal id: the token then closes that proposal and must match its action.
    let proposal_id = serde_json::from_slice::<serde_json::Value>(&req.body)
        .ok()
        .and_then(|v| v["proposal"].as_str().map(str::to_owned));
    let (household, subject) = match app.caller(req)? {
        Caller::Owner { household, subject } => (household, subject),
        Caller::Contributor(Session::Upload {
            household,
            contributor,
            ..
        }) => {
            let c = app
                .storage
                .get_contributor(&household, &contributor)
                .await?
                .filter(Contributor::is_active)
                .ok_or_else(|| ApiError::forbidden("unknown contributor"))?;
            if !c.can_edit() || action.owner_only() {
                return Err(ApiError::forbidden("owner only"));
            }
            (household, contributor.to_string())
        }
        Caller::Contributor(_) => return Err(ApiError::forbidden("owner only")),
    };
    let token = ConfirmationToken::mint_by(household.clone(), action, subject, app.now());
    if let Some(pid) = &proposal_id {
        let mut p = app
            .storage
            .get_proposal(
                &household,
                &mantelpiece_core::ids::ProposalId::new(pid.as_str()),
            )
            .await?
            .ok_or_else(|| ApiError::not_found("no such proposal"))?;
        if p.action != token.action {
            return Err(ApiError::bad_request(
                "confirmation does not match the proposal",
            ));
        }
        p.confirm(&token.token, app.now())
            .map_err(|e| ApiError::conflict(e.to_string()))?;
        app.storage.put_proposal(&p).await?;
    }
    app.storage.put_confirmation(&token).await?;
    Resp::created(json!({
        "token": token.token,
        "tool": token.action.tool_name(),
        "expires_at": token.expires_at.unix_timestamp(),
        "proposal": proposal_id,
    }))
}

/// Redeem the `X-Confirmation` value carried in the body as `confirmation`, or in the
/// query string, for exactly `action`.
pub async fn redeem(
    app: &App,
    req: &Req,
    household: &HouseholdId,
    action: &ConfirmedAction,
) -> Result<String, ApiError> {
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
    crate::handlers::proposals::mark_applied(app, household, &token).await?;
    Ok(token)
}

/// The subject a redeemed token was confirmed by, for the RemoveMedia role check.
pub async fn subject_of(app: &App, req: &Req) -> Result<Option<String>, ApiError> {
    let from_body = serde_json::from_slice::<serde_json::Value>(&req.body)
        .ok()
        .and_then(|v| v["confirmation"].as_str().map(str::to_owned));
    let Some(token) = from_body.or_else(|| req.query.get("confirmation").cloned()) else {
        return Ok(None);
    };
    Ok(app
        .storage
        .get_confirmation(&token)
        .await?
        .and_then(|t| t.subject))
}
