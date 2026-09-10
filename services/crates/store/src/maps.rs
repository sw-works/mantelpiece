//! Static maps for trips and days (F-207) and the all-trips map (F-707) through Amazon
//! Location Service. The route itself is computed in `memorylane-trips`; this only draws.

use async_trait::async_trait;
use memorylane_core::geo::GeoPoint;
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

    fn bounded(points: &[GeoPoint]) -> String {
        points
            .iter()
            .map(|p| format!("{},{}", p.lon, p.lat))
            .collect::<Vec<_>>()
            .join(";")
    }

    fn overlay(points: &[GeoPoint], line: bool) -> String {
        let coords: Vec<serde_json::Value> = points
            .iter()
            .map(|p| serde_json::json!([p.lon, p.lat]))
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
        let out = self
            .client
            .get_static_map()
            .file_name("map.png")
            .width(i32::try_from(width).unwrap_or(1280))
            .height(i32::try_from(height).unwrap_or(720))
            .style(aws_sdk_geomaps::types::StaticMapStyle::Standard)
            .bounded_positions(Self::bounded(points))
            .geo_json_overlay(Self::overlay(points, line))
            .padding(40)
            .send()
            .await
            .map_err(backend)?;
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
        assert_eq!(LocationMaps::bounded(&pts), "135.7,35;135.8,35.1");
        let line: serde_json::Value =
            serde_json::from_str(&LocationMaps::overlay(&pts, true)).unwrap();
        assert_eq!(line["features"][0]["geometry"]["type"], "LineString");
        assert_eq!(line["features"][0]["geometry"]["coordinates"][0][0], 135.7);
        let pins: serde_json::Value =
            serde_json::from_str(&LocationMaps::overlay(&pts, false)).unwrap();
        assert_eq!(pins["features"].as_array().unwrap().len(), 2);
    }
}
