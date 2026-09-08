//! Confirmation tokens for irreversible tools (F-410, F-504).
//!
//! The TV's Review Changes modal mints one when the user confirms on the remote. The
//! Gateway's Cedar policy requires it on `apply_dedup`, `delete_media`, `share_album` and
//! `remove_contributor`. Agents cannot mint one.

use crate::ids::HouseholdId;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

pub const DEFAULT_TTL: Duration = Duration::minutes(5);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ConfirmedAction {
    ApplyDedup { group_ids: Vec<String> },
    DeleteMedia { media_ids: Vec<String> },
    ShareAlbum { album_id: String },
    RemoveContributor { contributor_id: String },
}

impl ConfirmedAction {
    /// Tool name as exposed through the Gateway, for the Cedar policy.
    pub fn tool_name(&self) -> &'static str {
        match self {
            Self::ApplyDedup { .. } => "apply_dedup",
            Self::DeleteMedia { .. } => "delete_media",
            Self::ShareAlbum { .. } => "share_album",
            Self::RemoveContributor { .. } => "remove_contributor",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmationToken {
    pub token: String,
    pub household: HouseholdId,
    #[serde(flatten)]
    pub action: ConfirmedAction,
    #[serde(with = "time::serde::rfc3339")]
    pub issued_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
    pub used: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfirmationError {
    #[error("token expired")]
    Expired,
    #[error("token already used")]
    Used,
    #[error("token belongs to another household")]
    WrongHousehold,
    #[error("token confirms a different action")]
    WrongAction,
}

impl ConfirmationToken {
    pub fn mint(household: HouseholdId, action: ConfirmedAction, now: OffsetDateTime) -> Self {
        Self {
            token: Uuid::new_v4().to_string(),
            household,
            action,
            issued_at: now,
            expires_at: now + DEFAULT_TTL,
            used: false,
        }
    }

    /// Consume the token for exactly this action on this household.
    pub fn redeem(
        &mut self,
        household: &HouseholdId,
        action: &ConfirmedAction,
        now: OffsetDateTime,
    ) -> Result<(), ConfirmationError> {
        if &self.household != household {
            return Err(ConfirmationError::WrongHousehold);
        }
        if self.used {
            return Err(ConfirmationError::Used);
        }
        if now >= self.expires_at {
            return Err(ConfirmationError::Expired);
        }
        if &self.action != action {
            return Err(ConfirmationError::WrongAction);
        }
        self.used = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn redeem_once_for_matching_action() {
        let now = datetime!(2026-09-08 10:00:00 UTC);
        let action = ConfirmedAction::ApplyDedup {
            group_ids: vec!["g1".into()],
        };
        let mut t = ConfirmationToken::mint(HouseholdId::new("h"), action.clone(), now);
        assert_eq!(t.action.tool_name(), "apply_dedup");
        assert_eq!(t.redeem(&HouseholdId::new("h"), &action, now), Ok(()));
        assert_eq!(
            t.redeem(&HouseholdId::new("h"), &action, now),
            Err(ConfirmationError::Used)
        );
    }

    #[test]
    fn rejects_other_household_action_and_expiry() {
        let now = datetime!(2026-09-08 10:00:00 UTC);
        let action = ConfirmedAction::DeleteMedia {
            media_ids: vec!["m".into()],
        };
        let mut t = ConfirmationToken::mint(HouseholdId::new("h"), action.clone(), now);
        assert_eq!(
            t.redeem(&HouseholdId::new("other"), &action, now),
            Err(ConfirmationError::WrongHousehold)
        );
        let other = ConfirmedAction::DeleteMedia {
            media_ids: vec!["x".into()],
        };
        assert_eq!(
            t.redeem(&HouseholdId::new("h"), &other, now),
            Err(ConfirmationError::WrongAction)
        );
        assert_eq!(
            t.redeem(&HouseholdId::new("h"), &action, now + DEFAULT_TTL),
            Err(ConfirmationError::Expired)
        );
    }
}
