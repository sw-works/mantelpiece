//! Trip and household codes (F-151 to F-153) and joining by code (F-152).

use memorylane_core::contributor::{Contributor, ContributorKind};
use memorylane_core::ids::{ContributorId, HouseholdId, TripId};
use memorylane_core::trip_code::{CodeScope, TripCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::Duration;

use crate::app::App;
use crate::error::ApiError;
use crate::req::{Req, Resp};
use crate::tokens::{Claims, Session, UPLOAD_TTL};

#[derive(Deserialize, Default)]
struct IssueBody {
    /// Days until the code stops accepting uploads. Omit for no expiry.
    expires_in_days: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct CodeView {
    pub code: String,
    pub scope: String,
    pub trip: Option<TripId>,
    pub link: String,
    pub accepts_uploads: bool,
    pub expires_at: Option<String>,
}

pub fn view(app: &App, c: &TripCode) -> CodeView {
    let (scope, trip) = match &c.scope {
        CodeScope::Household => ("household", None),
        CodeScope::Trip { trip } => ("trip", Some(trip.clone())),
    };
    CodeView {
        code: c.code.clone(),
        scope: scope.into(),
        trip,
        link: format!("{}/{}", app.config.upload_base, c.link_query()),
        accepts_uploads: c.accepts_uploads(app.now()),
        expires_at: c.expires_at.and_then(|e| {
            e.format(&time::format_description::well_known::Rfc3339)
                .ok()
        }),
    }
}

async fn issue(
    app: &App,
    req: &Req,
    household: HouseholdId,
    scope: CodeScope,
) -> Result<Resp, ApiError> {
    let body: IssueBody = if req.body.is_empty() {
        IssueBody::default()
    } else {
        req.json()?
    };
    let now = app.now();
    let expires_at = body
        .expires_in_days
        .map(|d| now + Duration::days(d.clamp(1, 365)));
    let code = TripCode::issue(&mut rand::rng(), household, scope, now, expires_at);
    app.storage.put_trip_code(&code).await?;
    Resp::created(view(app, &code))
}

pub async fn issue_household(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    issue(app, req, household, CodeScope::Household).await
}

pub async fn issue_trip(app: &App, req: &Req, trip: &str) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let trip = TripId::new(trip);
    if app.storage.get_trip(&household, &trip).await?.is_none() {
        return Err(ApiError::not_found("no such trip"));
    }
    issue(app, req, household, CodeScope::Trip { trip }).await
}

pub async fn list(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let codes = app.storage.list_trip_codes(&household).await?;
    Resp::ok(codes.iter().map(|c| view(app, c)).collect::<Vec<_>>())
}

async fn owned_code(app: &App, req: &Req, code: &str) -> Result<TripCode, ApiError> {
    let household = app.owner(req)?;
    let code = app
        .storage
        .get_trip_code(code)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown code"))?;
    if code.household != household {
        return Err(ApiError::forbidden("not your code"));
    }
    Ok(code)
}

pub async fn rotate(app: &App, req: &Req, code: &str) -> Result<Resp, ApiError> {
    let mut old = owned_code(app, req, code).await?;
    let fresh = old.rotate(&mut rand::rng(), app.now());
    app.storage.put_trip_code(&old).await?;
    app.storage.put_trip_code(&fresh).await?;
    Resp::created(view(app, &fresh))
}

pub async fn revoke(app: &App, req: &Req, code: &str) -> Result<Resp, ApiError> {
    let mut old = owned_code(app, req, code).await?;
    old.revoke(app.now());
    app.storage.put_trip_code(&old).await?;
    Resp::no_content()
}

#[derive(Deserialize)]
struct JoinBody {
    code: String,
    /// Required for guests; ignored when a JWT is present and carries a name.
    display_name: Option<String>,
}

/// Join a trip by code. With a JWT the contributor is the Amazon account; without one
/// they are a named guest. Either way the answer is an upload session token.
pub async fn join(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let body: JoinBody = req.json()?;
    let code_str = memorylane_core::codes::normalize(&body.code)
        .ok_or_else(|| ApiError::bad_request("invalid code"))?;
    let code = app
        .storage
        .get_trip_code(&code_str)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown code"))?;
    let now = app.now();
    if !code.accepts_uploads(now) {
        return Err(ApiError::gone("this code no longer accepts uploads"));
    }
    let household = code.household.clone();

    let (id, display_name, kind) = match &req.jwt {
        Some(jwt) if jwt.sub == household.as_str() => (
            ContributorId::new(household.as_str()),
            jwt.name.clone().unwrap_or_else(|| "Owner".into()),
            ContributorKind::Owner,
        ),
        Some(jwt) => (
            ContributorId::new(format!("lwa:{}", jwt.sub)),
            jwt.name
                .clone()
                .or(body.display_name.clone())
                .unwrap_or_else(|| "Friend".into()),
            ContributorKind::Amazon {
                sub: jwt.sub.clone(),
            },
        ),
        None => {
            let name = body
                .display_name
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty());
            let name =
                name.ok_or_else(|| ApiError::bad_request("display_name required for guests"))?;
            (
                ContributorId::random(),
                name.to_owned(),
                ContributorKind::Guest,
            )
        }
    };

    let contributor = match app.storage.get_contributor(&household, &id).await? {
        Some(existing) if existing.removed_at.is_some() => {
            return Err(ApiError::forbidden("removed from this household"))
        }
        Some(existing) => existing,
        None => {
            let c = Contributor {
                id,
                household: household.clone(),
                display_name,
                kind,
                joined_at: now,
                removed_at: None,
            };
            app.storage.put_contributor(&c).await?;
            c
        }
    };

    let trip = match &code.scope {
        CodeScope::Household => None,
        CodeScope::Trip { trip } => Some(trip.clone()),
    };
    let claims = Claims {
        session: Session::Upload {
            household: household.clone(),
            contributor: contributor.id.clone(),
            trip: trip.clone(),
            code: code.code.clone(),
        },
        exp: now + UPLOAD_TTL,
    };
    Resp::ok(json!({
        "token": app.signer.sign(&claims),
        "household": household,
        "contributor": contributor.id,
        "display_name": contributor.display_name,
        "trip": trip,
    }))
}
