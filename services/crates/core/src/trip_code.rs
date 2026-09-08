//! Trip codes carried on the Invite QR (F-151 to F-153).

use crate::codes;
use crate::ids::{HouseholdId, TripId};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

pub const CODE_LEN: usize = 6;

/// What the QR opens. `t` is set inside a trip, otherwise `h` targets the household inbox.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "scope")]
pub enum CodeScope {
    Household,
    Trip { trip: TripId },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TripCode {
    pub code: String,
    pub household: HouseholdId,
    #[serde(flatten)]
    pub scope: CodeScope,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub expires_at: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub revoked_at: Option<OffsetDateTime>,
}

impl TripCode {
    pub fn issue(
        rng: &mut impl RngCore,
        household: HouseholdId,
        scope: CodeScope,
        now: OffsetDateTime,
        expires_at: Option<OffsetDateTime>,
    ) -> Self {
        Self {
            code: codes::generate(rng, CODE_LEN),
            household,
            scope,
            created_at: now,
            expires_at,
            revoked_at: None,
        }
    }

    /// Expired or rotated codes stop new uploads but keep existing contributions (F-153).
    pub fn accepts_uploads(&self, now: OffsetDateTime) -> bool {
        self.revoked_at.is_none() && self.expires_at.is_none_or(|e| now < e)
    }

    pub fn revoke(&mut self, now: OffsetDateTime) {
        self.revoked_at.get_or_insert(now);
    }

    /// Revoke this code and mint a replacement with the same scope and expiry policy.
    #[must_use = "the replacement code must be stored"]
    pub fn rotate(&mut self, rng: &mut impl RngCore, now: OffsetDateTime) -> TripCode {
        self.revoke(now);
        let ttl = self.expires_at.map(|e| e - self.created_at);
        TripCode::issue(
            rng,
            self.household.clone(),
            self.scope.clone(),
            now,
            ttl.map(|t| now + t),
        )
    }

    /// Path and query for the uploader link, without scheme or host.
    pub fn link_query(&self) -> String {
        match &self.scope {
            CodeScope::Household => format!("?h={}", self.code),
            CodeScope::Trip { .. } => format!("?t={}", self.code),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes::test_rng::SeqRng;
    use time::macros::datetime;
    use time::Duration;

    #[test]
    fn lifecycle_revoke_and_rotate() {
        let now = datetime!(2026-09-08 10:00:00 UTC);
        let mut rng = SeqRng(3);
        let mut code = TripCode::issue(
            &mut rng,
            HouseholdId::new("h"),
            CodeScope::Trip {
                trip: TripId::new("t"),
            },
            now,
            Some(now + Duration::days(7)),
        );
        assert!(code.accepts_uploads(now));
        assert!(!code.accepts_uploads(now + Duration::days(7)));
        assert_eq!(code.link_query(), format!("?t={}", code.code));

        let fresh = code.rotate(&mut rng, now + Duration::days(1));
        assert!(!code.accepts_uploads(now + Duration::days(1)));
        assert!(fresh.accepts_uploads(now + Duration::days(1)));
        assert_ne!(fresh.code, code.code);
        assert_eq!(fresh.expires_at, Some(now + Duration::days(8)));
        assert_eq!(fresh.scope, code.scope);
    }

    #[test]
    fn household_code_without_expiry_stays_open() {
        let now = datetime!(2026-09-08 10:00:00 UTC);
        let code = TripCode::issue(
            &mut SeqRng(0),
            HouseholdId::new("h"),
            CodeScope::Household,
            now,
            None,
        );
        assert!(code.accepts_uploads(now + Duration::days(400)));
        assert!(code.link_query().starts_with("?h="));
    }
}
