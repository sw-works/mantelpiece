//! Memories that resurface (F-701 to F-703): pure queries over the household's media.
//! On this day, Then and now, and best of a month or year. Keepers only, nothing hidden
//! or removed, ranked by the score the pipeline already computed (constitution V).

use mantelpiece_core::geo::GeoPoint;
use mantelpiece_core::ids::{HouseholdId, MediaId};
use mantelpiece_core::media::MediaItem;
use std::collections::BTreeMap;
use time::{Date, Month};

pub const ON_THIS_DAY_LIMIT: usize = 12;
pub const BEST_OF_LIMIT: usize = 24;
/// Two photos of the same place: within this radius (metres).
pub const SAME_PLACE_M: f64 = 500.0;
/// And at least this far apart in time.
pub const MIN_YEARS_APART: i32 = 1;

fn eligible<'a>(
    media: &'a [MediaItem],
    viewer: &'a HouseholdId,
) -> impl Iterator<Item = &'a MediaItem> {
    media
        .iter()
        .filter(move |m| m.is_keeper && !m.is_removed() && !m.hidden_for(viewer))
}

fn score(m: &MediaItem) -> f32 {
    m.quality_score.unwrap_or(0.0)
}

fn local_date(m: &MediaItem) -> Option<Date> {
    m.captured.map(|c| c.local().date())
}

/// Photos taken on `date`'s month and day in an earlier year, best first (F-701).
pub fn on_this_day(media: &[MediaItem], viewer: &HouseholdId, date: Date) -> Vec<MediaId> {
    let mut hits: Vec<&MediaItem> = eligible(media, viewer)
        .filter(|m| {
            local_date(m).is_some_and(|d| {
                d.month() == date.month() && d.day() == date.day() && d.year() < date.year()
            })
        })
        .collect();
    hits.sort_by(|a, b| score(b).total_cmp(&score(a)).then_with(|| a.id.cmp(&b.id)));
    hits.into_iter()
        .take(ON_THIS_DAY_LIMIT)
        .map(|m| m.id.clone())
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThenAndNow {
    pub then: MediaId,
    pub now: MediaId,
    pub place: GeoPoint,
    pub years_apart: i32,
}

/// Cell key about 1 km wide, so nearby photos land together before the distance check.
fn cell(p: GeoPoint) -> (i64, i64) {
    (
        (p.lat * 100.0).floor() as i64,
        (p.lon * 100.0).floor() as i64,
    )
}

/// Pairs of photos of the same place at least a year apart (F-702): the best photo from
/// the earliest year and the best from the latest, one pair per place.
pub fn then_and_now(media: &[MediaItem], viewer: &HouseholdId) -> Vec<ThenAndNow> {
    let mut by_cell: BTreeMap<(i64, i64), Vec<&MediaItem>> = BTreeMap::new();
    for m in eligible(media, viewer) {
        if let (Some(g), Some(_)) = (m.gps, local_date(m)) {
            if g.is_valid() {
                by_cell.entry(cell(g)).or_default().push(m);
            }
        }
    }
    let mut out = Vec::new();
    for (_, group) in by_cell {
        let mut sorted = group;
        sorted.sort_by_key(|m| local_date(m));
        let (Some(first), Some(last)) = (sorted.first(), sorted.last()) else {
            continue;
        };
        let (Some(fd), Some(ld)) = (local_date(first), local_date(last)) else {
            continue;
        };
        let years = ld.year() - fd.year();
        if years < MIN_YEARS_APART {
            continue;
        }
        let anchor = first.gps.expect("filtered");
        let near = |m: &MediaItem| m.gps.is_some_and(|g| g.distance_m(anchor) <= SAME_PLACE_M);
        let best = |year: i32| {
            sorted
                .iter()
                .filter(|m| near(m))
                .filter(|m| local_date(m).is_some_and(|d| d.year() == year))
                .max_by(|a, b| score(a).total_cmp(&score(b)))
                .map(|m| m.id.clone())
        };
        if let (Some(then), Some(now)) = (best(fd.year()), best(ld.year())) {
            out.push(ThenAndNow {
                then,
                now,
                place: anchor,
                years_apart: years,
            });
        }
    }
    out.sort_by(|a, b| {
        b.years_apart
            .cmp(&a.years_apart)
            .then_with(|| a.then.cmp(&b.then))
    });
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    Month { year: i32, month: Month },
    Year { year: i32 },
}

/// The best photos of a month or a year, one per burst group (F-703).
pub fn best_of(media: &[MediaItem], viewer: &HouseholdId, period: Period) -> Vec<MediaId> {
    let mut hits: Vec<&MediaItem> = eligible(media, viewer)
        .filter(|m| {
            local_date(m).is_some_and(|d| match period {
                Period::Month { year, month } => d.year() == year && d.month() == month,
                Period::Year { year } => d.year() == year,
            })
        })
        .collect();
    hits.sort_by(|a, b| score(b).total_cmp(&score(a)).then_with(|| a.id.cmp(&b.id)));
    let mut seen_groups = std::collections::BTreeSet::new();
    hits.into_iter()
        .filter(|m| {
            m.burst_group
                .as_ref()
                .is_none_or(|g| seen_groups.insert(g.clone()))
        })
        .take(BEST_OF_LIMIT)
        .map(|m| m.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mantelpiece_core::ids::{BatchId, ContributorId};
    use mantelpiece_core::media::{CaptureTime, Enrichment, MediaKind, ProcessingState, Signals};
    use std::collections::BTreeSet;
    use time::macros::datetime;
    use time::OffsetDateTime;

    fn photo(id: &str, at: OffsetDateTime, gps: Option<(f64, f64)>, score: f32) -> MediaItem {
        MediaItem {
            id: MediaId::new(id),
            household: HouseholdId::new("h"),
            contributor: ContributorId::new("c"),
            batch: BatchId::new("b"),
            kind: MediaKind::Photo,
            original_key: String::new(),
            original_filename: String::new(),
            bytes: 0,
            sha256: String::new(),
            captured: Some(CaptureTime::new(at, None)),
            gps: gps.map(|(a, b)| GeoPoint::new(a, b)),
            orientation: 1,
            signals: Signals::default(),
            enrichment: Enrichment::default(),
            quality_score: Some(score),
            burst_group: None,
            is_keeper: true,
            trip: None,
            pinned: false,
            hidden: false,
            pinned_by: BTreeSet::new(),
            hidden_by: BTreeSet::new(),
            text: None,
            description: None,
            cleanup_reason: None,
            people: vec![],
            removed_at: None,
            restore_until: None,
            state: ProcessingState::Enriched,
            uploaded_at: at,
        }
    }

    #[test]
    fn on_this_day_matches_month_and_day_in_earlier_years_best_first() {
        let h = HouseholdId::new("h");
        let media = vec![
            photo("a", datetime!(2024-04-04 10:00:00 UTC), None, 6.0),
            photo("b", datetime!(2025-04-04 10:00:00 UTC), None, 8.0),
            photo("c", datetime!(2026-04-04 10:00:00 UTC), None, 9.0), // today itself
            photo("d", datetime!(2025-04-05 10:00:00 UTC), None, 9.0),
        ];
        let mut hidden = photo("e", datetime!(2023-04-04 10:00:00 UTC), None, 9.9);
        hidden.hidden = true;
        let mut media = media;
        media.push(hidden);
        let ids = on_this_day(&media, &h, time::macros::date!(2026 - 04 - 04));
        assert_eq!(ids, vec![MediaId::new("b"), MediaId::new("a")]);
    }

    #[test]
    fn then_and_now_pairs_the_same_place_a_year_apart() {
        let h = HouseholdId::new("h");
        let kinkakuji = Some((35.0394, 135.7292));
        let media = vec![
            photo("then1", datetime!(2024-04-04 10:00:00 UTC), kinkakuji, 5.0),
            photo(
                "then2",
                datetime!(2024-04-04 10:05:00 UTC),
                Some((35.0395, 135.7293)),
                7.0,
            ),
            photo("now", datetime!(2026-04-04 10:00:00 UTC), kinkakuji, 8.0),
            photo(
                "elsewhere",
                datetime!(2026-04-04 10:00:00 UTC),
                Some((38.7, -9.1)),
                8.0,
            ),
            photo(
                "same-year",
                datetime!(2026-05-04 10:00:00 UTC),
                Some((38.7, -9.1)),
                8.0,
            ),
        ];
        let pairs = then_and_now(&media, &h);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].then, MediaId::new("then2"));
        assert_eq!(pairs[0].now, MediaId::new("now"));
        assert_eq!(pairs[0].years_apart, 2);
    }

    #[test]
    fn best_of_period_dedups_bursts() {
        let h = HouseholdId::new("h");
        let mut a = photo("a", datetime!(2026-04-04 10:00:00 UTC), None, 9.0);
        a.burst_group = Some("g".into());
        let mut b = photo("b", datetime!(2026-04-04 10:00:01 UTC), None, 8.0);
        b.burst_group = Some("g".into());
        let media = vec![
            a,
            b,
            photo("c", datetime!(2026-04-20 10:00:00 UTC), None, 7.0),
            photo("d", datetime!(2026-05-01 10:00:00 UTC), None, 9.5),
            photo("e", datetime!(2025-04-01 10:00:00 UTC), None, 9.9),
        ];
        assert_eq!(
            best_of(
                &media,
                &h,
                Period::Month {
                    year: 2026,
                    month: Month::April
                }
            ),
            vec![MediaId::new("a"), MediaId::new("c")]
        );
        assert_eq!(
            best_of(&media, &h, Period::Year { year: 2026 }),
            vec![MediaId::new("d"), MediaId::new("a"), MediaId::new("c")]
        );
    }
}
