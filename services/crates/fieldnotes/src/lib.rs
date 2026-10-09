//! Field notes (F-711): a quiet map of where a highlight was taken, laid into the photo's
//! calmest empty space as a separate layer. Pure: no AWS and no network (map data comes
//! through [`Tiles`]), so `make test` covers it.

// Map geometry reads best as x, y, w, h, u, v; pixel work casts between float and integer
// on every line, always within range.
#![allow(
    clippy::many_single_char_names,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

pub mod certainty;
pub mod contours;
pub mod drawing;
pub mod geo;
pub mod mode;
pub mod placement;
pub mod raster;

use drawing::{Candidate, Class, Drawing, Label, LabelKind, Marker, MarkerKind, Shape, Title};
use geo::{TileId, View};
use image::{DynamicImage, GenericImageView};
use mantelpiece_core::field_note::{
    Caption, Certainty, Feature, FeatureKind, Ink, MapMode, NormBox, Subject,
};
use mantelpiece_core::geo::GeoPoint;
use mantelpiece_core::media::MediaItem;
use mantelpiece_core::trip::Trip;
use sha2::{Digest, Sha256};

/// Where map data comes from. `Ok(None)` is a tile that does not exist (open sea);
/// `Err` is a failure, which leaves the photo without a note until the next run.
pub trait Tiles {
    fn vector(&self, id: TileId) -> Result<Option<Vec<u8>>, String>;
    fn terrain(&self, id: TileId) -> Result<Option<Vec<u8>>, String>;
}

/// Everything a field note is made from.
#[derive(Clone, Copy, Debug)]
pub struct Input<'a> {
    pub media: &'a MediaItem,
    pub trip: &'a Trip,
    pub home: Option<GeoPoint>,
    /// The upright TV rendition.
    pub photo: &'a DynamicImage,
    /// Rekognition boxes from enrich plus the scenery boxes from the subject call.
    pub subjects: &'a [Subject],
    /// Compass bearing the camera faced, when the photo recorded it.
    pub heading: Option<f32>,
}

/// A made note: what to store as JSON, and the record's fields.
#[derive(Clone, Debug)]
pub struct Made {
    pub drawing: Drawing,
    pub placement: NormBox,
    pub mode: MapMode,
    pub certainty: Certainty,
    pub ink: Ink,
    pub caption: Caption,
    pub credit: String,
    /// Metres across the map, for the public form's ring.
    pub width_m: f32,
    /// What the map is mainly about, for "This place".
    pub feature: Option<Feature>,
}

/// Inverse of the view's projection: a box point back to latitude and longitude.
fn geo_of(view: &View, u: f32, v: f32) -> GeoPoint {
    let wx = view.centre.x - view.half_w + f64::from(u) * 2.0 * view.half_w;
    let wy = view.centre.y - view.half_h + f64::from(v) * 2.0 * view.half_h;
    let lat = (std::f64::consts::PI * (1.0 - 2.0 * wy))
        .sinh()
        .atan()
        .to_degrees();
    GeoPoint::new(lat, wx * 360.0 - 180.0)
}

/// The trip's landmark tied to the photo, else the most prominent named feature within a
/// third of the map's width of the marker: peaks first, then water, then towns.
fn main_feature(
    input: &Input<'_>,
    candidates: &[Candidate],
    view: &View,
    marker: &Marker,
    aspect: f32,
) -> Option<Feature> {
    let landmarks = || input.trip.days.iter().flat_map(|d| d.landmarks.iter());
    let country = landmarks().find_map(|l| l.country.clone());
    if let Some(l) = landmarks().find(|l| l.media.contains(&input.media.id)) {
        return Some(Feature {
            name: l.name.clone(),
            kind: FeatureKind::Landmark,
            at: l.at,
            detail: None,
            country: l.country.clone(),
        });
    }
    let near =
        |c: &&Candidate| (c.label.x - marker.x).hypot((c.label.y - marker.y) / aspect) <= 1.0 / 3.0;
    let rank = |k: LabelKind| match k {
        LabelKind::Peak => 0,
        LabelKind::Landmark => 1,
        LabelKind::Water => 2,
        LabelKind::Place => 3,
    };
    candidates
        .iter()
        .filter(near)
        .min_by(|a, b| {
            rank(a.label.kind)
                .cmp(&rank(b.label.kind))
                .then(a.priority.total_cmp(&b.priority))
        })
        .map(|c| Feature {
            name: c.label.text.clone(),
            kind: match c.label.kind {
                LabelKind::Peak => FeatureKind::Peak,
                LabelKind::Water => FeatureKind::Water,
                LabelKind::Landmark => FeatureKind::Landmark,
                LabelKind::Place => FeatureKind::Place,
            },
            at: geo_of(view, c.label.x, c.label.y),
            detail: c.label.sub.clone(),
            country,
        })
}

/// Why a highlight got no note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skip {
    NoPlace,
    NoCalmRegion,
    MapUnavailable,
}

impl Skip {
    #[must_use]
    pub fn reason(self) -> &'static str {
        match self {
            Self::NoPlace => "no place",
            Self::NoCalmRegion => "no calm region",
            Self::MapUnavailable => "map data unavailable",
        }
    }
}

/// What a note depends on, hashed: a different value means remake. The subject call is
/// left out on purpose (a model may answer slightly differently each time).
#[must_use]
pub fn fingerprint(media: &MediaItem, trip: &Trip, home: Option<GeoPoint>) -> String {
    let mut h = Sha256::new();
    h.update(drawing::STYLE_VERSION.to_le_bytes());
    h.update(media.sha256.as_bytes());
    h.update(serde_json::to_vec(&certainty::certainty(media, trip, home)).unwrap_or_default());
    if let Some(day) = trip.days.iter().find(|d| d.media.contains(&media.id)) {
        h.update(serde_json::to_vec(&day.route.points).unwrap_or_default());
        for l in &day.landmarks {
            h.update(l.name.as_bytes());
        }
    }
    hex::encode(h.finalize())
}

fn fetch(tiles: &dyn Tiles, ids: &[TileId], vector: bool) -> Result<Vec<(TileId, Vec<u8>)>, Skip> {
    let mut out = Vec::new();
    for id in ids {
        let got = if vector {
            tiles.vector(*id)
        } else {
            tiles.terrain(*id)
        };
        match got {
            Ok(Some(bytes)) => out.push((*id, bytes)),
            Ok(None) => {}
            Err(_) => return Err(Skip::MapUnavailable),
        }
    }
    Ok(out)
}

/// Make the note for one highlight, or say why not.
///
/// # Errors
/// A [`Skip`] when the place is unknown, nothing is calm enough, or map data failed.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::too_many_lines
)]
pub fn make(input: &Input<'_>, tiles: &dyn Tiles) -> Result<Made, Skip> {
    let certainty =
        certainty::certainty(input.media, input.trip, input.home).ok_or(Skip::NoPlace)?;
    let (pw, ph) = input.photo.dimensions();
    let photo_aspect = pw as f32 / ph.max(1) as f32;
    let placement = placement::place(&placement::grid(input.photo), photo_aspect, input.subjects)
        .ok_or(Skip::NoCalmRegion)?;
    let bbox = placement.bbox;
    let aspect = (bbox.width * pw as f32) / (bbox.height * ph as f32).max(1.0);
    let box_px = f64::from(bbox.width * pw as f32);
    let at = certainty.at();
    let radius_m = match certainty {
        Certainty::Approximate { radius_m, .. } => f64::from(radius_m),
        Certainty::Precise { .. } => 0.0,
    };

    // Relief over a wide view decides whether this is mountain country at all.
    let wide = View::new(at, 10_000.0, f64::from(aspect));
    let tz = wide.zoom_for(box_px, geo::MAX_TERRAIN_ZOOM).min(12);
    let terrain = fetch(tiles, &wide.tiles(tz), false)?
        .into_iter()
        .filter_map(|(id, png)| contours::decode(&png).map(|m| (id, m)))
        .collect::<Vec<_>>();
    let terrain = contours::stitch(&terrain);
    let wide_relief = terrain.as_ref().map_or(0.0, |t| {
        let (lo, hi) = t.relief(&wide);
        if lo.is_finite() {
            hi - lo
        } else {
            0.0
        }
    });

    // Start from the mode's width, widen for coast or trail, and for an uncertain place.
    let first = if wide_relief >= mode::MOUNTAIN_RELIEF_M {
        MapMode::Mountains
    } else {
        MapMode::Town
    };
    let mut width = mode::width_m(first).max(radius_m * 6.0);
    let mut view = View::new(at, width, f64::from(aspect));
    let mut f = drawing::features(
        &fetch(
            tiles,
            &view.tiles(view.zoom_for(box_px, geo::MAX_VECTOR_ZOOM)),
            true,
        )?,
        &view,
        aspect,
        bbox.width * pw as f32,
    );
    let relief_in = |v: &View| {
        terrain.as_ref().map_or(0.0, |t| {
            let (lo, hi) = t.relief(v);
            if lo.is_finite() {
                hi - lo
            } else {
                0.0
            }
        })
    };
    let mut map_mode = mode::mode(f.coast_len, relief_in(&view), f.road_len);
    let wanted = mode::width_m(map_mode).max(radius_m * 6.0);
    if wanted > width * 1.5 {
        width = wanted;
        view = View::new(at, width, f64::from(aspect));
        f = drawing::features(
            &fetch(
                tiles,
                &view.tiles(view.zoom_for(box_px, geo::MAX_VECTOR_ZOOM)),
                true,
            )?,
            &view,
            aspect,
            bbox.width * pw as f32,
        );
        map_mode = mode::mode(f.coast_len, relief_in(&view), f.road_len);
    }
    if f.fills.is_empty() && f.strokes.is_empty() && f.candidates.is_empty() {
        return Err(Skip::MapUnavailable);
    }

    let mut strokes = f.strokes;
    let mut credit = drawing::OSM_CREDIT.to_owned();
    let relief = relief_in(&view);
    if let (Some(t), true) = (
        &terrain,
        matches!(map_mode, MapMode::Mountains | MapMode::Trail) && relief >= 40.0,
    ) {
        strokes.extend(t.contours(&view, contours::interval(relief)));
        credit = format!("{credit} · {}", drawing::TERRAIN_CREDIT);
    }
    // The day's route as a fine dotted path.
    if let Some(day) = input
        .trip
        .days
        .iter()
        .find(|d| d.media.contains(&input.media.id))
    {
        let pts: Vec<[f32; 2]> = day
            .route
            .points
            .iter()
            .map(|p| {
                let (u, v) = view.to_box(geo::world(p.at));
                [u, v]
            })
            .collect();
        if pts.len() > 1 {
            strokes.push(Shape {
                class: Class::Route,
                parts: vec![pts],
            });
        }
    }

    let (mu, mv) = view.to_box(geo::world(at));
    let marker = match certainty {
        Certainty::Precise { .. } => Marker {
            kind: MarkerKind::Dot,
            x: mu,
            y: mv,
            r: 0.0,
            heading: input.heading,
        },
        Certainty::Approximate { radius_m, .. } => Marker {
            kind: MarkerKind::Ring,
            x: mu,
            y: mv,
            r: view.metres_to_box(f64::from(radius_m)),
            heading: None,
        },
    };

    // Title: the most important town or city in view, and the country the trip names.
    let mut candidates = f.candidates;
    candidates.sort_by(|a, b| a.priority.total_cmp(&b.priority));
    let title_at = candidates.iter().position(|c| {
        c.label.kind == LabelKind::Place && c.label.icon.is_none() && c.priority < 20.0
    });
    let region = input
        .trip
        .days
        .iter()
        .flat_map(|d| d.landmarks.iter())
        .find_map(|l| l.country.clone());
    let title = title_at
        .map(|i| candidates.remove(i).label.text)
        .map(|place| Title { place, region });

    // Trip landmarks in view come first: they are what this trip is about.
    for l in input.trip.days.iter().flat_map(|d| d.landmarks.iter()) {
        let (u, v) = view.to_box(geo::world(l.at));
        if (0.06..=0.94).contains(&u)
            && (0.08..=0.92).contains(&v)
            && !candidates.iter().any(|c| c.label.text == l.name)
        {
            candidates.push(Candidate {
                label: Label {
                    text: l.name.clone(),
                    sub: None,
                    x: u,
                    y: v,
                    kind: LabelKind::Landmark,
                    angle: 0.0,
                    icon: None,
                },
                priority: 0.0,
            });
        }
    }
    let label_size = 0.02 * pw.min(ph) as f32 / (bbox.height * ph as f32);
    let feature = main_feature(input, &candidates, &view, &marker, aspect);
    // Keep the title block (top left) and the caption line (bottom) free of names.
    let labels = drawing::choose_labels(
        candidates,
        &marker,
        aspect,
        label_size,
        &[[0.0, 0.0, 0.5, 0.36], [0.0, 0.86, 1.0, 1.0]],
    );

    let caption = certainty::caption(input.media, input.trip, &certainty);
    let seed = u64::from_le_bytes(
        Sha256::digest(input.media.id.as_str().as_bytes())[..8]
            .try_into()
            .unwrap_or([0; 8]),
    );
    let drawing = Drawing {
        version: drawing::STYLE_VERSION,
        aspect,
        mode: map_mode,
        ink: placement.ink,
        fills: f.fills,
        strokes,
        labels,
        marker,
        title,
        scale: Some(drawing::scale(&view)),
        caption: caption.date.clone(),
        coords: caption.coords.clone(),
        fade: 0.22,
        grain_seed: seed,
        credit: credit.clone(),
    };
    Ok(Made {
        drawing,
        placement: bbox,
        mode: map_mode,
        certainty,
        ink: placement.ink,
        caption,
        credit,
        width_m: view.width_m() as f32,
        feature,
    })
}
