//! Who uploaded what.

use crate::ids::{ContributorId, HouseholdId, MediaId};
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
    /// A phone paired with the household's TV: it scanned the pairing code, gave a name,
    /// and waits for the TV to approve. Approved, it acts for the household.
    Phone { status: PairStatus },
}

/// Where a paired phone stands with the TV.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairStatus {
    Pending,
    Approved,
    /// Turned away on the TV, or forgotten later.
    Denied,
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

/// A browser's web push subscription (`PushSubscription.toJSON()`), so the TV can reach
/// this phone when someone reacts to its photos (family channel).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushSubscription {
    pub endpoint: String,
    /// The browser's P-256 public key, base64url.
    pub p256dh: String,
    /// The browser's 16-byte auth secret, base64url.
    pub auth: String,
}

/// Something a household did with this contributor's photos, shown on their phone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxNote {
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
    /// Who reacted: the TV's household name.
    pub from: String,
    /// `love` for now.
    pub kind: String,
    /// How many photos the reaction covers.
    pub count: u32,
    /// One of those photos, for a thumbnail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<MediaId>,
}

/// Notes kept per contributor; older ones fall off.
pub const INBOX_LIMIT: usize = 20;

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
    /// Where to push reactions, when the phone turned notifications on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub push: Option<PushSubscription>,
    /// Reactions to this contributor's photos, newest first, at most [`INBOX_LIMIT`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inbox: Vec<InboxNote>,
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
            push: None,
            inbox: Vec::new(),
        }
    }

    /// A phone asking to pair with the TV; inactive until the TV approves it.
    pub fn phone(
        household: HouseholdId,
        display_name: impl Into<String>,
        now: OffsetDateTime,
    ) -> Self {
        Self {
            id: ContributorId::random(),
            household,
            display_name: display_name.into(),
            kind: ContributorKind::Phone {
                status: PairStatus::Pending,
            },
            joined_at: now,
            removed_at: None,
            grace_until: None,
            role: Role::Contributor,
            push: None,
            inbox: Vec::new(),
        }
    }

    /// Put a note at the top of the inbox, dropping the oldest past [`INBOX_LIMIT`].
    pub fn note(&mut self, note: InboxNote) {
        self.inbox.insert(0, note);
        self.inbox.truncate(INBOX_LIMIT);
    }

    /// The pairing status, for a paired phone.
    pub fn pair_status(&self) -> Option<PairStatus> {
        match self.kind {
            ContributorKind::Phone { status } => Some(status),
            _ => None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.removed_at.is_none()
            && !matches!(
                self.pair_status(),
                Some(PairStatus::Pending | PairStatus::Denied)
            )
    }

    /// Owners and approved phones act as co-editors everywhere without holding the role.
    pub fn can_edit(&self) -> bool {
        matches!(self.kind, ContributorKind::Owner)
            || self.pair_status() == Some(PairStatus::Approved)
            || self.role == Role::CoEditor
    }

    /// Change the role. A guest never becomes a co-editor (F-165); the owner never needs
    /// one.
    pub fn set_role(&mut self, role: Role) -> Result<(), RoleError> {
        match (&self.kind, role) {
            (ContributorKind::Owner | ContributorKind::Phone { .. }, _) => {
                Err(RoleError::OwnerRole)
            }
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
            push: None,
            inbox: Vec::new(),
        }
    }

    #[test]
    fn a_paired_phone_is_inactive_until_the_tv_approves() {
        let mut phone = Contributor::phone(
            HouseholdId::new("h"),
            "Maya",
            datetime!(2026-09-08 10:00:00 UTC),
        );
        assert!(!phone.is_active());
        assert!(!phone.can_edit());
        phone.kind = ContributorKind::Phone {
            status: PairStatus::Approved,
        };
        assert!(phone.is_active());
        assert!(phone.can_edit());
        phone.kind = ContributorKind::Phone {
            status: PairStatus::Denied,
        };
        assert!(!phone.is_active());
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
