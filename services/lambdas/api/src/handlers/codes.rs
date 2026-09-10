//! Trip and household codes (F-151 to F-153) and joining by code (F-152).

use memorylane_core::contributor::{Contributor, ContributorKind, Role};
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
    /// Live counts for the Invite screen (F-151, F-162).
    pub contributors: u32,
    pub member_households: u32,
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

async fn counts(app: &App, c: &TripCode) -> Result<(u32, u32), ApiError> {
    let contributors = app.storage.list_contributors(&c.household).await?;
    let active = |id: &ContributorId| contributors.iter().any(|c| &c.id == id && c.is_active());
    match &c.scope {
        CodeScope::Trip { trip } => {
            let on_trip = app
                .storage
                .get_trip(&c.household, trip)
                .await?
                .map_or(0, |t| t.contributors.iter().filter(|id| active(id)).count());
            let members = app
                .storage
                .list_trip_members(&c.household, trip)
                .await?
                .iter()
                .filter(|m| m.is_active())
                .count();
            Ok((count(on_trip), count(members)))
        }
        CodeScope::Household => {
            let all = contributors.iter().filter(|c| c.is_active()).count();
            let mut members = 0;
            for t in app.storage.list_trips(&c.household).await? {
                members += app
                    .storage
                    .list_trip_members(&c.household, &t.id)
                    .await?
                    .iter()
                    .filter(|m| m.is_active())
                    .count();
            }
            Ok((count(all), count(members)))
        }
    }
}

pub async fn view(app: &App, c: &TripCode) -> Result<CodeView, ApiError> {
    let (scope, trip) = match &c.scope {
        CodeScope::Household => ("household", None),
        CodeScope::Trip { trip } => ("trip", Some(trip.clone())),
    };
    let (contributors, member_households) = counts(app, c).await?;
    Ok(CodeView {
        code: c.code.clone(),
        scope: scope.into(),
        trip,
        link: format!("{}/{}", app.config.upload_base, c.link_query()),
        accepts_uploads: c.accepts_uploads(app.now()),
        expires_at: c.expires_at.and_then(|e| {
            e.format(&time::format_description::well_known::Rfc3339)
                .ok()
        }),
        contributors,
        member_households,
    })
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
    Resp::created(view(app, &code).await?)
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
    let mut views = Vec::with_capacity(codes.len());
    for c in &codes {
        views.push(view(app, c).await?);
    }
    Resp::ok(views)
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
    Resp::created(view(app, &fresh).await?)
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
    /// Browser install id; a guest session is then accepted only with the matching
    /// `x-device-id` header (F-153).
    device_id: Option<String>,
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
                grace_until: None,
                role: Role::Contributor,
            };
            app.storage.put_contributor(&c).await?;
            c
        }
    };

    let trip = match &code.scope {
        CodeScope::Household => None,
        CodeScope::Trip { trip } => Some(trip.clone()),
    };
    // A guest token never outlives its code (F-153); Amazon accounts can always re-join.
    let device = match contributor.kind {
        ContributorKind::Guest => body.device_id.filter(|d| !d.is_empty()),
        _ => None,
    };
    let exp = match (&contributor.kind, code.expires_at) {
        (ContributorKind::Guest, Some(e)) => e.min(now + UPLOAD_TTL),
        _ => now + UPLOAD_TTL,
    };
    let claims = Claims {
        session: Session::Upload {
            household: household.clone(),
            contributor: contributor.id.clone(),
            trip: trip.clone(),
            code: code.code.clone(),
            device,
        },
        exp,
    };
    Resp::ok(json!({
        "token": app.signer.sign(&claims),
        "household": household,
        "contributor": contributor.id,
        "display_name": contributor.display_name,
        "trip": trip,
    }))
}
