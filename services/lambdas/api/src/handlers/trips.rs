//! Read trips. Owners see all; contributors see the trip their code names (F-159).

use memorylane_core::ids::TripId;

use crate::app::{App, Caller};
use crate::error::ApiError;
use crate::req::{Req, Resp};
use crate::tokens::Session;

pub async fn list(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let caller = app.caller(req)?;
    let trips = app.storage.list_trips(caller.household()).await?;
    let visible: Vec<_> = match &caller {
        Caller::Owner { .. } => trips,
        Caller::Contributor(Session::Upload { trip: Some(t), .. }) => {
            trips.into_iter().filter(|x| &x.id == t).collect()
        }
        Caller::Contributor(_) => vec![],
    };
    Resp::ok(visible)
}

pub async fn get(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let caller = app.caller(req)?;
    let id = TripId::new(id);
    if let Caller::Contributor(Session::Upload { trip, .. }) = &caller {
        if trip.as_ref() != Some(&id) {
            return Err(ApiError::forbidden("not your trip"));
        }
    }
    let trip = app
        .storage
        .get_trip(caller.household(), &id)
        .await?
        .ok_or_else(|| ApiError::not_found("no such trip"))?;
    Resp::ok(trip)
}
