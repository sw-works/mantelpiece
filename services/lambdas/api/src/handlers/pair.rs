//! Pairing, the way Bluetooth devices pair. The TV names its household on first launch and
//! shows a code; a phone that scans it gives a name and waits; the TV approves or turns it
//! away. No account anywhere: the code proves the phone saw the screen, the approval
//! proves someone at the TV wants it in.

use memorylane_core::contributor::{Contributor, ContributorKind, PairStatus};
use memorylane_core::device_code::DeviceCode;
use memorylane_core::events::EventKind;
use memorylane_core::household::Household;
use memorylane_core::ids::{ContributorId, DeviceId, HouseholdId};
use serde::Deserialize;
use serde_json::json;
use time::format_description::well_known::Rfc3339;

use crate::app::App;
use crate::error::ApiError;
use crate::req::{Req, Resp};
use crate::tokens::{Claims, Session, PAIRING_TTL, PHONE_TTL, TV_TTL};

/// Phones allowed to wait at once; a code photographed and passed around stops here.
const MAX_PENDING: usize = 12;
const MAX_NAME: usize = 40;

fn clean_name(raw: &str, what: &str) -> Result<String, ApiError> {
    let name: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err(ApiError::bad_request(format!("{what} required")));
    }
    Ok(name.chars().take(MAX_NAME).collect())
}

/// The TV session behind a request; phones and guests are turned away.
fn tv_household(app: &App, req: &Req) -> Result<HouseholdId, ApiError> {
    match app.session(req)? {
        Some(Session::Tv { household, .. }) => Ok(household),
        Some(_) => Err(ApiError::forbidden("only the TV can do this")),
        None => Err(ApiError::unauthorized("TV session required")),
    }
}

#[derive(Deserialize)]
struct SetupBody {
    device_id: String,
    name: String,
}

/// `POST /tv/setup`: first launch. Creates the household under the name chosen on the TV
/// and hands the TV its session.
pub async fn setup(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let body: SetupBody = req.json()?;
    if body.device_id.is_empty() {
        return Err(ApiError::bad_request("device_id required"));
    }
    let name = clean_name(&body.name, "name")?;
    let now = app.now();
    let household = HouseholdId::new(format!("tv-{}", ContributorId::random()));
    app.storage
        .put_household(&Household::new(household.clone(), name.clone(), now))
        .await?;
    app.storage
        .put_contributor(&Contributor::owner(household.clone(), name.clone(), now))
        .await?;
    let token = app.signer.sign(&Claims {
        session: Session::Tv {
            household: household.clone(),
            subject: household.to_string(),
        },
        exp: now + TV_TTL,
    });
    Resp::created(json!({ "household": household, "household_name": name, "token": token }))
}

#[derive(Deserialize)]
struct RenameBody {
    name: String,
}

/// `PATCH /household`: the household name is set on the TV and nowhere else.
pub async fn rename(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = tv_household(app, req)?;
    let body: RenameBody = req.json()?;
    let name = clean_name(&body.name, "name")?;
    let mut record = app
        .storage
        .get_household(&household)
        .await?
        .unwrap_or_else(|| Household::new(household.clone(), name.clone(), app.now()));
    record.name.clone_from(&name);
    app.storage.put_household(&record).await?;
    Resp::ok(json!({ "household_name": name }))
}

#[derive(Deserialize)]
struct CodeBody {
    device_id: String,
}

/// `POST /pair/code`: the code and link the TV shows. Many phones may use one code while
/// it lasts; each still needs the TV's approval.
pub async fn code(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = tv_household(app, req)?;
    let body: CodeBody = req.json()?;
    let now = app.now();
    let dc = DeviceCode::issue(&mut rand::rng(), DeviceId::new(body.device_id), now)
        .for_household(household);
    app.storage.put_device_code(&dc).await?;
    Resp::created(json!({
        "link": format!("{}/?p={}", app.config.link_base, dc.code),
        "expires_at": dc.expires_at.format(&Rfc3339).unwrap_or_default(),
        "code": dc.code,
    }))
}

/// The household a live pairing code belongs to.
async fn household_for(app: &App, raw: &str) -> Result<Household, ApiError> {
    let code = memorylane_core::codes::normalize(raw)
        .ok_or_else(|| ApiError::bad_request("invalid code"))?;
    let dc = app
        .storage
        .get_device_code(&code)
        .await?
        .filter(|dc| !dc.is_expired(app.now()))
        .ok_or_else(|| ApiError::not_found("that code is no longer on the TV"))?;
    let household = dc
        .household
        .ok_or_else(|| ApiError::not_found("that code is no longer on the TV"))?;
    app.storage
        .get_household(&household)
        .await?
        .filter(|h| !h.is_deleting())
        .ok_or_else(|| ApiError::not_found("that code is no longer on the TV"))
}

/// `GET /pair/code/{code}`: what the phone shows before asking, "Connect to Living room".
pub async fn describe(app: &App, code: &str) -> Result<Resp, ApiError> {
    let household = household_for(app, code).await?;
    Resp::ok(json!({ "tv_name": household.name }))
}

#[derive(Deserialize)]
struct RequestBody {
    code: String,
    name: String,
}

/// `POST /pair/requests`: a phone asks to be let in. It gets a token good only for asking
/// how the request stands.
pub async fn request(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let body: RequestBody = req.json()?;
    let household = household_for(app, &body.code).await?;
    let name = clean_name(&body.name, "your name")?;
    let waiting = app
        .storage
        .list_contributors(&household.id)
        .await?
        .iter()
        .filter(|c| c.pair_status() == Some(PairStatus::Pending))
        .count();
    if waiting >= MAX_PENDING {
        return Err(ApiError::new(
            429,
            "too many phones are waiting; approve some on the TV",
        ));
    }
    let now = app.now();
    let phone = Contributor::phone(household.id.clone(), name.clone(), now);
    app.storage.put_contributor(&phone).await?;
    app.emit(
        &household.id,
        EventKind::PhoneRequested {
            contributor: phone.id.clone(),
            display_name: name.clone(),
        },
    )
    .await?;
    let token = app.signer.sign(&Claims {
        session: Session::Pairing {
            household: household.id.clone(),
            contributor: phone.id.clone(),
        },
        exp: now + PAIRING_TTL,
    });
    Resp::created(json!({
        "request": phone.id,
        "name": name,
        "tv_name": household.name,
        "token": token,
    }))
}

/// `GET /pair/status`: the waiting phone. Holds like the TV's poll, then answers pending;
/// an approval carries the phone's session.
pub async fn status(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let Some(Session::Pairing {
        household,
        contributor,
    }) = app.session(req)?
    else {
        return Err(ApiError::unauthorized("pairing token required"));
    };
    let deadline = app.now() + app.config.poll_hold;
    loop {
        let phone = app
            .storage
            .get_contributor(&household, &contributor)
            .await?;
        let state = phone
            .as_ref()
            .filter(|c| c.removed_at.is_none())
            .and_then(Contributor::pair_status)
            .unwrap_or(PairStatus::Denied);
        match state {
            PairStatus::Approved => {
                let tv_name = app.storage.get_household(&household).await?.map(|h| h.name);
                let token = app.signer.sign(&Claims {
                    session: Session::Phone {
                        household: household.clone(),
                        subject: contributor.to_string(),
                    },
                    exp: app.now() + PHONE_TTL,
                });
                return Resp::ok(json!({
                    "state": "approved",
                    "token": token,
                    "household": household,
                    "tv_name": tv_name,
                    "name": phone.map(|c| c.display_name),
                }));
            }
            PairStatus::Denied => return Resp::ok(json!({ "state": "denied" })),
            PairStatus::Pending => {
                if app.now() < deadline {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    continue;
                }
                return Resp::ok(json!({ "state": "pending" }));
            }
        }
    }
}

fn row(c: &Contributor) -> serde_json::Value {
    json!({
        "id": c.id,
        "name": c.display_name,
        "at": c.joined_at.format(&Rfc3339).unwrap_or_default(),
    })
}

/// `GET /pair/requests`: the TV's approval queue, oldest first, and the phones let in.
pub async fn list(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = tv_household(app, req)?;
    let mut phones: Vec<Contributor> = app
        .storage
        .list_contributors(&household)
        .await?
        .into_iter()
        .filter(|c| c.removed_at.is_none())
        .collect();
    phones.sort_by_key(|c| c.joined_at);
    let with = |status| -> Vec<serde_json::Value> {
        phones
            .iter()
            .filter(|c| c.pair_status() == Some(status))
            .map(row)
            .collect()
    };
    Resp::ok(json!({ "pending": with(PairStatus::Pending), "phones": with(PairStatus::Approved) }))
}

/// `POST /pair/requests/{id}/approve|deny`: the TV lets a phone in, turns it away, or
/// forgets one it let in before. The phone's photos stay either way.
pub async fn decide(app: &App, req: &Req, id: &str, approve: bool) -> Result<Resp, ApiError> {
    let household = tv_household(app, req)?;
    let mut phone = app
        .storage
        .get_contributor(&household, &ContributorId::new(id))
        .await?
        .filter(|c| c.pair_status().is_some())
        .ok_or_else(|| ApiError::not_found("no such phone"))?;
    let status = if approve {
        PairStatus::Approved
    } else {
        PairStatus::Denied
    };
    phone.kind = ContributorKind::Phone { status };
    app.storage.put_contributor(&phone).await?;
    Resp::ok(json!({ "id": phone.id, "name": phone.display_name, "approved": approve }))
}
