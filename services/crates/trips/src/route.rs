//! Route from GPS (F-206): ordered points, outliers dropped, dense runs thinned.
//! Without GPS, a route can be inferred from landmark positions in capture order and is
//! flagged as such.

use crate::Sample;
use mantelpiece_core::geo::GeoPoint;
use mantelpiece_core::trip::{Route, RoutePoint};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// Implied speed above this between consecutive fixes marks the later fix as an outlier.
    pub max_speed_mps: f64,
    /// Consecutive points closer than this collapse into one.
    pub min_step_m: f64,
    /// A photo without GPS takes an interpolated position when a geotagged photo lies
    /// within this much time on each side (or on one side, then it snaps to that fix).
    pub estimate_within: time::Duration,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            max_speed_mps: 300.0,
            min_step_m: 50.0,
            estimate_within: time::Duration::hours(2),
        }
    }
}

/// Give photos without GPS a position from their geotagged neighbours in time (F-206):
/// linear interpolation between the nearest fix before and after when both are within
/// `estimate_within`; the one fix when only one side has it. Cameras without GPS on a
/// trip with a phone get a route this way. Marks `gps_estimated`; the caller draws those
/// points dashed and never uses them for clock alignment.
pub fn estimate_positions(samples: &mut [Sample], params: Params) -> usize {
    let mut fixes: Vec<(time::OffsetDateTime, GeoPoint)> = samples
        .iter()
        .filter(|s| s.dated && !s.gps_estimated)
        .filter_map(|s| s.gps.map(|g| (s.captured, g)))
        .collect();
    fixes.sort_by_key(|(t, _)| *t);
    if fixes.is_empty() {
        return 0;
    }
    let mut estimated = 0;
    for s in samples.iter_mut().filter(|s| s.dated && s.gps.is_none()) {
        let after = fixes.partition_point(|(t, _)| *t <= s.captured);
        let before = after.checked_sub(1).map(|i| fixes[i]);
        let after = fixes.get(after).copied();
        let near = |f: Option<(time::OffsetDateTime, GeoPoint)>| {
            f.filter(|(t, _)| (*t - s.captured).abs() <= params.estimate_within)
        };
        let at = match (near(before), near(after)) {
            (Some((t0, a)), Some((t1, b))) => {
                let span = (t1 - t0).as_seconds_f64();
                let k = if span <= 0.0 {
                    0.0
                } else {
                    (s.captured - t0).as_seconds_f64() / span
                };
                GeoPoint::new(a.lat + (b.lat - a.lat) * k, a.lon + (b.lon - a.lon) * k)
            }
            (Some((_, a)), None) | (None, Some((_, a))) => a,
            (None, None) => continue,
        };
        s.gps = Some(at);
        s.gps_estimated = true;
        estimated += 1;
    }
    estimated
}

/// Build from geotagged samples in capture order. Non-geotagged samples are skipped.
pub fn from_gps(samples: &[Sample], params: Params) -> Route {
    let mut sorted: Vec<&Sample> = samples.iter().filter(|s| s.gps.is_some()).collect();
    sorted.sort_by_key(|s| s.captured);

    let mut points: Vec<RoutePoint> = Vec::new();
    for s in sorted {
        let at = s.gps.expect("filtered");
        if let Some(prev) = points.last() {
            let dist = prev.at.distance_m(at);
            let secs = (s.time_since(prev.time)).max(1.0);
            if dist / secs > params.max_speed_mps {
                continue;
            }
            if dist < params.min_step_m {
                // Same spot: a real fix replaces an estimate, anything else is dropped.
                if prev.estimated && !s.gps_estimated {
                    points.pop();
                } else {
                    continue;
                }
            }
        }
        points.push(RoutePoint {
            at,
            time: s.captured,
            media: s.id.clone(),
            estimated: s.gps_estimated,
        });
    }
    Route {
        points,
        inferred: false,
        map_key: None,
    }
}

/// Build from already-geocoded landmark positions when GPS is missing. Each entry is a
/// sample paired with where its landmark is. Flagged `inferred`.
pub fn from_landmarks(located: &[(&Sample, GeoPoint)], params: Params) -> Route {
    let mut ordered: Vec<&(&Sample, GeoPoint)> = located.iter().collect();
    ordered.sort_by_key(|(s, _)| s.captured);
    let mut points: Vec<RoutePoint> = Vec::new();
    for (s, at) in ordered {
        if points
            .last()
            .is_some_and(|p| p.at.distance_m(*at) < params.min_step_m)
        {
            continue;
        }
        points.push(RoutePoint {
            at: *at,
            time: s.captured,
            media: s.id.clone(),
            estimated: true,
        });
    }
    Route {
        points,
        inferred: true,
        map_key: None,
    }
}

/// Up to `max` positions spread evenly along the route by distance, first and last
/// included, never closer than `min_step_m`: where to ask "what is here" so a long day
/// gets named along its whole length, not just its first kilometres.
pub fn probes(route: &Route, max: usize, min_step_m: f64) -> Vec<GeoPoint> {
    let pts = &route.points;
    let (Some(first), Some(last)) = (pts.first(), pts.last()) else {
        return Vec::new();
    };
    if max <= 1 || pts.len() == 1 {
        return vec![first.at];
    }
    let total = length_m(route);
    #[allow(clippy::cast_precision_loss)]
    let spacing = (total / (max - 1) as f64).max(min_step_m);
    let mut out = vec![first.at];
    let mut walked = 0.0;
    let mut next_at = spacing;
    for w in pts.windows(2) {
        walked += w[0].at.distance_m(w[1].at);
        if walked >= next_at && out.len() < max - 1 {
            out.push(w[1].at);
            next_at = walked + spacing;
        }
    }
    if out
        .last()
        .is_some_and(|p| p.distance_m(last.at) >= min_step_m)
        && out.len() < max
    {
        out.push(last.at);
    }
    out
}

/// Total path length in metres.
pub fn length_m(route: &Route) -> f64 {
    route
        .points
        .windows(2)
        .map(|w| w[0].at.distance_m(w[1].at))
        .sum()
}

impl Sample {
    fn time_since(&self, earlier: time::OffsetDateTime) -> f64 {
        (self.captured - earlier).as_seconds_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::sample;

    #[test]
    fn positions_are_interpolated_between_nearby_fixes_only() {
        let mut samples = vec![
            sample("a", 0.0, Some((35.0, 135.0))),
            sample("b", 0.5, None), // halfway in time: halfway in space
            sample("c", 1.0, Some((35.2, 135.4))),
            sample("d", 1.5, None), // only a fix before, within reach: snaps to c
            sample("e", 9.0, None), // nothing within two hours: stays unplaced
            sample("f", 20.0, Some((36.0, 136.0))),
        ];
        assert_eq!(estimate_positions(&mut samples, Params::default()), 2);
        let b = samples[1].gps.unwrap();
        assert!((b.lat - 35.1).abs() < 1e-9 && (b.lon - 135.2).abs() < 1e-9);
        assert!(samples[1].gps_estimated);
        assert_eq!(samples[3].gps, Some(GeoPoint::new(35.2, 135.4)));
        assert!(samples[4].gps.is_none());
        let r = from_gps(&samples, Params::default());
        let flags: Vec<(&str, bool)> = r
            .points
            .iter()
            .map(|p| (p.media.as_str(), p.estimated))
            .collect();
        assert_eq!(
            flags,
            vec![("a", false), ("b", true), ("c", false), ("f", false)]
        );
    }

    #[test]
    fn probes_spread_along_the_whole_route() {
        // Ten fixes 10 km apart along a meridian: 90 km.
        let samples: Vec<Sample> = (0..10)
            .map(|i| {
                sample(
                    &format!("p{i}"),
                    f64::from(i),
                    Some((35.0 + f64::from(i) * 0.09, 135.0)),
                )
            })
            .collect();
        let r = from_gps(&samples, Params::default());
        let p = probes(&r, 4, 1_000.0);
        assert_eq!(p.len(), 4);
        assert!((p[0].lat - 35.0).abs() < 1e-9);
        assert!((p[3].lat - 35.81).abs() < 1e-9, "{p:?}");
        assert!(p[1].lat > 35.2 && p[2].lat > p[1].lat + 0.2, "{p:?}");
        assert_eq!(probes(&r, 1, 1_000.0).len(), 1);
        assert!(probes(&Route::default(), 4, 1_000.0).is_empty());
    }

    #[test]
    fn a_real_fix_replaces_an_estimate_at_the_same_spot() {
        let mut samples = vec![
            sample("cam", 0.0, None),
            sample("phone", 0.1, Some((35.0, 135.0))),
        ];
        assert_eq!(estimate_positions(&mut samples, Params::default()), 1);
        let r = from_gps(&samples, Params::default());
        assert_eq!(r.points.len(), 1);
        assert_eq!(r.points[0].media.as_str(), "phone");
        assert!(!r.points[0].estimated);
    }

    #[test]
    fn thins_dense_points_and_drops_teleports() {
        let samples = vec![
            sample("a", 0.0, Some((35.0000, 135.0000))),
            sample("b", 0.01, Some((35.0001, 135.0000))), // 11 m: thinned
            sample("c", 0.5, Some((35.0100, 135.0000))),  // ~1.1 km in 30 min: fine
            sample("d", 0.51, Some((40.0000, 140.0000))), // teleport: dropped
            sample("e", 1.0, Some((35.0200, 135.0000))),
            sample("f", 1.5, None),
        ];
        let r = from_gps(&samples, Params::default());
        let ids: Vec<&str> = r.points.iter().map(|p| p.media.as_str()).collect();
        assert_eq!(ids, vec!["a", "c", "e"]);
        assert!(!r.inferred);
        assert!((length_m(&r) - 2_224.0).abs() < 50.0, "{}", length_m(&r));
    }

    #[test]
    fn landmark_route_is_flagged_inferred() {
        let a = sample("a", 0.0, None);
        let b = sample("b", 2.0, None);
        let r = from_landmarks(
            &[
                (&b, GeoPoint::new(35.0394, 135.7292)),
                (&a, GeoPoint::new(35.0116, 135.7681)),
            ],
            Params::default(),
        );
        assert!(r.inferred);
        assert_eq!(r.points[0].media.as_str(), "a");
        assert_eq!(r.points.len(), 2);
    }

    #[test]
    fn empty_route() {
        let r = from_gps(&[sample("a", 0.0, None)], Params::default());
        assert!(r.points.is_empty());
        assert!(length_m(&r).abs() < f64::EPSILON);
    }
}
