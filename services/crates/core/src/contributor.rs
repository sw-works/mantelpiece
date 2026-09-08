//! Who uploaded what.

use crate::ids::{ContributorId, HouseholdId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContributorKind {
    /// The household owner, whose Cognito `sub` is the household id.
    Owner,
    /// A friend signed in with their own Amazon account.
    Amazon { sub: String },
    /// A named guest bound to a trip code and a browser session.
    Guest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contributor {
    pub id: ContributorId,
    pub household: HouseholdId,
    pub display_name: String,
    pub kind: ContributorKind,
    #[serde(with = "time::serde::rfc3339")]
    pub joined_at: OffsetDateTime,
    /// Set when the owner removes this contributor (F-158). Their media is detached and
    /// deleted after a grace period.
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub removed_at: Option<OffsetDateTime>,
}

impl Contributor {
    pub fn owner(
        household: HouseholdId,
        display_name: impl Into<String>,
        now: OffsetDateTime,
    ) -> Self {
        Self {
            id: ContributorId::new(household.as_str()),
            household,
            display_name: display_name.into(),
            kind: ContributorKind::Owner,
            joined_at: now,
            removed_at: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.removed_at.is_none()
    }
}
