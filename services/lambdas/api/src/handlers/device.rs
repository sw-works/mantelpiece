//! Device-code sign-in (F-401).

use mantelpiece_core::contributor::Contributor;
use mantelpiece_core::device_code::{DeviceCode, DeviceCodeError, DeviceCodeState, Poll};
use mantelpiece_core::household::Household;
use mantelpiece_core::ids::{ContributorId, DeviceId, HouseholdId};
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;

use crate::app::App;
use crate::error::ApiError;
use crate::req::{Req, Resp};
use crate::tokens::{Claims, Session, PHONE_TTL, TV_TTL};

#[derive(Deserialize)]
struct IssueBody {
    device_id: String,
    #[serde(default)]
    tv_name: Option<String>,
}

#[derive(Serialize)]
struct Issued {
    code: String,
    link: String,
    expires_at: String,
    poll_seconds: i64,
}

pub async fn issue(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let body: IssueBody = req.json()?;
    if body.device_id.is_empty() {
        return Err(ApiError::bad_request("device_id required"));
    }
    let now = app.now();
    let dc = DeviceCode::issue(&mut rand::rng(), DeviceId::new(body.device_id), now)
        .with_tv_name(body.tv_name);
    app.storage.put_device_code(&dc).await?;
    Resp::created(Issued {
        link: format!("{}/?c={}", app.config.link_base, dc.code),
        expires_at: dc
            .expires_at
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
        poll_seconds: mantelpiece_core::device_code::POLL_HOLD.whole_seconds(),
        code: dc.code,
    })
}

#[derive(Deserialize)]
struct ApproveBody {
    code: String,
}

/// Phone, signed in with Cognito. Approves the code and makes the JWT subject the owner
/// of its household (creating the owner contributor on first sign-in).
pub async fn approve(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let jwt = req
        .jwt
        .as_ref()
        .ok_or_else(|| ApiError::unauthorized("sign in with Amazon first"))?;
    let body: ApproveBody = req.json()?;
    let code = mantelpiece_core::codes::normalize(&body.code)
        .ok_or_else(|| ApiError::bad_request("invalid code"))?;
    let mut dc = app
        .storage
        .get_device_code(&code)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown code"))?;
    let now = app.now();
    let household = HouseholdId::new(jwt.sub.clone());
    dc.approve(household.clone(), jwt.sub.clone(), now)
        .map_err(|e| match e {
            DeviceCodeError::AlreadyApproved | DeviceCodeError::Consumed => {
                ApiError::conflict("already signed in")
            }
            other => ApiError::conflict(other.to_string()),
        })?;
    app.storage.put_device_code(&dc).await?;

    let owner_id = ContributorId::new(household.as_str());
    if app
        .storage
        .get_contributor(&household, &owner_id)
        .await?
        .is_none()
    {
        let name = jwt.name.clone().unwrap_or_else(|| "Owner".into());
        app.storage
            .put_contributor(&Contributor::owner(household.clone(), name, now))
            .await?;
    }
    // The phone continues straight to the uploader with its own owner session.
    let token = phone_session(app, &household, &jwt.sub, now);
    Resp::ok(json!({ "approved": true, "household": household, "token": token }))
}

/// An owner session for a signed-in Amazon account: the phone after approving a code, or
/// an Alexa skill after account linking. Same session shape as the phone's.
fn phone_session(app: &App, household: &HouseholdId, subject: &str, now: OffsetDateTime) -> String {
    let claims = Claims {
        session: Session::Phone {
            household: household.clone(),
            subject: subject.to_owned(),
        },
        exp: now + PHONE_TTL,
    };
    app.signer.sign(&claims)
}

/// Alexa skill (Echo Show slideshow). Account linking hands the skill a Cognito token;
/// this trades it for the same owner session the phone gets, so every later call carries
/// no Cognito JWT. Creates the owner contributor on a first sign-in, like `approve`.
pub async fn session_from_amazon(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let jwt = req
        .jwt
        .as_ref()
        .ok_or_else(|| ApiError::unauthorized("sign in with Amazon first"))?;
    let now = app.now();
    let household = HouseholdId::new(jwt.sub.clone());
    let owner_id = ContributorId::new(household.as_str());
    if app
        .storage
        .get_contributor(&household, &owner_id)
        .await?
        .is_none()
    {
        let name = jwt.name.clone().unwrap_or_else(|| "Owner".into());
        app.storage
            .put_contributor(&Contributor::owner(household.clone(), name, now))
            .await?;
    }
    let token = phone_session(app, &household, &jwt.sub, now);
    let name = app.storage.get_household(&household).await?.map(|h| h.name);
    Resp::ok(json!({ "household": household, "household_name": name, "token": token }))
}

/// TV. Holds up to `poll_hold`, then answers pending. Approval is handed over once as a
/// TV session token.
pub async fn poll(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let code = req.query_param("code")?.to_owned();
    let device = DeviceId::new(req.query_param("device_id")?);
    let deadline = app.now() + app.config.poll_hold;
    loop {
        let mut dc = app
            .storage
            .get_device_code(&code)
            .await?
            .ok_or_else(|| ApiError::not_found("unknown code"))?;
        let now = app.now();
        match dc
            .poll(&device, now)
            .map_err(|e| ApiError::forbidden(e.to_string()))?
        {
            Poll::Approved { household, subject } => {
                app.storage.put_device_code(&dc).await?;
                // First TV to sign in names the household; later TVs keep the name.
                if app.storage.get_household(&household).await?.is_none() {
                    let owner = app
                        .storage
                        .get_contributor(&household, &ContributorId::new(household.as_str()))
                        .await?;
                    let name = dc.tv_name.clone().unwrap_or_else(|| {
                        format!(
                            "{}'s TV",
                            owner.map_or_else(|| "Owner".to_owned(), |c| c.display_name)
                        )
                    });
                    app.storage
                        .put_household(&Household::new(household.clone(), name, now))
                        .await?;
                }
                let claims = Claims {
                    session: Session::Tv {
                        household: household.clone(),
                        subject,
                    },
                    exp: now + TV_TTL,
                };
                let token = app.signer.sign(&claims);
                return Resp::ok(
                    json!({ "state": "approved", "household": household, "token": token }),
                );
            }
            Poll::Expired => return Resp::ok(json!({ "state": "expired" })),
            Poll::Pending => {
                if matches!(dc.state, DeviceCodeState::Pending) && app.now() < deadline {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    continue;
                }
                return Resp::ok(json!({ "state": "pending" }));
            }
        }
    }
}
