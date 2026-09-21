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
    /// The photo had no GPS; its position was interpolated in time between geotagged
    /// neighbours (F-206). Drawn dashed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub estimated: bool,
}

/// A named place along a day's route, looked up from the route's positions (F-206,
/// constitution V: a lookup, not a model). Feeds day titles, the trip name, the map and
/// the facts a story may use.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Landmark {
    pub name: String,
    pub at: GeoPoint,
    /// Metres from the closest route point.
    pub distance_m: f64,
    /// `wikipedia`, `aws`, or `photo` (recognised in the picture itself).
    pub source: String,
    /// Provider's type when it gave one: `city`, `mountain`, `waterbody`, `landmark`, `edu`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Provider's size estimate in metres, when given (a city is 10 km, a church 200 m).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dim_m: Option<u32>,
    /// Photos taken within reach of the place.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<MediaId>,
}

impl Landmark {
    /// How far from the place a photo still counts as taken there: two kilometres for a
    /// building, half the provider's size for a town or a region, thirty at most.
    #[must_use]
    pub fn reach_m(&self) -> f64 {
        let scaled = self.is_region()
            || matches!(
                self.kind.as_deref(),
                Some("mountain" | "waterbody" | "forest" | "glacier")
            );
        let half = if scaled {
            self.dim_m.map_or(0.0, |d| f64::from(d) / 2.0)
        } else {
            0.0
        };
        half.clamp(2_000.0, 30_000.0)
    }

    /// True for a town, an island, a district or a country: the places a day or a trip
    /// is naturally named after.
    #[must_use]
    pub fn is_region(&self) -> bool {
        match self.kind.as_deref() {
            Some("city" | "isle" | "country") => true,
            Some(k) => k.starts_with("adm"),
            None => false,
        }
    }

    /// The place to name a day or a trip after: regions first (the widest one, up to a
    /// hundred kilometres, so a district beats an islet), then nature, then any named
    /// place; most photographed within a tier, the first met on the route on a tie; never
    /// a noise entry.
    #[must_use]
    pub fn best_name<'a>(
        landmarks: impl IntoIterator<Item = &'a Landmark>,
    ) -> Option<&'a Landmark> {
        let key = |l: &Landmark| {
            let width = if l.is_region() {
                l.dim_m.unwrap_or(0).min(100_000)
            } else {
                0
            };
            (l.prominence(), width, l.media.len())
        };
        let mut best: Option<&'a Landmark> = None;
        for l in landmarks.into_iter().filter(|l| l.prominence() >= 4) {
            let better = best.is_none_or(|b| {
                key(l)
                    .cmp(&key(b))
                    .then(b.distance_m.total_cmp(&l.distance_m))
                    == std::cmp::Ordering::Greater
            });
            if better {
                best = Some(l);
            }
        }
        best
    }

    /// Words that mark an article as a building or body nobody names a day after.
    const NOISE: &'static [&'static str] = &[
        "court",
        "hospital",
        "society",
        "bookshop",
        "shop",
        "radio",
        "foundry",
        "drill hall",
        "lifeboat",
        "school",
        "sgoil",
        "constituency",
        "district",
        "diocese",
        "street",
        "road",
        "company",
        "& co",
        "station",
        "club",
        "office",
        "centre",
        "center",
        "council",
    ];

    /// How fit a place is to name a day or a trip: higher is better. Towns, mountains and
    /// waters first, then plain named places, then buildings the provider calls landmarks,
    /// then institutions; anything with a noise word or a parenthesised qualifier last.
    #[must_use]
    pub fn prominence(&self) -> u8 {
        let lower = self.name.to_lowercase();
        if lower.contains('(') || Self::NOISE.iter().any(|w| lower.contains(w)) {
            return 0;
        }
        if self.is_region() {
            return 6;
        }
        match self.kind.as_deref() {
            Some("mountain" | "waterbody" | "river" | "forest" | "glacier" | "pass") => 5,
            None if self.dim_m.is_none_or(|d| d >= 1_000) => 4,
            Some("landmark") => 2,
            // edu, event, airport, railwaystation, camera, or a tiny untyped spot.
            _ => 1,
        }
    }
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
    /// `2025-03-25` on the wire. Rows written before 2026-09-20 hold `[year, ordinal]`,
    /// which still reads.
    #[serde(with = "iso_date")]
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
    /// Places along the day's route, nearest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub landmarks: Vec<Landmark>,
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
    /// Every place along the trip, merged across days with their photos, most
    /// photographed first.
    #[must_use]
    pub fn landmarks(&self) -> Vec<Landmark> {
        let mut all: Vec<Landmark> = Vec::new();
        for l in self.days.iter().flat_map(|d| d.landmarks.iter()) {
            if let Some(seen) = all.iter_mut().find(|s| s.name == l.name) {
                seen.distance_m = seen.distance_m.min(l.distance_m);
                for m in &l.media {
                    if !seen.media.contains(m) {
                        seen.media.push(m.clone());
                    }
                }
            } else {
                all.push(l.clone());
            }
        }
        all.sort_by(|a, b| {
            (b.prominence() > 0)
                .cmp(&(a.prominence() > 0))
                .then(b.media.len().cmp(&a.media.len()))
                .then(b.prominence().cmp(&a.prominence()))
                .then(a.name.cmp(&b.name))
        });
        all
    }

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

/// A calendar date as `YYYY-MM-DD`; also reads the `[year, ordinal]` pair the `time` crate
/// writes by default, which is what older rows hold.
mod iso_date {
    use serde::de::{self, SeqAccess, Visitor};
    use serde::{Deserializer, Serializer};
    use time::{Date, Month};

    // serde's `with` hands the field over by reference.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn serialize<S: Serializer>(date: &Date, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!(
            "{:04}-{:02}-{:02}",
            date.year(),
            u8::from(date.month()),
            date.day()
        ))
    }

    struct DateVisitor;

    impl<'de> Visitor<'de> for DateVisitor {
        type Value = Date;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a YYYY-MM-DD date or a [year, ordinal] pair")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Date, E> {
            let mut parts = v.splitn(3, '-');
            let mut next = || parts.next().and_then(|p| p.parse::<i32>().ok());
            let (Some(y), Some(m), Some(d)) = (next(), next(), next()) else {
                return Err(E::custom(format!("not a date: {v}")));
            };
            let month = u8::try_from(m)
                .ok()
                .and_then(|m| Month::try_from(m).ok())
                .ok_or_else(|| E::custom(format!("not a date: {v}")))?;
            let day = u8::try_from(d).map_err(|_| E::custom(format!("not a date: {v}")))?;
            Date::from_calendar_date(y, month, day).map_err(E::custom)
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Date, A::Error> {
            let year: i32 = seq
                .next_element()?
                .ok_or_else(|| de::Error::custom("missing year"))?;
            let ordinal: u16 = seq
                .next_element()?
                .ok_or_else(|| de::Error::custom("missing ordinal"))?;
            Date::from_ordinal_date(year, ordinal).map_err(de::Error::custom)
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Date, D::Error> {
        d.deserialize_any(DateVisitor)
    }
}

#[cfg(test)]
mod iso_date_tests {
    use super::TripDay;

    #[test]
    fn a_day_writes_an_iso_date_and_still_reads_the_old_pair() {
        let old = serde_json::json!({ "index": 2, "date": [2025, 84], "media": [], "title": null,
            "route": { "points": [], "inferred": false, "map_key": null } });
        let day: TripDay = serde_json::from_value(old).unwrap();
        let wire = serde_json::to_value(&day).unwrap();
        assert_eq!(wire["date"], "2025-03-25");
        let again: TripDay = serde_json::from_value(wire).unwrap();
        assert_eq!(again.date, day.date);
    }
}
