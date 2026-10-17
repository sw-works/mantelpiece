//! Day films (F-714): where a trip day's rendered film lives. The marker beside it holds the
//! fingerprint of the plan it was rendered from, its duration and when it was made; a
//! matching fingerprint means the film is up to date.

use crate::ids::{HouseholdId, TripId};

/// The objects of one trip's day films.
#[derive(Clone, Debug)]
pub struct FilmKeys {
    base: String,
}

impl FilmKeys {
    #[must_use]
    pub fn new(household: &HouseholdId, trip: &TripId) -> Self {
        Self {
            base: format!("films/{household}/{trip}"),
        }
    }

    /// The film of day `day`, an H.264 and AAC MP4.
    #[must_use]
    pub fn film(&self, day: u32) -> String {
        format!("{}/day-{day}.mp4", self.base)
    }

    /// The fingerprint and duration the film was rendered with.
    #[must_use]
    pub fn marker(&self, day: u32) -> String {
        format!("{}/day-{day}.fingerprint", self.base)
    }

    /// Every object under the trip: one prefix holds them all.
    #[must_use]
    pub fn prefix(&self) -> String {
        format!("{}/", self.base)
    }

    /// Every film and marker of days `1..=days`, for deleting the trip.
    #[must_use]
    pub fn all(&self, days: u32) -> Vec<String> {
        (1..=days)
            .flat_map(|d| [self.film(d), self.marker(d)])
            .collect()
    }

    /// The marker's content: the fingerprint, the duration in milliseconds, and when the
    /// film was made (RFC 3339), one per line.
    #[must_use]
    pub fn marker_body(fingerprint: &str, duration_ms: u64, made_at: &str) -> Vec<u8> {
        format!("{fingerprint}\n{duration_ms}\n{made_at}").into_bytes()
    }

    /// A marker's fingerprint, duration (0 when unreadable) and time made (empty when
    /// unknown).
    #[must_use]
    pub fn read_marker(body: &[u8]) -> (String, u64, String) {
        let text = String::from_utf8_lossy(body);
        let mut lines = text.lines();
        let f = lines.next().unwrap_or_default().to_owned();
        let d = lines
            .next()
            .and_then(|d| d.trim().parse().ok())
            .unwrap_or(0);
        let m = lines.next().unwrap_or_default().trim().to_owned();
        (f, d, m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_sit_under_one_prefix_outside_originals() {
        let k = FilmKeys::new(&HouseholdId::new("h1"), &TripId::new("t1"));
        assert_eq!(k.film(2), "films/h1/t1/day-2.mp4");
        assert_eq!(k.marker(2), "films/h1/t1/day-2.fingerprint");
        assert!(k.film(2).starts_with(&k.prefix()));
        assert!(!k.film(2).starts_with("originals/"));
    }

    #[test]
    fn marker_round_trips() {
        let body = FilmKeys::marker_body("abc123", 41_240, "2026-10-17T20:01:12Z");
        assert_eq!(
            FilmKeys::read_marker(&body),
            (
                "abc123".to_owned(),
                41_240,
                "2026-10-17T20:01:12Z".to_owned()
            )
        );
        assert_eq!(
            FilmKeys::read_marker(b"only"),
            ("only".to_owned(), 0, String::new())
        );
        let k = FilmKeys::new(&HouseholdId::new("h"), &TripId::new("t"));
        assert_eq!(k.all(2).len(), 4);
    }
}
