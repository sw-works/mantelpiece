//! How much is known about where a photo was taken (spec US2): a dot for its own GPS, a
//! ring for a place inferred from the day's route or a landmark, nothing when unknown.
//! Near home a dot becomes a ring; on shared views every dot does (`Drawing::public`).

use mantelpiece_core::field_note::{Caption, Certainty};
use mantelpiece_core::geo::GeoPoint;
use mantelpiece_core::media::MediaItem;
use mantelpiece_core::trip::Trip;

/// Inside this distance of home no precise marker is shown.
pub const HOME_RADIUS_M: u32 = 2_000;
/// Ring radius bounds for a position estimated between geotagged neighbours.
pub const ESTIMATE_MIN_M: f64 = 300.0;
pub const ESTIMATE_MAX_M: f64 = 5_000.0;

/// The place of `media` within `trip`, or `None` when it is unknown.
#[must_use]
pub fn certainty(media: &MediaItem, trip: &Trip, home: Option<GeoPoint>) -> Option<Certainty> {
    if let Some(at) = media.gps {
        if home.is_some_and(|h| h.distance_m(at) <= f64::from(HOME_RADIUS_M)) {
            return Some(Certainty::Approximate {
                at,
                radius_m: HOME_RADIUS_M,
            });
        }
        return Some(Certainty::Precise { at });
    }
    for day in &trip.days {
        let points = &day.route.points;
        if let Some(i) = points.iter().position(|p| p.media == media.id) {
            let at = points[i].at;
            // The nearer geotagged neighbour bounds how far off the estimate can be.
            let near = points
                .iter()
                .filter(|p| !p.estimated)
                .map(|p| p.at.distance_m(at))
                .fold(f64::INFINITY, f64::min);
            let radius = if near.is_finite() {
                near / 2.0
            } else {
                ESTIMATE_MAX_M
            };
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let radius_m = radius.clamp(ESTIMATE_MIN_M, ESTIMATE_MAX_M) as u32;
            return Some(Certainty::Approximate { at, radius_m });
        }
    }
    trip.days
        .iter()
        .flat_map(|d| d.landmarks.iter())
        .find(|l| l.media.contains(&media.id))
        .map(|l| Certainty::Approximate {
            at: l.at,
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            radius_m: l.reach_m() as u32,
        })
}

/// `35.01° N, 135.77° E`: two decimals, about a kilometre.
#[must_use]
pub fn coords(at: GeoPoint) -> String {
    let ns = if at.lat >= 0.0 { 'N' } else { 'S' };
    let ew = if at.lon >= 0.0 { 'E' } else { 'W' };
    format!("{:.2}° {ns}, {:.2}° {ew}", at.lat.abs(), at.lon.abs())
}

/// The note's caption: the nearest trip landmark within reach, the day's date, and
/// coordinates only for a precise place.
#[must_use]
pub fn caption(media: &MediaItem, trip: &Trip, certainty: &Certainty) -> Caption {
    let at = certainty.at();
    let place = trip
        .days
        .iter()
        .flat_map(|d| d.landmarks.iter())
        .filter(|l| l.at.distance_m(at) <= l.reach_m())
        .min_by(|a, b| a.at.distance_m(at).total_cmp(&b.at.distance_m(at)))
        .map(|l| l.name.clone());
    let date = media
        .captured
        .map(|c| c.local())
        .or_else(|| {
            trip.days
                .iter()
                .find(|d| d.media.contains(&media.id))
                .map(|d| d.date.midnight().assume_utc())
        })
        .map(|t| format!("{} {} {}", t.day(), t.month(), t.year()));
    Caption {
        place,
        date,
        coords: certainty.is_precise().then(|| coords(at)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mantelpiece_core::ids::{HouseholdId, MediaId, TripId};
    use mantelpiece_core::trip::{Landmark, Route, RoutePoint, TripDay};
    use time::macros::{date, datetime};

    fn media(id: &str, gps: Option<GeoPoint>) -> MediaItem {
        let mut m: MediaItem = serde_json::from_value(serde_json::json!({
            "id": id, "household": "h", "contributor": "c", "batch": "b", "kind": "photo",
            "original_key": "k", "original_filename": "f.jpg", "bytes": 1, "sha256": "00",
            "captured": null, "gps": null, "orientation": 1,
            "signals": {"sharpness": 0.0, "exposure": 0.0, "phash": null, "width": null, "height": null},
            "enrichment": {"scene_tags": [], "landmark": null, "face_count": 0, "smile_count": 0, "inherited_from": null},
            "quality_score": null, "burst_group": null, "is_keeper": true, "trip": null,
            "pinned": false, "hidden": false, "state": "enriched", "uploaded_at": "2026-04-03T10:00:00Z"
        }))
        .expect("media fixture");
        m.gps = gps;
        m
    }

    fn trip() -> Trip {
        let pt = |id: &str, lat: f64, estimated: bool| RoutePoint {
            at: GeoPoint::new(lat, 11.25),
            time: datetime!(2026-04-03 10:00 UTC),
            media: MediaId::new(id),
            estimated,
        };
        Trip {
            id: TripId::new("t"),
            household: HouseholdId::new("h"),
            name: "Florence".into(),
            start: datetime!(2026-04-03 08:00 UTC),
            end: datetime!(2026-04-04 20:00 UTC),
            days: vec![TripDay {
                index: 1,
                date: date!(2026 - 04 - 03),
                media: vec![
                    MediaId::new("gps"),
                    MediaId::new("est"),
                    MediaId::new("lm"),
                    MediaId::new("none"),
                ],
                route: Route {
                    points: vec![
                        pt("gps", 43.76, false),
                        pt("est", 43.77, true),
                        pt("far", 43.80, false),
                    ],
                    inferred: false,
                    map_key: None,
                },
                title: None,
                story: None,
                undated: vec![],
                landmarks: vec![Landmark {
                    name: "Ponte Vecchio".into(),
                    at: GeoPoint::new(43.7679, 11.2531),
                    distance_m: 10.0,
                    source: "photo".into(),
                    kind: Some("landmark".into()),
                    dim_m: None,
                    media: vec![MediaId::new("lm")],
                    country: Some("Italy".into()),
                }],
            }],
            route: Route::default(),
            highlights: vec![],
            contributors: vec![],
            clock_offsets: vec![],
            narrative: None,
            hero: None,
            members: vec![],
        }
    }

    #[test]
    fn gps_is_a_dot_an_estimate_or_landmark_a_ring_and_nothing_else_is_nothing() {
        let t = trip();
        let p = certainty(
            &media("gps", Some(GeoPoint::new(43.7679, 11.2531))),
            &t,
            None,
        )
        .unwrap();
        assert!(p.is_precise());
        match certainty(&media("est", None), &t, None).unwrap() {
            Certainty::Approximate { radius_m, .. } => {
                // Nearest geotagged neighbour is about 1.1 km away: half of it.
                assert!((500..=600).contains(&radius_m), "{radius_m}");
            }
            c @ Certainty::Precise { .. } => panic!("{c:?}"),
        }
        assert!(matches!(
            certainty(&media("lm", None), &t, None),
            Some(Certainty::Approximate {
                radius_m: 2_000,
                ..
            })
        ));
        assert!(certainty(&media("none", None), &t, None).is_none());
    }

    #[test]
    fn near_home_a_dot_becomes_a_ring() {
        let home = GeoPoint::new(43.7680, 11.2532);
        let c = certainty(
            &media("gps", Some(GeoPoint::new(43.7679, 11.2531))),
            &trip(),
            Some(home),
        )
        .unwrap();
        assert_eq!(
            c,
            Certainty::Approximate {
                at: GeoPoint::new(43.7679, 11.2531),
                radius_m: HOME_RADIUS_M
            }
        );
    }

    #[test]
    fn coordinates_only_for_a_precise_place_and_rounded() {
        let t = trip();
        let m = media("gps", Some(GeoPoint::new(43.767_93, 11.253_14)));
        let c = caption(&m, &t, &certainty(&m, &t, None).unwrap());
        assert_eq!(c.coords.as_deref(), Some("43.77° N, 11.25° E"));
        assert_eq!(c.place.as_deref(), Some("Ponte Vecchio"));
        assert_eq!(c.date.as_deref(), Some("3 April 2026"));
        let m = media("lm", None);
        let c = caption(&m, &t, &certainty(&m, &t, None).unwrap());
        assert!(c.coords.is_none());
        assert_eq!(coords(GeoPoint::new(-33.857, -70.65)), "33.86° S, 70.65° W");
    }
}
