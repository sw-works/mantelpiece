//! Latitude / longitude with the little geometry the pipeline needs.

use serde::{Deserialize, Serialize};

const EARTH_RADIUS_M: f64 = 6_371_008.8;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeoPoint {
    pub lat: f64,
    pub lon: f64,
}

impl GeoPoint {
    pub fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }

    /// Great-circle distance in metres (haversine).
    pub fn distance_m(self, other: GeoPoint) -> f64 {
        let (lat1, lon1) = (self.lat.to_radians(), self.lon.to_radians());
        let (lat2, lon2) = (other.lat.to_radians(), other.lon.to_radians());
        let dlat = lat2 - lat1;
        let dlon = lon2 - lon1;
        let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
        2.0 * EARTH_RADIUS_M * a.sqrt().asin()
    }

    pub fn is_valid(self) -> bool {
        (-90.0..=90.0).contains(&self.lat)
            && (-180.0..=180.0).contains(&self.lon)
            && !(self.lat == 0.0 && self.lon == 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_paris_to_london_roughly_344km() {
        let paris = GeoPoint::new(48.8566, 2.3522);
        let london = GeoPoint::new(51.5074, -0.1278);
        let d = paris.distance_m(london);
        assert!((340_000.0..348_000.0).contains(&d), "{d}");
    }

    #[test]
    fn zero_distance_to_self() {
        let p = GeoPoint::new(35.0, 135.0);
        assert!(p.distance_m(p).abs() < f64::EPSILON);
    }

    #[test]
    fn null_island_is_not_valid() {
        assert!(!GeoPoint::new(0.0, 0.0).is_valid());
        assert!(GeoPoint::new(35.0, 135.0).is_valid());
        assert!(!GeoPoint::new(95.0, 0.0).is_valid());
    }
}
