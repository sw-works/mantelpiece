//! Trip sketch maps (F-712): where the hand-drawn trip and day maps live. Each map has an
//! owner form, a public form (home end of the route cut) and a fingerprint marker beside
//! them; a matching marker means the pair is up to date.

use crate::ids::{HouseholdId, TripId};

/// Which map of a trip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    Trip,
    Day(u32),
}

impl Which {
    fn stem(self) -> String {
        match self {
            Self::Trip => "trip".to_owned(),
            Self::Day(n) => format!("day-{n}"),
        }
    }
}

/// The objects of one trip's sketch maps.
#[derive(Clone, Debug)]
pub struct SketchKeys {
    base: String,
}

impl SketchKeys {
    #[must_use]
    pub fn new(household: &HouseholdId, trip: &TripId) -> Self {
        Self {
            base: format!("sketch/{household}/{trip}"),
        }
    }

    /// The owner form.
    #[must_use]
    pub fn picture(&self, which: Which) -> String {
        format!("{}/{}.jpg", self.base, which.stem())
    }

    /// The form for member households, shared links and code views.
    #[must_use]
    pub fn public_picture(&self, which: Which) -> String {
        format!("{}/{}-public.jpg", self.base, which.stem())
    }

    /// The fingerprint the pair was drawn from.
    #[must_use]
    pub fn marker(&self, which: Which) -> String {
        format!("{}/{}.fingerprint", self.base, which.stem())
    }

    /// The marker's content: the fingerprint, then the map-data credit on the next line.
    #[must_use]
    pub fn marker_body(fingerprint: &str, credit: &str) -> Vec<u8> {
        format!("{fingerprint}\n{credit}").into_bytes()
    }

    /// A marker's fingerprint and credit.
    #[must_use]
    pub fn read_marker(body: &[u8]) -> (String, String) {
        let text = String::from_utf8_lossy(body);
        let (f, c) = text.split_once('\n').unwrap_or((&text, ""));
        (f.to_owned(), c.to_owned())
    }

    /// Every object of one map.
    #[must_use]
    pub fn of(&self, which: Which) -> [String; 3] {
        [
            self.picture(which),
            self.public_picture(which),
            self.marker(which),
        ]
    }

    /// Every object of the trip map and of days `1..=days`, for deleting the trip.
    #[must_use]
    pub fn all(&self, days: u32) -> Vec<String> {
        std::iter::once(Which::Trip)
            .chain((1..=days).map(Which::Day))
            .flat_map(|w| self.of(w))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_sit_beside_each_other_under_the_trip() {
        let k = SketchKeys::new(&HouseholdId::new("h"), &TripId::new("t"));
        assert_eq!(k.picture(Which::Trip), "sketch/h/t/trip.jpg");
        assert_eq!(
            k.public_picture(Which::Day(3)),
            "sketch/h/t/day-3-public.jpg"
        );
        assert_eq!(k.marker(Which::Day(3)), "sketch/h/t/day-3.fingerprint");
        assert_eq!(k.all(2).len(), 9);
        let body = SketchKeys::marker_body("abc", "© OpenStreetMap contributors");
        assert_eq!(
            SketchKeys::read_marker(&body),
            ("abc".to_owned(), "© OpenStreetMap contributors".to_owned())
        );
    }
}
