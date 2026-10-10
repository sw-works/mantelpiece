//! `make()` end to end on fixture tiles around Ponte Vecchio, no network.

use image::{DynamicImage, Rgb, RgbImage};
use mantelpiece_core::field_note::{Certainty, MapMode};
use mantelpiece_core::geo::GeoPoint;
use mantelpiece_core::ids::{HouseholdId, MediaId, TripId};
use mantelpiece_core::media::MediaItem;
use mantelpiece_core::trip::{Route, Trip, TripDay};
use mantelpiece_fieldnotes::drawing::{Drawing, MarkerKind};
use mantelpiece_fieldnotes::geo::TileId;
use mantelpiece_fieldnotes::{fingerprint, make, raster, Input, Skip, Tiles};

struct Fixtures {
    fail: bool,
}

impl Tiles for Fixtures {
    fn vector(&self, id: TileId) -> Result<Option<Vec<u8>>, String> {
        if self.fail {
            return Err("offline".into());
        }
        let path = format!(
            "{}/tests/fixtures/omt-{}-{}-{}.pbf",
            env!("CARGO_MANIFEST_DIR"),
            id.z,
            id.x,
            id.y
        );
        Ok(std::fs::read(path).ok())
    }
    fn terrain(&self, id: TileId) -> Result<Option<Vec<u8>>, String> {
        let path = format!(
            "{}/tests/fixtures/terrarium-{}-{}-{}.png",
            env!("CARGO_MANIFEST_DIR"),
            id.z,
            id.x,
            id.y
        );
        Ok(std::fs::read(path).ok())
    }
}

fn media(gps: Option<GeoPoint>) -> MediaItem {
    let mut m: MediaItem = serde_json::from_value(serde_json::json!({
        "id": "m1", "household": "h", "contributor": "c", "batch": "b", "kind": "photo",
        "original_key": "k", "original_filename": "f.jpg", "bytes": 1, "sha256": "ab",
        "captured": null, "gps": null, "orientation": 1,
        "signals": {"sharpness": 0.0, "exposure": 0.0, "phash": null, "width": null, "height": null},
        "enrichment": {"scene_tags": [], "landmark": null, "face_count": 0, "smile_count": 0, "inherited_from": null},
        "quality_score": null, "burst_group": null, "is_keeper": true, "trip": null,
        "pinned": false, "hidden": false, "state": "enriched", "uploaded_at": "2026-04-03T10:00:00Z"
    }))
    .unwrap();
    m.gps = gps;
    m
}

fn trip() -> Trip {
    Trip {
        id: TripId::new("t"),
        household: HouseholdId::new("h"),
        name: "Florence".into(),
        start: time::OffsetDateTime::UNIX_EPOCH,
        end: time::OffsetDateTime::UNIX_EPOCH,
        days: vec![TripDay {
            index: 1,
            date: time::macros::date!(2026 - 04 - 03),
            media: vec![MediaId::new("m1")],
            route: Route::default(),
            title: None,
            story: None,
            undated: vec![],
            landmarks: vec![],
        }],
        route: Route::default(),
        highlights: vec![MediaId::new("m1")],
        contributors: vec![],
        clock_offsets: vec![],
        narrative: None,
        hero: None,
        members: vec![],
    }
}

/// Calm sky over a busy town: a small frame keeps the map at the fixture's zoom (13).
fn photo() -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_fn(480, 320, |x, y| {
        if y < 160 {
            Rgb([110, 160, 220])
        } else {
            Rgb([((x / 6 * 37 + y / 6 * 91) % 230) as u8, 120, 80])
        }
    }))
}

#[test]
fn a_gps_highlight_in_florence_becomes_a_town_map_with_a_dot() {
    let m = media(Some(GeoPoint::new(43.7679, 11.2531)));
    let t = trip();
    let p = photo();
    let input = Input {
        media: &m,
        trip: &t,
        home: None,
        photo: &p,
        subjects: &[],
        heading: Some(110.0),
    };
    let made = make(&input, &Fixtures { fail: false }).expect("a note");
    assert_eq!(made.mode, MapMode::Town);
    assert!(matches!(made.certainty, Certainty::Precise { .. }));
    assert_eq!(made.drawing.marker.kind, MarkerKind::Dot);
    assert_eq!(made.drawing.marker.heading, Some(110.0));
    assert!(
        made.placement.bottom() <= 0.55,
        "in the sky: {:?}",
        made.placement
    );
    assert!(made.drawing.labels.len() <= 4);
    assert!(made.caption.coords.is_some());
    assert!(made.credit.contains("OpenStreetMap"));
    assert!(made.feature.is_some(), "a named feature near the dot");

    // The display list survives storage, and its public form drops the dot and coordinates.
    let json = serde_json::to_string(&made.drawing).unwrap();
    let back: Drawing = serde_json::from_str(&json).unwrap();
    assert_eq!(back, made.drawing);
    let public = made.drawing.public(made.width_m);
    assert_eq!(public.marker.kind, MarkerKind::Ring);
    assert!(public.coords.is_none());

    let layer = raster::layer(&made.drawing, made.placement, 480, 320);
    assert_eq!((layer.width(), layer.height()), (480, 320));
}

#[test]
fn no_place_and_no_map_data_mean_no_note() {
    let t = trip();
    let p = photo();
    let unplaced = media(None);
    let input = Input {
        media: &unplaced,
        trip: &t,
        home: None,
        photo: &p,
        subjects: &[],
        heading: None,
    };
    assert_eq!(
        make(&input, &Fixtures { fail: false }).unwrap_err(),
        Skip::NoPlace
    );
    let placed = media(Some(GeoPoint::new(43.7679, 11.2531)));
    let input = Input {
        media: &placed,
        trip: &t,
        home: None,
        photo: &p,
        subjects: &[],
        heading: None,
    };
    assert_eq!(
        make(&input, &Fixtures { fail: true }).unwrap_err(),
        Skip::MapUnavailable
    );
}

#[test]
fn the_fingerprint_follows_the_place_not_the_run() {
    let t = trip();
    let a = media(Some(GeoPoint::new(43.7679, 11.2531)));
    assert_eq!(fingerprint(&a, &t, None), fingerprint(&a, &t, None));
    let moved = media(Some(GeoPoint::new(43.7700, 11.2531)));
    assert_ne!(fingerprint(&a, &t, None), fingerprint(&moved, &t, None));
    // Home nearby turns the dot into a ring: a different note.
    assert_ne!(
        fingerprint(&a, &t, None),
        fingerprint(&a, &t, Some(GeoPoint::new(43.768, 11.253)))
    );
}

#[test]
fn a_landmark_in_the_photo_wins_over_the_municipality_around_it() {
    let at = GeoPoint::new(43.7680, 11.2531);
    let m = media(Some(at));
    let mut t = trip();
    let tied = |name: &str, kind: &str, lat: f64| mantelpiece_core::trip::Landmark {
        name: name.into(),
        at: GeoPoint::new(lat, 11.2531),
        distance_m: 0.0,
        source: "aws".into(),
        kind: Some(kind.into()),
        dim_m: None,
        media: vec![MediaId::new("m1")],
        country: Some("Italy".into()),
    };
    t.days[0].landmarks = vec![
        tied("Comune di Firenze", "city", 43.7690),
        tied("Uffizi", "landmark", 43.7700),
        tied("Ponte Vecchio", "landmark", 43.7681),
    ];
    let p = photo();
    let input = Input {
        media: &m,
        trip: &t,
        home: None,
        photo: &p,
        subjects: &[],
        heading: None,
    };
    let made = make(&input, &Fixtures { fail: false }).expect("a note");
    assert_eq!(
        made.feature.unwrap().name,
        "Ponte Vecchio",
        "the nearest landmark, not the region"
    );
}
