//! TV long-poll (F-407): `GET /events?since=<seq>` holds up to `poll_hold`.

use crate::app::App;
use crate::error::ApiError;
use crate::req::{Req, Resp};

const LIMIT: usize = 100;

pub async fn list(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let since: u64 = req
        .query
        .get("since")
        .map_or(Ok(0), |s| s.parse())
        .map_err(|_| ApiError::bad_request("since must be an integer"))?;
    let deadline = app.now() + app.config.poll_hold;
    loop {
        let events = app.storage.events_since(&household, since, LIMIT).await?;
        if !events.is_empty() || app.now() >= deadline {
            let next = events.last().map_or(since, |e| e.seq);
            return Resp::ok(serde_json::json!({ "events": events, "next": next }));
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}
