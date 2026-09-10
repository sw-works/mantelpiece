//! What a TV is showing right now, so a voice question can refer to "that statue" (R3).

use crate::ids::{DeviceId, HouseholdId, MediaId, TripId};
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

/// A record older than this counts as nothing showing.
pub const STALE_AFTER: Duration = Duration::minutes(10);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presence {
    pub household: HouseholdId,
    pub device_id: DeviceId,
    pub media: MediaId,
    pub trip: Option<TripId>,
    pub day: Option<u32>,
    pub paused: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
}

impl Presence {
    pub fn is_fresh(&self, now: OffsetDateTime) -> bool {
        now - self.at < STALE_AFTER
    }
}

/// The freshest presence among a household's TVs, or none when every record is stale.
pub fn freshest(records: &[Presence], now: OffsetDateTime) -> Option<&Presence> {
    records
        .iter()
        .filter(|p| p.is_fresh(now))
        .max_by_key(|p| p.at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn presence(device: &str, at: OffsetDateTime) -> Presence {
        Presence {
            household: HouseholdId::new("h"),
            device_id: DeviceId::new(device),
            media: MediaId::new("m"),
            trip: Some(TripId::new("t")),
            day: Some(2),
            paused: true,
            at,
        }
    }

    #[test]
    fn stale_records_do_not_count() {
        let now = datetime!(2026-09-08 10:00:00 UTC);
        let living = presence("tv-1", now - Duration::minutes(3));
        let bedroom = presence("tv-2", now - Duration::minutes(11));
        assert!(living.is_fresh(now));
        assert!(!bedroom.is_fresh(now));
        let records = vec![bedroom, living.clone()];
        assert_eq!(freshest(&records, now), Some(&living));
        assert_eq!(freshest(&records, now + Duration::minutes(8)), None);
    }
}
