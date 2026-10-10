//! Trip sketch maps (F-712): the map a traveller would draw in the back of a journal, of a
//! whole trip or of one day. Real geography from the same tiles as field notes, drawn with
//! a hand that wanders a little (`hand`), the route as dashed ink with the days circled,
//! little drawings at the trip's places (`pictogram`). Two forms: the owner's, and a public
//! one without the soft area marking home. Route points near home are left out of both.

use crate::contours;
use crate::drawing::{self, Class, LabelKind};
use crate::geo::{self, TileId, View, World};
use crate::hand::{self, Pen};
use crate::pictogram::{self, Kind};
use crate::raster::{self, paint};
use crate::{certainty, fetch, Skip, Tiles};
use mantelpiece_core::geo::GeoPoint;
use mantelpiece_core::sketch::Which;
use mantelpiece_core::trip::{Landmark, Trip, TripDay};
use sha2::{Digest, Sha256};
use tiny_skia::{FillRule, Pixmap, Transform};
use ttf_parser::Face;

/// Bumped whenever the drawing changes enough that stored sketch maps should be redrawn.
pub const SKETCH_STYLE: u32 = 1;
/// Picture size: 16:9 for the TV.
pub const W: u32 = 1600;
pub const H: u32 = 900;
/// A gap this long between two points is a flight or a ferry, not a drive.
const HOP_M: f64 = 300_000.0;
const MAX_PICTOGRAMS: usize = 12;
const MAX_PLACE_LABELS: usize = 7;
/// Most tiles fetched per map.
const MAX_TILES: usize = 16;

const INK: (u8, u8, u8) = (58, 46, 38);
const RED: (u8, u8, u8) = (176, 52, 40);
const SEA: (u8, u8, u8) = (104, 156, 196);
const ICE: (u8, u8, u8) = (196, 222, 236);
const RIVER: (u8, u8, u8) = (70, 115, 160);
const ROAD: (u8, u8, u8) = (128, 92, 66);
const PENCIL: (u8, u8, u8) = (96, 86, 76);
const SOFT: (u8, u8, u8) = (110, 92, 76);

/// A drawn trip or day map in both forms.
#[derive(Debug)]
pub struct Sketch {
    pub owner: Vec<u8>,
    pub public: Vec<u8>,
    pub fingerprint: String,
    pub credit: String,
}

#[derive(Clone, Copy, Debug)]
struct Pt {
    at: GeoPoint,
    estimated: bool,
}

#[derive(Clone, Debug)]
struct Leg {
    day: u32,
    pts: Vec<Pt>,
    /// A flight or ferry: drawn as a dotted arc, kept out of the frame.
    hop: bool,
}

fn days_of(trip: &Trip, which: Which) -> Vec<&TripDay> {
    match which {
        Which::Trip => trip.days.iter().collect(),
        Which::Day(n) => trip.days.iter().filter(|d| d.index == n).collect(),
    }
}

fn near_home(home: Option<GeoPoint>, at: GeoPoint) -> bool {
    home.is_some_and(|h| h.distance_m(at) <= f64::from(certainty::HOME_RADIUS_M))
}

/// The route as legs, one or more per day, with points near home taken out. On the trip map
/// each day's leg starts from the previous day's last point, so the line is unbroken.
fn legs(trip: &Trip, which: Which, home: Option<GeoPoint>) -> Vec<Leg> {
    let mut out = Vec::new();
    let mut prev: Option<Pt> = None;
    for day in days_of(trip, which) {
        let pts: Vec<Pt> = day
            .route
            .points
            .iter()
            .filter(|p| !near_home(home, p.at))
            .map(|p| Pt {
                at: p.at,
                estimated: p.estimated || day.route.inferred,
            })
            .collect();
        let mut seq: Vec<Pt> = prev.into_iter().chain(pts.iter().copied()).collect();
        if which != Which::Trip {
            seq.clone_from(&pts);
        }
        let mut cur: Vec<Pt> = Vec::new();
        for p in seq {
            if let Some(last) = cur.last().copied() {
                if last.at.distance_m(p.at) > HOP_M {
                    out.push(Leg {
                        day: day.index,
                        pts: std::mem::take(&mut cur),
                        hop: false,
                    });
                    out.push(Leg {
                        day: day.index,
                        pts: vec![last, p],
                        hop: true,
                    });
                }
            }
            cur.push(p);
        }
        if !cur.is_empty() {
            out.push(Leg {
                day: day.index,
                pts: cur,
                hop: false,
            });
        }
        if let Some(last) = pts.last() {
            prev = Some(*last);
        }
    }
    out.retain(|l| !l.pts.is_empty());
    out
}

/// The places worth a drawing: not a municipality or region, not noise, not near home.
/// Those the family took photos at come first, then the more prominent.
fn places(trip: &Trip, which: Which, home: Option<GeoPoint>) -> Vec<&Landmark> {
    let mut seen: Vec<&str> = Vec::new();
    let mut out: Vec<&Landmark> = Vec::new();
    for l in days_of(trip, which)
        .into_iter()
        .flat_map(|d| d.landmarks.iter())
    {
        let lower = l.name.to_lowercase();
        let open_water = ["ocean", "sea", "atlantic", "pacific"]
            .iter()
            .any(|w| lower.contains(w));
        if l.is_region() || l.prominence() == 0 || open_water || near_home(home, l.at) {
            continue;
        }
        if seen.contains(&l.name.as_str()) {
            continue;
        }
        seen.push(&l.name);
        out.push(l);
    }
    out.sort_by(|a, b| {
        a.media
            .is_empty()
            .cmp(&b.media.is_empty())
            .then(b.prominence().cmp(&a.prominence()))
            .then(a.distance_m.total_cmp(&b.distance_m))
    });
    out
}

/// What a map is drawn from; a different fingerprint means draw again.
#[must_use]
pub fn fingerprint(trip: &Trip, which: Which, home: Option<GeoPoint>) -> String {
    let mut h = Sha256::new();
    h.update(format!("sketch/{SKETCH_STYLE}/{which:?}"));
    if which == Which::Trip {
        h.update(&trip.name);
        h.update(trip.start.unix_timestamp().to_le_bytes());
        h.update(trip.end.unix_timestamp().to_le_bytes());
    }
    for d in days_of(trip, which) {
        h.update(format!(
            "|d{}|{}|{:?}|{}",
            d.index, d.date, d.title, d.route.inferred
        ));
        for p in &d.route.points {
            h.update(format!("{:.5},{:.5},{}", p.at.lat, p.at.lon, p.estimated));
        }
        for l in &d.landmarks {
            h.update(format!(
                "|{}|{:?}|{:.4},{:.4}|{}",
                l.name,
                l.kind,
                l.at.lat,
                l.at.lon,
                l.media.is_empty()
            ));
        }
    }
    if let Some(home) = home {
        h.update(format!("home{:.4},{:.4}", home.lat, home.lon));
    }
    hex::encode(h.finalize())
}

/// A single point reached and left only by flights or ferries: a photo from the plane or
/// the airport on the way. Drawn at the end of its arc, but it does not set the frame.
fn stranded(leg: &Leg, legs: &[Leg]) -> bool {
    leg.pts.len() == 1
        && legs
            .iter()
            .filter(|l| l.hop)
            .any(|h| h.pts.iter().any(|p| p.at == leg.pts[0].at))
}

fn lat_of(y: f64) -> f64 {
    (std::f64::consts::PI * (1.0 - 2.0 * y))
        .sinh()
        .atan()
        .to_degrees()
}

/// The ground a map covers: every route point (flights and ferries aside), or the day's
/// places when it has no route, with a margin, at least `min_m` across, at 16:9.
fn frame(legs: &[Leg], fallback: &[GeoPoint], min_m: f64) -> Option<View> {
    let mut pts: Vec<GeoPoint> = legs
        .iter()
        .filter(|l| !l.hop && !stranded(l, legs))
        .flat_map(|l| l.pts.iter().map(|p| p.at))
        .collect();
    if pts.is_empty() {
        pts = legs
            .iter()
            .flat_map(|l| l.pts.iter().map(|p| p.at))
            .collect();
    }
    if pts.is_empty() {
        pts = fallback.to_vec();
    }
    if pts.is_empty() {
        return None;
    }
    let ws: Vec<World> = pts.iter().map(|p| geo::world(*p)).collect();
    let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for w in &ws {
        x0 = x0.min(w.x);
        x1 = x1.max(w.x);
        y0 = y0.min(w.y);
        y1 = y1.max(w.y);
    }
    let (cx, cy) = (f64::midpoint(x0, x1), f64::midpoint(y0, y1));
    let lat = lat_of(cy);
    let min_w = min_m / geo::metres_per_unit(lat);
    let mut w = ((x1 - x0) * 1.3).max(min_w);
    let mut hgt = (y1 - y0) * 1.36;
    let aspect = f64::from(W) / f64::from(H);
    if w / hgt.max(1e-12) < aspect {
        w = hgt * aspect;
    } else {
        hgt = w / aspect;
    }
    Some(View {
        centre: World { x: cx, y: cy },
        half_w: w / 2.0,
        half_h: hgt / 2.0,
        lat,
    })
}

fn tiles_for(view: &View, start: u8, max: u8) -> (u8, Vec<TileId>) {
    let mut z = start.min(max);
    let mut ids = view.tiles(z);
    while ids.len() > MAX_TILES && z > 0 {
        z -= 1;
        ids = view.tiles(z);
    }
    (z, ids)
}

/// A label's baseline-left corner and its text.
type Placed = ([f32; 2], String);

/// Everything placed on the sheet before inking, shared by both forms.
struct Layout {
    view: View,
    features: drawing::Features,
    hachures: Vec<Vec<[f32; 2]>>,
    legs: Vec<Leg>,
    dots: Vec<[f32; 2]>,
    /// Day number, the day's last point, and where its circle sits.
    days: Vec<(u32, [f32; 2], [f32; 2])>,
    pictograms: Vec<(Kind, [f32; 2], Option<Placed>)>,
    places: Vec<([f32; 2], LabelKind, Option<Placed>)>,
    title: Option<([f32; 4], String, String)>,
    compass: [f32; 2],
    scale: ([f32; 2], f32, String),
    home: Option<([f32; 2], f32)>,
    credit: String,
    seed: u64,
}

type Rect = [f32; 4];

fn hits(a: Rect, b: Rect) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

fn free(r: Rect, taken: &[Rect]) -> bool {
    let (w, h) = (W as f32, H as f32);
    r[0] >= 20.0
        && r[1] >= 20.0
        && r[2] <= w - 20.0
        && r[3] <= h - 20.0
        && !taken.iter().any(|t| hits(r, *t))
}

fn px(view: &View, p: GeoPoint) -> [f32; 2] {
    let (u, v) = view.to_box(geo::world(p));
    [u * W as f32, v * H as f32]
}

/// A label beside a mark at `at`, in the first free spot of right, left, above, below.
fn place_label(
    face: &Face<'_>,
    text: &str,
    size: f32,
    at: [f32; 2],
    gap: f32,
    taken: &mut Vec<Rect>,
) -> Option<([f32; 2], String)> {
    if text
        .chars()
        .any(|c| !c.is_whitespace() && face.glyph_index(c).is_none())
    {
        return None;
    }
    let w = hand::measure(face, text, size);
    let tries = [
        [at[0] + gap, at[1] + size * 0.35],
        [at[0] - gap - w, at[1] + size * 0.35],
        [at[0] - w / 2.0, at[1] - gap],
        [at[0] - w / 2.0, at[1] + gap + size * 0.8],
    ];
    for b in tries {
        let r = [
            b[0] - 3.0,
            b[1] - size * 0.85,
            b[0] + w + 3.0,
            b[1] + size * 0.3,
        ];
        if free(r, taken) {
            taken.push(r);
            return Some((b, text.to_owned()));
        }
    }
    None
}

fn month(m: time::Month) -> &'static str {
    use time::Month::{
        April, August, December, February, January, July, June, March, May, November, October,
        September,
    };
    match m {
        January => "January",
        February => "February",
        March => "March",
        April => "April",
        May => "May",
        June => "June",
        July => "July",
        August => "August",
        September => "September",
        October => "October",
        November => "November",
        December => "December",
    }
}

/// "27 June – 4 July 2015", "3 – 9 May 2015", "30 December 2015 – 2 January 2016".
#[must_use]
pub fn dates(start: time::Date, end: time::Date) -> String {
    if start == end {
        format!("{} {} {}", start.day(), month(start.month()), start.year())
    } else if start.year() != end.year() {
        format!(
            "{} {} {} – {} {} {}",
            start.day(),
            month(start.month()),
            start.year(),
            end.day(),
            month(end.month()),
            end.year()
        )
    } else if start.month() == end.month() {
        format!(
            "{} – {} {} {}",
            start.day(),
            end.day(),
            month(end.month()),
            end.year()
        )
    } else {
        format!(
            "{} {} – {} {} {}",
            start.day(),
            month(start.month()),
            end.day(),
            month(end.month()),
            end.year()
        )
    }
}

fn nice_length(m: f64) -> f64 {
    let p = 10f64.powf(m.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|k| k * p)
        .rfind(|v| *v <= m)
        .unwrap_or(p)
}

fn length_label(m: f64) -> String {
    if m >= 1000.0 {
        format!("{} km", (m / 1000.0).round())
    } else {
        format!("{} m", m.round())
    }
}

/// Hachures: short strokes across each contour line, pointing downhill.
fn hachures(terrain: &contours::Terrain, view: &View, relief: f64) -> Vec<Vec<[f32; 2]>> {
    let interval = contours::interval(relief) * 2.0;
    let (w, h) = (W as f32, H as f32);
    let mut out = Vec::new();
    for shape in terrain.contours(view, interval) {
        for part in &shape.parts {
            let line: Vec<[f32; 2]> = part.iter().map(|p| [p[0] * w, p[1] * h]).collect();
            let mut walked = 0.0;
            for seg in line.windows(2) {
                let (a, b) = (seg[0], seg[1]);
                let len = (b[0] - a[0]).hypot(b[1] - a[1]);
                if len < 1e-3 {
                    continue;
                }
                walked += len;
                if walked < 11.0 {
                    continue;
                }
                walked = 0.0;
                let (nx, ny) = (-(b[1] - a[1]) / len, (b[0] - a[0]) / len);
                let el = |x: f32, y: f32| terrain.at(view, f64::from(x / w), f64::from(y / h));
                let (Some(e1), Some(e2)) = (
                    el(b[0] + nx * 4.0, b[1] + ny * 4.0),
                    el(b[0] - nx * 4.0, b[1] - ny * 4.0),
                ) else {
                    continue;
                };
                // The terrain tiles carry the seabed too; hatch hills, not the sea floor.
                if e1 <= 0.0 || e2 <= 0.0 {
                    continue;
                }
                let s = if e1 < e2 { 1.0 } else { -1.0 };
                out.push(vec![b, [b[0] + nx * s * 6.5, b[1] + ny * s * 6.5]]);
            }
        }
    }
    out
}

/// Fetch, decide and place everything; nothing is inked yet.
#[allow(clippy::too_many_lines)]
fn layout(
    trip: &Trip,
    which: Which,
    home: Option<GeoPoint>,
    tiles: &dyn Tiles,
    face: &Face<'_>,
) -> Result<Layout, Skip> {
    let legs = legs(trip, which, home);
    let places_all = places(trip, which, home);
    let fallback: Vec<GeoPoint> = places_all.iter().map(|l| l.at).collect();
    let min_m = if which == Which::Trip {
        5_000.0
    } else {
        2_000.0
    };
    let view = frame(&legs, &fallback, min_m).ok_or(Skip::NoPlace)?;

    let (_, ids) = tiles_for(
        &view,
        view.zoom_for(800.0, geo::MAX_VECTOR_ZOOM),
        geo::MAX_VECTOR_ZOOM,
    );
    let vector = fetch(tiles, &ids, true)?;
    let aspect = W as f32 / H as f32;
    let features = drawing::features(&vector, &view, aspect, W as f32);
    if features.fills.is_empty() && features.strokes.is_empty() {
        return Err(Skip::MapUnavailable);
    }
    let mut credit = drawing::OSM_CREDIT.to_owned();
    let (tz, tids) = tiles_for(&view, view.zoom_for(800.0, 12) + 1, 12);
    let _ = tz;
    let mut hatch = Vec::new();
    if let Ok(t) = fetch(tiles, &tids, false) {
        let decoded: Vec<_> = t
            .into_iter()
            .filter_map(|(id, png)| contours::decode(&png).map(|m| (id, m)))
            .collect();
        if let Some(terrain) = contours::stitch(&decoded) {
            let (lo, hi) = terrain.relief(&view);
            if lo.is_finite() && hi - lo > 200.0 {
                hatch = hachures(&terrain, &view, hi - lo);
                if !hatch.is_empty() {
                    credit = format!("{credit} · {}", drawing::TERRAIN_CREDIT);
                }
            }
        }
    }

    let seed = hand::seed(
        trip.id.as_str(),
        match which {
            Which::Trip => 0,
            Which::Day(n) => u64::from(n),
        },
    );
    let mut taken: Vec<Rect> = Vec::new();
    let route_px: Vec<[f32; 2]> = legs
        .iter()
        .filter(|l| !l.hop)
        .flat_map(|l| l.pts.iter().map(|p| px(&view, p.at)))
        .collect();
    let crowd = |r: Rect| {
        route_px
            .iter()
            .filter(|p| p[0] >= r[0] && p[0] <= r[2] && p[1] >= r[1] && p[1] <= r[3])
            .count()
    };
    let (w, h) = (W as f32, H as f32);
    let corners = |bw: f32, bh: f32| -> [Rect; 4] {
        [
            [36.0, 36.0, 36.0 + bw, 36.0 + bh],
            [w - 36.0 - bw, 36.0, w - 36.0, 36.0 + bh],
            [36.0, h - 36.0 - bh, 36.0 + bw, h - 36.0],
            [w - 36.0 - bw, h - 36.0 - bh, w - 36.0, h - 36.0],
        ]
    };

    // Title box (trip) or day note.
    let (line1, line2, s1) = match which {
        Which::Trip => (
            trip.name.clone(),
            dates(trip.start.date(), trip.end.date()),
            46.0,
        ),
        Which::Day(n) => {
            let d = trip.days.iter().find(|d| d.index == n);
            (
                d.and_then(|d| d.title.clone())
                    .unwrap_or_else(|| format!("Day {n}")),
                d.map(|d| dates(d.date, d.date)).unwrap_or_default(),
                34.0,
            )
        }
    };
    let bw = hand::measure(face, &line1, s1).max(hand::measure(face, &line2, 24.0)) + 56.0;
    let bh = s1 + 24.0 + 40.0;
    let title_rect = corners(bw, bh)
        .into_iter()
        .min_by_key(|r| crowd(*r))
        .unwrap_or([36.0, 36.0, 36.0 + bw, 36.0 + bh]);
    taken.push(title_rect);
    let title = Some((title_rect, line1, line2));

    // Credit along the bottom right, scale bar bottom left, compass in the freest corner.
    let credit_w = hand::measure(face, &credit, 17.0);
    taken.push([w - 40.0 - credit_w, h - 48.0, w - 30.0, h - 22.0]);
    let mpp = view.width_m() / f64::from(W);
    let metres = nice_length(220.0 * mpp);
    #[allow(clippy::cast_possible_truncation)]
    let bar = (metres / mpp) as f32;
    let scale_at = if title_rect[0] < 100.0 && title_rect[1] > h / 2.0 {
        [w - 60.0 - bar.max(credit_w), h - 70.0]
    } else {
        [56.0, h - 56.0]
    };
    taken.push([
        scale_at[0] - 10.0,
        scale_at[1] - 34.0,
        scale_at[0] + bar + 70.0,
        scale_at[1] + 14.0,
    ]);
    let compass_rect = corners(70.0, 90.0)
        .into_iter()
        .filter(|r| free(*r, &taken))
        .min_by_key(|r| crowd(*r))
        .unwrap_or([w - 106.0, 36.0, w - 36.0, 126.0]);
    taken.push(compass_rect);
    let compass = [
        f32::midpoint(compass_rect[0], compass_rect[2]),
        f32::midpoint(compass_rect[1], compass_rect[3]) + 8.0,
    ];

    // Day numbers at each day's last point.
    let mut days: Vec<(u32, [f32; 2], [f32; 2])> = Vec::new();
    let mut circles: Vec<Rect> = Vec::new();
    for d in days_of(trip, which) {
        if let Some(p) = legs
            .iter()
            .filter(|l| l.day == d.index && !l.hop && !stranded(l, &legs))
            .filter_map(|l| l.pts.last())
            .next_back()
        {
            let at = px(&view, p.at);
            // Where days end at the same place, step the circle aside on a short leader.
            let spots = std::iter::once([0.0, 0.0]).chain((0..16).map(|i| {
                let a = i as f32 / 8.0 * std::f32::consts::PI - std::f32::consts::FRAC_PI_2;
                let r = if i < 8 { 40.0 } else { 74.0 };
                [r * a.cos(), r * a.sin()]
            }));
            let mut chosen = at;
            for o in spots {
                let c = [at[0] + o[0], at[1] + o[1]];
                let r = [c[0] - 17.0, c[1] - 17.0, c[0] + 17.0, c[1] + 17.0];
                if !circles.iter().any(|t| hits(r, *t)) && free(r, &taken[..taken.len().min(4)]) {
                    chosen = c;
                    break;
                }
            }
            let r = [
                chosen[0] - 17.0,
                chosen[1] - 17.0,
                chosen[0] + 17.0,
                chosen[1] + 17.0,
            ];
            circles.push(r);
            taken.push(r);
            days.push((d.index, at, chosen));
        }
    }

    // Photo spots on day maps: GPS points only.
    let dots: Vec<[f32; 2]> = if which == Which::Trip {
        vec![]
    } else {
        legs.iter()
            .filter(|l| !l.hop)
            .flat_map(|l| {
                l.pts
                    .iter()
                    .filter(|p| !p.estimated)
                    .map(|p| px(&view, p.at))
            })
            .collect()
    };

    // Drawings at the trip's places, then a few names from the map itself.
    let mut pictograms = Vec::new();
    for l in places_all {
        if pictograms.len() >= MAX_PICTOGRAMS {
            break;
        }
        let at = px(&view, l.at);
        let r = [at[0] - 15.0, at[1] - 15.0, at[0] + 15.0, at[1] + 15.0];
        if !free(r, &taken) {
            continue;
        }
        taken.push(r);
        let label = place_label(face, &l.name, 22.0, at, 19.0, &mut taken);
        pictograms.push((pictogram::kind_of(&l.name, l.kind.as_deref()), at, label));
    }
    let named: Vec<String> = pictograms
        .iter()
        .filter_map(|p| p.2.as_ref().map(|l| l.1.clone()))
        .collect();
    let mut cands: Vec<_> = features
        .candidates
        .iter()
        .filter(|c| {
            matches!(
                c.label.kind,
                LabelKind::Place | LabelKind::Peak | LabelKind::Water
            )
        })
        .filter(|c| c.label.icon.is_none() || c.label.kind == LabelKind::Peak)
        .filter(|c| !named.contains(&c.label.text))
        .collect();
    cands.sort_by(|a, b| a.priority.total_cmp(&b.priority));
    let mut places = Vec::new();
    for c in cands {
        if places.len() >= MAX_PLACE_LABELS {
            break;
        }
        let at = [c.label.x * w, c.label.y * h];
        let r = [at[0] - 5.0, at[1] - 5.0, at[0] + 5.0, at[1] + 5.0];
        if !free(r, &taken) {
            continue;
        }
        let size = if c.label.kind == LabelKind::Water {
            20.0
        } else {
            19.0
        };
        if let Some(label) = place_label(face, &c.label.text, size, at, 9.0, &mut taken) {
            taken.push(r);
            places.push((at, c.label.kind, Some(label)));
        }
    }

    let home_mark = home.and_then(|hm| {
        let at = px(&view, hm);
        let r = (view.metres_to_box(f64::from(certainty::HOME_RADIUS_M)) * w).max(16.0);
        (at[0] > -r && at[0] < w + r && at[1] > -r && at[1] < h + r).then_some((at, r))
    });

    Ok(Layout {
        view,
        features,
        hachures: hatch,
        legs,
        dots,
        days,
        pictograms,
        places,
        title,
        compass,
        scale: (scale_at, bar, length_label(metres)),
        home: home_mark,
        credit,
        seed,
    })
}

fn circle_line(c: [f32; 2], r: f32) -> Vec<[f32; 2]> {
    (0..=28)
        .map(|i| {
            let a = i as f32 / 28.0 * std::f32::consts::TAU;
            [c[0] + r * a.cos(), c[1] + r * a.sin()]
        })
        .collect()
}

/// Polygons of a class, one per shape, in pixels.
fn polygons_px(shapes: &[drawing::Shape], class: Class) -> Vec<Vec<Vec<[f32; 2]>>> {
    shapes
        .iter()
        .filter(|s| s.class == class)
        .map(|s| {
            s.parts
                .iter()
                .map(|p| {
                    p.iter()
                        .map(|q| [q[0] * W as f32, q[1] * H as f32])
                        .collect()
                })
                .collect()
        })
        .collect()
}

fn shapes_px(shapes: &[drawing::Shape], class: Class) -> Vec<Vec<[f32; 2]>> {
    shapes
        .iter()
        .filter(|s| s.class == class)
        .flat_map(|s| s.parts.iter())
        .map(|p| {
            p.iter()
                .map(|q| [q[0] * W as f32, q[1] * H as f32])
                .collect()
        })
        .collect()
}

/// Ink the sheet. `public` leaves out the soft area around home.
#[allow(clippy::too_many_lines)]
fn ink(l: &Layout, which: Which, public: bool, face: &Face<'_>) -> Pixmap {
    let mut map = Pixmap::new(W, H).expect("sheet size");
    hand::paper(&mut map, l.seed);
    let s = l.seed;
    let pen = |rgb, width, alpha, wander| Pen {
        rgb,
        width,
        alpha,
        wander,
        dash: None,
        second_pass: false,
    };

    hand::wash(
        &mut map,
        &polygons_px(&l.features.fills, Class::Water),
        SEA,
        0.42,
        s ^ 1,
    );
    hand::wash(
        &mut map,
        &polygons_px(&l.features.fills, Class::Ice),
        ICE,
        0.85,
        s ^ 2,
    );
    hand::ink(&mut map, &l.hachures, pen(PENCIL, 0.9, 0.5, 0.3), s ^ 3);
    if which != Which::Trip {
        hand::ink(
            &mut map,
            &shapes_px(&l.features.strokes, Class::RoadMinor),
            pen(ROAD, 0.8, 0.4, 0.6),
            s ^ 4,
        );
    }
    hand::ink(
        &mut map,
        &shapes_px(&l.features.strokes, Class::RoadMajor),
        pen(ROAD, 1.3, 0.62, 0.9),
        s ^ 5,
    );
    hand::ink(
        &mut map,
        &shapes_px(&l.features.strokes, Class::River),
        pen(RIVER, 1.1, 0.75, 1.0),
        s ^ 6,
    );
    hand::ink(
        &mut map,
        &shapes_px(&l.features.strokes, Class::Coast),
        Pen {
            second_pass: true,
            ..pen(INK, 1.7, 0.9, 1.3)
        },
        s ^ 7,
    );

    if let (Some((at, r)), false) = (l.home, public) {
        hand::wash(&mut map, &[vec![circle_line(at, r)]], RED, 0.3, s ^ 8);
    }

    // The route.
    for (i, leg) in l.legs.iter().enumerate() {
        let k = s ^ (0x100 + i as u64);
        let pts: Vec<[f32; 2]> = leg.pts.iter().map(|p| px(&l.view, p.at)).collect();
        if leg.hop {
            let (a, b) = (pts[0], pts[pts.len() - 1]);
            let mid = [f32::midpoint(a[0], b[0]), f32::midpoint(a[1], b[1])];
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let c = [mid[0] - dy * 0.18, mid[1] + dx * 0.18];
            let arc: Vec<[f32; 2]> = (0..=32)
                .map(|j| {
                    let t = j as f32 / 32.0;
                    let u = 1.0 - t;
                    [
                        u * u * a[0] + 2.0 * u * t * c[0] + t * t * b[0],
                        u * u * a[1] + 2.0 * u * t * c[1] + t * t * b[1],
                    ]
                })
                .collect();
            hand::ink(
                &mut map,
                &[arc],
                Pen {
                    dash: Some((2.5, 8.0)),
                    ..pen(RED, 1.6, 0.75, 0.4)
                },
                k,
            );
            continue;
        }
        // Runs of GPS and of estimated points, drawn differently.
        let mut run: Vec<[f32; 2]> = Vec::new();
        let mut est = leg.pts.first().is_some_and(|p| p.estimated);
        for (j, p) in leg.pts.iter().enumerate() {
            let e = p.estimated;
            if e != est && !run.is_empty() {
                run.push(pts[j]);
                draw_run(&mut map, &run, est, k ^ j as u64);
                run = vec![pts[j]];
                est = e;
            } else {
                run.push(pts[j]);
            }
        }
        if run.len() > 1 {
            draw_run(&mut map, &run, est, k);
        }
    }
    for (i, d) in l.dots.iter().enumerate() {
        if let Some(c) = raster::circle(d[0], d[1], 3.4) {
            map.fill_path(
                &c,
                &paint(RED, 0.8),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
        let _ = i;
    }
    for (n, end, at) in &l.days {
        if (end[0] - at[0]).hypot(end[1] - at[1]) > 0.5 {
            hand::ink(
                &mut map,
                &[vec![*end, *at]],
                pen(RED, 1.0, 0.7, 0.3),
                s ^ (0x800 + u64::from(*n)),
            );
        }
        if let Some(c) = raster::circle(at[0], at[1], 15.0) {
            map.fill_path(
                &c,
                &paint((246, 239, 222), 0.95),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
        hand::ink(
            &mut map,
            &[circle_line(*at, 15.0)],
            Pen {
                second_pass: true,
                ..pen(RED, 1.8, 0.95, 0.9)
            },
            s ^ (0x900 + u64::from(*n)),
        );
        let t = n.to_string();
        let tw = hand::measure(face, &t, 23.0);
        hand::letter(
            &mut map,
            face,
            &t,
            23.0,
            at[0] - tw / 2.0,
            at[1] + 8.0,
            RED,
            1.0,
        );
    }

    // Places.
    for (i, (kind, at, label)) in l.pictograms.iter().enumerate() {
        hand::ink(
            &mut map,
            &pictogram::strokes(*kind, at[0], at[1], 24.0),
            pen(INK, 1.5, 0.95, 0.4),
            s ^ (0x500 + i as u64),
        );
        if let Some((b, t)) = label {
            hand::letter(&mut map, face, t, 22.0, b[0], b[1], INK, 1.0);
        }
    }
    for (i, (at, kind, label)) in l.places.iter().enumerate() {
        match kind {
            LabelKind::Peak => hand::ink(
                &mut map,
                &pictogram::strokes(Kind::Peak, at[0], at[1], 12.0),
                pen(SOFT, 1.1, 0.8, 0.2),
                s ^ (0x700 + i as u64),
            ),
            LabelKind::Water => {}
            _ => {
                if let Some(c) = raster::circle(at[0], at[1], 3.0) {
                    map.fill_path(
                        &c,
                        &paint(SOFT, 0.85),
                        FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                }
            }
        }
        if let Some((b, t)) = label {
            let rgb = if *kind == LabelKind::Water {
                RIVER
            } else {
                SOFT
            };
            let size = if *kind == LabelKind::Water {
                20.0
            } else {
                19.0
            };
            hand::letter(&mut map, face, t, size, b[0], b[1], rgb, 0.95);
        }
    }

    // Title box.
    if let Some((r, a, b)) = &l.title {
        let rect = |inset: f32| {
            vec![
                [r[0] + inset, r[1] + inset],
                [r[2] - inset, r[1] + inset],
                [r[2] - inset, r[3] - inset],
                [r[0] + inset, r[3] - inset],
                [r[0] + inset, r[1] + inset],
            ]
        };
        if let Some(rr) = tiny_skia::Rect::from_ltrb(r[0], r[1], r[2], r[3]) {
            let p = tiny_skia::PathBuilder::from_rect(rr);
            map.fill_path(
                &p,
                &paint((247, 240, 224), 0.93),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
        hand::ink(
            &mut map,
            &[rect(0.0)],
            Pen {
                second_pass: true,
                ..pen(INK, 1.6, 0.9, 1.0)
            },
            s ^ 0xa1,
        );
        hand::ink(&mut map, &[rect(6.0)], pen(INK, 0.8, 0.7, 0.8), s ^ 0xa2);
        let s1 = if which == Which::Trip { 46.0 } else { 34.0 };
        hand::letter(
            &mut map,
            face,
            a,
            s1,
            r[0] + 28.0,
            r[1] + 22.0 + s1 * 0.85,
            INK,
            1.0,
        );
        hand::letter(&mut map, face, b, 24.0, r[0] + 28.0, r[3] - 22.0, SOFT, 1.0);
    }

    // Compass.
    let c = l.compass;
    hand::ink(
        &mut map,
        &[
            vec![[c[0], c[1] + 26.0], [c[0], c[1] - 26.0]],
            vec![
                [c[0] - 7.0, c[1] - 14.0],
                [c[0], c[1] - 27.0],
                [c[0] + 7.0, c[1] - 14.0],
            ],
            vec![[c[0] - 14.0, c[1]], [c[0] + 14.0, c[1]]],
        ],
        pen(INK, 1.5, 0.9, 0.3),
        s ^ 0xc0,
    );
    let nw = hand::measure(face, "N", 22.0);
    hand::letter(
        &mut map,
        face,
        "N",
        22.0,
        c[0] - nw / 2.0,
        c[1] - 32.0,
        INK,
        1.0,
    );

    // Scale bar.
    let (at, bar, label) = &l.scale;
    hand::ink(
        &mut map,
        &[
            vec![*at, [at[0] + bar, at[1]]],
            vec![[at[0], at[1] - 6.0], [at[0], at[1] + 6.0]],
            vec![
                [at[0] + bar / 2.0, at[1] - 4.0],
                [at[0] + bar / 2.0, at[1] + 4.0],
            ],
            vec![[at[0] + bar, at[1] - 6.0], [at[0] + bar, at[1] + 6.0]],
        ],
        pen(INK, 1.5, 0.85, 0.5),
        s ^ 0xd0,
    );
    hand::letter(
        &mut map,
        face,
        label,
        20.0,
        at[0] + bar + 10.0,
        at[1] + 7.0,
        INK,
        0.9,
    );

    // Border and credit.
    let (w, h) = (W as f32, H as f32);
    hand::ink(
        &mut map,
        &[vec![
            [16.0, 16.0],
            [w - 16.0, 16.0],
            [w - 16.0, h - 16.0],
            [16.0, h - 16.0],
            [16.0, 16.0],
        ]],
        Pen {
            second_pass: true,
            ..pen(INK, 1.4, 0.7, 1.2)
        },
        s ^ 0xe0,
    );
    let cw = hand::measure(face, &l.credit, 17.0);
    hand::letter(
        &mut map,
        face,
        &l.credit,
        17.0,
        w - 34.0 - cw,
        h - 28.0,
        SOFT,
        0.8,
    );
    map
}

fn draw_run(map: &mut Pixmap, run: &[[f32; 2]], estimated: bool, seed: u64) {
    let pen = if estimated {
        Pen {
            rgb: RED,
            width: 2.1,
            alpha: 0.45,
            wander: 1.0,
            dash: Some((6.0, 11.0)),
            second_pass: false,
        }
    } else {
        Pen {
            rgb: RED,
            width: 2.6,
            alpha: 0.92,
            wander: 1.0,
            dash: Some((12.0, 6.0)),
            second_pass: true,
        }
    };
    hand::ink(map, &[run.to_vec()], pen, seed);
}

/// Draw one map of a trip in both forms.
///
/// # Errors
/// `Skip::NoPlace` when the trip or day has no known place; `Skip::MapUnavailable` when the
/// map data cannot be fetched (try again later).
pub fn draw(
    trip: &Trip,
    which: Which,
    home: Option<GeoPoint>,
    tiles: &dyn Tiles,
) -> Result<Sketch, Skip> {
    let face = hand::face();
    let l = layout(trip, which, home, tiles, &face)?;
    Ok(Sketch {
        owner: raster::png(&ink(&l, which, false, &face)),
        public: raster::png(&ink(&l, which, true, &face)),
        fingerprint: fingerprint(trip, which, home),
        credit: l.credit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mantelpiece_core::ids::{HouseholdId, MediaId, TripId};
    use mantelpiece_core::trip::{Route, RoutePoint};

    fn rp(lat: f64, lon: f64) -> RoutePoint {
        RoutePoint {
            at: GeoPoint::new(lat, lon),
            time: time::OffsetDateTime::UNIX_EPOCH,
            media: MediaId::new("m"),
            estimated: false,
        }
    }

    fn trip_of(days: Vec<Vec<RoutePoint>>) -> Trip {
        let mut t: Trip = serde_json::from_value(serde_json::json!({
            "id": "t", "household": "h", "name": "T", "start": "2015-06-27T00:00:00Z",
            "end": "2015-07-04T00:00:00Z", "days": [], "route": {"points": [], "inferred": false, "map_key": null},
            "highlights": [], "contributors": [], "clock_offsets": [], "narrative": null, "hero": null
        }))
        .unwrap();
        t.id = TripId::new("t");
        t.household = HouseholdId::new("h");
        for (i, pts) in days.into_iter().enumerate() {
            t.days.push(
                serde_json::from_value(serde_json::json!({
                    "index": i + 1, "date": "2015-06-27", "media": [], "title": null,
                    "route": {"points": [], "inferred": false, "map_key": null}
                }))
                .unwrap(),
            );
            t.days[i].route = Route {
                points: pts,
                inferred: false,
                map_key: None,
            };
        }
        t
    }

    #[test]
    fn route_points_near_home_are_left_out() {
        let home = GeoPoint::new(64.1466, -21.9426);
        let t = trip_of(vec![vec![
            rp(64.1470, -21.9430),
            rp(64.2500, -21.0000),
            rp(64.3000, -20.5000),
        ]]);
        let l = legs(&t, Which::Trip, Some(home));
        assert!(l
            .iter()
            .flat_map(|l| &l.pts)
            .all(|p| p.at.distance_m(home) > 2_000.0));
        assert_eq!(l.iter().map(|l| l.pts.len()).sum::<usize>(), 2);
    }

    #[test]
    fn a_flight_is_a_hop_and_its_lone_photo_does_not_set_the_frame() {
        // A photo at Frankfurt airport, then the trip in Iceland.
        let t = trip_of(vec![vec![
            rp(50.0379, 8.5622),
            rp(64.1466, -21.9426),
            rp(64.2, -21.0),
        ]]);
        let l = legs(&t, Which::Trip, None);
        assert!(l.iter().any(|l| l.hop));
        let v = frame(&l, &[], 5_000.0).unwrap();
        assert!(
            v.width_m() < 200_000.0,
            "framed on Iceland, not Europe: {}",
            v.width_m()
        );
    }

    #[test]
    fn dates_read_like_a_person_wrote_them() {
        use time::macros::date;
        assert_eq!(
            dates(date!(2015 - 06 - 27), date!(2015 - 07 - 04)),
            "27 June – 4 July 2015"
        );
        assert_eq!(
            dates(date!(2015 - 05 - 03), date!(2015 - 05 - 09)),
            "3 – 9 May 2015"
        );
        assert_eq!(
            dates(date!(2015 - 05 - 03), date!(2015 - 05 - 03)),
            "3 May 2015"
        );
        assert!((nice_length(23_456.0) - 20_000.0).abs() < 1e-6);
        assert_eq!(length_label(500.0), "500 m");
    }
}
