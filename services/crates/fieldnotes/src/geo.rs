//! Web Mercator: what part of the world a field note shows, which tiles cover it, and
//! where a point lands inside the note's box.

use mantelpiece_core::geo::GeoPoint;

/// Earth's circumference at the equator in metres (WGS 84).
const CIRCUMFERENCE_M: f64 = 40_075_016.686;
/// Highest zoom the vector tiles carry.
pub const MAX_VECTOR_ZOOM: u8 = 14;
/// Highest zoom worth fetching terrain at for a note's size.
pub const MAX_TERRAIN_ZOOM: u8 = 13;

/// A point in Web Mercator world space, 0 to 1 on each axis, y down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct World {
    pub x: f64,
    pub y: f64,
}

#[must_use]
pub fn world(p: GeoPoint) -> World {
    let lat = p.lat.clamp(-85.051_128, 85.051_128).to_radians();
    World {
        x: (p.lon + 180.0) / 360.0,
        y: (1.0 - lat.tan().asinh() / std::f64::consts::PI) / 2.0,
    }
}

/// Metres covered by one world unit at a latitude.
#[must_use]
pub fn metres_per_unit(lat: f64) -> f64 {
    CIRCUMFERENCE_M * lat.to_radians().cos()
}

/// The rectangle of the world a note shows, with the aspect of its box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub centre: World,
    pub half_w: f64,
    pub half_h: f64,
    /// Latitude of the centre, for metres.
    pub lat: f64,
}

/// A tile address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileId {
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

impl View {
    /// `width_m` across, `aspect` = box width / box height.
    #[must_use]
    pub fn new(centre: GeoPoint, width_m: f64, aspect: f64) -> Self {
        let half_w = width_m / metres_per_unit(centre.lat) / 2.0;
        Self {
            centre: world(centre),
            half_w,
            half_h: half_w / aspect.max(0.1),
            lat: centre.lat,
        }
    }

    /// Box fractions (0 to 1 each way, y down) of a world point. Outside the view is
    /// outside 0 to 1.
    #[must_use]
    pub fn to_box(&self, w: World) -> (f32, f32) {
        let u = (w.x - (self.centre.x - self.half_w)) / (2.0 * self.half_w);
        let v = (w.y - (self.centre.y - self.half_h)) / (2.0 * self.half_h);
        #[allow(clippy::cast_possible_truncation)]
        (u as f32, v as f32)
    }

    /// Metres across the view.
    #[must_use]
    pub fn width_m(&self) -> f64 {
        2.0 * self.half_w * metres_per_unit(self.lat)
    }

    /// A length in metres as a fraction of the box width.
    #[must_use]
    pub fn metres_to_box(&self, m: f64) -> f32 {
        #[allow(clippy::cast_possible_truncation)]
        let f = (m / self.width_m()) as f32;
        f
    }

    /// The zoom at which the view is about `box_px` tile pixels across, capped.
    #[must_use]
    pub fn zoom_for(&self, box_px: f64, max: u8) -> u8 {
        let z = (box_px / (256.0 * 2.0 * self.half_w)).log2().round();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let z = z.clamp(0.0, f64::from(max)) as u8;
        z
    }

    /// Tiles at `z` covering the view.
    #[must_use]
    pub fn tiles(&self, z: u8) -> Vec<TileId> {
        let n = f64::from(1u32 << z);
        let max = (1u32 << z) - 1;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let cell = |v: f64| ((v * n).floor().max(0.0) as u32).min(max);
        let (x0, x1) = (
            cell(self.centre.x - self.half_w),
            cell(self.centre.x + self.half_w),
        );
        let (y0, y1) = (
            cell(self.centre.y - self.half_h),
            cell(self.centre.y + self.half_h),
        );
        let mut out = Vec::new();
        for y in y0..=y1 {
            for x in x0..=x1 {
                out.push(TileId { z, x, y });
            }
        }
        out
    }
}

impl TileId {
    /// World position of a point given in tile-local units of `extent` (MVT is 4096).
    #[must_use]
    pub fn world(&self, gx: f64, gy: f64, extent: f64) -> World {
        let n = f64::from(1u32 << self.z);
        World {
            x: (f64::from(self.x) + gx / extent) / n,
            y: (f64::from(self.y) + gy / extent) / n,
        }
    }

    #[must_use]
    pub fn key(&self) -> String {
        format!("{}/{}/{}", self.z, self.x, self.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ponte_vecchio_sits_in_the_fixture_tiles() {
        let p = GeoPoint::new(43.7679, 11.2531);
        let v = View::new(p, 3_000.0, 1.5);
        assert!(v.tiles(13).contains(&TileId {
            z: 13,
            x: 4352,
            y: 2986
        }));
        assert!(v.tiles(12).contains(&TileId {
            z: 12,
            x: 2176,
            y: 1493
        }));
        let (u, w) = v.to_box(world(p));
        assert!((u - 0.5).abs() < 1e-4 && (w - 0.5).abs() < 1e-4);
        assert!((v.width_m() - 3_000.0).abs() < 1.0);
        assert!((v.metres_to_box(1_500.0) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn zoom_follows_the_box_size() {
        let v = View::new(GeoPoint::new(43.77, 11.25), 3_000.0, 1.5);
        let z = v.zoom_for(600.0, MAX_VECTOR_ZOOM);
        assert!((13..=14).contains(&z), "{z}");
        assert_eq!(v.zoom_for(1e9, MAX_VECTOR_ZOOM), MAX_VECTOR_ZOOM);
    }
}
