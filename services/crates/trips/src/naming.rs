//! Auto-names for trips, days and albums (F-208). Names come only from data present in
//! metadata: landmark labels and dates. No invented places.

use std::collections::BTreeMap;
use time::{Date, Month};

/// Most frequent landmark among the given labels, ties broken alphabetically.
pub fn dominant_landmark<'a>(landmarks: impl Iterator<Item = &'a str>) -> Option<String> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for l in landmarks {
        *counts.entry(l).or_default() += 1;
    }
    counts
        .into_iter()
        .min_by_key(|(l, n)| (std::cmp::Reverse(*n), *l))
        .map(|(l, _)| l.to_owned())
}

/// "Kyoto, April 2026" or "Trip, April 3 to 9, 2026" when no landmark is known.
pub fn trip_name(landmark: Option<&str>, start: Date, end: Date) -> String {
    match landmark {
        Some(l) => format!("{l}, {} {}", month_name(start.month()), start.year()),
        None => format!("Trip, {}", date_range(start, end)),
    }
}

/// "Day 3: Kinkaku-ji" or "Day 3".
pub fn day_title(index: u32, landmark: Option<&str>) -> String {
    match landmark {
        Some(l) => format!("Day {index}: {l}"),
        None => format!("Day {index}"),
    }
}

pub fn date_range(start: Date, end: Date) -> String {
    let m = month_name(start.month());
    if start == end {
        format!("{m} {}, {}", start.day(), start.year())
    } else if start.month() == end.month() && start.year() == end.year() {
        format!("{m} {} to {}, {}", start.day(), end.day(), start.year())
    } else if start.year() == end.year() {
        format!(
            "{m} {} to {} {}, {}",
            start.day(),
            month_name(end.month()),
            end.day(),
            start.year()
        )
    } else {
        format!(
            "{m} {}, {} to {} {}, {}",
            start.day(),
            start.year(),
            month_name(end.month()),
            end.day(),
            end.year()
        )
    }
}

fn month_name(m: Month) -> &'static str {
    match m {
        Month::January => "January",
        Month::February => "February",
        Month::March => "March",
        Month::April => "April",
        Month::May => "May",
        Month::June => "June",
        Month::July => "July",
        Month::August => "August",
        Month::September => "September",
        Month::October => "October",
        Month::November => "November",
        Month::December => "December",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    #[test]
    fn names() {
        let start = date!(2026 - 04 - 03);
        let end = date!(2026 - 04 - 09);
        assert_eq!(trip_name(Some("Kyoto"), start, end), "Kyoto, April 2026");
        assert_eq!(trip_name(None, start, end), "Trip, April 3 to 9, 2026");
        assert_eq!(trip_name(None, start, start), "Trip, April 3, 2026");
        assert_eq!(
            date_range(start, date!(2026 - 05 - 02)),
            "April 3 to May 2, 2026"
        );
        assert_eq!(
            date_range(date!(2025 - 12 - 30), date!(2026 - 01 - 02)),
            "December 30, 2025 to January 2, 2026"
        );
        assert_eq!(day_title(3, Some("Kinkaku-ji")), "Day 3: Kinkaku-ji");
        assert_eq!(day_title(4, None), "Day 4");
    }

    #[test]
    fn dominant_landmark_by_count_then_alpha() {
        let d = dominant_landmark(["Fushimi Inari", "Kinkaku-ji", "Fushimi Inari"].into_iter());
        assert_eq!(d.as_deref(), Some("Fushimi Inari"));
        let d = dominant_landmark(["B", "A"].into_iter());
        assert_eq!(d.as_deref(), Some("A"));
        assert!(dominant_landmark(std::iter::empty()).is_none());
    }
}
