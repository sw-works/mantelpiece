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
