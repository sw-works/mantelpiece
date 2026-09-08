//! Events pushed to the TV (long-poll or AppSync) and onto EventBridge.
//!
//! One schema for both transports (architecture section 3).

use crate::ids::{ContributorId, HouseholdId, MediaId, PlaylistId, TripId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    /// Ingest finished for a batch and a trip was found or extended.
    #[serde(rename = "trip.detected")]
    TripDetected { trip: TripId, new_media: u32 },
    /// Trip Curator finished: album, narrative and playlist exist.
    #[serde(rename = "memory.ready")]
    MemoryReady { trip: TripId },
    #[serde(rename = "playlist.created")]
    PlaylistCreated { playlist: PlaylistId, name: String },
    /// Dedup Steward has groups to review; TV opens Review Changes.
    #[serde(rename = "dedup.proposal")]
    DedupProposal {
        trip: Option<TripId>,
        groups: u32,
        removable: u32,
    },
    /// A contributor's batch landed; drives W6 and the live ticker (F-160).
    #[serde(rename = "contribution.received")]
    ContributionReceived {
        trip: Option<TripId>,
        contributor: ContributorId,
        display_name: String,
        count: u32,
    },
    #[serde(rename = "upload.progress")]
    UploadProgress {
        contributor: ContributorId,
        done: u32,
        total: u32,
    },
    #[serde(rename = "media.removed")]
    MediaRemoved { media: Vec<MediaId> },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub household: HouseholdId,
    /// Monotonic per household; the TV polls `/events?since=<seq>`.
    pub seq: u64,
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
    #[serde(flatten)]
    pub kind: EventKind,
}

impl Event {
    /// EventBridge `detail-type` for this event.
    pub fn detail_type(&self) -> &'static str {
        match self.kind {
            EventKind::TripDetected { .. } => "trip.detected",
            EventKind::MemoryReady { .. } => "memory.ready",
            EventKind::PlaylistCreated { .. } => "playlist.created",
            EventKind::DedupProposal { .. } => "dedup.proposal",
            EventKind::ContributionReceived { .. } => "contribution.received",
            EventKind::UploadProgress { .. } => "upload.progress",
            EventKind::MediaRemoved { .. } => "media.removed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn event_wire_format_is_flat_with_dotted_type() {
        let e = Event {
            household: HouseholdId::new("h"),
            seq: 7,
            at: datetime!(2026-09-08 10:00:00 UTC),
            kind: EventKind::ContributionReceived {
                trip: Some(TripId::new("t")),
                contributor: ContributorId::new("c"),
                display_name: "Maya".into(),
                count: 42,
            },
        };
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["type"], "contribution.received");
        assert_eq!(json["seq"], 7);
        assert_eq!(json["display_name"], "Maya");
        assert_eq!(e.detail_type(), "contribution.received");
        let back: Event = serde_json::from_value(json).unwrap();
        assert_eq!(back, e);
    }
}
