//! Frames of a day film (F-714), drawn from a reel plan with the crop formula the TV uses
//! (contracts/api.md): the slide on screen at time `t`, its window interpolated from `from`
//! to `to`, the picture scaled and placed with sub-pixel precision, then the text layers at
//! their opacity, then the fade from and to black. ffmpeg only encodes what this draws.

use crate::plan::{Frame, Reel, Window, TEXT_FADE_MS};
use crate::text::{self, Overlay};
use mantelpiece_core::ids::MediaId;
use std::collections::HashMap;
use tiny_skia::{Color, FilterQuality, Paint, Pixmap, PixmapPaint, Rect, Transform};

/// Fade from black at the start.
pub const FADE_IN_MS: u64 = 400;

/// A JPEG decoded for drawing.
///
/// # Errors
/// When the bytes are not a decodable image.
pub fn decode(jpeg: &[u8]) -> Result<Pixmap, String> {
    let img = image::load_from_memory(jpeg)
        .map_err(|e| e.to_string())?
        .to_rgba8();
    let (w, h) = img.dimensions();
    let size = tiny_skia::IntSize::from_wh(w, h).ok_or("empty image")?;
    // Photos are opaque, so straight and premultiplied alpha are the same bytes.
    Pixmap::from_vec(img.into_raw(), size).ok_or_else(|| "bad image size".to_owned())
}

/// Opacity of something shown from `start` to `end`, fading over `fade` at each end.
#[must_use]
pub fn opacity(t: u64, start: u64, end: u64, fade: u64) -> f32 {
    if t < start || t >= end {
        return 0.0;
    }
    let fade = fade.max(1);
    #[allow(clippy::cast_precision_loss)]
    let a = ((t - start) as f32 / fade as f32)
        .min((end - t) as f32 / fade as f32)
        .min(1.0);
    a.max(0.0)
}

/// Scale and top-left offset that put window `w` of a `pw` x `ph` picture on the frame,
/// clamped so the picture always covers the frame.
#[must_use]
pub fn placement(w: Window, pw: f32, ph: f32, frame: Frame) -> (f32, f32, f32) {
    #[allow(clippy::cast_precision_loss)]
    let (fw, fh) = (frame.width as f32, frame.height as f32);
    #[allow(clippy::cast_possible_truncation)]
    let s = (fw / pw).max(fh / ph) * w.z as f32;
    #[allow(clippy::cast_possible_truncation)]
    let left = (fw / 2.0 - w.cx as f32 * pw * s).clamp(fw - pw * s, 0.0);
    #[allow(clippy::cast_possible_truncation)]
    let top = (fh / 2.0 - w.cy as f32 * ph * s).clamp(fh - ph * s, 0.0);
    (s, left, top)
}

/// The text layers of a reel, drawn once.
#[derive(Debug)]
pub struct Layers {
    title: Overlay,
    title_span: (u64, u64),
    labels: Vec<(u64, u64, Overlay)>,
    credit: Overlay,
    credit_span: (u64, u64),
}

impl Layers {
    #[must_use]
    pub fn new(reel: &Reel, frame: Frame) -> Self {
        Self {
            title: text::title(frame, &reel.title.text, reel.title.sub.as_deref()),
            title_span: (reel.title.start_ms, reel.title.end_ms),
            labels: reel
                .slides
                .iter()
                .filter_map(|s| s.label.as_ref())
                .map(|l| (l.start_ms, l.end_ms, text::label(frame, &l.text)))
                .collect(),
            credit: text::credit(frame, &reel.credit.text),
            credit_span: (reel.credit.start_ms, reel.credit.end_ms),
        }
    }
}

fn layer(canvas: &mut Pixmap, o: &Overlay, alpha: f32) {
    if alpha > 0.0 {
        canvas.draw_pixmap(
            o.x,
            o.y,
            o.pixmap.as_ref(),
            &PixmapPaint {
                opacity: alpha,
                ..PixmapPaint::default()
            },
            Transform::identity(),
            None,
        );
    }
}

/// Draw the frame at `t_ms` onto `canvas` (the frame's size). Pictures are keyed by media id;
/// a missing picture leaves its slide black.
pub fn draw<S: std::hash::BuildHasher>(
    canvas: &mut Pixmap,
    reel: &Reel,
    t_ms: u64,
    pictures: &HashMap<MediaId, Pixmap, S>,
    layers: &Layers,
    frame: Frame,
) {
    canvas.fill(Color::BLACK);
    let slide = reel
        .slides
        .iter()
        .find(|s| t_ms >= s.start_ms && t_ms < s.end_ms)
        .or_else(|| reel.slides.last());
    if let Some(slide) = slide {
        if let Some(pic) = pictures.get(&slide.media) {
            #[allow(clippy::cast_precision_loss)]
            let p = (t_ms.saturating_sub(slide.start_ms) as f64
                / slide.duration_ms().max(1) as f64)
                .clamp(0.0, 1.0);
            let w = slide.from.lerp(slide.to, p);
            #[allow(clippy::cast_precision_loss)]
            let (s, left, top) = placement(w, pic.width() as f32, pic.height() as f32, frame);
            canvas.draw_pixmap(
                0,
                0,
                pic.as_ref(),
                &PixmapPaint {
                    quality: FilterQuality::Bilinear,
                    ..PixmapPaint::default()
                },
                Transform::from_row(s, 0.0, 0.0, s, left, top),
                None,
            );
        }
    }
    layer(
        canvas,
        &layers.title,
        opacity(t_ms, layers.title_span.0, layers.title_span.1, TEXT_FADE_MS),
    );
    for (start, end, o) in &layers.labels {
        layer(canvas, o, opacity(t_ms, *start, *end, TEXT_FADE_MS));
    }
    layer(
        canvas,
        &layers.credit,
        opacity(
            t_ms,
            layers.credit_span.0,
            layers.credit_span.1,
            TEXT_FADE_MS,
        ),
    );

    // Fade from black at the start and to black at the end.
    #[allow(clippy::cast_precision_loss)]
    let black = if t_ms < FADE_IN_MS {
        1.0 - t_ms as f32 / FADE_IN_MS as f32
    } else if t_ms + reel.fade_out_ms > reel.duration_ms {
        1.0 - reel.duration_ms.saturating_sub(t_ms) as f32 / reel.fade_out_ms.max(1) as f32
    } else {
        0.0
    };
    if black > 0.0 {
        #[allow(clippy::cast_precision_loss)]
        if let Some(r) = Rect::from_xywh(0.0, 0.0, frame.width as f32, frame.height as f32) {
            let mut paint = Paint::default();
            paint.set_color(Color::from_rgba(0.0, 0.0, 0.0, black.clamp(0.0, 1.0)).expect("valid"));
            canvas.fill_rect(r, &paint, Transform::identity(), None);
        }
    }
}

/// The times of every frame at `fps`, in milliseconds.
pub fn frame_times(duration_ms: u64, fps: u32) -> impl Iterator<Item = u64> {
    let n = duration_ms * u64::from(fps) / 1000;
    (0..n).map(move |i| i * 1000 / u64::from(fps))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{Slide, Timed, Title, TrackRef};
    use mantelpiece_core::ids::TripId;

    fn reel(slides: Vec<Slide>) -> Reel {
        let duration_ms = slides.last().map_or(0, |s| s.end_ms);
        Reel {
            trip: TripId::new("t"),
            day: Some(1),
            title: Title {
                text: "T".into(),
                sub: None,
                start_ms: 300,
                end_ms: 1_000,
            },
            track: TrackRef {
                id: "x".into(),
                title: "x".into(),
                artist: "x".into(),
                licence: "CC BY 3.0".into(),
                credit: "c".into(),
                url: String::new(),
                bpm: 120.0,
            },
            duration_ms,
            fade_out_ms: 500,
            slides,
            credit: Timed {
                text: "c".into(),
                start_ms: duration_ms - 500,
                end_ms: duration_ms,
            },
        }
    }

    /// A picture whose left half is red and right half blue.
    #[allow(clippy::cast_precision_loss)]
    fn halves(w: u32, h: u32) -> Pixmap {
        let mut p = Pixmap::new(w, h).unwrap();
        p.fill(Color::from_rgba8(0, 0, 255, 255));
        p.fill_rect(
            Rect::from_xywh(0.0, 0.0, (w / 2) as f32, h as f32).unwrap(),
            &{
                let mut paint = Paint::default();
                paint.set_color(Color::from_rgba8(255, 0, 0, 255));
                paint
            },
            Transform::identity(),
            None,
        );
        p
    }

    #[test]
    fn placement_follows_the_contract_and_covers_the_frame() {
        let f = Frame {
            width: 160,
            height: 90,
        };
        // A 4:3 picture at z 1 covers by width: scale 160 / 400.
        let (s, left, top) = placement(
            Window {
                cx: 0.5,
                cy: 0.5,
                z: 1.0,
            },
            400.0,
            300.0,
            f,
        );
        assert!((s - 0.4).abs() < 1e-6);
        assert!((left - 0.0).abs() < 1e-6);
        assert!((top - (45.0 - 60.0)).abs() < 1e-4);
        // A window past the edge is clamped: the picture still covers the frame.
        let (s, left, top) = placement(
            Window {
                cx: 0.0,
                cy: 1.0,
                z: 1.2,
            },
            400.0,
            300.0,
            f,
        );
        assert!(left <= 0.0 && left + 400.0 * s >= 160.0 - 1e-3);
        assert!(top <= 0.0 && top + 300.0 * s >= 90.0 - 1e-3);
    }

    #[test]
    fn frames_follow_the_window_and_fade() {
        let f = Frame {
            width: 160,
            height: 90,
        };
        let m = MediaId::new("m");
        // Pan from the red left edge to the blue right edge of a wide picture.
        let r = reel(vec![Slide {
            media: m.clone(),
            aspect: 4.0,
            start_ms: 0,
            end_ms: 4_000,
            from: Window {
                cx: 0.0,
                cy: 0.5,
                z: 1.0,
            },
            to: Window {
                cx: 1.0,
                cy: 0.5,
                z: 1.0,
            },
            label: None,
        }]);
        let pictures = HashMap::from([(m, halves(360, 90))]);
        let layers = Layers::new(&r, f);
        let mut canvas = Pixmap::new(160, 90).unwrap();
        let centre = |c: &Pixmap| c.pixel(80, 80).unwrap();

        draw(&mut canvas, &r, 0, &pictures, &layers, f);
        assert_eq!(centre(&canvas).red(), 0, "fully black at t = 0");
        draw(&mut canvas, &r, 1_000, &pictures, &layers, f);
        assert!(centre(&canvas).red() > 200, "red side early in the pan");
        draw(&mut canvas, &r, 3_400, &pictures, &layers, f);
        assert!(centre(&canvas).blue() > 200, "blue side late in the pan");
        draw(&mut canvas, &r, 3_999, &pictures, &layers, f);
        assert!(centre(&canvas).blue() < 10, "faded out at the end");
    }

    #[test]
    fn opacity_ramps_and_frame_times_step() {
        assert!((opacity(0, 0, 1_000, 300) - 0.0).abs() < 1e-6);
        assert!((opacity(150, 0, 1_000, 300) - 0.5).abs() < 1e-6);
        assert!((opacity(500, 0, 1_000, 300) - 1.0).abs() < 1e-6);
        assert!((opacity(1_000, 0, 1_000, 300) - 0.0).abs() < 1e-6);
        let t: Vec<u64> = frame_times(100, 30).collect();
        assert_eq!(t, vec![0, 33, 66]);
    }
}
