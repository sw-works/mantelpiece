//! Expiring links to an album or playlist for people outside the household (F-601).
//! Creating one is irreversible in the sense that the link is out there, so it needs a
//! confirmation token.

use crate::ids::{AlbumId, HouseholdId, PlaylistId, ShareLinkId};
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

pub const DEFAULT_TTL: Duration = Duration::days(7);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum ShareTarget {
    Album { id: AlbumId },
    Playlist { id: PlaylistId },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShareLink {
    pub id: ShareLinkId,
    pub household: HouseholdId,
    #[serde(flatten)]
    pub target: ShareTarget,
    pub name: String,
    /// The confirmation token that authorised the share.
    pub confirmation: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub revoked_at: Option<OffsetDateTime>,
}

impl ShareLink {
    pub fn new(
        household: HouseholdId,
        target: ShareTarget,
        name: impl Into<String>,
        confirmation: impl Into<String>,
        now: OffsetDateTime,
        ttl: Option<Duration>,
    ) -> Self {
        Self {
            id: ShareLinkId::random(),
            household,
            target,
            name: name.into(),
            confirmation: confirmation.into(),
            created_at: now,
            expires_at: now + ttl.unwrap_or(DEFAULT_TTL),
            revoked_at: None,
        }
    }

    /// Path on the public share page.
    pub fn path(&self) -> String {
        format!("/s/{}", self.id)
    }

    pub fn is_live(&self, now: OffsetDateTime) -> bool {
        self.revoked_at.is_none() && now < self.expires_at
    }

    pub fn revoke(&mut self, now: OffsetDateTime) {
        self.revoked_at.get_or_insert(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn links_expire_after_a_week_or_when_revoked() {
        let now = datetime!(2026-09-08 10:00:00 UTC);
        let mut l = ShareLink::new(
            HouseholdId::new("h"),
            ShareTarget::Album {
                id: AlbumId::new("a"),
            },
            "Kyoto highlights",
            "tok",
            now,
            None,
        );
        assert!(l.is_live(now + Duration::days(6)));
        assert!(!l.is_live(now + DEFAULT_TTL));
        assert!(l.path().starts_with("/s/"));
        l.revoke(now);
        assert!(!l.is_live(now));
        let json = serde_json::to_value(&l).unwrap();
        assert_eq!(json["target"], "album");
    }
}
