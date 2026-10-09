//! The tenant: one Amazon account, one or more TVs, settings that gate model features.

use crate::ids::{ContributorId, HouseholdId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    /// Phones asking to go live, one per phone; the TV grants or declines each.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub live_requests: Vec<LiveRequest>,
    /// Centre of the household's everyday photos outside any trip; field notes show no
    /// precise marker within 2 km of it (F-711).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<crate::geo::GeoPoint>,
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
    /// The phone asked to keep this broadcast and IVS is recording it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save: Option<LiveSave>,
}

/// A phone asking the TV to show its camera. Nothing streams, and no channel exists,
/// until the TV grants it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveRequest {
    pub id: String,
    pub contributor: ContributorId,
    pub display_name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub requested_at: OffsetDateTime,
    pub status: LiveRequestStatus,
    /// The phone wants the broadcast kept in the household's memories.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save: Option<LiveSave>,
    /// Where and how the phone streams once granted. Only the asking phone reads it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant: Option<LiveGrant>,
}

/// Keep this broadcast: record it and add the video to the household's memories.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveSave {
    /// The phone's zone, so the saved video lands on the right local day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utc_offset_seconds: Option<i32>,
}

/// Left in the media bucket when a saved broadcast's channel is made. IVS files the
/// recording under the channel id alone; this is how the recording finds its household.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordingClaim {
    pub household: HouseholdId,
    pub broadcast: String,
    pub contributor: ContributorId,
    pub display_name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: OffsetDateTime,
    #[serde(default)]
    pub save: LiveSave,
}

impl RecordingClaim {
    /// `live/recordings/{channel id}.json`, the channel id being the last part of its ARN.
    pub fn key(channel_arn: &str) -> String {
        let id = channel_arn.rsplit('/').next().unwrap_or(channel_arn);
        format!("live/recordings/{id}.json")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveRequestStatus {
    Waiting,
    Granted,
    Declined,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveGrant {
    pub broadcast: String,
    pub ingest_endpoint: String,
    pub stream_key: String,
    pub playback_url: String,
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
            live_requests: Vec::new(),
            home: None,
        }
    }

    pub fn is_deleting(&self) -> bool {
        self.deleting_since.is_some()
    }
}
