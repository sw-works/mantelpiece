//! The tenant: one Amazon account, one or more TVs, settings that gate model features.

use crate::ids::{ContributorId, HouseholdId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Household {
    pub id: HouseholdId,
    /// Named from the TV, e.g. "Hanyu's living room"; shown to member households.
    pub name: String,
    /// Opt-in face grouping (F-704). Turning it off deletes the collection and people.
    #[serde(default)]
    pub people_enabled: bool,
    /// Story read aloud by Polly (F-709), behind a flag until the audio spike passes.
    #[serde(default)]
    pub narration_enabled: bool,
    /// Set while account deletion runs; uploads answer 410 (F-604).
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub deleting_since: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// The phone broadcasting to this household's TV right now, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live: Option<LiveBroadcast>,
    /// IVS channels whose stream stopped but which could not be deleted yet (IVS refuses
    /// while a stream is still winding down); retried on the next live call.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stale_channels: Vec<String>,
}

/// One phone broadcast: an IVS channel made for it and deleted after.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveBroadcast {
    pub id: String,
    pub channel_arn: String,
    pub playback_url: String,
    pub contributor: ContributorId,
    pub display_name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: OffsetDateTime,
}

impl Household {
    pub fn new(id: HouseholdId, name: impl Into<String>, now: OffsetDateTime) -> Self {
        Self {
            id,
            name: name.into(),
            people_enabled: false,
            narration_enabled: false,
            deleting_since: None,
            created_at: now,
            live: None,
            stale_channels: Vec::new(),
        }
    }

    pub fn is_deleting(&self) -> bool {
        self.deleting_since.is_some()
    }
}
