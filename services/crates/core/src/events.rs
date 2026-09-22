//! Events pushed to the TV (long-poll or AppSync) and onto EventBridge.
//!
//! One schema for both transports (architecture section 3).

use crate::ids::{BatchId, ContributorId, HouseholdId, MediaId, PlaylistId, TripId};
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
    /// Someone asked for a slideshow on the TV: the Echo Show skill, the phone, Alexa+.
    /// Exactly one of `trip` and `playlist` is set; the TV opens it.
    #[serde(rename = "play.requested")]
    PlayRequested {
        trip: Option<TripId>,
        playlist: Option<PlaylistId>,
        name: String,
        /// Where the request came from: `alexa`, `phone`, `agent`.
        source: String,
        /// Open the show at this photo instead of the first one: "play this photo on the
        /// TV" from the phone, the Chromecast moment.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start: Option<MediaId>,
    },
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
    /// A phone scanned the pairing code and waits for the TV to let it in.
    #[serde(rename = "phone.requested")]
    PhoneRequested {
        contributor: ContributorId,
        display_name: String,
    },
    /// Photos from a phone are ready to show: their renditions exist. Emitted by Signal as
    /// a batch lands, so the TV can put them up before any trip exists (family channel).
    /// `media` is the batch so far, newest last, at most [`ARRIVAL_MEDIA`] ids; `count` is
    /// every photo of the batch signalled so far.
    #[serde(rename = "photos.arrived")]
    PhotosArrived {
        contributor: ContributorId,
        display_name: String,
        batch: BatchId,
        media: Vec<MediaId>,
        count: u32,
    },
    /// A phone went live: the TV plays `playback_url` (HLS, a few seconds behind).
    #[serde(rename = "live.started")]
    LiveStarted {
        broadcast: String,
        display_name: String,
        playback_url: String,
    },
    /// The broadcast ended, from the phone or the TV.
    #[serde(rename = "live.ended")]
    LiveEnded { broadcast: String },
    #[serde(rename = "upload.progress")]
    UploadProgress {
        contributor: ContributorId,
        done: u32,
        total: u32,
    },
    #[serde(rename = "media.removed")]
    MediaRemoved { media: Vec<MediaId> },
    /// Quick cleanup found candidates; TV opens Review Changes (F-705).
    #[serde(rename = "cleanup.proposal")]
    CleanupProposal {
        trip: Option<TripId>,
        count: u32,
        reasons: Vec<String>,
    },
    /// A like or comment from a contributor or member household (F-710).
    #[serde(rename = "reaction.received")]
    ReactionReceived {
        trip: TripId,
        target: String,
        actor_name: String,
        kind: String,
        text: Option<String>,
    },
    /// Another household joined a trip by code (F-162).
    #[serde(rename = "membership.joined")]
    MembershipJoined { trip: TripId, member_name: String },
    /// Answer to a question about the photo on screen, drawn over it (F-405).
    #[serde(rename = "answer.card")]
    AnswerCard {
        question: String,
        answer: String,
        media: Option<MediaId>,
    },
    /// The share workflow found the recipient has a MemoryLane TV: show the trip's join
    /// code instead of a link (F-601, W5).
    #[serde(rename = "share.join_code")]
    ShareJoinCode {
        trip: TripId,
        recipient: String,
        code: String,
    },
    /// Narration audio for one day is ready (F-709, behind a flag).
    #[serde(rename = "narration.ready")]
    NarrationReady {
        trip: TripId,
        day: u32,
        audio_key: String,
    },
    /// Enrich flagged keepers for quick cleanup; EventBridge only, the Concierge turns it
    /// into a proposal (F-705).
    #[serde(rename = "cleanup.candidates")]
    CleanupCandidates {
        trip: Option<TripId>,
        media: Vec<MediaId>,
        reasons: Vec<String>,
    },
    /// Nightly memories were computed (F-701, F-702).
    #[serde(rename = "memories.ready")]
    MemoriesReady {
        date: String,
        on_this_day: u32,
        then_and_now: u32,
    },
}

/// Most photos a `photos.arrived` event names; the TV shows these and the count.
pub const ARRIVAL_MEDIA: usize = 12;

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
            EventKind::PlayRequested { .. } => "play.requested",
            EventKind::DedupProposal { .. } => "dedup.proposal",
            EventKind::ContributionReceived { .. } => "contribution.received",
            EventKind::PhoneRequested { .. } => "phone.requested",
            EventKind::UploadProgress { .. } => "upload.progress",
            EventKind::PhotosArrived { .. } => "photos.arrived",
            EventKind::LiveStarted { .. } => "live.started",
            EventKind::LiveEnded { .. } => "live.ended",
            EventKind::MediaRemoved { .. } => "media.removed",
            EventKind::CleanupProposal { .. } => "cleanup.proposal",
            EventKind::ReactionReceived { .. } => "reaction.received",
            EventKind::MembershipJoined { .. } => "membership.joined",
            EventKind::AnswerCard { .. } => "answer.card",
            EventKind::ShareJoinCode { .. } => "share.join_code",
            EventKind::NarrationReady { .. } => "narration.ready",
            EventKind::MemoriesReady { .. } => "memories.ready",
            EventKind::CleanupCandidates { .. } => "cleanup.candidates",
        }
    }

    /// Kinds a member household receives for a joined trip (contracts/events.md).
    pub fn visible_to_members(&self) -> bool {
        matches!(
            self.kind,
            EventKind::ContributionReceived { .. }
                | EventKind::MemoryReady { .. }
                | EventKind::ReactionReceived { .. }
                | EventKind::MediaRemoved { .. }
        )
    }

    /// The trip this event is about, when it names one.
    pub fn trip(&self) -> Option<&TripId> {
        match &self.kind {
            EventKind::TripDetected { trip, .. }
            | EventKind::MemoryReady { trip }
            | EventKind::ReactionReceived { trip, .. }
            | EventKind::MembershipJoined { trip, .. }
            | EventKind::ShareJoinCode { trip, .. }
            | EventKind::NarrationReady { trip, .. } => Some(trip),
            EventKind::DedupProposal { trip, .. }
            | EventKind::ContributionReceived { trip, .. }
            | EventKind::CleanupProposal { trip, .. }
            | EventKind::CleanupCandidates { trip, .. }
            | EventKind::PlayRequested { trip, .. } => trip.as_ref(),
            EventKind::PlaylistCreated { .. }
            | EventKind::PhoneRequested { .. }
            | EventKind::UploadProgress { .. }
            | EventKind::PhotosArrived { .. }
            | EventKind::LiveStarted { .. }
            | EventKind::LiveEnded { .. }
            | EventKind::MediaRemoved { .. }
            | EventKind::AnswerCard { .. }
            | EventKind::MemoriesReady { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn arrivals_and_live_events_carry_dotted_types_and_no_trip() {
        let arrived = Event {
            household: HouseholdId::new("h"),
            seq: 1,
            at: datetime!(2026-09-21 10:00:00 UTC),
            kind: EventKind::PhotosArrived {
                contributor: ContributorId::new("c"),
                display_name: "Maya".into(),
                batch: BatchId::new("b"),
                media: vec![MediaId::new("m1")],
                count: 1,
            },
        };
        let json = serde_json::to_value(&arrived).unwrap();
        assert_eq!(json["type"], "photos.arrived");
        assert_eq!(json["batch"], "b");
        assert_eq!(arrived.detail_type(), "photos.arrived");
        assert_eq!(arrived.trip(), None);
        assert!(!arrived.visible_to_members());
        let live = Event {
            kind: EventKind::LiveStarted {
                broadcast: "l1".into(),
                display_name: "Maya".into(),
                playback_url: "https://x/y.m3u8".into(),
            },
            ..arrived.clone()
        };
        assert_eq!(serde_json::to_value(&live).unwrap()["type"], "live.started");
        let ended = Event {
            kind: EventKind::LiveEnded {
                broadcast: "l1".into(),
            },
            ..arrived
        };
        assert_eq!(ended.detail_type(), "live.ended");
    }

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

    #[test]
    fn new_kinds_round_trip_and_member_filter() {
        let mk = |kind| Event {
            household: HouseholdId::new("h"),
            seq: 1,
            at: datetime!(2026-09-08 10:00:00 UTC),
            kind,
        };
        let reaction = mk(EventKind::ReactionReceived {
            trip: TripId::new("t"),
            target: "day:3".into(),
            actor_name: "Grandma Ruth".into(),
            kind: "like".into(),
            text: None,
        });
        assert_eq!(reaction.detail_type(), "reaction.received");
        assert!(reaction.visible_to_members());
        assert_eq!(reaction.trip(), Some(&TripId::new("t")));
        let card = mk(EventKind::AnswerCard {
            question: "what is that statue".into(),
            answer: "The Great Buddha at Todai-ji".into(),
            media: Some(MediaId::new("m")),
        });
        assert!(!card.visible_to_members());
        let json = serde_json::to_value(&card).unwrap();
        assert_eq!(json["type"], "answer.card");
        let back: Event = serde_json::from_value(json).unwrap();
        assert_eq!(back, card);
        for kind in [
            EventKind::CleanupProposal {
                trip: None,
                count: 3,
                reasons: vec!["screenshot".into()],
            },
            EventKind::MembershipJoined {
                trip: TripId::new("t"),
                member_name: "Ruth's living room".into(),
            },
            EventKind::NarrationReady {
                trip: TripId::new("t"),
                day: 1,
                audio_key: "narration/h/t/1.mp3".into(),
            },
            EventKind::ShareJoinCode {
                trip: TripId::new("t"),
                recipient: "Grandma".into(),
                code: "T2C3D4".into(),
            },
            EventKind::MemoriesReady {
                date: "2026-09-08".into(),
                on_this_day: 2,
                then_and_now: 1,
            },
        ] {
            let e = mk(kind);
            let json = serde_json::to_value(&e).unwrap();
            assert_eq!(json["type"], e.detail_type());
        }
    }
}
