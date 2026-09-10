//! Audit log (F-605): who did what, when, with which confirmation. Written by the API for
//! sign-in, shares, deletions and every tool route; read back by the owner. One entry per
//! request, kept for a year.

use crate::ids::{AuditId, HouseholdId};
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

pub const RETENTION: Duration = Duration::days(365);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: AuditId,
    pub household: HouseholdId,
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
    /// Owner subject, contributor id, or "session" for a TV or phone session.
    pub actor: String,
    /// Dotted action name, e.g. `share.create`, `media.delete`, `confirmation.mint`.
    pub action: String,
    /// The request path, so the entry points at what changed.
    pub subject: String,
    /// The confirmation token the request carried, when it did (SC-006).
    #[serde(default)]
    pub confirmation: Option<String>,
    /// HTTP status the request ended with.
    pub outcome: u16,
}

impl AuditEntry {
    #[must_use]
    pub fn new(
        household: HouseholdId,
        actor: impl Into<String>,
        action: impl Into<String>,
        subject: impl Into<String>,
        now: OffsetDateTime,
    ) -> Self {
        Self {
            id: AuditId::random(),
            household,
            at: now,
            actor: actor.into(),
            action: action.into(),
            subject: subject.into(),
            confirmation: None,
            outcome: 200,
        }
    }

    pub fn expires_at(&self) -> OffsetDateTime {
        self.at + RETENTION
    }
}

/// Which requests are audited, by method and path segments. Everything under the token
/// gate, sign-in, account deletion, shares and memberships.
#[must_use]
pub fn action_for(method: &str, segments: &[&str]) -> Option<&'static str> {
    Some(match (method, segments) {
        ("POST", ["device", "approve"]) => "auth.approve",
        ("POST", ["join"] | ["join", ..]) => "auth.join",
        ("DELETE", ["me"]) => "account.delete",
        ("POST", ["shares"]) => "share.create",
        ("DELETE", ["shares", _]) => "share.revoke",
        ("POST", ["confirmations"]) => "confirmation.mint",
        ("POST", ["media", "delete"]) => "media.delete",
        ("POST", ["media", "dedup", "apply"]) => "media.dedup",
        ("POST", ["media", "cleanup", "apply"]) => "media.cleanup",
        ("POST", ["media", "purge"]) => "media.purge",
        ("POST", ["media", _, "restore"]) => "media.restore",
        ("DELETE", ["contributors", _]) => "contributor.remove",
        ("POST", ["memberships"]) => "membership.join",
        ("DELETE", ["memberships", _]) => "membership.leave",
        ("POST", ["trips", _, "narration"]) => "tool.synthesize_narration",
        ("POST", ["people", "enable" | "disable"]) => "people.toggle",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gated_and_auth_routes_are_audited_reads_are_not() {
        assert_eq!(action_for("POST", &["shares"]), Some("share.create"));
        assert_eq!(
            action_for("POST", &["media", "dedup", "apply"]),
            Some("media.dedup")
        );
        assert_eq!(
            action_for("DELETE", &["contributors", "c1"]),
            Some("contributor.remove")
        );
        assert_eq!(
            action_for("POST", &["device", "approve"]),
            Some("auth.approve")
        );
        assert_eq!(action_for("GET", &["shares"]), None);
        assert_eq!(action_for("GET", &["trips"]), None);
    }
}
