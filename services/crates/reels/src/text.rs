//! Text over a day film: the title, place labels and the music credit, each drawn once into
//! a small transparent layer and composited over the frames while it is on screen. Set in
//! the serif field notes use (Instrument Serif, SIL Open Font Licence). The ffmpeg layer has
//! no `drawtext`, and drawing here keeps the text identical on every platform.

use crate::plan::Frame;
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};
use ttf_parser::{Face, OutlineBuilder};

/// Instrument Serif, SIL Open Font Licence (crates/fieldnotes/assets/OFL.txt).
static FONT: &[u8] = include_bytes!("../../fieldnotes/assets/InstrumentSerif-Regular.ttf");

/// A layer and where its top-left corner sits on the frame.
#[derive(Debug, Clone)]
pub struct Overlay {
    pub pixmap: Pixmap,
    pub x: i32,
    pub y: i32,
}

struct Pen<'a> {
    pb: &'a mut PathBuilder,
    x: f32,
    scale: f32,
}

impl OutlineBuilder for Pen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.pb.move_to(self.x + x * self.scale, -y * self.scale);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.pb.line_to(self.x + x * self.scale, -y * self.scale);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.pb.quad_to(
            self.x + x1 * self.scale,
            -y1 * self.scale,
            self.x + x * self.scale,
            -y * self.scale,
        );
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.pb.cubic_to(
            self.x + x1 * self.scale,
            -y1 * self.scale,
            self.x + x2 * self.scale,
            -y2 * self.scale,
            self.x + x * self.scale,
            -y * self.scale,
        );
    }
    fn close(&mut self) {
        self.pb.close();
    }
}

fn face() -> Face<'static> {
    Face::parse(FONT, 0).expect("the bundled font parses")
}

/// The text's outline with its baseline at y = 0 and left edge at x = 0, and its width.
fn shape(face: &Face<'_>, s: &str, px: f32) -> (Option<tiny_skia::Path>, f32) {
    let scale = px / f32::from(face.units_per_em());
    let mut pb = PathBuilder::new();
    let mut x = 0.0;
    for ch in s.chars() {
        let Some(id) = face.glyph_index(ch) else {
            x += px * 0.3;
            continue;
        };
        face.outline_glyph(
            id,
            &mut Pen {
                pb: &mut pb,
                x,
                scale,
            },
        );
        x += f32::from(face.glyph_hor_advance(id).unwrap_or(0)) * scale;
    }
    (pb.finish(), x)
}

fn paint(red: u8, green: u8, blue: u8, alpha: f32) -> Paint<'static> {
    let mut paint = Paint::default();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let a8 = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    paint.set_color(Color::from_rgba8(red, green, blue, a8));
    paint.anti_alias = true;
    paint
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn layer(w: f32, h: f32) -> Pixmap {
    Pixmap::new(w.ceil().max(1.0) as u32, h.ceil().max(1.0) as u32).expect("non-zero layer")
}

fn line_px(frame: Frame, share: f32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let h = frame.height as f32;
    h * share
}

/// The title, centred low in the frame, white with a soft dark halo; the subtitle under it.
#[must_use]
pub fn title(frame: Frame, text: &str, sub: Option<&str>) -> Overlay {
    let face = face();
    let big = line_px(frame, 0.14);
    let small = line_px(frame, 0.055);
    let (path, w) = shape(&face, text, big);
    let sub_shape = sub.map(|s| shape(&face, s, small));
    let sub_w = sub_shape.as_ref().map_or(0.0, |s| s.1);
    let pad = big * 0.3;
    let width = w.max(sub_w) + 2.0 * pad;
    let height = big * 1.25 + sub_shape.as_ref().map_or(0.0, |_| small * 1.4) + pad;
    let mut pm = layer(width, height);
    let baseline = pad + big * 0.85;
    if let Some(path) = path {
        let t = Transform::from_translate((width - w) / 2.0, baseline);
        let halo = Stroke {
            width: big * 0.12,
            ..Stroke::default()
        };
        pm.stroke_path(&path, &paint(0, 0, 0, 0.35), &halo, t, None);
        pm.fill_path(
            &path,
            &paint(255, 255, 255, 1.0),
            FillRule::Winding,
            t,
            None,
        );
    }
    if let Some((Some(path), sw)) = sub_shape {
        let t = Transform::from_translate((width - sw) / 2.0, baseline + small * 1.35);
        let halo = Stroke {
            width: small * 0.14,
            ..Stroke::default()
        };
        pm.stroke_path(&path, &paint(0, 0, 0, 0.35), &halo, t, None);
        pm.fill_path(
            &path,
            &paint(255, 255, 255, 0.92),
            FillRule::Winding,
            t,
            None,
        );
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    let (x, y) = (
        ((frame.width as f32 - width) / 2.0) as i32,
        (line_px(frame, 0.72) - baseline) as i32,
    );
    Overlay { pixmap: pm, x, y }
}

/// Text on a rounded dark plate, at `x` (left edge, or right edge when `right`) with its
/// baseline at `baseline`.
fn plate(frame: Frame, text: &str, px: f32, x: f32, baseline: f32, right: bool) -> Overlay {
    let face = face();
    let (path, w) = shape(&face, text, px);
    let pad_x = px * 0.45;
    let pad_y = px * 0.3;
    let width = w + 2.0 * pad_x;
    let height = px * 1.15 + 2.0 * pad_y;
    let mut pm = layer(width, height);
    if let Some(rect) = Rect::from_xywh(0.0, 0.0, width, height) {
        let r = px * 0.25;
        let mut pb = PathBuilder::new();
        pb.move_to(r, 0.0);
        pb.line_to(width - r, 0.0);
        pb.quad_to(width, 0.0, width, r);
        pb.line_to(width, height - r);
        pb.quad_to(width, height, width - r, height);
        pb.line_to(r, height);
        pb.quad_to(0.0, height, 0.0, height - r);
        pb.line_to(0.0, r);
        pb.quad_to(0.0, 0.0, r, 0.0);
        pb.close();
        match pb.finish() {
            Some(p) => pm.fill_path(
                &p,
                &paint(0, 0, 0, 0.55),
                FillRule::Winding,
                Transform::identity(),
                None,
            ),
            None => pm.fill_rect(rect, &paint(0, 0, 0, 0.55), Transform::identity(), None),
        }
    }
    let base = pad_y + px * 0.85;
    if let Some(path) = path {
        pm.fill_path(
            &path,
            &paint(255, 255, 255, 1.0),
            FillRule::Winding,
            Transform::from_translate(pad_x, base),
            None,
        );
    }
    let left = if right { x - width } else { x };
    #[allow(clippy::cast_possible_truncation)]
    let (x, y) = (left as i32, (baseline - base) as i32);
    let _ = frame;
    Overlay { pixmap: pm, x, y }
}

/// A place label, bottom left inside the title-safe area.
#[must_use]
pub fn label(frame: Frame, text: &str) -> Overlay {
    #[allow(clippy::cast_precision_loss)]
    let x = frame.width as f32 * 0.05;
    plate(
        frame,
        text,
        line_px(frame, 0.042),
        x,
        line_px(frame, 0.88),
        false,
    )
}

/// The music credit, small, bottom right inside the title-safe area.
#[must_use]
pub fn credit(frame: Frame, text: &str) -> Overlay {
    #[allow(clippy::cast_precision_loss)]
    let x = frame.width as f32 * 0.95;
    plate(
        frame,
        text,
        line_px(frame, 0.028),
        x,
        line_px(frame, 0.92),
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inside(o: &Overlay, f: Frame) -> bool {
        #[allow(clippy::cast_possible_wrap)]
        let (w, h) = (f.width as i32, f.height as i32);
        #[allow(clippy::cast_possible_wrap)]
        let (ow, oh) = (o.pixmap.width() as i32, o.pixmap.height() as i32);
        o.x >= 0 && o.y >= 0 && o.x + ow <= w && o.y + oh <= h
    }

    #[test]
    fn overlays_draw_ink_and_stay_in_frame() {
        let f = Frame::TV;
        for o in [
            title(f, "Florence", Some("Day 2")),
            label(f, "Ponte Vecchio"),
            credit(
                f,
                "Music: \u{201c}Carefree\u{201d} by Kevin MacLeod, CC BY 3.0",
            ),
        ] {
            assert!(
                inside(&o, f),
                "in frame: {} x {} at {},{}",
                o.pixmap.width(),
                o.pixmap.height(),
                o.x,
                o.y
            );
            assert!(
                o.pixmap
                    .pixels()
                    .iter()
                    .any(|p| p.alpha() == 255 && p.red() > 200),
                "white ink"
            );
        }
        let c = credit(f, "x");
        assert!(c.x + i32::try_from(c.pixmap.width()).unwrap() <= 1824 + 1);
    }

    #[test]
    fn drawing_is_deterministic() {
        let a = label(Frame::TV, "Santa Maria Novella");
        let b = label(Frame::TV, "Santa Maria Novella");
        assert_eq!(a.pixmap.data(), b.pixmap.data());
    }
}
