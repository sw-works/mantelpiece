//! The hand-drawn look for sketch maps (F-712, research R3): lines that wander a little and
//! get a second pass of the pen, watercolour washes with uneven edges, paper grain and
//! handwritten lettering. Every irregularity comes from seeded noise, so the same map drawn
//! twice is the same picture.

use crate::raster::{noise, paint, stroke, text_path};
use tiny_skia::{FillRule, PathBuilder, Pixmap, PixmapPaint, Transform};
use ttf_parser::Face;

/// Patrick Hand, SIL Open Font Licence (assets/OFL-PatrickHand.txt).
static HAND: &[u8] = include_bytes!("../assets/PatrickHand-Regular.ttf");

/// The handwritten face.
#[must_use]
pub fn face() -> Face<'static> {
    Face::parse(HAND, 0).expect("bundled font parses")
}

/// A stable 64-bit hash of a string and a number, for seeds.
#[must_use]
pub fn seed(s: &str, n: u64) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ n.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Smooth noise in -1 to 1 along one axis: hashed values at whole `t`, eased between.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn noise1(seed: u64, t: f32) -> f32 {
    let i = t.floor();
    let f = t - i;
    let at = |k: f32| noise(seed, (k as i64) as u32, 7) * 2.0 - 1.0;
    let e = f * f * (3.0 - 2.0 * f);
    at(i) * (1.0 - e) + at(i + 1.0) * e
}

/// A polyline in pixels resampled every `step` px and pushed along its normal by smooth
/// noise of `amplitude` px with a period of about `period` px.
#[must_use]
pub fn wobble(line: &[[f32; 2]], seed: u64, amplitude: f32, period: f32) -> Vec<[f32; 2]> {
    let step = 4.0;
    let mut dense: Vec<[f32; 2]> = Vec::with_capacity(line.len() * 2);
    for w in line.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = ((len / step).ceil() as usize).max(1);
        for k in 0..n {
            #[allow(clippy::cast_precision_loss)]
            let t = k as f32 / n as f32;
            dense.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    if let Some(last) = line.last() {
        dense.push(*last);
    }
    if dense.len() < 2 || amplitude <= 0.0 {
        return dense;
    }
    let mut out = Vec::with_capacity(dense.len());
    let mut s = 0.0;
    for i in 0..dense.len() {
        let prev = dense[i.saturating_sub(1)];
        let next = dense[(i + 1).min(dense.len() - 1)];
        let (dx, dy) = (next[0] - prev[0], next[1] - prev[1]);
        let len = dx.hypot(dy).max(1e-3);
        let (nx, ny) = (-dy / len, dx / len);
        if i > 0 {
            s += (dense[i][0] - dense[i - 1][0]).hypot(dense[i][1] - dense[i - 1][1]);
        }
        let o = amplitude * noise1(seed, s / period);
        out.push([dense[i][0] + nx * o, dense[i][1] + ny * o]);
    }
    out
}

fn path(lines: &[Vec<[f32; 2]>], close: bool) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    for l in lines {
        let mut it = l.iter();
        let Some(first) = it.next() else { continue };
        pb.move_to(first[0], first[1]);
        for p in it {
            pb.line_to(p[0], p[1]);
        }
        if close {
            pb.close();
        }
    }
    pb.finish()
}

/// Lay `canvas` onto `map` at `opacity`, so overlaps inside one pass never darken.
fn lay(map: &mut Pixmap, canvas: &Pixmap, opacity: f32) {
    map.draw_pixmap(
        0,
        0,
        canvas.as_ref(),
        &PixmapPaint {
            opacity,
            ..PixmapPaint::default()
        },
        Transform::identity(),
        None,
    );
}

/// How a line is inked.
#[derive(Clone, Copy, Debug)]
pub struct Pen {
    pub rgb: (u8, u8, u8),
    pub width: f32,
    pub alpha: f32,
    /// Wobble amplitude in px.
    pub wander: f32,
    /// `Some((dash, gap))` for a hand-dashed line, lengths in px before variation.
    pub dash: Option<(f32, f32)>,
    /// Draw a second, slightly offset pass at a third of the ink.
    pub second_pass: bool,
}

/// Split a polyline into hand dashes: each dash and gap varies by a third with noise.
fn dashes(line: &[[f32; 2]], seed: u64, dash: f32, gap: f32) -> Vec<Vec<[f32; 2]>> {
    let mut out = Vec::new();
    let mut cur: Vec<[f32; 2]> = Vec::new();
    let mut on = true;
    let mut k = 0u32;
    let mut left = dash;
    for w in line.windows(2) {
        let (mut a, b) = (w[0], w[1]);
        let mut len = (b[0] - a[0]).hypot(b[1] - a[1]);
        while len > 0.0 {
            let take = left.min(len);
            let t = take / len;
            let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            if on {
                if cur.is_empty() {
                    cur.push(a);
                }
                cur.push(p);
            }
            a = p;
            len -= take;
            left -= take;
            if left <= 1e-3 {
                if on && cur.len() > 1 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
                on = !on;
                k += 1;
                let vary = 1.0 + 0.33 * (noise(seed, k, 3) * 2.0 - 1.0);
                left = if on { dash } else { gap } * vary;
            }
        }
    }
    if on && cur.len() > 1 {
        out.push(cur);
    }
    out
}

/// Ink polylines (pixels) with `pen`, wobbled by `seed`.
pub fn ink(map: &mut Pixmap, lines: &[Vec<[f32; 2]>], pen: Pen, seed: u64) {
    let Some(mut canvas) = Pixmap::new(map.width(), map.height()) else {
        return;
    };
    let passes: &[(u64, f32, f32)] = if pen.second_pass {
        &[(0, 0.0, 1.0), (1, 0.6, 0.35)]
    } else {
        &[(0, 0.0, 1.0)]
    };
    for &(pass, shift, alpha) in passes {
        canvas.fill(tiny_skia::Color::TRANSPARENT);
        let mut parts = Vec::new();
        for (i, l) in lines.iter().enumerate() {
            let s = seed ^ ((i as u64) << 20) ^ (pass << 40);
            let mut w = wobble(l, s, pen.wander, 70.0);
            if shift > 0.0 {
                for p in &mut w {
                    p[0] += shift;
                    p[1] += shift * 0.5;
                }
            }
            match pen.dash {
                Some((d, g)) => parts.extend(dashes(&w, s, d, g)),
                None => parts.push(w),
            }
        }
        if let Some(p) = path(&parts, false) {
            canvas.stroke_path(
                &p,
                &paint(pen.rgb, 1.0),
                &stroke(pen.width, None),
                Transform::identity(),
                None,
            );
        }
        lay(map, &canvas, pen.alpha * alpha);
    }
}

/// A watercolour wash over polygons (pixels, each a list of rings): one flat fill, so tiles
/// meet without seams, with the pigment made uneven by slow noise, as wet paint dries
/// patchy. Each polygon is filled on its own: neighbouring tiles' polygons overlap in their
/// buffers, and one even-odd path would turn every overlap into a hole.
pub fn wash(
    map: &mut Pixmap,
    polygons: &[Vec<Vec<[f32; 2]>>],
    rgb: (u8, u8, u8),
    alpha: f32,
    seed: u64,
) {
    if polygons.is_empty() {
        return;
    }
    let Some(mut canvas) = Pixmap::new(map.width(), map.height()) else {
        return;
    };
    for rings in polygons {
        if let Some(p) = path(rings, true) {
            canvas.fill_path(
                &p,
                &paint(rgb, 1.0),
                FillRule::EvenOdd,
                Transform::identity(),
                None,
            );
        }
    }
    let w = canvas.width();
    let data = canvas.data_mut();
    for (i, px) in data.chunks_exact_mut(4).enumerate() {
        if px[3] == 0 {
            continue;
        }
        #[allow(clippy::cast_possible_truncation)]
        let (x, y) = ((i as u32) % w, (i as u32) / w);
        #[allow(clippy::cast_precision_loss)]
        let (fx, fy) = (x as f32, y as f32);
        let slow = noise1(seed, fx / 90.0 + noise1(seed ^ 9, fy / 70.0) * 2.0);
        let blot = noise1(seed ^ 0x77, fy / 55.0 + fx / 140.0);
        let k = (0.78 + 0.16 * slow + 0.1 * blot).clamp(0.5, 1.0);
        for c in px.iter_mut() {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                *c = (f32::from(*c) * k).round() as u8;
            }
        }
    }
    lay(map, &canvas, alpha);
}

/// Warm paper with grain and a faint darker rim.
pub fn paper(map: &mut Pixmap, seed: u64) {
    let (w, h) = (map.width(), map.height());
    let data = map.data_mut();
    for y in 0..h {
        for x in 0..w {
            let g = noise(seed, x, y) * 0.06 + noise(seed ^ 0x51, x / 7, y / 7) * 0.04;
            #[allow(clippy::cast_precision_loss)]
            let edge = {
                let d = x.min(w - 1 - x).min(y.min(h - 1 - y)) as f32;
                (1.0 - (d / 60.0).min(1.0)) * 0.05
            };
            let k = 1.0 - g - edge;
            let i = ((y * w + x) * 4) as usize;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                data[i] = (244.0 * k) as u8;
                data[i + 1] = (236.0 * k) as u8;
                data[i + 2] = (218.0 * k) as u8;
            }
            data[i + 3] = 255;
        }
    }
}

/// Width in px of `s` lettered at `px`.
#[must_use]
pub fn measure(face: &Face<'_>, s: &str, px: f32) -> f32 {
    text_path(face, s, px, 0.0).1
}

/// Handwritten text with its baseline-left at `(x, y)`, a paper halo behind it.
#[allow(clippy::too_many_arguments)]
pub fn letter(
    map: &mut Pixmap,
    face: &Face<'_>,
    s: &str,
    px: f32,
    x: f32,
    y: f32,
    rgb: (u8, u8, u8),
    alpha: f32,
) -> f32 {
    let (path, width) = text_path(face, s, px, 0.0);
    let Some(path) = path else { return width };
    let t = Transform::from_translate(x, y);
    map.stroke_path(
        &path,
        &paint((244, 236, 218), 0.85 * alpha),
        &stroke(px * 0.22, None),
        t,
        None,
    );
    map.fill_path(&path, &paint(rgb, alpha), FillRule::Winding, t, None);
    width
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wobble_is_the_same_every_time_and_stays_close() {
        let line = vec![[0.0, 0.0], [200.0, 0.0]];
        let a = wobble(&line, 42, 1.5, 60.0);
        assert_eq!(a, wobble(&line, 42, 1.5, 60.0));
        assert_ne!(a, wobble(&line, 43, 1.5, 60.0));
        assert!(a.iter().all(|p| p[1].abs() <= 1.5 + 1e-3));
        assert!(a.len() >= 50, "resampled every 4 px");
    }

    #[test]
    fn dashes_leave_gaps() {
        let line: Vec<[f32; 2]> = (0..=100).map(|i| [i as f32 * 2.0, 0.0]).collect();
        let d = dashes(&line, 1, 9.0, 6.0);
        assert!(d.len() > 8 && d.len() < 20, "{}", d.len());
    }

    #[test]
    fn the_hand_has_icelandic_letters() {
        let f = face();
        assert!("ðþæöáÐÞ".chars().all(|c| f.glyph_index(c).is_some()));
    }
}
