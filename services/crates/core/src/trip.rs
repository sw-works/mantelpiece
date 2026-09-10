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
    /// The narrative paragraph for this day, split out for narration sync (F-709).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story: Option<String>,
    /// Media with no capture time or GPS, placed here by upload time (edge case, US4).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub undated: Vec<MediaId>,
}

/// Outcome of the place-and-date check every story goes through (F-301).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NarrativeCheck {
    #[default]
    Passed,
    /// The model named something outside the metadata twice; nothing is shown.
    Withheld,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Narrative {
    /// Empty when `check` is `Withheld`.
    pub text: String,
    pub model_id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub generated_at: OffsetDateTime,
    #[serde(default)]
    pub check: NarrativeCheck,
}

impl Narrative {
    pub fn withheld(model_id: impl Into<String>, generated_at: OffsetDateTime) -> Self {
        Self {
            text: String::new(),
            model_id: model_id.into(),
            generated_at,
            check: NarrativeCheck::Withheld,
        }
    }

    /// The story to show, or nothing when it was withheld.
    pub fn shown(&self) -> Option<&str> {
        match self.check {
            NarrativeCheck::Passed if !self.text.is_empty() => Some(&self.text),
            _ => None,
        }
    }
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
    /// Households that joined this trip from another TV (F-162). The trip itself stays
    /// under the owner's partition.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<HouseholdId>,
}

impl Trip {
    pub fn media_count(&self) -> usize {
        self.days.iter().map(|d| d.media.len()).sum()
    }

    /// The household that owns the trip. Same as `household`; named for the access matrix.
    pub fn owner_household(&self) -> &HouseholdId {
        &self.household
    }

    pub fn is_member(&self, household: &HouseholdId) -> bool {
        self.members.contains(household)
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

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn withheld_narrative_shows_nothing_and_old_json_passes() {
        let n = Narrative::withheld("nova", datetime!(2026-09-08 10:00:00 UTC));
        assert_eq!(n.shown(), None);
        let old: Narrative = serde_json::from_str(
            r#"{"text":"A day in Kyoto.","model_id":"nova","generated_at":"2026-09-08T10:00:00Z"}"#,
        )
        .unwrap();
        assert_eq!(old.check, NarrativeCheck::Passed);
        assert_eq!(old.shown(), Some("A day in Kyoto."));
    }
}
