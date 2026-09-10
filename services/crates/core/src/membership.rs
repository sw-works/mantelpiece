//! A household that joined another household's trip from its own TV (F-162).
//!
//! The trip stays under the owner's partition; the member only holds a pointer plus its
//! own pins and hides on the media records.

use crate::ids::{HouseholdId, TripId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Membership {
    pub member_household: HouseholdId,
    pub owner_household: HouseholdId,
    pub trip: TripId,
    /// How the member household shows up on the owner's ticker and Invite screen.
    pub member_name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub joined_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub left_at: Option<OffsetDateTime>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MembershipError {
    #[error("a household cannot join its own trip")]
    OwnTrip,
}

impl Membership {
    pub fn join(
        member_household: HouseholdId,
        owner_household: HouseholdId,
        trip: TripId,
        member_name: impl Into<String>,
        now: OffsetDateTime,
    ) -> Result<Self, MembershipError> {
        if member_household == owner_household {
            return Err(MembershipError::OwnTrip);
        }
        Ok(Self {
            member_household,
            owner_household,
            trip,
            member_name: member_name.into(),
            joined_at: now,
            left_at: None,
        })
    }

    pub fn is_active(&self) -> bool {
        self.left_at.is_none()
    }

    pub fn leave(&mut self, now: OffsetDateTime) {
        self.left_at.get_or_insert(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn cannot_join_own_trip() {
        let now = datetime!(2026-09-08 10:00:00 UTC);
        assert_eq!(
            Membership::join(
                HouseholdId::new("h"),
                HouseholdId::new("h"),
                TripId::new("t"),
                "Me",
                now
            ),
            Err(MembershipError::OwnTrip)
        );
        let mut m = Membership::join(
            HouseholdId::new("ruth"),
            HouseholdId::new("hanyu"),
            TripId::new("kyoto"),
            "Ruth's living room",
            now,
        )
        .unwrap();
        assert!(m.is_active());
        m.leave(now);
        m.leave(now + time::Duration::hours(1));
        assert_eq!(m.left_at, Some(now));
    }
}
