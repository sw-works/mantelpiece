//! The display list a field note is drawn from: shapes, labels, a marker, a title block
//! and a scale, in the coordinates of the note's box (0 to 1 each way, y down). Built
//! once from map data, stored as JSON, rasterised at any size and for any audience.

use crate::geo::{TileId, View, World};
use geo_types::{Coord, Geometry, LineString};
use mantelpiece_core::field_note::{Ink, MapMode};
use mvt_reader::feature::Value;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Bumped whenever the drawing changes enough that stored notes should be remade.
pub const STYLE_VERSION: u32 = 3;
/// At most this many names on a map (FR-003).
pub const MAX_LABELS: usize = 4;
/// Credit for every note drawn from OpenStreetMap data.
pub const OSM_CREDIT: &str = "© OpenStreetMap contributors";
/// Added when contours or relief are drawn from open terrain tiles.
pub const TERRAIN_CREDIT: &str = "Terrain Tiles (AWS Open Data)";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    Water,
    Park,
    Coast,
    River,
    RoadMajor,
    RoadMinor,
    Path,
    Contour,
    ContourIndex,
    Route,
}

/// Polygons (fills) or polylines (strokes), each a list of points.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub class: Class,
    pub parts: Vec<Vec<[f32; 2]>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelKind {
    Place,
    Water,
    Peak,
    Landmark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Icon {
    Peak,
    Hut,
    Church,
    Castle,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Label {
    pub text: String,
    /// Second line in smaller type, e.g. "2,999 m".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    pub x: f32,
    pub y: f32,
    pub kind: LabelKind,
    /// Radians, for names that follow a river.
    #[serde(default)]
    pub angle: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerKind {
    Dot,
    Ring,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    pub kind: MarkerKind,
    pub x: f32,
    pub y: f32,
    /// Ring radius as a fraction of the box width; 0 for a dot.
    pub r: f32,
    /// Compass bearing the camera faced, degrees, for the view cone (dots only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Title {
    pub place: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
}

/// A scale bar: `metres` is a round length, `frac` its share of the box width.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scale {
    pub metres: f32,
    pub frac: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Drawing {
    pub version: u32,
    /// Box width / box height in pixels.
    pub aspect: f32,
    pub mode: MapMode,
    pub ink: Ink,
    pub fills: Vec<Shape>,
    pub strokes: Vec<Shape>,
    pub labels: Vec<Label>,
    pub marker: Marker,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<Title>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<Scale>,
    /// Caption line under the title: date, and coordinates where allowed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// Coordinates in the caption, kept apart so the public form can drop them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coords: Option<String>,
    /// Edge feather, as a share of the box's shorter side.
    pub fade: f32,
    pub grain_seed: u64,
    pub credit: String,
}

impl Drawing {
    /// The form for member households, code views and shared links (FR-016): a ring of at
    /// least 500 m, no direction, no coordinates.
    #[must_use]
    pub fn public(&self, metres_per_box_width: f32) -> Self {
        let mut d = self.clone();
        let min_r = 500.0 / metres_per_box_width.max(1.0);
        d.marker = Marker {
            kind: MarkerKind::Ring,
            x: d.marker.x,
            y: d.marker.y,
            r: d.marker.r.max(min_r),
            heading: None,
        };
        d.coords = None;
        d
    }
}

/// What a vector tile contributes before labels are chosen.
#[derive(Debug, Default)]
pub struct Features {
    pub fills: Vec<Shape>,
    pub strokes: Vec<Shape>,
    pub candidates: Vec<Candidate>,
    /// Coastline length inside the box, in box widths.
    pub coast_len: f32,
    /// Road length inside the box, in box widths.
    pub road_len: f32,
}

/// A name that may go on the map, best first by `priority` (lower is better).
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub label: Label,
    pub priority: f32,
}

fn prop<'a, S: std::hash::BuildHasher>(
    props: &'a HashMap<String, Value, S>,
    key: &str,
) -> Option<&'a Value> {
    props.get(key)
}

fn text<S: std::hash::BuildHasher>(props: &HashMap<String, Value, S>, key: &str) -> Option<String> {
    match prop(props, key)? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn number(props: &HashMap<String, Value>, key: &str) -> Option<f64> {
    #[allow(clippy::cast_precision_loss)]
    match prop(props, key)? {
        Value::Float(v) => Some(f64::from(*v)),
        Value::Double(v) => Some(*v),
        Value::Int(v) | Value::SInt(v) => Some(*v as f64),
        Value::UInt(v) => Some(*v as f64),
        _ => None,
    }
}

/// True when every letter is Latin script, so a name reads for an English audience.
fn latin(s: &str) -> bool {
    s.chars().all(|c| {
        !c.is_alphabetic() || (c as u32) < 0x0250 || (0x1E00..0x1F00).contains(&(c as u32))
    })
}

/// The English name: `name:en`, then `name_en`, then `name` if it is in Latin script.
#[must_use]
pub fn english_name<S: std::hash::BuildHasher>(
    props: &HashMap<String, Value, S>,
) -> Option<String> {
    text(props, "name:en")
        .or_else(|| text(props, "name_en"))
        .or_else(|| text(props, "name").filter(|n| latin(n)))
}

/// Thousands with a comma: 2999 → "2,999 m".
#[must_use]
pub fn metres(ele: f64) -> String {
    #[allow(clippy::cast_possible_truncation)]
    let n = ele.round() as i64;
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    format!("{}{out} m", if n < 0 { "-" } else { "" })
}

/// Projection from one tile's local units into the box, with a minimum spacing between
/// kept points so lines are not drawn finer than the output can show.
struct Projector<'a> {
    view: &'a View,
    tile: TileId,
    extent: f64,
    /// Smallest step worth keeping, in box widths.
    step: f32,
}

impl Projector<'_> {
    fn point(&self, c: Coord<f32>) -> [f32; 2] {
        let w: World = self.tile.world(f64::from(c.x), f64::from(c.y), self.extent);
        let (u, v) = self.view.to_box(w);
        [u, v]
    }

    fn line(&self, ls: &LineString<f32>) -> Vec<[f32; 2]> {
        let mut out: Vec<[f32; 2]> = Vec::with_capacity(ls.0.len());
        for c in &ls.0 {
            let p = self.point(*c);
            if out
                .last()
                .is_none_or(|q| (p[0] - q[0]).hypot(p[1] - q[1]) >= self.step)
            {
                out.push(p);
            }
        }
        if let (Some(last_c), Some(last)) = (ls.0.last(), out.last()) {
            let p = self.point(*last_c);
            if p[0].to_bits() != last[0].to_bits() || p[1].to_bits() != last[1].to_bits() {
                out.push(p);
            }
        }
        out
    }

    /// A polygon ring's outline without the edges a tile cut adds along its border.
    fn outline(&self, ls: &LineString<f32>) -> Vec<Vec<[f32; 2]>> {
        #[allow(clippy::cast_possible_truncation)]
        let e = self.extent as f32;
        let on_border = |a: Coord<f32>, b: Coord<f32>| {
            (a.x <= 0.0 && b.x <= 0.0)
                || (a.x >= e && b.x >= e)
                || (a.y <= 0.0 && b.y <= 0.0)
                || (a.y >= e && b.y >= e)
        };
        let mut runs = Vec::new();
        let mut run: Vec<Coord<f32>> = Vec::new();
        for w in ls.0.windows(2) {
            if on_border(w[0], w[1]) {
                if run.len() > 1 {
                    runs.push(self.line(&LineString(std::mem::take(&mut run))));
                }
                run.clear();
            } else {
                if run.is_empty() {
                    run.push(w[0]);
                }
                run.push(w[1]);
            }
        }
        if run.len() > 1 {
            runs.push(self.line(&LineString(run)));
        }
        runs
    }
}

fn inside(p: [f32; 2]) -> bool {
    (0.0..=1.0).contains(&p[0]) && (0.0..=1.0).contains(&p[1])
}

/// Length of the parts of a polyline inside the box, in box widths (y scaled by aspect).
fn inside_len(line: &[[f32; 2]], aspect: f32) -> f32 {
    line.windows(2)
        .filter(|w| inside(w[0]) && inside(w[1]))
        .map(|w| (w[1][0] - w[0][0]).hypot((w[1][1] - w[0][1]) / aspect))
        .sum()
}

fn lines_of(g: &Geometry<f32>) -> Vec<&LineString<f32>> {
    match g {
        Geometry::LineString(l) => vec![l],
        Geometry::MultiLineString(m) => m.0.iter().collect(),
        _ => vec![],
    }
}

fn rings_of(g: &Geometry<f32>) -> Vec<&LineString<f32>> {
    match g {
        Geometry::Polygon(p) => std::iter::once(p.exterior()).chain(p.interiors()).collect(),
        Geometry::MultiPolygon(m) => {
            m.0.iter()
                .flat_map(|p| std::iter::once(p.exterior()).chain(p.interiors()))
                .collect()
        }
        _ => vec![],
    }
}

fn point_of(g: &Geometry<f32>) -> Option<Coord<f32>> {
    match g {
        Geometry::Point(p) => Some(p.0),
        Geometry::MultiPoint(m) => m.0.first().map(|p| p.0),
        _ => None,
    }
}

/// Decode OpenMapTiles vector tiles into box-space shapes and name candidates.
/// `box_px` is the box width in output pixels, for simplification.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn features(tiles: &[(TileId, Vec<u8>)], view: &View, aspect: f32, box_px: f32) -> Features {
    let mut f = Features::default();
    let step = 0.6 / box_px.max(1.0);
    let mut seen_names: Vec<String> = Vec::new();
    for (tile, bytes) in tiles {
        let Ok(reader) = mvt_reader::Reader::new(bytes.clone()) else {
            continue;
        };
        let Ok(layers) = reader.get_layer_metadata() else {
            continue;
        };
        for layer in layers {
            let proj = Projector {
                view,
                tile: *tile,
                extent: f64::from(layer.extent),
                step,
            };
            let Ok(feats) = reader.get_features(layer.layer_index) else {
                continue;
            };
            for feat in feats {
                let empty = HashMap::new();
                let props = feat.properties.as_ref().unwrap_or(&empty);
                let class = text(props, "class").unwrap_or_default();
                match layer.name.as_str() {
                    "water" => {
                        if matches!(class.as_str(), "swimming_pool") {
                            continue;
                        }
                        let rings = rings_of(&feat.geometry);
                        f.fills.push(Shape {
                            class: Class::Water,
                            parts: rings.iter().map(|r| proj.line(r)).collect(),
                        });
                        if class == "ocean" || class == "lake" {
                            for r in rings {
                                for run in proj.outline(r) {
                                    if class == "ocean" {
                                        f.coast_len += inside_len(&run, aspect);
                                    }
                                    f.strokes.push(Shape {
                                        class: Class::Coast,
                                        parts: vec![run],
                                    });
                                }
                            }
                        }
                    }
                    "park" => f.fills.push(Shape {
                        class: Class::Park,
                        parts: rings_of(&feat.geometry)
                            .iter()
                            .map(|r| proj.line(r))
                            .collect(),
                    }),
                    "waterway" => {
                        let parts: Vec<_> = lines_of(&feat.geometry)
                            .iter()
                            .map(|l| proj.line(l))
                            .collect();
                        if let Some(name) = english_name(props).filter(|_| class == "river") {
                            if let Some((p, angle)) = parts.iter().find_map(|l| midpoint(l, aspect))
                            {
                                if !seen_names.contains(&name) {
                                    seen_names.push(name.clone());
                                    f.candidates.push(Candidate {
                                        label: Label {
                                            text: name,
                                            sub: None,
                                            x: p[0],
                                            y: p[1],
                                            kind: LabelKind::Water,
                                            angle,
                                            icon: None,
                                        },
                                        priority: 30.0,
                                    });
                                }
                            }
                        }
                        f.strokes.push(Shape {
                            class: Class::River,
                            parts,
                        });
                    }
                    "transportation" => {
                        if text(props, "brunnel").as_deref() == Some("tunnel") {
                            continue;
                        }
                        let class = match class.as_str() {
                            "motorway" | "trunk" | "primary" | "secondary" => Class::RoadMajor,
                            "tertiary" | "minor" | "service" | "raceway" | "busway" => {
                                Class::RoadMinor
                            }
                            "path" | "track" | "bridleway" => Class::Path,
                            _ => continue,
                        };
                        let parts: Vec<_> = lines_of(&feat.geometry)
                            .iter()
                            .map(|l| proj.line(l))
                            .collect();
                        if class != Class::Path {
                            f.road_len += parts.iter().map(|l| inside_len(l, aspect)).sum::<f32>();
                        }
                        f.strokes.push(Shape { class, parts });
                    }
                    "place" | "mountain_peak" | "water_name" | "poi" => {
                        let Some(c) = point_of(&feat.geometry) else {
                            continue;
                        };
                        let p = proj.point(c);
                        if !(0.04..=0.96).contains(&p[0]) || !(0.06..=0.94).contains(&p[1]) {
                            continue;
                        }
                        let Some(name) = english_name(props) else {
                            continue;
                        };
                        if seen_names.contains(&name) {
                            continue;
                        }
                        let rank = number(props, "rank").unwrap_or(20.0);
                        #[allow(clippy::cast_possible_truncation)]
                        let (kind, sub, icon, priority) = match layer.name.as_str() {
                            "mountain_peak" => {
                                let ele = number(props, "ele");
                                (
                                    LabelKind::Peak,
                                    ele.map(metres),
                                    Some(Icon::Peak),
                                    10.0 + rank as f32 - ele.unwrap_or(0.0) as f32 / 1000.0,
                                )
                            }
                            "water_name" => (LabelKind::Water, None, None, 25.0 + rank as f32),
                            "poi" => {
                                let icon = match class.as_str() {
                                    "place_of_worship" => Some(Icon::Church),
                                    "castle" => Some(Icon::Castle),
                                    "alpine_hut" | "shelter" => Some(Icon::Hut),
                                    _ => continue,
                                };
                                if rank > 4.0 {
                                    continue;
                                }
                                (LabelKind::Place, None, icon, 40.0 + rank as f32)
                            }
                            _ => {
                                let base = match class.as_str() {
                                    "city" => 12.0,
                                    "town" => 15.0,
                                    "village" | "suburb" => 20.0,
                                    "quarter" | "neighbourhood" => 24.0,
                                    _ => continue,
                                };
                                (LabelKind::Place, None, None, base + rank as f32 / 10.0)
                            }
                        };
                        seen_names.push(name.clone());
                        f.candidates.push(Candidate {
                            label: Label {
                                text: name,
                                sub,
                                x: p[0],
                                y: p[1],
                                kind,
                                angle: 0.0,
                                icon,
                            },
                            priority,
                        });
                    }
                    _ => {}
                }
            }
        }
    }
    f
}

/// The middle of a polyline's visible part and its direction, kept upright.
fn midpoint(line: &[[f32; 2]], aspect: f32) -> Option<([f32; 2], f32)> {
    let visible: Vec<&[[f32; 2]]> = line
        .windows(2)
        .filter(|w| inside(w[0]) && inside(w[1]))
        .collect();
    let w = visible.get(visible.len() / 2)?;
    let p = [
        f32::midpoint(w[0][0], w[1][0]),
        f32::midpoint(w[0][1], w[1][1]),
    ];
    if !(0.12..=0.88).contains(&p[0]) || !(0.12..=0.88).contains(&p[1]) {
        return None;
    }
    let mut angle = ((w[1][1] - w[0][1]) / aspect).atan2(w[1][0] - w[0][0]);
    if angle > std::f32::consts::FRAC_PI_2 {
        angle -= std::f32::consts::PI;
    } else if angle < -std::f32::consts::FRAC_PI_2 {
        angle += std::f32::consts::PI;
    }
    Some((p, angle))
}

/// Pick up to `MAX_LABELS` names that do not collide with each other or the marker.
/// Sizes are in box heights; `reserved` areas (title, caption) stay free.
#[must_use]
pub fn choose_labels(
    mut candidates: Vec<Candidate>,
    marker: &Marker,
    aspect: f32,
    size: f32,
    reserved: &[[f32; 4]],
) -> Vec<Label> {
    candidates.sort_by(|a, b| a.priority.total_cmp(&b.priority));
    let mut chosen: Vec<Label> = Vec::new();
    let mut boxes: Vec<[f32; 4]> = reserved.to_vec();
    boxes.push({
        let r = (marker.r.max(0.02)) * 1.2;
        [
            marker.x - r,
            marker.y - r * aspect,
            marker.x + r,
            marker.y + r * aspect,
        ]
    });
    for c in candidates {
        if chosen.len() >= MAX_LABELS {
            break;
        }
        #[allow(clippy::cast_precision_loss)]
        let w = c.label.text.chars().count() as f32 * size * 0.5 / aspect;
        // Rivers and peaks are set centred on their point (a peak's name sits under its
        // triangle); everything else starts just right of its dot.
        let b = match c.label.kind {
            LabelKind::Water => [
                c.label.x - w / 2.0,
                c.label.y - size * 0.8,
                c.label.x + w / 2.0,
                c.label.y + size * 0.8,
            ],
            LabelKind::Peak => {
                let below = size * if c.label.sub.is_some() { 2.4 } else { 1.5 };
                [
                    c.label.x - w / 2.0,
                    c.label.y - size * 0.5,
                    c.label.x + w / 2.0,
                    c.label.y + below,
                ]
            }
            _ => {
                let h = size * if c.label.sub.is_some() { 2.3 } else { 1.3 };
                [
                    c.label.x - 0.01,
                    c.label.y - h / 2.0,
                    c.label.x + 0.02 + w,
                    c.label.y + h / 2.0,
                ]
            }
        };
        if b[2] > 0.97 || b[1] < 0.03 || b[3] > 0.97 {
            continue;
        }
        if boxes
            .iter()
            .any(|o| b[0] < o[2] && o[0] < b[2] && b[1] < o[3] && o[1] < b[3])
        {
            continue;
        }
        boxes.push(b);
        chosen.push(c.label);
    }
    chosen
}

/// A round scale length near a fifth of the view's width.
#[must_use]
pub fn scale(view: &View) -> Scale {
    let target = view.width_m() / 5.0;
    let pow = 10f64.powf(target.log10().floor());
    let nice = [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|m| m * pow)
        .rfind(|m| *m <= target)
        .unwrap_or(pow);
    #[allow(clippy::cast_possible_truncation)]
    Scale {
        metres: nice as f32,
        frac: view.metres_to_box(nice),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mantelpiece_core::geo::GeoPoint;

    fn florence() -> (Vec<(TileId, Vec<u8>)>, View) {
        let bytes = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/omt-13-4352-2986.pbf"
        ))
        .unwrap();
        let view = View::new(GeoPoint::new(43.7679, 11.2531), 3_000.0, 1.5);
        (
            vec![(
                TileId {
                    z: 13,
                    x: 4352,
                    y: 2986,
                },
                bytes,
            )],
            view,
        )
    }

    #[test]
    fn florence_has_the_arno_streets_and_english_names() {
        let (tiles, view) = florence();
        let f = features(&tiles, &view, 1.5, 600.0);
        assert!(f.fills.iter().any(|s| s.class == Class::Water));
        assert!(f.strokes.iter().any(|s| s.class == Class::RoadMajor));
        assert!(f.road_len > 3.0, "Florence is a town: {}", f.road_len);
        assert!(f.coast_len < 0.1, "no sea in Florence: {}", f.coast_len);
        let names: Vec<&str> = f.candidates.iter().map(|c| c.label.text.as_str()).collect();
        assert!(!names.is_empty(), "{names:?}");
        assert!(names.iter().all(|n| latin(n)), "{names:?}");
    }

    #[test]
    fn at_most_four_labels_clear_of_the_marker() {
        let (tiles, view) = florence();
        let f = features(&tiles, &view, 1.5, 600.0);
        let marker = Marker {
            kind: MarkerKind::Dot,
            x: 0.5,
            y: 0.5,
            r: 0.0,
            heading: None,
        };
        let labels = choose_labels(f.candidates, &marker, 1.5, 0.06, &[]);
        assert!(labels.len() <= MAX_LABELS);
        for l in &labels {
            assert!(
                !((l.x - 0.5).abs() < 0.02 && (l.y - 0.5).abs() < 0.03),
                "{l:?} on the marker"
            );
        }
    }

    #[test]
    fn public_form_has_no_dot_no_direction_and_no_coordinates() {
        let (tiles, view) = florence();
        let f = features(&tiles, &view, 1.5, 600.0);
        let d = Drawing {
            version: STYLE_VERSION,
            aspect: 1.5,
            mode: MapMode::Town,
            ink: Ink::Light,
            fills: f.fills,
            strokes: f.strokes,
            labels: vec![],
            marker: Marker {
                kind: MarkerKind::Dot,
                x: 0.5,
                y: 0.5,
                r: 0.0,
                heading: Some(90.0),
            },
            title: Some(Title {
                place: "Florence".into(),
                region: Some("Italy".into()),
            }),
            scale: Some(scale(&view)),
            caption: Some("3 April 2026".into()),
            coords: Some("43.77° N, 11.25° E".into()),
            fade: 0.12,
            grain_seed: 7,
            credit: OSM_CREDIT.into(),
        };
        #[allow(clippy::cast_possible_truncation)]
        let p = d.public(view.width_m() as f32);
        assert_eq!(p.marker.kind, MarkerKind::Ring);
        assert!(p.marker.heading.is_none() && p.coords.is_none());
        assert!(p.marker.r * 3_000.0 >= 499.0);
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains("° N"));
    }

    #[test]
    fn numbers_and_scales_read_naturally() {
        assert_eq!(metres(2999.4), "2,999 m");
        assert_eq!(metres(812.0), "812 m");
        let s = scale(&View::new(GeoPoint::new(43.77, 11.25), 3_000.0, 1.5));
        assert!((s.metres - 500.0).abs() < 1e-3, "{s:?}");
    }
}
