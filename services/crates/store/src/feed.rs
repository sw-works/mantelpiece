//! Household feed writes with fan-out to member households (F-162, contracts/events.md).
//!
//! A member household's TV long-polls its own feed, so events about a joined trip are
//! copied there at write time, filtered to the kinds a member may see. Both Lambdas use
//! this instead of `Storage::append_event` directly.

use memorylane_core::events::{Event, EventKind};
use memorylane_core::ids::HouseholdId;
use time::OffsetDateTime;

use crate::{Result, Storage};

/// Append to the household's feed and mirror to every active member of the trip named
/// by the event, when the kind is visible to members.
pub async fn emit(
    storage: &dyn Storage,
    household: &HouseholdId,
    at: OffsetDateTime,
    kind: EventKind,
) -> Result<Event> {
    let event = storage.append_event(household, at, kind).await?;
    if !event.visible_to_members() {
        return Ok(event);
    }
    let Some(trip) = event.trip() else {
        return Ok(event);
    };
    for m in storage.list_trip_members(household, trip).await? {
        if m.is_active() {
            storage
                .append_event(&m.member_household, at, event.kind.clone())
                .await?;
        }
    }
    Ok(event)
}
