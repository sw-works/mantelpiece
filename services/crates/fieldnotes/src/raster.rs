//! Rasterise a `Drawing` into a transparent layer the size of the photo, and compose it
//! for sharing. Lines are hairlines at TV size and scale with the frame; text is the
//! bundled serif as outlines, so it stays sharp at any size.

use crate::drawing::{Class, Drawing, Icon, Label, LabelKind, MarkerKind};
use image::{DynamicImage, RgbImage};
use mantelpiece_core::field_note::{Ink, NormBox};
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke,
    StrokeDash, Transform,
};
use ttf_parser::{Face, OutlineBuilder};

/// Instrument Serif, SIL Open Font Licence (assets/OFL.txt).
static FONT: &[u8] = include_bytes!("../assets/InstrumentSerif-Regular.ttf");
const RED: (u8, u8, u8) = (200, 64, 47);

struct Palette {
    ink: (u8, u8, u8),
    halo: (u8, u8, u8),
    water: (u8, u8, u8),
    park: (u8, u8, u8),
    contour: (u8, u8, u8),
}

fn palette(ink: Ink) -> Palette {
    match ink {
        Ink::Light => Palette {
            ink: (245, 242, 235),
            halo: (20, 24, 32),
            water: (176, 204, 222),
            park: (190, 212, 176),
            contour: (238, 228, 208),
        },
        Ink::Dark => Palette {
            ink: (40, 38, 35),
            halo: (250, 248, 242),
            water: (84, 122, 146),
            park: (98, 132, 92),
            contour: (112, 92, 70),
        },
    }
}

pub(crate) fn paint(rgb: (u8, u8, u8), alpha: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(Color::from_rgba8(
        rgb.0,
        rgb.1,
        rgb.2,
        (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    ));
    p.anti_alias = true;
    p
}

pub(crate) fn path_of(
    parts: &[Vec<[f32; 2]>],
    w: f32,
    h: f32,
    close: bool,
) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    for part in parts {
        let mut it = part.iter();
        let Some(first) = it.next() else { continue };
        pb.move_to(first[0] * w, first[1] * h);
        for p in it {
            pb.line_to(p[0] * w, p[1] * h);
        }
        if close {
            pb.close();
        }
    }
    pb.finish()
}

/// Draw every shape of one class opaquely on its own canvas, then lay that canvas down at
/// `alpha`, so crossings and tile overlaps do not darken.
fn class_layer(
    map: &mut Pixmap,
    d: &Drawing,
    class: Class,
    fill: bool,
    rgb: (u8, u8, u8),
    alpha: f32,
    stroke: Option<&Stroke>,
) {
    let (w, h) = (map.width() as f32, map.height() as f32);
    let shapes: Vec<_> = if fill { &d.fills } else { &d.strokes }
        .iter()
        .filter(|s| s.class == class)
        .collect();
    if shapes.is_empty() {
        return;
    }
    let Some(mut canvas) = Pixmap::new(map.width(), map.height()) else {
        return;
    };
    let solid = paint(rgb, 1.0);
    for s in shapes {
        if let Some(path) = path_of(&s.parts, w, h, fill) {
            if fill {
                canvas.fill_path(
                    &path,
                    &solid,
                    FillRule::EvenOdd,
                    Transform::identity(),
                    None,
                );
            } else if let Some(st) = stroke {
                canvas.stroke_path(&path, &solid, st, Transform::identity(), None);
            }
        }
    }
    map.draw_pixmap(
        0,
        0,
        canvas.as_ref(),
        &PixmapPaint {
            opacity: alpha,
            ..PixmapPaint::default()
        },
        Transform::identity(),
        None,
    );
}

pub(crate) fn stroke(width: f32, dash: Option<[f32; 2]>) -> Stroke {
    Stroke {
        width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        dash: dash.and_then(|d| StrokeDash::new(vec![d[0], d[1]], 0.0)),
        ..Stroke::default()
    }
}

/// Glyph outlines into a path builder, in pixels from a pen position.
pub(crate) struct Pen<'a> {
    pub(crate) pb: &'a mut PathBuilder,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) scale: f32,
}

impl OutlineBuilder for Pen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.pb
            .move_to(self.x + x * self.scale, self.y - y * self.scale);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.pb
            .line_to(self.x + x * self.scale, self.y - y * self.scale);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.pb.quad_to(
            self.x + x1 * self.scale,
            self.y - y1 * self.scale,
            self.x + x * self.scale,
            self.y - y * self.scale,
        );
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.pb.cubic_to(
            self.x + x1 * self.scale,
            self.y - y1 * self.scale,
            self.x + x2 * self.scale,
            self.y - y2 * self.scale,
            self.x + x * self.scale,
            self.y - y * self.scale,
        );
    }
    fn close(&mut self) {
        self.pb.close();
    }
}

/// Text set in the bundled face: its path with the baseline at y = 0 and its width.
pub(crate) fn text_path(
    face: &Face<'_>,
    s: &str,
    px: f32,
    tracking: f32,
) -> (Option<tiny_skia::Path>, f32) {
    let scale = px / f32::from(face.units_per_em());
    let mut pb = PathBuilder::new();
    let mut x = 0.0;
    for ch in s.chars() {
        let Some(id) = face.glyph_index(ch) else {
            x += px * 0.3;
            continue;
        };
        let mut pen = Pen {
            pb: &mut pb,
            x,
            y: 0.0,
            scale,
        };
        face.outline_glyph(id, &mut pen);
        x += f32::from(face.glyph_hor_advance(id).unwrap_or(0)) * scale + tracking * px;
    }
    (pb.finish(), (x - tracking * px).max(0.0))
}

/// Text with a soft halo for legibility, at `(x, y)` (baseline left), rotated by `angle`.
#[allow(clippy::too_many_arguments)]
fn draw_text(
    canvas: &mut Pixmap,
    face: &Face<'_>,
    s: &str,
    px: f32,
    tracking: f32,
    x: f32,
    y: f32,
    angle: f32,
    pal: &Palette,
    alpha: f32,
) -> f32 {
    let (path, width) = text_path(face, s, px, tracking);
    let Some(path) = path else { return width };
    let t = Transform::from_translate(x, y).pre_concat(Transform::from_rotate(angle.to_degrees()));
    canvas.stroke_path(
        &path,
        &paint(pal.halo, 0.28 * alpha),
        &stroke(px * 0.16, None),
        t,
        None,
    );
    canvas.fill_path(&path, &paint(pal.ink, alpha), FillRule::Winding, t, None);
    width
}

pub(crate) fn circle(cx: f32, cy: f32, r: f32) -> Option<tiny_skia::Path> {
    PathBuilder::from_circle(cx, cy, r.max(0.1))
}

/// Small hash noise in 0 to 1, stable for a seed.
pub(crate) fn noise(seed: u64, x: u32, y: u32) -> f32 {
    let mut v = seed ^ (u64::from(x) << 32 | u64::from(y));
    v ^= v >> 33;
    v = v.wrapping_mul(0xff51_afd7_ed55_8ccd);
    v ^= v >> 33;
    v = v.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    v ^= v >> 33;
    #[allow(clippy::cast_precision_loss)]
    let f = (v >> 40) as f32 / (1u64 << 24) as f32;
    f
}

/// Feather the edges and add grain: alpha scaled by distance to the edge and by noise.
fn fade_and_grain(map: &mut Pixmap, fade: f32, seed: u64) {
    let (w, h) = (map.width(), map.height());
    let feather = (fade * w.min(h) as f32).max(1.0);
    let data = map.data_mut();
    for y in 0..h {
        for x in 0..w {
            // Feather each axis and multiply, so corners round off instead of showing.
            let s = |d: u32| {
                let t = (d as f32 / feather).clamp(0.0, 1.0);
                t * t * (3.0 - 2.0 * t)
            };
            let smooth = s(x.min(w - 1 - x)) * s(y.min(h - 1 - y));
            let grain = 1.0 - 0.22 * noise(seed, x, y);
            let k = smooth * grain;
            let i = ((y * w + x) * 4) as usize;
            for c in &mut data[i..i + 4] {
                *c = (f32::from(*c) * k).round() as u8;
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
fn label(canvas: &mut Pixmap, face: &Face<'_>, l: &Label, w: f32, h: f32, px: f32, pal: &Palette) {
    let (x, y) = (l.x * w, l.y * h);
    match l.kind {
        LabelKind::Water => {
            let (_, tw) = text_path(face, &l.text, px * 0.92, 0.08);
            let (c, s) = (l.angle.cos(), l.angle.sin());
            draw_text(
                canvas,
                face,
                &l.text,
                px * 0.92,
                0.08,
                x - c * tw / 2.0,
                y - s * tw / 2.0 + px * 0.3,
                l.angle,
                pal,
                0.8,
            );
        }
        LabelKind::Peak => {
            let r = px * 0.32;
            let mut pb = PathBuilder::new();
            pb.move_to(x, y - r);
            pb.line_to(x + r, y + r * 0.7);
            pb.line_to(x - r, y + r * 0.7);
            pb.close();
            if let Some(p) = pb.finish() {
                canvas.stroke_path(
                    &p,
                    &paint(pal.ink, 0.95),
                    &stroke(px * 0.07, None),
                    Transform::identity(),
                    None,
                );
            }
            let (_, tw) = text_path(face, &l.text, px, 0.0);
            draw_text(
                canvas,
                face,
                &l.text,
                px,
                0.0,
                x - tw / 2.0,
                y + r + px * 1.0,
                0.0,
                pal,
                1.0,
            );
            if let Some(sub) = &l.sub {
                let (_, sw) = text_path(face, sub, px * 0.72, 0.02);
                draw_text(
                    canvas,
                    face,
                    sub,
                    px * 0.72,
                    0.02,
                    x - sw / 2.0,
                    y + r + px * 1.85,
                    0.0,
                    pal,
                    0.85,
                );
            }
        }
        LabelKind::Place | LabelKind::Landmark => {
            match l.icon {
                Some(Icon::Church | Icon::Castle | Icon::Hut) => {
                    // A small house shape: the generic pictogram for a building.
                    let s = px * 0.34;
                    let mut pb = PathBuilder::new();
                    pb.move_to(x - s, y + s * 0.6);
                    pb.line_to(x - s, y - s * 0.1);
                    pb.line_to(x, y - s * 0.8);
                    pb.line_to(x + s, y - s * 0.1);
                    pb.line_to(x + s, y + s * 0.6);
                    pb.close();
                    if let Some(p) = pb.finish() {
                        canvas.stroke_path(
                            &p,
                            &paint(pal.ink, 0.95),
                            &stroke(px * 0.07, None),
                            Transform::identity(),
                            None,
                        );
                    }
                }
                _ => {
                    if let Some(c) = circle(x, y, px * 0.16) {
                        canvas.fill_path(
                            &c,
                            &paint(pal.ink, 0.95),
                            FillRule::Winding,
                            Transform::identity(),
                            None,
                        );
                    }
                }
            }
            draw_text(
                canvas,
                face,
                &l.text,
                px,
                0.0,
                x + px * 0.5,
                y + px * 0.33,
                0.0,
                pal,
                1.0,
            );
            if let Some(sub) = &l.sub {
                draw_text(
                    canvas,
                    face,
                    sub,
                    px * 0.72,
                    0.02,
                    x + px * 0.5,
                    y + px * 1.15,
                    0.0,
                    pal,
                    0.85,
                );
            }
        }
    }
}

fn scale_label(m: f32) -> String {
    if m >= 1000.0 {
        let km = m / 1000.0;
        if (km - km.round()).abs() < 1e-3 {
            format!("{km:.0} km")
        } else {
            format!("{km} km")
        }
    } else {
        format!("{m:.0} m")
    }
}

/// The transparent layer for a photo of `frame_w` × `frame_h` with the map in `placement`.
#[must_use]
#[allow(
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn layer(d: &Drawing, placement: NormBox, frame_w: u32, frame_h: u32) -> Pixmap {
    let mut frame = Pixmap::new(frame_w.max(1), frame_h.max(1)).expect("frame size");
    let bx = (placement.left * frame_w as f32).round() as i32;
    let by = (placement.top * frame_h as f32).round() as i32;
    let bw = ((placement.width * frame_w as f32).round() as u32).max(8);
    let bh = ((placement.height * frame_h as f32).round() as u32).max(8);
    let Some(mut map) = Pixmap::new(bw, bh) else {
        return frame;
    };
    let (w, h) = (bw as f32, bh as f32);
    let pal = palette(d.ink);
    // Line weights follow the frame: hairlines at 1920 px, thicker for a full-size share.
    let b = frame_w.max(frame_h) as f32 / 1920.0;
    let light = d.ink == Ink::Light;

    class_layer(
        &mut map,
        d,
        Class::Water,
        true,
        pal.water,
        if light { 0.15 } else { 0.18 },
        None,
    );
    class_layer(&mut map, d, Class::Park, true, pal.park, 0.1, None);
    class_layer(
        &mut map,
        d,
        Class::Contour,
        false,
        pal.contour,
        0.32,
        Some(&stroke(0.6 * b, None)),
    );
    class_layer(
        &mut map,
        d,
        Class::ContourIndex,
        false,
        pal.contour,
        0.5,
        Some(&stroke(0.9 * b, None)),
    );
    class_layer(
        &mut map,
        d,
        Class::Coast,
        false,
        pal.ink,
        0.75,
        Some(&stroke(1.0 * b, None)),
    );
    class_layer(
        &mut map,
        d,
        Class::River,
        false,
        pal.water,
        0.6,
        Some(&stroke(0.9 * b, None)),
    );
    class_layer(
        &mut map,
        d,
        Class::RoadMinor,
        false,
        pal.ink,
        0.38,
        Some(&stroke(0.5 * b, None)),
    );
    class_layer(
        &mut map,
        d,
        Class::RoadMajor,
        false,
        pal.ink,
        0.65,
        Some(&stroke(1.0 * b, None)),
    );
    class_layer(
        &mut map,
        d,
        Class::Path,
        false,
        pal.ink,
        0.55,
        Some(&stroke(0.7 * b, Some([2.5 * b, 2.5 * b]))),
    );
    class_layer(
        &mut map,
        d,
        Class::Route,
        false,
        pal.ink,
        0.9,
        Some(&stroke(1.3 * b, Some([1.0 * b, 3.2 * b]))),
    );

    // Marker under the fade so a ring near the edge softens with the map.
    let (mx, my) = (d.marker.x * w, d.marker.y * h);
    let base = frame_w.min(frame_h) as f32;
    let px = 0.02 * base;
    match d.marker.kind {
        MarkerKind::Ring => {
            if let Some(c) = circle(mx, my, d.marker.r * w) {
                map.fill_path(
                    &c,
                    &paint(RED, 0.10),
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
                map.stroke_path(
                    &c,
                    &paint(RED, 0.85),
                    &stroke(1.5 * b, Some([4.0 * b, 3.0 * b])),
                    Transform::identity(),
                    None,
                );
            }
        }
        MarkerKind::Dot => {}
    }
    fade_and_grain(&mut map, d.fade, d.grain_seed);

    // Crisp on top: the dot and its view cone, names, the title block, caption and credit.
    let face = Face::parse(FONT, 0).expect("bundled font");
    if d.marker.kind == MarkerKind::Dot {
        if let Some(heading) = d.marker.heading {
            let len = 0.13 * w.min(h);
            let spread = 14f32.to_radians();
            let dir = heading.to_radians();
            let mut pb = PathBuilder::new();
            pb.move_to(mx, my);
            pb.line_to(
                mx + len * (dir - spread).sin(),
                my - len * (dir - spread).cos(),
            );
            pb.line_to(
                mx + len * (dir + spread).sin(),
                my - len * (dir + spread).cos(),
            );
            pb.close();
            if let Some(p) = pb.finish() {
                map.fill_path(
                    &p,
                    &paint(RED, 0.32),
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
        if let Some(c) = circle(mx, my, 4.6 * b) {
            map.fill_path(
                &c,
                &paint((255, 255, 255), 0.95),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
        if let Some(c) = circle(mx, my, 3.4 * b) {
            map.fill_path(
                &c,
                &paint(RED, 1.0),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }
    for l in &d.labels {
        label(&mut map, &face, l, w, h, px, &pal);
    }
    let pad = 0.06 * w.min(h);
    let mut ty = pad;
    if let Some(t) = &d.title {
        let tpx = (0.034 * base).min(0.04 * frame_h as f32);
        ty += tpx;
        draw_text(
            &mut map,
            &face,
            &t.place.to_uppercase(),
            tpx,
            0.12,
            pad,
            ty,
            0.0,
            &pal,
            1.0,
        );
        if let Some(r) = &t.region {
            ty += tpx * 0.75;
            draw_text(
                &mut map,
                &face,
                &r.to_uppercase(),
                tpx * 0.42,
                0.22,
                pad,
                ty,
                0.0,
                &pal,
                0.85,
            );
        }
        ty += tpx * 0.6;
    }
    if let Some(s) = d.scale {
        let len = s.frac * w;
        let y = ty + px * 0.9;
        let mut pb = PathBuilder::new();
        pb.move_to(pad, y);
        pb.line_to(pad + len, y);
        for t in [0.0, 0.5, 1.0] {
            pb.move_to(pad + len * t, y);
            pb.line_to(pad + len * t, y - px * 0.3);
        }
        if let Some(p) = pb.finish() {
            map.stroke_path(
                &p,
                &paint(pal.ink, 0.85),
                &stroke(0.8 * b, None),
                Transform::identity(),
                None,
            );
        }
        let small = px * 0.62;
        draw_text(
            &mut map,
            &face,
            "0",
            small,
            0.0,
            pad - small * 0.2,
            y - px * 0.45,
            0.0,
            &pal,
            0.85,
        );
        let end = scale_label(s.metres);
        let (_, ew) = text_path(&face, &end, small, 0.0);
        draw_text(
            &mut map,
            &face,
            &end,
            small,
            0.0,
            pad + len - ew / 2.0,
            y - px * 0.45,
            0.0,
            &pal,
            0.85,
        );
        // North: a short arrow with N above, right of the bar.
        let nx = pad + len + px * 1.4;
        let mut pb = PathBuilder::new();
        pb.move_to(nx, y);
        pb.line_to(nx, y - px * 0.9);
        pb.move_to(nx - px * 0.18, y - px * 0.65);
        pb.line_to(nx, y - px * 0.9);
        pb.line_to(nx + px * 0.18, y - px * 0.65);
        if let Some(p) = pb.finish() {
            map.stroke_path(
                &p,
                &paint(pal.ink, 0.85),
                &stroke(0.8 * b, None),
                Transform::identity(),
                None,
            );
        }
        let (_, nw) = text_path(&face, "N", small, 0.0);
        draw_text(
            &mut map,
            &face,
            "N",
            small,
            0.0,
            nx - nw / 2.0,
            y - px * 1.1,
            0.0,
            &pal,
            0.85,
        );
    }
    let foot = h - pad * 0.7;
    let line = [d.caption.clone(), d.coords.clone()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("  ·  ");
    if !line.is_empty() {
        draw_text(
            &mut map,
            &face,
            &line,
            px * 0.68,
            0.04,
            pad,
            foot,
            0.0,
            &pal,
            0.85,
        );
    }
    let (_, cw) = text_path(&face, &d.credit, px * 0.5, 0.02);
    draw_text(
        &mut map,
        &face,
        &d.credit,
        px * 0.5,
        0.02,
        w - pad - cw,
        foot,
        0.0,
        &pal,
        0.6,
    );

    frame.draw_pixmap(
        bx,
        by,
        map.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    frame
}

/// The map alone, `width` pixels wide at the drawing's own aspect, edge to edge on a
/// transparent ground: the large map in "This place".
#[must_use]
pub fn map_only(d: &Drawing, width: u32) -> Pixmap {
    let height = ((width as f32 / d.aspect.max(0.1)).round() as u32).max(1);
    let whole = NormBox {
        left: 0.0,
        top: 0.0,
        width: 1.0,
        height: 1.0,
    };
    let mut d = d.clone();
    // A standalone map needs only a light feather at its edge.
    d.fade = d.fade.min(0.08);
    layer(&d, whole, width, height)
}

/// PNG bytes of a layer.
#[must_use]
pub fn png(layer: &Pixmap) -> Vec<u8> {
    layer.encode_png().unwrap_or_default()
}

/// The photo with the layer laid over it. The photo's own pixels are untouched wherever
/// the layer is transparent.
#[must_use]
pub fn compose(photo: &DynamicImage, layer: &Pixmap) -> RgbImage {
    let mut out = photo.to_rgb8();
    let (w, h) = (
        out.width().min(layer.width()),
        out.height().min(layer.height()),
    );
    let data = layer.data();
    for y in 0..h {
        for x in 0..w {
            let i = ((y * layer.width() + x) * 4) as usize;
            let a = u32::from(data[i + 3]);
            if a == 0 {
                continue;
            }
            let p = out.get_pixel_mut(x, y);
            for c in 0..3 {
                // Premultiplied source over opaque destination.
                let v = u32::from(data[i + c]) + u32::from(p[c]) * (255 - a) / 255;
                p[c] = v.min(255) as u8;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawing::{Marker, Scale, Title, OSM_CREDIT, STYLE_VERSION};
    use mantelpiece_core::field_note::MapMode;

    fn drawing() -> Drawing {
        Drawing {
            version: STYLE_VERSION,
            aspect: 1.5,
            mode: MapMode::Town,
            ink: Ink::Light,
            fills: vec![crate::drawing::Shape {
                class: Class::Water,
                parts: vec![vec![[0.0, 0.4], [1.0, 0.5], [1.0, 0.6], [0.0, 0.5]]],
            }],
            strokes: vec![crate::drawing::Shape {
                class: Class::RoadMajor,
                parts: vec![vec![[0.1, 0.1], [0.9, 0.9]]],
            }],
            labels: vec![Label {
                text: "Ponte Vecchio".into(),
                sub: None,
                x: 0.55,
                y: 0.45,
                kind: LabelKind::Landmark,
                angle: 0.0,
                icon: None,
            }],
            marker: Marker {
                kind: MarkerKind::Dot,
                x: 0.5,
                y: 0.5,
                r: 0.0,
                heading: Some(120.0),
            },
            title: Some(Title {
                place: "Florence".into(),
                region: Some("Italy".into()),
            }),
            scale: Some(Scale {
                metres: 500.0,
                frac: 0.2,
            }),
            caption: Some("3 April 2026".into()),
            coords: Some("43.77° N, 11.25° E".into()),
            fade: 0.12,
            grain_seed: 42,
            credit: OSM_CREDIT.into(),
        }
    }

    #[test]
    fn the_layer_is_the_frame_size_and_empty_outside_the_box() {
        let placement = NormBox::new(0.5, 0.05, 0.4, 0.4).unwrap();
        let l = layer(&drawing(), placement, 1920, 1280);
        assert_eq!((l.width(), l.height()), (1920, 1280));
        let alpha = |x: u32, y: u32| l.data()[((y * 1920 + x) * 4 + 3) as usize];
        assert_eq!(alpha(100, 1000), 0);
        assert_eq!(alpha(1919, 1279), 0);
        let inside: u32 = (0..400)
            .map(|i| u32::from(alpha(960 + 768 * i / 400, 64 + 512 * i / 400)))
            .sum();
        assert!(inside > 0, "something is drawn inside the box");
    }

    #[test]
    fn composing_an_empty_layer_leaves_the_photo_untouched() {
        let photo = DynamicImage::ImageRgb8(RgbImage::from_fn(64, 48, |x, y| {
            image::Rgb([(x * 3) as u8, (y * 5) as u8, 90])
        }));
        let empty = Pixmap::new(64, 48).unwrap();
        assert_eq!(compose(&photo, &empty), photo.to_rgb8());
        let placement = NormBox::new(0.5, 0.0, 0.5, 0.5).unwrap();
        let l = layer(&drawing(), placement, 64, 48);
        let out = compose(&photo, &l);
        for (x, y) in [(5, 40), (10, 30), (30, 45)] {
            assert_eq!(
                out.get_pixel(x, y),
                photo.to_rgb8().get_pixel(x, y),
                "({x},{y}) outside the box"
            );
        }
    }
}
