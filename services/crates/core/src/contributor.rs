//! Who uploaded what.

use crate::ids::{ContributorId, HouseholdId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
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

/// What a contributor may do beyond uploading (F-164, F-165).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Uploads, own removals, reactions.
    #[default]
    Contributor,
    /// Also renames, keepers, and removal of any media behind a confirmation token.
    CoEditor,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RoleError {
    #[error("guests cannot be co-editors")]
    GuestCoEditor,
    #[error("the owner already has every permission")]
    OwnerRole,
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
    /// Removed contributors keep an export link until this instant; their media is purged
    /// after it (F-158).
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub grace_until: Option<OffsetDateTime>,
    #[serde(default)]
    pub role: Role,
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
            grace_until: None,
            role: Role::Contributor,
        }
    }

    pub fn is_active(&self) -> bool {
        self.removed_at.is_none()
    }

    /// Owners act as co-editors everywhere without holding the role.
    pub fn can_edit(&self) -> bool {
        matches!(self.kind, ContributorKind::Owner) || self.role == Role::CoEditor
    }

    /// Change the role. A guest never becomes a co-editor (F-165); the owner never needs
    /// one.
    pub fn set_role(&mut self, role: Role) -> Result<(), RoleError> {
        match (&self.kind, role) {
            (ContributorKind::Owner, _) => Err(RoleError::OwnerRole),
            (ContributorKind::Guest, Role::CoEditor) => Err(RoleError::GuestCoEditor),
            _ => {
                self.role = role;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn contributor(kind: ContributorKind) -> Contributor {
        Contributor {
            id: ContributorId::new("c"),
            household: HouseholdId::new("h"),
            display_name: "Maya".into(),
            kind,
            joined_at: datetime!(2026-09-08 10:00:00 UTC),
            removed_at: None,
            grace_until: None,
            role: Role::Contributor,
        }
    }

    #[test]
    fn guests_never_become_co_editors() {
        let mut guest = contributor(ContributorKind::Guest);
        assert_eq!(
            guest.set_role(Role::CoEditor),
            Err(RoleError::GuestCoEditor)
        );
        assert!(!guest.can_edit());
        let mut friend = contributor(ContributorKind::Amazon { sub: "s".into() });
        assert_eq!(friend.set_role(Role::CoEditor), Ok(()));
        assert!(friend.can_edit());
        let mut owner = Contributor::owner(HouseholdId::new("h"), "Hanyu", friend.joined_at);
        assert_eq!(owner.set_role(Role::CoEditor), Err(RoleError::OwnerRole));
        assert!(owner.can_edit());
    }

    #[test]
    fn role_defaults_when_missing_from_stored_json() {
        let json = r#"{"id":"c","household":"h","display_name":"M","kind":{"kind":"guest"},"joined_at":"2026-09-08T10:00:00Z"}"#;
        let c: Contributor = serde_json::from_str(json).unwrap();
        assert_eq!(c.role, Role::Contributor);
        assert!(c.grace_until.is_none());
    }
}
