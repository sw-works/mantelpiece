//! Turn a pile of timestamped, geotagged media into trips, days, routes and highlights
//! (F-155, F-205 to F-209). Pure functions over [`Sample`]s; no I/O.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

pub mod clock;
pub mod cluster;
pub mod highlights;
pub mod naming;
pub mod route;

use memorylane_core::geo::GeoPoint;
use memorylane_core::ids::{ContributorId, MediaId};
use time::{OffsetDateTime, UtcOffset};

/// The slice of a media item the trip pipeline needs.
#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    pub id: MediaId,
    pub contributor: ContributorId,
    /// Capture instant in UTC, after any clock correction.
    pub captured: OffsetDateTime,
    /// Camera zone if EXIF carried one; used to bucket by local date.
    pub offset: Option<UtcOffset>,
    pub gps: Option<GeoPoint>,
    pub score: f32,
    pub burst_group: Option<String>,
    pub landmark: Option<String>,
    /// False when the camera wrote no timestamp and `captured` is the upload instant.
    pub dated: bool,
}

impl Sample {
    pub fn local(&self) -> OffsetDateTime {
        self.offset
            .map_or(self.captured, |o| self.captured.to_offset(o))
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use time::macros::datetime;
    use time::Duration;

    pub fn base() -> OffsetDateTime {
        datetime!(2026-04-03 09:00:00 UTC)
    }

    pub fn sample(id: &str, hours: f64, gps: Option<(f64, f64)>) -> Sample {
        Sample {
            id: MediaId::new(id),
            contributor: ContributorId::new("owner"),
            captured: base() + Duration::seconds((hours * 3600.0) as i64),
            offset: Some(UtcOffset::from_hms(9, 0, 0).unwrap()),
            gps: gps.map(|(lat, lon)| GeoPoint::new(lat, lon)),
            score: 5.0,
            burst_group: None,
            landmark: None,
            dated: true,
        }
    }
}
