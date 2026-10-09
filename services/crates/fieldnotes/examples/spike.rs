#![allow(
    clippy::many_single_char_names,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
//! Spike: field notes on real photos, with real map data over HTTPS (no AWS).
//!
//!     cargo run --release -p mantelpiece-fieldnotes --example spike -- <out-dir> <photo>...
//!
//! Writes `<name>.jpg` (photo with its note) and `<name>-mask.jpg` (detail heat, chosen
//! box) per photo, and prints the outcome. Tiles are cached under `<out-dir>/tiles`.

use image::{DynamicImage, GenericImageView, Rgb};
use mantelpiece_core::ids::MediaId;
use mantelpiece_core::media::MediaItem;
use mantelpiece_core::trip::{Route, Trip, TripDay};
use mantelpiece_fieldnotes::geo::TileId;
use mantelpiece_fieldnotes::{make, placement, raster, Input, Tiles};
use std::path::{Path, PathBuf};
use std::time::Instant;

struct Http {
    cache: PathBuf,
    vector_template: String,
}

fn get(url: &str) -> Result<Option<Vec<u8>>, String> {
    match ureq::get(url)
        .header("User-Agent", "Mantelpiece field notes spike")
        .call()
    {
        Ok(mut r) => r
            .body_mut()
            .with_config()
            .limit(10 << 20)
            .read_to_vec()
            .map(Some)
            .map_err(|e| e.to_string()),
        Err(ureq::Error::StatusCode(404 | 204)) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

impl Http {
    fn cached(&self, kind: &str, id: TileId, url: &str) -> Result<Option<Vec<u8>>, String> {
        let path = self
            .cache
            .join(format!("{kind}-{}-{}-{}", id.z, id.x, id.y));
        if let Ok(b) = std::fs::read(&path) {
            return Ok((!b.is_empty()).then_some(b));
        }
        let got = get(url)?;
        let _ = std::fs::write(&path, got.as_deref().unwrap_or_default());
        Ok(got)
    }
}

impl Tiles for Http {
    fn vector(&self, id: TileId) -> Result<Option<Vec<u8>>, String> {
        let url = self
            .vector_template
            .replace("{z}", &id.z.to_string())
            .replace("{x}", &id.x.to_string())
            .replace("{y}", &id.y.to_string());
        self.cached("omt", id, &url)
    }
    fn terrain(&self, id: TileId) -> Result<Option<Vec<u8>>, String> {
        let url = format!(
            "https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{}/{}/{}.png",
            id.z, id.x, id.y
        );
        self.cached("dem", id, &url)
    }
}

fn media(id: &str, bytes: &[u8]) -> MediaItem {
    let exif = mantelpiece_ingest::exif::parse(bytes);
    let mut m: MediaItem = serde_json::from_value(serde_json::json!({
        "id": id, "household": "h", "contributor": "c", "batch": "b", "kind": "photo",
        "original_key": "k", "original_filename": "f.jpg", "bytes": bytes.len(), "sha256": "00",
        "captured": null, "gps": null, "orientation": 1,
        "signals": {"sharpness": 0.0, "exposure": 0.0, "phash": null, "width": null, "height": null},
        "enrichment": {"scene_tags": [], "landmark": null, "face_count": 0, "smile_count": 0, "inherited_from": null},
        "quality_score": null, "burst_group": null, "is_keeper": true, "trip": null,
        "pinned": false, "hidden": false, "state": "enriched", "uploaded_at": "2026-01-01T00:00:00Z"
    }))
    .unwrap();
    m.gps = exif.gps;
    m.captured = exif.captured;
    m.orientation = exif.orientation;
    m
}

fn mask(photo: &DynamicImage, p: Option<placement::Placement>) -> DynamicImage {
    let g = placement::grid(photo);
    let (w, h) = photo.dimensions();
    let mut out = photo.to_rgb8();
    for (x, y, px) in out.enumerate_pixels_mut() {
        let c = g.cell(x * g.w / w, y * g.h / h);
        let heat = (c.detail / placement::MAX_DETAIL).min(1.0);
        let calm = if heat < 1.0 { 0.0 } else { 1.0 };
        *px = Rgb([
            ((f32::from(px[0]) * 0.5) + 127.0 * heat) as u8,
            ((f32::from(px[1]) * 0.5) + 60.0 * (1.0 - calm)) as u8,
            (f32::from(px[2]) * 0.5) as u8,
        ]);
    }
    if let Some(p) = p {
        let (l, t) = (
            (p.bbox.left * w as f32) as u32,
            (p.bbox.top * h as f32) as u32,
        );
        let (r, b) = (
            (p.bbox.right() * w as f32) as u32,
            (p.bbox.bottom() * h as f32) as u32,
        );
        for x in l..r.min(w) {
            for y in [t, b.min(h - 1)] {
                out.put_pixel(x, y, Rgb([255, 255, 0]));
            }
        }
        for y in t..b.min(h) {
            for x in [l, r.min(w - 1)] {
                out.put_pixel(x, y, Rgb([255, 255, 0]));
            }
        }
    }
    DynamicImage::ImageRgb8(out)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = PathBuf::from(&args[0]);
    std::fs::create_dir_all(out.join("tiles")).unwrap();
    let tilejson: serde_json::Value = serde_json::from_slice(
        &get("https://tiles.openfreemap.org/planet")
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let http = Http {
        cache: out.join("tiles"),
        vector_template: tilejson["tiles"][0].as_str().unwrap().to_owned(),
    };
    for path in &args[1..] {
        let started = Instant::now();
        let name = Path::new(path)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let bytes = std::fs::read(path).unwrap();
        let m = media(&name, &bytes);
        let photo = mantelpiece_ingest::renditions::upright(
            image::load_from_memory(&bytes).unwrap(),
            m.orientation,
        );
        let photo = photo.resize(1920, 1920, image::imageops::FilterType::Triangle);
        let mut day = TripDay {
            index: 1,
            date: m
                .captured
                .map_or(time::macros::date!(2015 - 07 - 01), |c| c.local().date()),
            media: vec![MediaId::new(name.clone())],
            route: Route::default(),
            title: None,
            story: None,
            undated: vec![],
            landmarks: vec![],
        };
        day.media.push(MediaId::new("x"));
        let trip = Trip {
            id: mantelpiece_core::ids::TripId::new("t"),
            household: mantelpiece_core::ids::HouseholdId::new("h"),
            name: "Iceland".into(),
            start: time::OffsetDateTime::UNIX_EPOCH,
            end: time::OffsetDateTime::UNIX_EPOCH,
            days: vec![day],
            route: Route::default(),
            highlights: vec![],
            contributors: vec![],
            clock_offsets: vec![],
            narrative: None,
            hero: None,
            members: vec![],
        };
        let input = Input {
            media: &m,
            trip: &trip,
            home: None,
            photo: &photo,
            subjects: &[],
            heading: None,
        };
        let (pw, ph) = photo.dimensions();
        let placed = placement::place(&placement::grid(&photo), pw as f32 / ph as f32, &[]);
        mask(&photo, placed)
            .save(out.join(format!("{name}-mask.jpg")))
            .unwrap();
        match make(&input, &http) {
            Ok(made) => {
                let layer = raster::layer(&made.drawing, made.placement, pw, ph);
                let composed = raster::compose(&photo, &layer);
                composed.save(out.join(format!("{name}.jpg"))).unwrap();
                println!(
                    "{name}: {:?} {:?} area {:.2} detail {:.3} labels {:?} title {:?} in {:.1}s",
                    made.mode,
                    made.ink,
                    made.placement.area(),
                    placed.map_or(0.0, |p| p.detail),
                    made.drawing
                        .labels
                        .iter()
                        .map(|l| l.text.as_str())
                        .collect::<Vec<_>>(),
                    made.drawing.title.as_ref().map(|t| t.place.as_str()),
                    started.elapsed().as_secs_f32()
                );
            }
            Err(skip) => println!("{name}: no note ({})", skip.reason()),
        }
    }
}
