//! The tenant: one Amazon account, one or more TVs, settings that gate model features.

use crate::ids::HouseholdId;
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
        }
    }

    pub fn is_deleting(&self) -> bool {
        self.deleting_since.is_some()
    }
}
