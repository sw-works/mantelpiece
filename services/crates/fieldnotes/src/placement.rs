//! Where the map may go (spec US3, research R5): the calmest region of the photo that is
//! clear of its subjects, or nowhere. Deterministic, on the upright TV rendition.

use image::{DynamicImage, GenericImageView};
use mantelpiece_core::field_note::{Ink, NormBox, Subject};

/// Grid width the photo is reduced to.
pub const GRID_W: u32 = 96;
/// Subject padding, as a share of the shorter side.
pub const SUBJECT_PAD: f32 = 0.04;
/// Margin kept from the frame edge, as a share of each side.
pub const FRAME_MARGIN: f32 = 0.03;
/// Shares of the frame a map may cover, tried in this order.
pub const AREAS: [f32; 3] = [0.20, 0.25, 0.15];
/// Above this mean detail no region is calm enough for thin lines (tuned in the spike).
pub const MAX_DETAIL: f32 = 0.085;
/// Under this mean luminance the ink is light.
pub const DARK_REGION: f32 = 0.45;

/// One cell of the reduced photo.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cell {
    /// Mean gradient magnitude, 0 to about 1.
    pub detail: f32,
    pub luma: f32,
    /// Low detail, bright or blue, in the upper half.
    pub sky: f32,
    /// Low detail with mostly horizontal structure.
    pub water: f32,
}

/// The photo reduced to cells.
#[derive(Clone, Debug)]
pub struct Grid {
    pub w: u32,
    pub h: u32,
    pub cells: Vec<Cell>,
}

impl Grid {
    #[must_use]
    pub fn cell(&self, x: u32, y: u32) -> &Cell {
        &self.cells[(y * self.w + x) as usize]
    }
}

/// Where the map goes and the ink it is drawn in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub bbox: NormBox,
    pub ink: Ink,
    /// Mean detail of the chosen region (lower is calmer).
    pub detail: f32,
}

#[allow(clippy::cast_precision_loss)]
fn luma(p: image::Rgba<u8>) -> f32 {
    (0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2])) / 255.0
}

/// Reduce the photo to a grid `GRID_W` cells wide with detail, tone, sky and water per cell.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names
)]
pub fn grid(photo: &DynamicImage) -> Grid {
    let (pw, ph) = photo.dimensions();
    let w = GRID_W.min(pw.max(1));
    let h = ((ph as f32 * w as f32 / pw.max(1) as f32).round() as u32).max(1);
    // Sample at 4 px per cell so a Sobel inside the cell sees texture, not just tone.
    let small = photo.resize_exact(w * 4, h * 4, image::imageops::FilterType::Triangle);
    let (sw, sh) = (w * 4, h * 4);
    let px = |x: u32, y: u32| small.get_pixel(x.min(sw - 1), y.min(sh - 1));
    let l = |x: i64, y: i64| {
        luma(px(
            x.clamp(0, i64::from(sw) - 1) as u32,
            y.clamp(0, i64::from(sh) - 1) as u32,
        ))
    };
    let mut cells = Vec::with_capacity((w * h) as usize);
    for cy in 0..h {
        for cx in 0..w {
            let (mut grad, mut across, mut down, mut lsum, mut blue) =
                (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for dy in 0..4u32 {
                for dx in 0..4u32 {
                    let (x, y) = (i64::from(cx * 4 + dx), i64::from(cy * 4 + dy));
                    let gx = (l(x + 1, y - 1) + 2.0 * l(x + 1, y) + l(x + 1, y + 1))
                        - (l(x - 1, y - 1) + 2.0 * l(x - 1, y) + l(x - 1, y + 1));
                    let gy = (l(x - 1, y + 1) + 2.0 * l(x, y + 1) + l(x + 1, y + 1))
                        - (l(x - 1, y - 1) + 2.0 * l(x, y - 1) + l(x + 1, y - 1));
                    grad += (gx * gx + gy * gy).sqrt();
                    across += gx.abs();
                    down += gy.abs();
                    let p = px(x as u32, y as u32);
                    lsum += luma(p);
                    if p[2] > p[0] && p[2] >= p[1] {
                        blue += 1.0;
                    }
                }
            }
            let detail = (grad / 16.0 / 4.0).min(1.0);
            let luma = lsum / 16.0;
            let calm = (1.0 - detail / MAX_DETAIL).clamp(0.0, 1.0);
            let upper = if cy < h / 2 { 1.0 } else { 0.4 };
            let sky = calm * upper * (luma.max(blue / 16.0)).min(1.0);
            // Water: calm, and what structure there is runs horizontally (gy dominates).
            let horizontal = if down > 1.5 * across { 1.0 } else { 0.0 };
            let water = calm * horizontal * if cy >= h / 3 { 1.0 } else { 0.5 };
            cells.push(Cell {
                detail,
                luma,
                sky,
                water,
            });
        }
    }
    Grid { w, h, cells }
}

/// Integral image over one channel, for O(1) rectangle means.
struct Integral {
    w: usize,
    sums: Vec<f64>,
}

impl Integral {
    fn new(grid: &Grid, f: impl Fn(&Cell) -> f32) -> Self {
        let (w, h) = (grid.w as usize + 1, grid.h as usize + 1);
        let mut sums = vec![0.0f64; w * h];
        for y in 1..h {
            for x in 1..w {
                let v = f64::from(f(&grid.cells[(y - 1) * grid.w as usize + (x - 1)]));
                sums[y * w + x] =
                    v + sums[(y - 1) * w + x] + sums[y * w + x - 1] - sums[(y - 1) * w + x - 1];
            }
        }
        Self { w, sums }
    }
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    fn mean(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> f32 {
        let s =
            self.sums[y1 * self.w + x1] - self.sums[y0 * self.w + x1] - self.sums[y1 * self.w + x0]
                + self.sums[y0 * self.w + x0];
        (s / ((x1 - x0) * (y1 - y0)).max(1) as f64) as f32
    }
}

/// Aspect ratios (width / height in pixels) a map box may take for a photo.
fn aspects(photo_aspect: f32) -> Vec<f32> {
    if photo_aspect < 1.0 {
        vec![3.0 / 4.0, 1.0, 4.0 / 3.0]
    } else {
        vec![3.0 / 2.0, 4.0 / 3.0, 1.0]
    }
}

/// The calmest box clear of `subjects`, or `None` when nothing is calm enough.
/// `photo_aspect` is the photo's width / height.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn place(grid: &Grid, photo_aspect: f32, subjects: &[Subject]) -> Option<Placement> {
    let (gw, gh) = (grid.w as usize, grid.h as usize);
    let detail = Integral::new(grid, |c| c.detail);
    let bonus = Integral::new(grid, |c| c.sky.max(c.water));
    let tone = Integral::new(grid, |c| c.luma);
    // Padding is a share of the shorter side, in each axis's own fraction.
    let (pad_x, pad_y) = if photo_aspect >= 1.0 {
        (SUBJECT_PAD / photo_aspect, SUBJECT_PAD)
    } else {
        (SUBJECT_PAD, SUBJECT_PAD * photo_aspect)
    };
    let blocked: Vec<NormBox> = subjects
        .iter()
        .map(|s| s.bbox.padded(pad_x, pad_y))
        .collect();
    let mut best: Option<(f32, Placement)> = None;
    for area in AREAS {
        for aspect in aspects(photo_aspect) {
            // Box in frame fractions: bw * bh = area, (bw * photo_aspect) / bh = aspect.
            let bh = (area * photo_aspect / aspect).sqrt();
            let bw = area / bh;
            if bw > 1.0 - 2.0 * FRAME_MARGIN || bh > 1.0 - 2.0 * FRAME_MARGIN {
                continue;
            }
            let cw = ((bw * gw as f32).round() as usize).max(1);
            let ch = ((bh * gh as f32).round() as usize).max(1);
            let step = (gw / 32).max(1);
            let (mx, my) = (
                (FRAME_MARGIN * gw as f32).ceil() as usize,
                (FRAME_MARGIN * gh as f32).ceil() as usize,
            );
            let mut y = my;
            while y + ch + my <= gh {
                let mut x = mx;
                while x + cw + mx <= gw {
                    let bbox = NormBox {
                        left: x as f32 / gw as f32,
                        top: y as f32 / gh as f32,
                        width: cw as f32 / gw as f32,
                        height: ch as f32 / gh as f32,
                    };
                    if !blocked.iter().any(|b| b.intersects(&bbox)) {
                        let d = detail.mean(x, y, x + cw, y + ch);
                        let b = bonus.mean(x, y, x + cw, y + ch);
                        // The middle third is where photos keep their subject.
                        let cx = bbox.left + bbox.width / 2.0;
                        let cy = bbox.top + bbox.height / 2.0;
                        let centre = if (1.0 / 3.0..2.0 / 3.0).contains(&cx)
                            && (1.0 / 3.0..2.0 / 3.0).contains(&cy)
                        {
                            0.02
                        } else {
                            0.0
                        };
                        // Prefer the 20 percent size slightly over the others.
                        let size = (area - 0.20).abs() * 0.05;
                        let score = d - 0.02 * b + centre + size;
                        if d <= MAX_DETAIL && best.as_ref().is_none_or(|(s, _)| score < *s) {
                            let ink = if tone.mean(x, y, x + cw, y + ch) < DARK_REGION {
                                Ink::Light
                            } else {
                                Ink::Dark
                            };
                            best = Some((
                                score,
                                Placement {
                                    bbox,
                                    ink,
                                    detail: d,
                                },
                            ));
                        }
                    }
                    x += step;
                }
                y += step;
            }
        }
    }
    best.map(|(_, p)| p)
}

#[cfg(test)]
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};
    use mantelpiece_core::field_note::SubjectKind;

    /// Calm blue sky over the top `sky` share, noisy texture below.
    fn sky_over_texture(w: u32, h: u32, sky: f32) -> DynamicImage {
        let horizon = (h as f32 * sky) as u32;
        DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
            if y < horizon {
                Rgb([120, 170, 230])
            } else {
                // Blocks of 12 px: texture at the scale a grid cell sees.
                let n = (((x / 12) * 7919 + (y / 12) * 104_729) % 251) as u8;
                Rgb([n, 255 - n, n / 2])
            }
        }))
    }

    #[test]
    fn the_map_goes_into_the_sky_and_never_over_a_subject() {
        let img = sky_over_texture(1920, 1280, 0.5);
        let g = grid(&img);
        let person = Subject::new(
            SubjectKind::Person,
            "Person",
            NormBox::new(0.1, 0.05, 0.2, 0.9).unwrap(),
            0.9,
        )
        .unwrap();
        let p = place(&g, 1.5, std::slice::from_ref(&person)).expect("sky is calm");
        assert!(p.bbox.bottom() <= 0.52, "{p:?}");
        assert!(!person.bbox.padded(0.04 / 1.5, 0.04).intersects(&p.bbox));
        assert!((0.14..=0.26).contains(&p.bbox.area()), "{}", p.bbox.area());
        assert_eq!(p.ink, Ink::Dark, "the sky is light");
    }

    #[test]
    fn a_busy_photo_gets_no_map() {
        let img = sky_over_texture(1600, 1200, 0.0);
        assert!(place(&grid(&img), 4.0 / 3.0, &[]).is_none());
    }

    #[test]
    fn a_dark_sky_takes_light_ink_and_a_portrait_keeps_a_portrait_box() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_fn(1080, 1620, |x, y| {
            if y < 900 {
                Rgb([10, 14, 30])
            } else {
                let n = (((x / 12) * 31 + (y / 12) * 17) % 200) as u8;
                Rgb([n, n, n])
            }
        }));
        let p = place(&grid(&img), 1080.0 / 1620.0, &[]).unwrap();
        assert_eq!(p.ink, Ink::Light);
        assert!(p.bbox.bottom() <= 0.58);
        let px_aspect = p.bbox.width * 1080.0 / (p.bbox.height * 1620.0);
        assert!(px_aspect <= 1.34, "{px_aspect}");
    }
}
