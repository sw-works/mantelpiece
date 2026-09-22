//! Join a trip from another Fire TV by the same code (F-162, F-411). The trip stays
//! under its owner; the member household holds a pointer and its own pins and hides.

use mantelpiece_core::events::EventKind;
use mantelpiece_core::ids::TripId;
use mantelpiece_core::membership::Membership;
use mantelpiece_core::trip_code::CodeScope;
use serde::Deserialize;
use serde_json::json;

use crate::app::App;
use crate::error::ApiError;
use crate::req::{Req, Resp};

#[derive(Deserialize)]
struct JoinBody {
    code: String,
}

pub async fn join(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let member = app.owner(req)?;
    let body: JoinBody = req.json()?;
    let code = mantelpiece_core::codes::normalize(&body.code)
        .ok_or_else(|| ApiError::bad_request("invalid code"))?;
    let code = app
        .storage
        .get_trip_code(&code)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown code"))?;
    if code.revoked_at.is_some() {
        return Err(ApiError::gone("this code was replaced"));
    }
    let CodeScope::Trip { trip } = code.scope else {
        return Err(ApiError::bad_request(
            "a household code does not open a trip; ask for the trip's code",
        ));
    };
    let owner = code.household;
    let now = app.now();
    let member_name = app
        .storage
        .get_household(&member)
        .await?
        .map_or_else(|| "Another TV".to_owned(), |h| h.name);
    let membership = Membership::join(
        member.clone(),
        owner.clone(),
        trip.clone(),
        member_name.clone(),
        now,
    )
    .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let mut trip_record = app
        .storage
        .get_trip(&owner, &trip)
        .await?
        .ok_or_else(|| ApiError::not_found("no such trip"))?;
    if app
        .storage
        .get_membership(&member, &owner, &trip)
        .await?
        .is_some_and(|m| m.is_active())
    {
        return Resp::ok(json!({ "joined": false, "trip": trip, "owner": owner, "already": true }));
    }
    app.storage.put_membership(&membership).await?;
    if !trip_record.members.contains(&member) {
        trip_record.members.push(member.clone());
        app.storage.put_trip(&trip_record).await?;
    }
    app.emit(
        &owner,
        EventKind::MembershipJoined {
            trip: trip.clone(),
            member_name,
        },
    )
    .await?;
    Resp::created(json!({ "joined": true, "trip": trip, "owner": owner, "name": trip_record.name }))
}

pub async fn leave(app: &App, req: &Req, trip: &str) -> Result<Resp, ApiError> {
    let member = app.owner(req)?;
    let trip = TripId::new(trip);
    let mut membership = app
        .storage
        .list_memberships(&member)
        .await?
        .into_iter()
        .find(|m| m.trip == trip && m.is_active())
        .ok_or_else(|| ApiError::not_found("not a member of that trip"))?;
    membership.leave(app.now());
    app.storage.put_membership(&membership).await?;
    if let Some(mut t) = app
        .storage
        .get_trip(&membership.owner_household, &trip)
        .await?
    {
        t.members.retain(|h| h != &member);
        app.storage.put_trip(&t).await?;
    }
    Resp::no_content()
}
