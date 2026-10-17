//! When a day film is made (F-714): at 21:00 local time on that day, or ten minutes after
//! new photos of a day already past arrive, for days no more than two days old. Older days
//! get a film only on request.

use mantelpiece_core::ids::{HouseholdId, TripId};
use mantelpiece_core::media::MediaItem;
use mantelpiece_core::trip::TripDay;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use time::{Duration, OffsetDateTime, Time, UtcOffset};

/// Local time the day's film is made.
pub const END_OF_DAY: Time = time::macros::time!(21:00);
/// How long after new photos of a finished day the film is (re)made: uploads settle first.
pub const SETTLE: Duration = Duration::minutes(10);
/// Days older than this get a film only on request.
pub const MAX_AGE_DAYS: i64 = 2;

/// The UTC offset most of the day's photos were taken in; UTC when none says.
#[must_use]
pub fn day_offset(day: &TripDay, media: &[MediaItem]) -> UtcOffset {
    let ids: std::collections::HashSet<_> = day.media.iter().collect();
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for m in media.iter().filter(|m| ids.contains(&m.id)) {
        if let Some(s) = m.captured.and_then(|c| c.offset_seconds) {
            *counts.entry(s).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .max_by_key(|&(s, n)| (n, -s.abs()))
        .and_then(|(s, _)| UtcOffset::from_whole_seconds(s).ok())
        .unwrap_or(UtcOffset::UTC)
}

/// When the day's film should be made, or `None` for a day too old to schedule.
#[must_use]
pub fn film_time(
    day: &TripDay,
    media: &[MediaItem],
    now: OffsetDateTime,
) -> Option<OffsetDateTime> {
    let offset = day_offset(day, media);
    let today = now.to_offset(offset).date();
    if (today - day.date).whole_days() > MAX_AGE_DAYS {
        return None;
    }
    let end = day.date.with_time(END_OF_DAY).assume_offset(offset);
    Some(end.max(now + SETTLE).to_offset(UtcOffset::UTC))
}

/// Scheduler name of a day's film: `film-` (the prefix IAM allows), a hash of the trip, and
/// the day; within Scheduler's 64 characters and character set.
#[must_use]
pub fn schedule_name(household: &HouseholdId, trip: &TripId, day: u32) -> String {
    let digest = Sha256::digest(format!("{household}/{trip}").as_bytes());
    format!("film-{}-day-{day}", hex::encode(&digest[..12]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mantelpiece_core::ids::MediaId;
    use mantelpiece_core::media::CaptureTime;
    use mantelpiece_core::trip::Route;
    use time::macros::{date, datetime, offset};

    fn day(date: time::Date, ids: &[&str]) -> TripDay {
        TripDay {
            index: 1,
            date,
            media: ids.iter().map(|s| MediaId::new(*s)).collect(),
            route: Route::default(),
            title: None,
            story: None,
            undated: vec![],
            landmarks: vec![],
        }
    }

    fn taken(id: &str, at: OffsetDateTime, off: Option<UtcOffset>) -> MediaItem {
        let mut m: MediaItem = serde_json::from_value(serde_json::json!({
            "id": id, "household": "h", "contributor": "c", "batch": "b", "kind": "photo",
            "original_key": "k", "original_filename": "f.jpg", "bytes": 1, "sha256": "s",
            "captured": null, "gps": null, "orientation": 1, "signals": {}, "enrichment": {
                "scene_tags": [], "landmark": null, "face_count": 0, "smile_count": 0,
                "inherited_from": null },
            "quality_score": null, "burst_group": null, "is_keeper": true, "trip": null,
            "pinned": false, "hidden": false, "state": "enriched",
            "uploaded_at": "2026-03-25T10:00:00Z"
        }))
        .unwrap();
        m.captured = Some(CaptureTime::new(at, off));
        m
    }

    #[test]
    fn a_day_in_progress_is_filmed_at_nine_local() {
        let media = vec![
            taken("a", datetime!(2026-03-25 13:00 UTC), Some(offset!(+1))),
            taken("b", datetime!(2026-03-25 14:00 UTC), Some(offset!(+1))),
        ];
        let d = day(date!(2026 - 03 - 25), &["a", "b"]);
        let t = film_time(&d, &media, datetime!(2026-03-25 15:00 UTC)).unwrap();
        assert_eq!(t, datetime!(2026-03-25 20:00 UTC), "21:00 at +01:00");
    }

    #[test]
    fn a_finished_day_is_filmed_after_uploads_settle_and_old_days_are_left() {
        let media = vec![taken("a", datetime!(2026-03-24 10:00 UTC), None)];
        let d = day(date!(2026 - 03 - 24), &["a"]);
        let now = datetime!(2026-03-25 08:00 UTC);
        assert_eq!(film_time(&d, &media, now), Some(now + SETTLE));
        assert_eq!(film_time(&d, &media, datetime!(2026-03-30 08:00 UTC)), None);
    }

    #[test]
    fn names_fit_the_scheduler_and_keep_the_prefix() {
        let n = schedule_name(
            &HouseholdId::new("amzn1.account.ABCDEFGHIJKLMNOPQRSTUVWXYZ"),
            &TripId::new("t-0123456789abcdef"),
            12,
        );
        assert!(n.starts_with("film-") && n.len() <= 64, "{n}");
        assert!(n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
        assert_ne!(
            n,
            schedule_name(&HouseholdId::new("h"), &TripId::new("t"), 12)
        );
    }
}
