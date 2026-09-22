//! Static maps for trips and days (F-207) and the all-trips map (F-707) through Amazon
//! Location Service. The route itself is computed in `mantelpiece-trips`; this only draws.

use async_trait::async_trait;
use mantelpiece_core::geo::GeoPoint;
use std::sync::Mutex;

use crate::{Result, StorageError};

#[async_trait]
pub trait Maps: Send + Sync {
    /// PNG of a polyline through `points` in order.
    async fn route_map(&self, points: &[GeoPoint], width: u32, height: u32) -> Result<Vec<u8>>;
    /// PNG with one pin per point, framed to fit them all.
    async fn pins_map(&self, pins: &[GeoPoint], width: u32, height: u32) -> Result<Vec<u8>>;
}

/// Records every call and returns a tiny valid-looking PNG header so tests can store it.
#[derive(Debug, Default)]
pub struct MemoryMaps {
    pub log: Mutex<Vec<String>>,
}

const PNG_HEADER: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

#[async_trait]
impl Maps for MemoryMaps {
    async fn route_map(&self, points: &[GeoPoint], width: u32, height: u32) -> Result<Vec<u8>> {
        self.log
            .lock()
            .unwrap()
            .push(format!("route {} {width}x{height}", points.len()));
        Ok(PNG_HEADER.to_vec())
    }
    async fn pins_map(&self, pins: &[GeoPoint], width: u32, height: u32) -> Result<Vec<u8>> {
        self.log
            .lock()
            .unwrap()
            .push(format!("pins {} {width}x{height}", pins.len()));
        Ok(PNG_HEADER.to_vec())
    }
}

fn backend<E: std::fmt::Debug>(e: E) -> StorageError {
    StorageError::Backend(format!("{e:?}"))
}

/// Amazon Location Service Maps (v2) `GetStaticMap` with a GeoJSON overlay.
/// Compile-checked only.
#[derive(Debug, Clone)]
pub struct LocationMaps {
    client: aws_sdk_geomaps::Client,
}

impl LocationMaps {
    pub fn from_config(config: &aws_config::SdkConfig) -> Self {
        Self {
            client: aws_sdk_geomaps::Client::new(config),
        }
    }

    /// Amazon Location takes at most 14 decimals; EXIF coordinates print up to 17. Six is
    /// about 10 cm.
    fn coord(v: f64) -> String {
        let s = format!("{v:.6}");
        let s = s.trim_end_matches('0').trim_end_matches('.');
        if s == "-0" {
            "0".to_owned()
        } else {
            s.to_owned()
        }
    }

    fn pair(p: &GeoPoint) -> String {
        format!("{},{}", Self::coord(p.lon), Self::coord(p.lat))
    }

    /// `lon,lat,lon,lat…`: one comma-separated list.
    fn bounded(points: &[GeoPoint]) -> String {
        points.iter().map(Self::pair).collect::<Vec<_>>().join(",")
    }

    /// The overlay rides in the request URL, so a long route is thinned to this many
    /// points, first and last kept.
    const MAX_OVERLAY_POINTS: usize = 120;

    fn thinned(points: &[GeoPoint]) -> Vec<GeoPoint> {
        if points.len() <= Self::MAX_OVERLAY_POINTS {
            return points.to_vec();
        }
        let last = points.len() - 1;
        (0..Self::MAX_OVERLAY_POINTS)
            .map(|i| points[i * last / (Self::MAX_OVERLAY_POINTS - 1)])
            .collect()
    }

    fn overlay(points: &[GeoPoint], line: bool) -> String {
        let round = |v: f64| (v * 1e6).round() / 1e6;
        let coords: Vec<serde_json::Value> = points
            .iter()
            .map(|p| serde_json::json!([round(p.lon), round(p.lat)]))
            .collect();
        let features = if line {
            vec![serde_json::json!({
                "type": "Feature",
                "properties": {"stroke": "#e07a5f", "stroke-width": 4},
                "geometry": {"type": "LineString", "coordinates": coords}
            })]
        } else {
            coords
                .into_iter()
                .map(|c| {
                    serde_json::json!({
                        "type": "Feature",
                        "properties": {"marker-color": "#e07a5f"},
                        "geometry": {"type": "Point", "coordinates": c}
                    })
                })
                .collect()
        };
        serde_json::json!({"type": "FeatureCollection", "features": features}).to_string()
    }

    async fn render(
        &self,
        points: &[GeoPoint],
        width: u32,
        height: u32,
        line: bool,
    ) -> Result<Vec<u8>> {
        if points.is_empty() {
            return Err(StorageError::Backend("no points to map".into()));
        }
        let points = Self::thinned(points);
        // The file name is fixed by the service: `map` or `map@2x`.
        let request = self
            .client
            .get_static_map()
            .file_name("map")
            .width(i32::try_from(width).unwrap_or(1280))
            .height(i32::try_from(height).unwrap_or(720))
            .style(aws_sdk_geomaps::types::StaticMapStyle::Standard)
            .geo_json_overlay(Self::overlay(&points, line && points.len() > 1));
        // Bounds need two distinct positions; a day spent in one spot is a centred map.
        let distinct = points
            .iter()
            .any(|p| Self::pair(p) != Self::pair(&points[0]));
        let request = if distinct {
            request
                .bounded_positions(Self::bounded(&points))
                .padding(40)
        } else {
            request.center(Self::pair(&points[0])).zoom(13.0)
        };
        let out = request.send().await.map_err(backend)?;
        Ok(out.blob().map(|b| b.as_ref().to_vec()).unwrap_or_default())
    }
}

#[async_trait]
impl Maps for LocationMaps {
    async fn route_map(&self, points: &[GeoPoint], width: u32, height: u32) -> Result<Vec<u8>> {
        self.render(points, width, height, true).await
    }
    async fn pins_map(&self, pins: &[GeoPoint], width: u32, height: u32) -> Result<Vec<u8>> {
        self.render(pins, width, height, false).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_is_geojson_in_lon_lat_order() {
        let pts = vec![GeoPoint::new(35.0, 135.7), GeoPoint::new(35.1, 135.8)];
        assert_eq!(LocationMaps::bounded(&pts), "135.7,35,135.8,35.1");
        // EXIF coordinates carry more decimals than Amazon Location accepts.
        let exif = vec![GeoPoint::new(
            37.774_929_123_456_789,
            -122.419_415_555_555_55,
        )];
        assert_eq!(LocationMaps::bounded(&exif), "-122.419416,37.774929");
        let long: Vec<GeoPoint> = (0..500)
            .map(|i| GeoPoint::new(35.0 + f64::from(i) * 0.001, 135.0))
            .collect();
        let thin = LocationMaps::thinned(&long);
        assert_eq!(thin.len(), LocationMaps::MAX_OVERLAY_POINTS);
        assert_eq!(thin[0], long[0]);
        assert_eq!(thin.last(), long.last());
        let line: serde_json::Value =
            serde_json::from_str(&LocationMaps::overlay(&pts, true)).unwrap();
        assert_eq!(line["features"][0]["geometry"]["type"], "LineString");
        assert_eq!(line["features"][0]["geometry"]["coordinates"][0][0], 135.7);
        let pins: serde_json::Value =
            serde_json::from_str(&LocationMaps::overlay(&pts, false)).unwrap();
        assert_eq!(pins["features"].as_array().unwrap().len(), 2);
    }
}
