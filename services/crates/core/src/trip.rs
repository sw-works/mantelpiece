//! Trips, days, routes, albums, playlists, narratives.

use crate::geo::GeoPoint;
use crate::ids::{AlbumId, ContributorId, HouseholdId, MediaId, PlaylistId, TripId};
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoutePoint {
    pub at: GeoPoint,
    #[serde(with = "time::serde::rfc3339")]
    pub time: OffsetDateTime,
    pub media: MediaId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Route {
    pub points: Vec<RoutePoint>,
    /// True when GPS was missing and the route came from landmarks and scene order (F-206).
    pub inferred: bool,
    /// S3 key of the static map PNG (F-207), once rendered.
    pub map_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TripDay {
    pub index: u32,
    pub date: Date,
    pub media: Vec<MediaId>,
    pub route: Route,
    pub title: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Narrative {
    pub text: String,
    pub model_id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub generated_at: OffsetDateTime,
}

/// Clock correction applied to one contributor inside a trip (F-155).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClockOffset {
    pub contributor: ContributorId,
    pub seconds: i64,
    /// Number of overlapping GPS pairs the offset was estimated from.
    pub samples: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trip {
    pub id: TripId,
    pub household: HouseholdId,
    pub name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub start: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub end: OffsetDateTime,
    pub days: Vec<TripDay>,
    pub route: Route,
    pub highlights: Vec<MediaId>,
    pub contributors: Vec<ContributorId>,
    pub clock_offsets: Vec<ClockOffset>,
    pub narrative: Option<Narrative>,
    pub hero: Option<MediaId>,
}

impl Trip {
    pub fn media_count(&self) -> usize {
        self.days.iter().map(|d| d.media.len()).sum()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Album {
    pub id: AlbumId,
    pub household: HouseholdId,
    pub trip: Option<TripId>,
    pub name: String,
    pub media: Vec<MediaId>,
    pub cover: Option<MediaId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: PlaylistId,
    pub household: HouseholdId,
    pub name: String,
    /// The natural-language query or rationale that produced it.
    pub source: String,
    pub media: Vec<MediaId>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}
