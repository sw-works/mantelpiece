//! Contours from open terrain tiles (research R2): Terrarium PNGs decoded to metres,
//! stitched over the view, traced with marching squares.

use crate::drawing::{Class, Shape};
use crate::geo::{TileId, View};
use contour::ContourBuilder;

/// Elevation in metres over the stitched tiles, row-major, `256 * cols` wide.
#[derive(Clone, Debug)]
pub struct Terrain {
    pub z: u8,
    pub x0: u32,
    pub y0: u32,
    pub w: usize,
    pub h: usize,
    pub metres: Vec<f64>,
}

/// Decode one Terrarium PNG: `R * 256 + G + B / 256 - 32768`.
#[must_use]
pub fn decode(png: &[u8]) -> Option<Vec<f64>> {
    let img = image::load_from_memory(png).ok()?.to_rgb8();
    if img.width() != 256 || img.height() != 256 {
        return None;
    }
    Some(
        img.pixels()
            .map(|p| f64::from(p[0]) * 256.0 + f64::from(p[1]) + f64::from(p[2]) / 256.0 - 32768.0)
            .collect(),
    )
}

/// Stitch decoded tiles (all at one zoom, covering a rectangle) into one grid. Missing
/// tiles read as the lowest elevation seen, so they draw no lines.
#[must_use]
pub fn stitch(tiles: &[(TileId, Vec<f64>)]) -> Option<Terrain> {
    let z = tiles.first()?.0.z;
    let x0 = tiles.iter().map(|t| t.0.x).min()?;
    let y0 = tiles.iter().map(|t| t.0.y).min()?;
    let cols = (tiles.iter().map(|t| t.0.x).max()? - x0 + 1) as usize;
    let rows = (tiles.iter().map(|t| t.0.y).max()? - y0 + 1) as usize;
    let (w, h) = (cols * 256, rows * 256);
    let floor = tiles
        .iter()
        .flat_map(|t| t.1.iter().copied())
        .fold(f64::INFINITY, f64::min);
    let mut metres = vec![floor; w * h];
    for (id, data) in tiles {
        let (ox, oy) = (((id.x - x0) * 256) as usize, ((id.y - y0) * 256) as usize);
        for row in 0..256 {
            let dst = (oy + row) * w + ox;
            metres[dst..dst + 256].copy_from_slice(&data[row * 256..row * 256 + 256]);
        }
    }
    Some(Terrain {
        z,
        x0,
        y0,
        w,
        h,
        metres,
    })
}

impl Terrain {
    /// The grid cell under a box point, if inside.
    fn cell(&self, view: &View, u: f64, v: f64) -> Option<(usize, usize)> {
        let n = f64::from(1u32 << self.z);
        let wx = view.centre.x - view.half_w + u * 2.0 * view.half_w;
        let wy = view.centre.y - view.half_h + v * 2.0 * view.half_h;
        let px = (wx * n - f64::from(self.x0)) * 256.0;
        let py = (wy * n - f64::from(self.y0)) * 256.0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (x, y) = (px.floor() as isize, py.floor() as isize);
        #[allow(clippy::cast_possible_wrap)]
        ((0..self.w as isize).contains(&x) && (0..self.h as isize).contains(&y))
            .then(|| (x.unsigned_abs(), y.unsigned_abs()))
    }

    /// Lowest and highest elevation inside the view, sampled on a coarse grid.
    #[must_use]
    pub fn relief(&self, view: &View) -> (f64, f64) {
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for i in 0..=24 {
            for j in 0..=24 {
                if let Some((x, y)) = self.cell(view, f64::from(i) / 24.0, f64::from(j) / 24.0) {
                    let m = self.metres[y * self.w + x];
                    lo = lo.min(m);
                    hi = hi.max(m);
                }
            }
        }
        (lo, hi)
    }

    /// Elevation at a box point.
    #[must_use]
    pub fn at(&self, view: &View, u: f64, v: f64) -> Option<f64> {
        self.cell(view, u, v)
            .map(|(x, y)| self.metres[y * self.w + x])
    }

    /// Contour lines in box coordinates, every `interval` metres, every fifth heavier.
    #[must_use]
    pub fn contours(&self, view: &View, interval: f64) -> Vec<Shape> {
        let (lo, hi) = self.relief(view);
        if !lo.is_finite() || hi - lo < interval {
            return vec![];
        }
        let first = (lo / interval).ceil() * interval;
        let mut thresholds = Vec::new();
        let mut t = first;
        while t <= hi {
            thresholds.push(t);
            t += interval;
        }
        let Ok(lines) = ContourBuilder::new(self.w, self.h, true).lines(&self.metres, &thresholds)
        else {
            return vec![];
        };
        let n = f64::from(1u32 << self.z);
        let mut out = Vec::new();
        for line in lines {
            let (geom, threshold) = line.into_inner();
            #[allow(clippy::cast_possible_truncation)]
            let index = ((threshold / interval).round() as i64) % 5 == 0;
            let parts: Vec<Vec<[f32; 2]>> = geom
                .0
                .iter()
                .map(|ls| {
                    ls.0.iter()
                        .map(|c| {
                            let wx = (f64::from(self.x0) + c.x / 256.0) / n;
                            let wy = (f64::from(self.y0) + c.y / 256.0) / n;
                            let (u, v) = view.to_box(crate::geo::World { x: wx, y: wy });
                            [u, v]
                        })
                        .collect()
                })
                .collect();
            out.push(Shape {
                class: if index {
                    Class::ContourIndex
                } else {
                    Class::Contour
                },
                parts,
            });
        }
        out
    }
}

/// A contour interval that gives a readable number of lines for the relief in view.
#[must_use]
pub fn interval(relief_m: f64) -> f64 {
    [10.0, 20.0, 50.0, 100.0, 200.0]
        .into_iter()
        .find(|i| relief_m / i <= 30.0)
        .unwrap_or(200.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mantelpiece_core::geo::GeoPoint;

    #[test]
    fn florence_terrain_decodes_and_has_hills_to_the_south() {
        let png = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/terrarium-12-2176-1493.png"
        ))
        .unwrap();
        let m = decode(&png).unwrap();
        let t = stitch(&[(
            TileId {
                z: 12,
                x: 2176,
                y: 1493,
            },
            m,
        )])
        .unwrap();
        let view = View::new(GeoPoint::new(43.7679, 11.2531), 3_000.0, 1.5);
        let (lo, hi) = t.relief(&view);
        assert!((20.0..80.0).contains(&lo), "Arno valley floor {lo}");
        assert!(
            hi > lo + 30.0,
            "Boboli and Piazzale Michelangelo rise above it: {hi}"
        );
        let c = t.contours(&view, interval(hi - lo));
        assert!(!c.is_empty());
    }

    #[test]
    fn a_synthetic_cone_gives_rings() {
        let metres: Vec<f64> = (0..256 * 256)
            .map(|i| {
                let (x, y) = (f64::from(i % 256) - 128.0, f64::from(i / 256) - 128.0);
                1000.0 - (x * x + y * y).sqrt() * 5.0
            })
            .collect();
        let t = stitch(&[(
            TileId {
                z: 10,
                x: 500,
                y: 300,
            },
            metres,
        )])
        .unwrap();
        let n = f64::from(1u32 << 10);
        let view = View {
            centre: crate::geo::World {
                x: 500.5 / n,
                y: 300.5 / n,
            },
            half_w: 0.5 / n,
            half_h: 0.5 / n,
            lat: 0.0,
        };
        let c = t.contours(&view, 100.0);
        assert!(c.len() >= 5);
        assert!(c.iter().any(|s| s.class == Class::ContourIndex));
        assert!((interval(1200.0) - 50.0).abs() < f64::EPSILON);
    }
}
