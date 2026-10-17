//! The plan of a reel: which photos, for how many bars each, moving how, with what text.
//!
//! Rules, in order:
//! - Photos: the day's media in capture order (then its undated items), burst keepers only,
//!   without removed, hidden, cleanup-flagged or unrendered items. Fewer than
//!   [`MIN_SLIDES`] is no reel.
//! - Track: [`crate::library::pick`] by trip and day. The music starts at its own 0 with the
//!   first slide; every later cut lands on one of its bar lines, up to the final hit.
//! - Length: at most [`MAX_DAY_SLIDES`] or [`MAX_TRIP_SLIDES`], and as many as the bars
//!   hold. When there are more photos, the best by score are kept (at least one per day),
//!   still in capture order.
//! - Bars: the title slide lasts until the first bar line past [`TITLE_MS`]; every other
//!   slide one bar, two when bars are short and the photo is a highlight or carries a
//!   label; the last slide at least [`LAST_MS`], for the credit and the fade.
//! - Moves: a portrait picture in a landscape frame tilts across its band toward its
//!   subjects; a panorama pans; anything else pushes in toward its subjects or pulls out,
//!   never the same kind twice in a row. The picture's content takes at least
//!   [`MIN_CROSS_MS`] to cross the frame, and every window stays inside the picture.
//! - Text: the title is the trip's name (and the day for a day reel); a label is a
//!   landmark the photo's enrichment names with at least [`LABEL_CONFIDENCE`], shown on the
//!   first slide of a run of that landmark when the slide is long enough to read it
//!   (constitution IV: every word on screen comes from the metadata).

use crate::library::{pick, Track};
use mantelpiece_core::field_note::SubjectKind;
use mantelpiece_core::ids::{HouseholdId, MediaId, TripId};
use mantelpiece_core::media::{MediaItem, ProcessingState};
use mantelpiece_core::trip::Trip;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

pub const MIN_SLIDES: usize = 3;
pub const MAX_DAY_SLIDES: usize = 30;
pub const MAX_TRIP_SLIDES: usize = 48;
/// The title slide lasts at least this long.
pub const TITLE_MS: u64 = 3_500;
/// The last slide lasts at least this long: the credit shows over it and it fades out.
pub const LAST_MS: u64 = 4_000;
pub const FADE_OUT_MS: u64 = 2_000;
/// Picture content should take at least this long to cross the frame, or it judders.
pub const MIN_CROSS_MS: f64 = 4_000.0;
/// A photo's landmark tag must be at least this sure to be named on screen (ingest's
/// "grounded" level).
pub const LABEL_CONFIDENCE: f32 = 0.85;
/// Bars shorter than this give highlights and labelled photos two bars.
const SHORT_BAR_MS: u64 = 2_200;
/// Text fades in and out over this, and must then stay readable for 1 s + 0.3 s per word.
pub const TEXT_FADE_MS: u64 = 300;
/// Bump to re-render every film when the look changes.
pub const STYLE: &str = "reel-1";

/// The frame a reel is planned for. The TV and the films are both 1920 x 1080.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
}

impl Frame {
    pub const TV: Self = Self {
        width: 1920,
        height: 1080,
    };

    fn aspect(self) -> f64 {
        f64::from(self.width) / f64::from(self.height)
    }
}

/// A crop window: its centre in normalised picture coordinates and its zoom over a cover
/// fit (contracts/api.md).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Window {
    pub cx: f64,
    pub cy: f64,
    pub z: f64,
}

impl Window {
    #[must_use]
    pub fn lerp(self, to: Self, p: f64) -> Self {
        Self {
            cx: self.cx + (to.cx - self.cx) * p,
            cy: self.cy + (to.cy - self.cy) * p,
            z: self.z + (to.z - self.z) * p,
        }
    }

    fn rounded(self) -> Self {
        let r = |v: f64| (v * 10_000.0).round() / 10_000.0;
        Self {
            cx: r(self.cx),
            cy: r(self.cy),
            z: r(self.z),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Timed {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Title {
    pub text: String,
    pub sub: Option<String>,
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Slide {
    pub media: MediaId,
    /// Width over height of the picture as displayed.
    pub aspect: f64,
    pub start_ms: u64,
    pub end_ms: u64,
    pub from: Window,
    pub to: Window,
    pub label: Option<Timed>,
}

impl Slide {
    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        self.end_ms - self.start_ms
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackRef {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub licence: String,
    pub credit: String,
    /// The audio: a presigned URL filled in by the API; the S3 key until then.
    pub url: String,
    pub bpm: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reel {
    pub trip: TripId,
    pub day: Option<u32>,
    pub title: Title,
    pub track: TrackRef,
    pub duration_ms: u64,
    pub fade_out_ms: u64,
    pub slides: Vec<Slide>,
    pub credit: Timed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanError {
    NoSuchDay,
    TooFewPhotos,
}

/// What a reel is planned from.
#[derive(Debug)]
pub struct Request<'a> {
    pub trip: &'a Trip,
    /// `None` for the whole trip.
    pub day: Option<u32>,
    /// The trip's media; others are ignored.
    pub media: &'a [MediaItem],
    /// Whose pins and hides apply.
    pub viewer: &'a HouseholdId,
    pub frame: Frame,
}

struct Candidate<'a> {
    item: &'a MediaItem,
    day: u32,
    score: f64,
    aspect: f64,
}

/// Plan a reel.
///
/// # Errors
/// [`PlanError::NoSuchDay`] for a day the trip does not have; [`PlanError::TooFewPhotos`]
/// when fewer than [`MIN_SLIDES`] photos can be shown.
pub fn plan(req: &Request<'_>) -> Result<Reel, PlanError> {
    let days: Vec<_> = match req.day {
        Some(n) => vec![req
            .trip
            .days
            .iter()
            .find(|d| d.index == n)
            .ok_or(PlanError::NoSuchDay)?],
        None => req.trip.days.iter().collect(),
    };
    let by_id: HashMap<&MediaId, &MediaItem> = req.media.iter().map(|m| (&m.id, m)).collect();
    let highlights: HashSet<&MediaId> = req.trip.highlights.iter().collect();
    let mut already = HashSet::new();
    let mut candidates = Vec::new();
    for day in &days {
        for id in day.media.iter().chain(&day.undated) {
            let Some(item) = by_id.get(id) else { continue };
            if !already.insert(id) || !showable(item, req.viewer) {
                continue;
            }
            let (Some(w), Some(h)) = (item.signals.width, item.signals.height) else {
                continue;
            };
            let mut score = f64::from(item.quality_score.unwrap_or(5.0));
            if item.pinned_for(req.viewer) {
                score += 3.0;
            }
            if highlights.contains(id) {
                score += 2.0;
            }
            if label_of(item).is_some() {
                score += 1.0;
            }
            candidates.push(Candidate {
                item,
                day: day.index,
                score,
                aspect: f64::from(w) / f64::from(h.max(1)),
            });
        }
    }
    if candidates.len() < MIN_SLIDES {
        return Err(PlanError::TooFewPhotos);
    }

    let seed = match req.day {
        Some(n) => format!("{}/day-{n}", req.trip.id),
        None => req.trip.id.to_string(),
    };
    let track = pick(&seed);
    let cuts = track.cut_points();
    let max = if req.day.is_some() {
        MAX_DAY_SLIDES
    } else {
        MAX_TRIP_SLIDES
    };
    let mut keep = select(
        &candidates,
        max.min(cuts.len().saturating_sub(2)).max(MIN_SLIDES),
    );
    loop {
        match time_slides(&candidates, &keep, track, &cuts, &highlights) {
            Some(timed) => return Ok(assemble(req, track, &candidates, &timed)),
            None if keep.len() > MIN_SLIDES => {
                // Too long for the track: drop the weakest photo and time again.
                let weakest = (1..keep.len())
                    .min_by(|&a, &b| {
                        candidates[keep[a]]
                            .score
                            .total_cmp(&candidates[keep[b]].score)
                    })
                    .expect("more than one");
                keep.remove(weakest);
            }
            None => return Err(PlanError::TooFewPhotos),
        }
    }
}

fn showable(item: &MediaItem, viewer: &HouseholdId) -> bool {
    !item.is_removed()
        && !item.hidden_for(viewer)
        && item.cleanup_reason.is_none()
        && (item.is_keeper || item.burst_group.is_none())
        && matches!(
            item.state,
            ProcessingState::Signalled | ProcessingState::Enriched
        )
}

fn label_of(item: &MediaItem) -> Option<&str> {
    item.enrichment
        .landmark
        .as_ref()
        .filter(|t| t.confidence >= LABEL_CONFIDENCE && !t.name.trim().is_empty())
        .map(|t| t.name.trim())
}

/// Indexes of the photos to keep, in capture order: the best of each day first, then the
/// best of the rest, up to `max`.
fn select(c: &[Candidate<'_>], max: usize) -> Vec<usize> {
    if c.len() <= max {
        return (0..c.len()).collect();
    }
    let mut chosen = HashSet::new();
    let mut days: Vec<u32> = c.iter().map(|x| x.day).collect();
    days.dedup();
    for day in days {
        if chosen.len() >= max {
            break;
        }
        if let Some(best) = (0..c.len())
            .filter(|&i| c[i].day == day)
            .max_by(|&a, &b| c[a].score.total_cmp(&c[b].score))
        {
            chosen.insert(best);
        }
    }
    let mut rest: Vec<usize> = (0..c.len()).filter(|i| !chosen.contains(i)).collect();
    rest.sort_by(|&a, &b| c[b].score.total_cmp(&c[a].score).then(a.cmp(&b)));
    for i in rest {
        if chosen.len() >= max {
            break;
        }
        chosen.insert(i);
    }
    let mut keep: Vec<usize> = chosen.into_iter().collect();
    keep.sort_unstable();
    keep
}

struct Timing {
    index: usize,
    start_ms: u64,
    end_ms: u64,
    label: Option<Timed>,
}

fn read_ms(text: &str) -> u64 {
    let words = u64::try_from(text.split_whitespace().count()).unwrap_or(1);
    1_000 + 300 * words + 2 * TEXT_FADE_MS
}

/// Start and end of every kept slide on the bar lines, or `None` when they do not fit
/// before the final hit.
fn time_slides(
    c: &[Candidate<'_>],
    keep: &[usize],
    track: &Track,
    cuts: &[u64],
    highlights: &HashSet<&MediaId>,
) -> Option<Vec<Timing>> {
    let short = track.bar_ms() < SHORT_BAR_MS;
    let mut next_cut = cuts.iter().position(|&b| b >= TITLE_MS)?;
    let mut out = Vec::with_capacity(keep.len());
    let mut start = 0;
    let mut last_label: Option<&str> = None;
    for (n, &i) in keep.iter().enumerate() {
        let item = c[i].item;
        let label = label_of(item).filter(|l| last_label != Some(*l));
        last_label = label_of(item);
        let important = highlights.contains(&item.id) || item.pinned;
        let mut bars = if n == 0 {
            0
        } else if short && (important || label.is_some()) {
            2
        } else {
            1
        };
        let is_last = n + 1 == keep.len();
        let mut end_idx = next_cut + bars;
        // Grow the slide until its label can be read, and the last slide until it holds
        // the credit and the fade.
        let need = |s: u64, e: u64| -> bool {
            let len = e.saturating_sub(s);
            (label.is_none() || len >= read_ms(label.unwrap_or_default()))
                && (!is_last || len >= LAST_MS)
        };
        while end_idx < cuts.len() && !need(start, cuts[end_idx]) {
            end_idx += 1;
            bars += 1;
        }
        let end = *cuts.get(end_idx)?;
        let label = label.map(|l| Timed {
            text: l.to_owned(),
            start_ms: start + TEXT_FADE_MS * 2,
            end_ms: end.saturating_sub(TEXT_FADE_MS),
        });
        let label = label.filter(|l| l.end_ms > l.start_ms + 1_000);
        out.push(Timing {
            index: i,
            start_ms: start,
            end_ms: end,
            label,
        });
        start = end;
        next_cut = end_idx + 1;
        let _ = bars;
    }
    Some(out)
}

fn assemble(req: &Request<'_>, track: &Track, c: &[Candidate<'_>], timed: &[Timing]) -> Reel {
    let mut slides = Vec::with_capacity(timed.len());
    let mut last_kind = None;
    for (n, t) in timed.iter().enumerate() {
        let cand = &c[t.index];
        let (from, to, kind) = moves(
            cand.item,
            cand.aspect,
            req.frame,
            t.end_ms - t.start_ms,
            last_kind,
            n,
        );
        last_kind = Some(kind);
        slides.push(Slide {
            media: cand.item.id.clone(),
            aspect: (cand.aspect * 10_000.0).round() / 10_000.0,
            start_ms: t.start_ms,
            end_ms: t.end_ms,
            // Round for a compact wire form, then clamp again: rounding can step over the edge.
            from: clamp_window(from.rounded(), cand.aspect, req.frame),
            to: clamp_window(to.rounded(), cand.aspect, req.frame),
            label: t.label.clone(),
        });
    }
    let duration_ms = slides.last().map_or(0, |s| s.end_ms);
    let first_end = slides.first().map_or(0, |s| s.end_ms);
    let sub = req.day.map(|n| format!("Day {n}"));
    let credit_text = track.credit();
    Reel {
        trip: req.trip.id.clone(),
        day: req.day,
        title: Title {
            text: req.trip.name.clone(),
            sub,
            start_ms: TEXT_FADE_MS,
            end_ms: first_end.saturating_sub(TEXT_FADE_MS),
        },
        track: TrackRef {
            id: track.id.clone(),
            title: track.title.clone(),
            artist: track.artist.clone(),
            licence: track.licence.clone(),
            credit: credit_text.clone(),
            url: track.file.clone(),
            bpm: track.bpm,
        },
        duration_ms,
        fade_out_ms: FADE_OUT_MS.min(LAST_MS / 2),
        slides,
        credit: Timed {
            text: credit_text,
            start_ms: duration_ms.saturating_sub(LAST_MS - TEXT_FADE_MS),
            end_ms: duration_ms,
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    PushIn,
    PullOut,
    TiltDown,
    TiltUp,
    PanRight,
    PanLeft,
}

/// Where the photo's subjects are: people first, then scenery, then anything; the
/// upper-middle of the frame when nothing was detected.
fn focus(item: &MediaItem) -> (f64, f64) {
    let subjects = &item.enrichment.subjects;
    let pick = |kinds: &[SubjectKind]| {
        let (mut x, mut y, mut w) = (0.0, 0.0, 0.0);
        for s in subjects.iter().filter(|s| kinds.contains(&s.kind)) {
            let b = s.bbox;
            let area = f64::from(b.width * b.height);
            x += f64::from(b.left + b.width / 2.0) * area;
            y += f64::from(b.top + b.height / 2.0) * area;
            w += area;
        }
        (w > 0.0).then(|| (x / w, y / w))
    };
    pick(&[SubjectKind::Face, SubjectKind::Person])
        .or_else(|| pick(&[SubjectKind::Landscape]))
        .or_else(|| pick(&[SubjectKind::Object]))
        .unwrap_or((0.5, 0.45))
}

/// Half the window's extent along each axis, in normalised picture coordinates.
fn half_extent(aspect: f64, frame: Frame, z: f64) -> (f64, f64) {
    let f = frame.aspect();
    let s = (f / aspect).max(1.0) * z;
    (f / (aspect * s) / 2.0, 1.0 / s / 2.0)
}

fn clamp_window(w: Window, aspect: f64, frame: Frame) -> Window {
    let (hx, hy) = half_extent(aspect, frame, w.z);
    Window {
        cx: w.cx.clamp(hx, 1.0 - hx),
        cy: w.cy.clamp(hy, 1.0 - hy),
        z: w.z,
    }
}

/// The two windows of a slide and the kind of move between them.
fn moves(
    item: &MediaItem,
    aspect: f64,
    frame: Frame,
    duration_ms: u64,
    last: Option<Kind>,
    n: usize,
) -> (Window, Window, Kind) {
    let (fx, fy) = focus(item);
    let f = frame.aspect();
    #[allow(clippy::cast_precision_loss)]
    let secs_frac = duration_ms as f64 / MIN_CROSS_MS;
    let (from, to, kind) = if aspect < f * 0.85 {
        // Portrait in a landscape frame: tilt along the visible band toward the subject.
        let (_, hy) = half_extent(aspect, frame, 1.0);
        let reach = (2.0 * hy * secs_frac).min(1.0 - 2.0 * hy);
        let down = if last == Some(Kind::TiltDown) {
            false
        } else if last == Some(Kind::TiltUp) {
            true
        } else {
            fy >= 0.5
        };
        let end = fy.clamp(hy, 1.0 - hy);
        let start = if down {
            (end - reach).max(hy)
        } else {
            (end + reach).min(1.0 - hy)
        };
        let (a, b) = if (end - start).abs() < 0.02 {
            // The subject sits at the band's end already: tilt from the other end.
            if down {
                (1.0 - hy - reach, 1.0 - hy)
            } else {
                (hy + reach, hy)
            }
        } else {
            (start, end)
        };
        let kind = if b >= a { Kind::TiltDown } else { Kind::TiltUp };
        (
            Window {
                cx: 0.5,
                cy: a,
                z: 1.0,
            },
            Window {
                cx: 0.5,
                cy: b,
                z: 1.0,
            },
            kind,
        )
    } else if aspect > f * 1.6 {
        // A panorama: pan across toward the subject.
        let (hx, _) = half_extent(aspect, frame, 1.0);
        let reach = (2.0 * hx * secs_frac).min(1.0 - 2.0 * hx);
        let right = last != Some(Kind::PanRight) && (last == Some(Kind::PanLeft) || fx >= 0.5);
        let end = fx.clamp(hx, 1.0 - hx);
        let start = if right {
            (end - reach).max(hx)
        } else {
            (end + reach).min(1.0 - hx)
        };
        let kind = if end >= start {
            Kind::PanRight
        } else {
            Kind::PanLeft
        };
        (
            Window {
                cx: start,
                cy: 0.5,
                z: 1.0,
            },
            Window {
                cx: end,
                cy: 0.5,
                z: 1.0,
            },
            kind,
        )
    } else {
        // Push in toward the subject, or pull out from it; alternate.
        let zoom = (1.0 + 0.12 * secs_frac).min(1.15);
        let pull = last == Some(Kind::PushIn) || (last.is_none() && n % 2 == 1);
        let wide = Window {
            cx: 0.5,
            cy: 0.5,
            z: 1.0,
        };
        let near = clamp_window(
            Window {
                cx: fx,
                cy: fy,
                z: zoom,
            },
            aspect,
            frame,
        );
        if pull {
            (near, wide, Kind::PullOut)
        } else {
            (wide, near, Kind::PushIn)
        }
    };
    (
        clamp_window(from, aspect, frame),
        clamp_window(to, aspect, frame),
        kind,
    )
}

/// A digest of everything a film is drawn from, for skipping unchanged renders. The track
/// URL is left out: it changes with every presign.
#[must_use]
pub fn fingerprint(reel: &Reel) -> String {
    let mut stable = reel.clone();
    stable.track.url = String::new();
    let json = serde_json::to_vec(&stable).expect("a reel serialises");
    let mut h = Sha256::new();
    h.update(STYLE.as_bytes());
    h.update(&json);
    hex::encode(&h.finalize()[..16])
}

#[cfg(test)]
mod tests {
    use super::*;
    use mantelpiece_core::field_note::{NormBox, Subject};
    use mantelpiece_core::ids::{BatchId, ContributorId};
    use mantelpiece_core::media::{MediaKind, Signals, Tag};
    use mantelpiece_core::trip::{Route, TripDay};
    use time::macros::{date, datetime};

    fn photo(id: &str, w: u32, h: u32, score: f32) -> MediaItem {
        MediaItem {
            id: MediaId::new(id),
            household: HouseholdId::new("h"),
            contributor: ContributorId::new("c"),
            batch: BatchId::new("b"),
            kind: MediaKind::Photo,
            original_key: format!("originals/h/c/{id}.jpg"),
            original_filename: format!("{id}.jpg"),
            bytes: 1,
            sha256: id.into(),
            captured: None,
            gps: None,
            orientation: 1,
            signals: Signals {
                width: Some(w),
                height: Some(h),
                ..Signals::default()
            },
            enrichment: mantelpiece_core::media::Enrichment::default(),
            quality_score: Some(score),
            burst_group: None,
            is_keeper: true,
            trip: Some(TripId::new("t")),
            pinned: false,
            hidden: false,
            pinned_by: std::collections::BTreeSet::new(),
            hidden_by: std::collections::BTreeSet::new(),
            text: None,
            description: None,
            cleanup_reason: None,
            people: vec![],
            removed_at: None,
            restore_until: None,
            field_note: None,
            heading: None,
            state: ProcessingState::Enriched,
            uploaded_at: datetime!(2026-03-25 10:00 UTC),
        }
    }

    fn trip(days: &[&[&str]]) -> Trip {
        Trip {
            id: TripId::new("t"),
            household: HouseholdId::new("h"),
            name: "Florence".into(),
            start: datetime!(2026-03-24 10:00 UTC),
            end: datetime!(2026-03-25 20:00 UTC),
            days: days
                .iter()
                .enumerate()
                .map(|(i, ids)| TripDay {
                    index: u32::try_from(i + 1).unwrap(),
                    date: date!(2026 - 03 - 24),
                    media: ids.iter().map(|s| MediaId::new(*s)).collect(),
                    route: Route::default(),
                    title: None,
                    story: None,
                    undated: vec![],
                    landmarks: vec![],
                })
                .collect(),
            route: Route::default(),
            highlights: vec![],
            contributors: vec![],
            clock_offsets: vec![],
            narrative: None,
            hero: None,
            members: vec![],
        }
    }

    fn request<'a>(
        t: &'a Trip,
        media: &'a [MediaItem],
        day: Option<u32>,
        viewer: &'a HouseholdId,
    ) -> Request<'a> {
        Request {
            trip: t,
            day,
            media,
            viewer,
            frame: Frame::TV,
        }
    }

    #[test]
    fn cuts_land_on_bar_lines_and_slides_are_back_to_back() {
        let ids: Vec<String> = (0..10).map(|i| format!("m{i}")).collect();
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let t = trip(&[&refs]);
        let media: Vec<MediaItem> = ids.iter().map(|i| photo(i, 4032, 3024, 6.0)).collect();
        let viewer = HouseholdId::new("h");
        let reel = plan(&request(&t, &media, Some(1), &viewer)).unwrap();
        let track = crate::library::library()
            .iter()
            .find(|x| x.id == reel.track.id)
            .unwrap();
        assert_eq!(reel.slides.len(), 10);
        assert_eq!(reel.slides[0].start_ms, 0);
        for w in reel.slides.windows(2) {
            assert_eq!(w[0].end_ms, w[1].start_ms);
            assert!(
                track.bars_ms.contains(&w[0].end_ms),
                "cut {} on a bar",
                w[0].end_ms
            );
        }
        assert_eq!(reel.duration_ms, reel.slides.last().unwrap().end_ms);
        assert!(reel.slides[0].duration_ms() >= TITLE_MS);
        assert!(reel.slides.last().unwrap().duration_ms() >= LAST_MS);
        assert!(reel.duration_ms <= track.final_hit_ms);
        assert_eq!(reel.title.text, "Florence");
        assert_eq!(reel.title.sub.as_deref(), Some("Day 1"));
        assert_eq!(reel.credit.end_ms, reel.duration_ms);
    }

    #[test]
    fn windows_stay_inside_the_picture_and_moves_alternate() {
        let ids: Vec<String> = (0..8).map(|i| format!("m{i}")).collect();
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let t = trip(&[&refs]);
        let media: Vec<MediaItem> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| {
                if i % 3 == 0 {
                    photo(id, 3024, 4032, 6.0)
                } else {
                    photo(id, 4032, 3024, 6.0)
                }
            })
            .collect();
        let viewer = HouseholdId::new("h");
        let reel = plan(&request(&t, &media, None, &viewer)).unwrap();
        for s in &reel.slides {
            for w in [s.from, s.to] {
                let (hx, hy) = half_extent(s.aspect, Frame::TV, w.z);
                assert!(
                    w.cx >= hx - 1e-6 && w.cx <= 1.0 - hx + 1e-6,
                    "cx inside {s:?}"
                );
                assert!(
                    w.cy >= hy - 1e-6 && w.cy <= 1.0 - hy + 1e-6,
                    "cy inside {s:?}"
                );
                assert!(w.z >= 1.0);
            }
        }
        // No two consecutive landscape slides push in.
        for w in reel.slides.windows(2) {
            if w[0].aspect > 1.2 && w[1].aspect > 1.2 {
                let push = |s: &Slide| s.to.z > s.from.z;
                assert!(!(push(&w[0]) && push(&w[1])), "push twice: {w:?}");
            }
        }
    }

    #[test]
    fn a_portrait_tilts_toward_its_people_slowly_enough() {
        let mut p = photo("p", 3024, 4032, 6.0);
        p.enrichment.subjects = vec![Subject::new(
            SubjectKind::Person,
            "person",
            NormBox::new(0.4, 0.7, 0.2, 0.25).unwrap(),
            0.9,
        )
        .unwrap()];
        let (from, to, kind) = moves(&p, 0.75, Frame::TV, 2_500, None, 1);
        assert_eq!(kind, Kind::TiltDown);
        assert!(to.cy > from.cy);
        // The band is 42 % of the picture's height; the content moves (to - from) / band
        // frame heights in 2.5 s, which must take at least MIN_CROSS_MS per frame height.
        let band = 0.75 / Frame::TV.aspect();
        let frames_moved = (to.cy - from.cy) / band;
        assert!(frames_moved <= 2_500.0 / MIN_CROSS_MS + 1e-9);
    }

    #[test]
    fn labels_come_only_from_grounded_landmarks_once_per_run() {
        let mut media: Vec<MediaItem> = (0..6)
            .map(|i| photo(&format!("m{i}"), 4032, 3024, 6.0))
            .collect();
        for m in &mut media[1..3] {
            m.enrichment.landmark = Some(Tag {
                name: "Ponte Vecchio".into(),
                confidence: 0.9,
            });
        }
        media[4].enrichment.landmark = Some(Tag {
            name: "Uffizi".into(),
            confidence: 0.6,
        });
        let t = trip(&[&["m0", "m1", "m2", "m3", "m4", "m5"]]);
        let viewer = HouseholdId::new("h");
        let reel = plan(&request(&t, &media, Some(1), &viewer)).unwrap();
        let labels: Vec<_> = reel
            .slides
            .iter()
            .map(|s| s.label.as_ref().map(|l| l.text.as_str()))
            .collect();
        assert_eq!(labels[1], Some("Ponte Vecchio"));
        assert_eq!(labels[2], None, "same landmark again: no second label");
        assert_eq!(labels[4], None, "not confident enough");
        let l = reel.slides[1].label.as_ref().unwrap();
        assert!(l.end_ms - l.start_ms >= 1_000 + 300 * 2);
    }

    #[test]
    fn hidden_removed_and_burst_extras_are_left_out_and_too_few_is_an_error() {
        let mut media: Vec<MediaItem> = (0..4)
            .map(|i| photo(&format!("m{i}"), 4032, 3024, 6.0))
            .collect();
        media[0].hidden = true;
        media[1].burst_group = Some("g".into());
        media[1].is_keeper = false;
        let t = trip(&[&["m0", "m1", "m2", "m3"]]);
        let viewer = HouseholdId::new("h");
        assert_eq!(
            plan(&request(&t, &media, Some(1), &viewer)),
            Err(PlanError::TooFewPhotos)
        );
        assert_eq!(
            plan(&request(&t, &media, Some(9), &viewer)),
            Err(PlanError::NoSuchDay)
        );
    }

    #[test]
    fn a_long_day_keeps_the_best_and_fits_the_track() {
        let ids: Vec<String> = (0..120).map(|i| format!("m{i:03}")).collect();
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let t = trip(&[&refs]);
        #[allow(clippy::cast_precision_loss)]
        let media: Vec<MediaItem> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| photo(id, 4032, 3024, (i % 10) as f32))
            .collect();
        let viewer = HouseholdId::new("h");
        let reel = plan(&request(&t, &media, Some(1), &viewer)).unwrap();
        assert!(reel.slides.len() <= MAX_DAY_SLIDES);
        // Capture order is kept.
        let order: Vec<&str> = reel.slides.iter().map(|s| s.media.as_str()).collect();
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(order, sorted);
    }

    #[test]
    fn the_plan_and_its_fingerprint_are_deterministic() {
        let ids: Vec<String> = (0..9).map(|i| format!("m{i}")).collect();
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let t = trip(&[&refs[..4], &refs[4..]]);
        let media: Vec<MediaItem> = ids.iter().map(|i| photo(i, 4032, 3024, 6.0)).collect();
        let viewer = HouseholdId::new("h");
        let a = plan(&request(&t, &media, None, &viewer)).unwrap();
        let mut b = plan(&request(&t, &media, None, &viewer)).unwrap();
        assert_eq!(a, b);
        b.track.url = "https://presigned".into();
        assert_eq!(fingerprint(&a), fingerprint(&b));
    }
}
