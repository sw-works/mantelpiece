//! Route from GPS (F-206): ordered points, outliers dropped, dense runs thinned.
//! Without GPS, a route can be inferred from landmark positions in capture order and is
//! flagged as such.

use crate::Sample;
use memorylane_core::geo::GeoPoint;
use memorylane_core::trip::{Route, RoutePoint};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// Implied speed above this between consecutive fixes marks the later fix as an outlier.
    pub max_speed_mps: f64,
    /// Consecutive points closer than this collapse into one.
    pub min_step_m: f64,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            max_speed_mps: 300.0,
            min_step_m: 50.0,
        }
    }
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
                continue;
            }
        }
        points.push(RoutePoint {
            at,
            time: s.captured,
            media: s.id.clone(),
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
        });
    }
    Route {
        points,
        inferred: true,
        map_key: None,
    }
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
