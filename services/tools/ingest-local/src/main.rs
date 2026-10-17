//! Run the ingest pipeline over a folder of photos on this machine.
//!
//! Signal (EXIF, checksum, perceptual hash, sharpness, renditions) and organize (bursts,
//! keepers, clock alignment, trips, days, routes, highlights) are the real code with
//! in-memory stores. Enrich is real too, but its models are stubbed unless `--aws` is
//! given: without it every keeper gets no labels, no text and no description, which is
//! exactly what the pipeline would produce for a household with no model budget. With
//! `--aws` the keepers go to Rekognition and Bedrock using the ambient AWS credentials.
//!
//! ```text
//! cargo run --release -p ingest-local -- ~/Pictures/trip [more dirs] \
//!     [--limit 40] [--contributor hanyu] [--out ./ingest-local-out] [--aws] [--model id]
//! ```
//!
//! Writes `report.md`, `report.json` and the thumb and TV renditions under `--out`.

use mantelpiece_core::household::Household;
use mantelpiece_core::ids::{BatchId, ContributorId, HouseholdId, MediaId};
use mantelpiece_core::media::{Enrichment, MediaItem, MediaKind, ProcessingState, Signals};
use mantelpiece_ingest::checksum::sha256_hex;
use mantelpiece_ingest_lambda::{Ingest, Task};
use mantelpiece_store::bus::MemoryBus;
use mantelpiece_store::faces::MemoryFaces;
use mantelpiece_store::maps::MemoryMaps;
use mantelpiece_store::memory::{MemoryBlobs, MemoryStorage};
use mantelpiece_store::places::{NoPlaces, Places, WikipediaPlaces};
use mantelpiece_store::vectors::MemoryVectors;
use mantelpiece_store::vision::{AwsVision, MemoryVision};
use mantelpiece_store::{Blobs, Storage, Vision};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Debug)]
struct Args {
    dirs: Vec<PathBuf>,
    limit: usize,
    contributor: String,
    out: PathBuf,
    aws: bool,
    model: Option<String>,
    places: bool,
    min_trip_days: u32,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        dirs: vec![],
        limit: usize::MAX,
        contributor: "owner".into(),
        out: PathBuf::from("ingest-local-out"),
        aws: false,
        model: None,
        places: true,
        min_trip_days: 2,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--limit" => {
                args.limit = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("--limit needs a number")?;
            }
            "--contributor" => args.contributor = it.next().ok_or("--contributor needs a name")?,
            "--out" => args.out = PathBuf::from(it.next().ok_or("--out needs a path")?),
            "--model" => args.model = Some(it.next().ok_or("--model needs an id")?),
            "--aws" => args.aws = true,
            "--no-places" => args.places = false,
            "--min-trip-days" => {
                args.min_trip_days = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("--min-trip-days needs a number")?;
            }
            "-h" | "--help" => return Err("usage: ingest-local <dir>... [--limit N] [--contributor name] [--out dir] [--aws] [--model id] [--no-places] [--min-trip-days N]".into()),
            other if other.starts_with("--") => return Err(format!("unknown flag {other}")),
            dir => args.dirs.push(PathBuf::from(dir)),
        }
    }
    if args.dirs.is_empty() {
        return Err("give at least one folder of photos".into());
    }
    Ok(args)
}

const PHOTO_EXT: &[&str] = &["jpg", "jpeg", "png"];
const SKIPPED_EXT: &[&str] = &[
    "heic", "heif", "cr3", "cr2", "arw", "dng", "mp4", "mov", "m4v",
];

fn ext_of(p: &Path) -> String {
    p.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Photos the decoder handles, in name order; everything else counted as skipped.
fn collect(dirs: &[PathBuf], limit: usize) -> (Vec<PathBuf>, Vec<(PathBuf, &'static str)>) {
    let mut files = vec![];
    let mut skipped = vec![];
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(dir) else {
            skipped.push((dir.clone(), "not a readable folder"));
            continue;
        };
        let mut entries: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        entries.sort();
        for p in entries {
            let ext = ext_of(&p);
            if PHOTO_EXT.contains(&ext.as_str()) {
                files.push(p);
            } else if SKIPPED_EXT.contains(&ext.as_str()) {
                skipped.push((
                    p,
                    "format needs a build step or the poster Lambda (heic, raw, video)",
                ));
            }
        }
    }
    files.truncate(limit);
    (files, skipped)
}

fn item(
    household: &HouseholdId,
    contributor: &ContributorId,
    filename: &str,
    bytes: &[u8],
    now: OffsetDateTime,
) -> MediaItem {
    let id = MediaId::random();
    MediaItem {
        original_key: MediaItem::original_key(household, contributor, &id, filename),
        id,
        household: household.clone(),
        contributor: contributor.clone(),
        batch: BatchId::new("local"),
        kind: MediaKind::from_filename(filename),
        original_filename: filename.into(),
        bytes: bytes.len() as u64,
        sha256: sha256_hex(bytes),
        captured: None,
        gps: None,
        orientation: 1,
        signals: Signals::default(),
        enrichment: Enrichment::default(),
        quality_score: None,
        burst_group: None,
        is_keeper: false,
        trip: None,
        pinned: false,
        hidden: false,
        pinned_by: BTreeSet::default(),
        hidden_by: BTreeSet::default(),
        text: None,
        description: None,
        cleanup_reason: None,
        people: vec![],
        removed_at: None,
        restore_until: None,
        field_note: None,
        heading: None,
        state: ProcessingState::Uploaded,
        uploaded_at: now,
    }
}

fn short(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        out.push('…');
    }
    out.replace('\n', " ").replace('|', "\\|")
}

fn fmt_time(t: Option<OffsetDateTime>) -> String {
    t.and_then(|t| t.format(&Rfc3339).ok())
        .unwrap_or_else(|| "-".into())
}

#[allow(clippy::too_many_lines)]
#[tokio::main]
async fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let (files, skipped) = collect(&args.dirs, args.limit);
    if files.is_empty() {
        eprintln!("no jpg, jpeg or png files in {:?}", args.dirs);
        std::process::exit(1);
    }
    std::fs::create_dir_all(args.out.join("renditions")).expect("create out dir");

    let storage = Arc::new(MemoryStorage::new());
    let blobs = Arc::new(MemoryBlobs::default());
    let vision: Arc<dyn Vision> = if args.aws {
        let aws = aws_config::load_from_env().await;
        Arc::new(AwsVision::from_config(&aws, args.model.clone()))
    } else {
        Arc::new(MemoryVision::default())
    };
    let ingest = Ingest {
        storage: storage.clone(),
        blobs: blobs.clone(),
        vision,
        vectors: Arc::new(MemoryVectors::default()),
        faces: Arc::new(MemoryFaces::with_faces_per_image(0)),
        maps: Arc::new(MemoryMaps::default()),
        bus: Arc::new(MemoryBus::default()),
        places: if args.places {
            Arc::new(WikipediaPlaces::default()) as Arc<dyn Places>
        } else {
            Arc::new(NoPlaces)
        },
        encyclopedia: if args.places {
            Arc::new(mantelpiece_store::encyclopedia::Wikipedia::default())
                as Arc<dyn mantelpiece_store::Encyclopedia>
        } else {
            Arc::new(mantelpiece_store::encyclopedia::NoEncyclopedia)
        },
        geography: Arc::new(mantelpiece_store::geography::OpenGeography::default()),
        // Field notes need the pipeline's enrich and tiles; run them with the spike example.
        field_notes: false,
        cluster: mantelpiece_trips::cluster::Params {
            min_trip_days: args.min_trip_days,
            ..mantelpiece_trips::cluster::Params::default()
        },
        films: Arc::new(mantelpiece_ingest_lambda::films::MemoryDayFilms::default()),
        clock: Arc::new(OffsetDateTime::now_utc),
    };
    let household = HouseholdId::new("local");
    let contributor = ContributorId::new(&args.contributor);
    let now = OffsetDateTime::now_utc();
    storage
        .put_household(&Household::new(household.clone(), "Local run", now))
        .await
        .expect("household");

    // Signal: one task per file, as the Signal state machine would run them.
    let started = Instant::now();
    let mut names: Vec<(MediaId, String)> = vec![];
    let mut signal_out: Vec<Value> = vec![];
    for (i, path) in files.iter().enumerate() {
        let bytes = std::fs::read(path).expect("read photo");
        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        let m = item(&household, &contributor, &filename, &bytes, now);
        blobs
            .put(&m.original_key, bytes, "image/jpeg")
            .await
            .expect("put original");
        storage.put_media(&m).await.expect("put media");
        let t = Instant::now();
        let out = ingest
            .run(Task::Signal {
                key: m.original_key.clone(),
            })
            .await
            .unwrap_or_else(|e| json!({ "state": "error", "reason": e.to_string() }));
        eprintln!(
            "signal {:>3}/{} {:<40} {:>8} {:>5} ms",
            i + 1,
            files.len(),
            short(&filename, 40),
            out["state"].as_str().unwrap_or("?"),
            t.elapsed().as_millis()
        );
        names.push((m.id.clone(), filename));
        signal_out.push(out);
    }
    let signal_ms = started.elapsed().as_millis();

    // Organize, enrich keepers, organize again: the Organize state machine's shape.
    let t = Instant::now();
    let organize1 = ingest
        .run(Task::Organize {
            household: household.clone(),
        })
        .await
        .expect("organize");
    let organize1_ms = t.elapsed().as_millis();
    let keepers: Vec<MediaId> = organize1["keepers"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|k| k.as_str().map(MediaId::new))
                .collect()
        })
        .unwrap_or_default();
    let t = Instant::now();
    let mut enrich_out: Vec<Value> = vec![];
    for (i, id) in keepers.iter().enumerate() {
        let out = ingest
            .run(Task::Enrich {
                household: household.clone(),
                media_id: id.clone(),
            })
            .await
            .unwrap_or_else(|e| json!({ "state": "error", "reason": e.to_string() }));
        eprintln!(
            "enrich {:>3}/{} {} {}",
            i + 1,
            keepers.len(),
            id,
            out["state"].as_str().unwrap_or("?")
        );
        enrich_out.push(out);
    }
    let enrich_ms = t.elapsed().as_millis();
    let t = Instant::now();
    let organize2 = ingest
        .run(Task::Organize {
            household: household.clone(),
        })
        .await
        .expect("organize again");
    let organize2_ms = t.elapsed().as_millis();

    // Gather the records and write renditions out for eyeballing.
    let mut media = storage.list_media(&household).await.expect("list media");
    media.sort_by_key(|m| (m.captured.map(|c| c.utc), m.original_filename.clone()));
    for m in &media {
        for r in ["thumb", "tv"] {
            let key = format!("renditions/{}/{}/{r}.jpg", m.household, m.id);
            if let Ok(bytes) = blobs.get(&key).await {
                let stem = Path::new(&m.original_filename)
                    .file_stem()
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                std::fs::write(
                    args.out.join("renditions").join(format!("{stem}-{r}.jpg")),
                    bytes,
                )
                .expect("write rendition");
            }
        }
    }
    let trips = storage.list_trips(&household).await.expect("list trips");

    // Report.
    let mut md = String::new();
    let _ = writeln!(md, "# ingest-local report\n");
    let _ = writeln!(
        md,
        "Run at {} over {} folder(s); models: {}.\n",
        fmt_time(Some(now)),
        args.dirs.len(),
        if args.aws {
            "Rekognition + Bedrock (--aws)"
        } else {
            "stubbed (no --aws): labels, text and descriptions empty by design"
        }
    );
    let _ = writeln!(md, "| Stage | Items | Wall time |\n|---|---|---|");
    let _ = writeln!(md, "| signal | {} | {signal_ms} ms |", files.len());
    let _ = writeln!(
        md,
        "| organize | {} media, {} groups, {} keepers, {} trips | {organize1_ms} ms |",
        organize1["media"],
        organize1["groups"],
        keepers.len(),
        organize1["trips"].as_array().map_or(0, Vec::len)
    );
    let _ = writeln!(
        md,
        "| enrich | {} keepers | {enrich_ms} ms |",
        keepers.len()
    );
    let _ = writeln!(
        md,
        "| organize again | {} media | {organize2_ms} ms |\n",
        organize2["media"]
    );
    if !skipped.is_empty() {
        let _ = writeln!(
            md,
            "Skipped {} file(s) the local decoder does not take (heic, raw, video):",
            skipped.len()
        );
        for (p, why) in &skipped {
            let _ = writeln!(
                md,
                "- `{}`: {why}",
                p.file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default()
            );
        }
        md.push('\n');
    }

    let _ = writeln!(md, "## Trips\n");
    if trips.is_empty() {
        let _ = writeln!(md, "No trip detected.\n");
    }
    for trip in &trips {
        let _ = writeln!(
            md,
            "### {} ({} to {})\n",
            trip.name,
            fmt_time(Some(trip.start)),
            fmt_time(Some(trip.end))
        );
        let _ = writeln!(
            md,
            "- {} day(s), {} route point(s), {} highlight(s), hero {}",
            trip.days.len(),
            trip.route.points.len(),
            trip.highlights.len(),
            trip.hero
                .as_ref()
                .map_or("-".to_owned(), ToString::to_string)
        );
        for o in &trip.clock_offsets {
            let _ = writeln!(
                md,
                "- clock offset {}: {} s from {} GPS pair(s)",
                o.contributor, o.seconds, o.samples
            );
        }
        let _ = writeln!(
            md,
            "\n| Day | Date | Photos | Undated | Route points (estimated) | Route km | Title |\n|---|---|---|---|---|---|---|"
        );
        for d in &trip.days {
            let estimated = d.route.points.iter().filter(|p| p.estimated).count();
            let _ = writeln!(
                md,
                "| {} | {} | {} | {} | {} ({estimated}) | {:.1} | {} |",
                d.index,
                d.date,
                d.media.len(),
                d.undated.len(),
                d.route.points.len(),
                mantelpiece_trips::route::length_m(&d.route) / 1000.0,
                d.title.as_deref().unwrap_or("-")
            );
        }
        md.push('\n');
        for d in &trip.days {
            if d.landmarks.is_empty() {
                continue;
            }
            let _ = writeln!(
                md,
                "Day {} landmarks along the route (photos within {} m, source):\n",
                d.index,
                mantelpiece_ingest_lambda::organize::LANDMARK_RADIUS_M
            );
            for l in &d.landmarks {
                let _ = writeln!(
                    md,
                    "- {} ({} photo{}, {:.0} m off the route, {})",
                    l.name,
                    l.media.len(),
                    if l.media.len() == 1 { "" } else { "s" },
                    l.distance_m,
                    l.source
                );
            }
            md.push('\n');
        }
    }

    let _ = writeln!(md, "## Photos ({})\n", media.len());
    let _ = writeln!(md, "| File | Captured (local) | Zone | GPS | Size | Sharp | Expo | Group | Keeper | Score | Trip day | Labels | Landmark | Text | Description | Cleanup | State |");
    let _ = writeln!(
        md,
        "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"
    );
    let day_of = |m: &MediaItem| -> String {
        let Some(trip) = m.trip.as_ref() else {
            return "-".into();
        };
        trips
            .iter()
            .find(|t| &t.id == trip)
            .and_then(|t| {
                t.days
                    .iter()
                    .find(|d| d.media.contains(&m.id) || d.undated.contains(&m.id))
                    .map(|d| format!("{} d{}", short(&t.name, 14), d.index))
            })
            .unwrap_or_else(|| "trip, unplaced".into())
    };
    for m in &media {
        let labels: Vec<String> = m
            .enrichment
            .scene_tags
            .iter()
            .map(|t| format!("{} {:.2}", t.name, t.confidence))
            .collect();
        let _ = writeln!(
            md,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {}{} |",
            short(&m.original_filename, 28),
            m.captured.map_or("-".into(), |c| fmt_time(Some(c.local()))),
            m.captured.map_or("-", |c| if c.zone_assumed { "assumed" } else { "exif" }),
            m.gps.map_or("-".into(), |g| format!("{:.4},{:.4}", g.lat, g.lon)),
            match (m.signals.width, m.signals.height) { (Some(w), Some(h)) => format!("{w}x{h}"), _ => "-".into() },
            m.signals.sharpness.map_or("-".into(), |s| format!("{s:.0}")),
            m.signals.exposure.map_or("-".into(), |e| format!("{e:.2}")),
            m.burst_group.as_deref().map_or("-".into(), |g| short(g, 8)),
            if m.is_keeper { "yes" } else { "" },
            m.quality_score.map_or("-".into(), |s| format!("{s:.2}")),
            day_of(m),
            short(&labels.join(", "), 40),
            m.enrichment.landmark.as_ref().map_or("-".into(), |t| short(&t.name, 20)),
            m.text.as_ref().map_or("-".into(), |t| short(t, 30)),
            m.description.as_ref().map_or("-".into(), |d| short(&d.text, 60)),
            m.cleanup_reason.as_ref().map_or("-".into(), |c| format!("{c:?}")),
            format!("{:?}", m.state).to_lowercase(),
            if m.enrichment.inherited_from.is_some() { " (inherited)" } else { "" },
        );
    }

    let report = json!({
        "run_at": fmt_time(Some(now)),
        "dirs": args.dirs,
        "models": if args.aws { "aws" } else { "stubbed" },
        "timings_ms": { "signal": signal_ms, "organize": organize1_ms, "enrich": enrich_ms, "organize_again": organize2_ms },
        "signal": signal_out,
        "organize": organize1,
        "enrich": enrich_out,
        "organize_again": organize2,
        "skipped": skipped.iter().map(|(p, why)| json!({ "file": p, "why": why })).collect::<Vec<_>>(),
        "media": media,
        "trips": trips,
        "files": names.iter().map(|(id, name)| json!({ "id": id, "file": name })).collect::<Vec<_>>(),
    });
    std::fs::write(
        args.out.join("routes.geojson"),
        serde_json::to_vec_pretty(&geojson(&trips, &media)).unwrap(),
    )
    .expect("write geojson");
    std::fs::write(args.out.join("map.html"), map_html(&trips, &media)).expect("write map.html");
    std::fs::write(args.out.join("report.md"), &md).expect("write report.md");
    std::fs::write(
        args.out.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .expect("write report.json");

    let with_time = media.iter().filter(|m| m.captured.is_some()).count();
    let with_gps = media.iter().filter(|m| m.gps.is_some()).count();
    println!(
        "{} photos: {} with capture time, {} with GPS, {} groups, {} keepers, {} trip(s); signal {} ms, organize {} ms, enrich {} ms",
        media.len(), with_time, with_gps, organize1["groups"], keepers.len(), trips.len(), signal_ms, organize1_ms, enrich_ms
    );
    println!("report: {}", args.out.join("report.md").display());
    println!("map:    {}", args.out.join("map.html").display());
}

fn stem_of(media: &[MediaItem], id: &MediaId) -> String {
    media
        .iter()
        .find(|m| &m.id == id)
        .and_then(|m| {
            Path::new(&m.original_filename)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| id.to_string())
}

/// One FeatureCollection: a LineString per day (GPS and estimated points flagged per
/// vertex in `properties.estimated`), a Point per landmark with its photo count, and a
/// Point per route photo linking to its thumbnail.
fn geojson(trips: &[mantelpiece_core::trip::Trip], media: &[MediaItem]) -> Value {
    let mut features = vec![];
    for trip in trips {
        for d in &trip.days {
            let coords: Vec<Value> = d
                .route
                .points
                .iter()
                .map(|p| json!([p.at.lon, p.at.lat]))
                .collect();
            if coords.len() >= 2 {
                features.push(json!({
                    "type": "Feature",
                    "geometry": { "type": "LineString", "coordinates": coords },
                    "properties": { "trip": trip.name, "day": d.index, "date": d.date.to_string(), "title": d.title, "estimated": d.route.points.iter().map(|p| p.estimated).collect::<Vec<_>>(), "length_m": mantelpiece_trips::route::length_m(&d.route) }
                }));
            }
            for p in &d.route.points {
                features.push(json!({
                    "type": "Feature",
                    "geometry": { "type": "Point", "coordinates": [p.at.lon, p.at.lat] },
                    "properties": { "kind": "photo", "day": d.index, "media": p.media, "file": stem_of(media, &p.media), "time": fmt_time(Some(p.time)), "estimated": p.estimated, "thumb": format!("renditions/{}-thumb.jpg", stem_of(media, &p.media)) }
                }));
            }
            for l in &d.landmarks {
                features.push(json!({
                    "type": "Feature",
                    "geometry": { "type": "Point", "coordinates": [l.at.lon, l.at.lat] },
                    "properties": { "kind": "landmark", "day": d.index, "name": l.name, "photos": l.media.len(), "distance_m": l.distance_m, "source": l.source }
                }));
            }
        }
    }
    json!({ "type": "FeatureCollection", "features": features })
}

/// Leaflet over OpenStreetMap tiles, data inlined: routes per day, dashed where the
/// position was estimated, landmark pins with counts, photo dots with thumbnails.
fn map_html(trips: &[mantelpiece_core::trip::Trip], media: &[MediaItem]) -> String {
    let data = serde_json::to_string(&geojson(trips, media)).unwrap();
    let title = trips
        .iter()
        .map(|t| t.name.as_str())
        .collect::<Vec<_>>()
        .join(" · ");
    // Names carry apostrophes ("Martyrs' Memorial"); JSON is the only safe way into JS.
    let title_js = serde_json::to_string(&title).unwrap();
    let title = title.replace('<', "&lt;");
    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><title>{title}</title>
<link rel="stylesheet" href="https://unpkg.com/leaflet@1.9.4/dist/leaflet.css">
<script src="https://unpkg.com/leaflet@1.9.4/dist/leaflet.js"></script>
<style>html,body,#map{{height:100%;margin:0}} .legend{{background:#fff;padding:8px 10px;font:13px system-ui;line-height:1.5}} img.thumb{{max-width:220px;display:block;margin-top:4px}}</style>
</head><body><div id="map"></div>
<script>
const data = {data};
const title = {title_js};
const map = L.map('map');
// CARTO's basemap: OpenStreetMap's own tile servers refuse a page opened from disk (no Referer).
L.tileLayer('https://{{s}}.basemaps.cartocdn.com/rastertiles/voyager/{{z}}/{{x}}/{{y}}{{r}}.png', {{maxZoom: 19, subdomains: 'abcd', attribution: '&copy; OpenStreetMap contributors &copy; CARTO'}}).addTo(map);
const colors = ['#d62828','#1d3557','#2a9d8f','#e76f51','#6a4c93','#f4a261','#264653'];
const bounds = [];
for (const f of data.features) {{
  const c = colors[(f.properties.day - 1) % colors.length];
  if (f.geometry.type === 'LineString') {{
    const pts = f.geometry.coordinates.map(([lon, lat]) => [lat, lon]);
    for (let i = 1; i < pts.length; i++) {{
      const est = f.properties.estimated[i] || f.properties.estimated[i - 1];
      L.polyline([pts[i - 1], pts[i]], {{color: c, weight: 4, opacity: 0.85, dashArray: est ? '6 8' : null}}).addTo(map)
        .bindTooltip(`${{f.properties.title || 'Day ' + f.properties.day}} · ${{(f.properties.length_m / 1000).toFixed(1)}} km`);
    }}
    pts.forEach(p => bounds.push(p));
  }} else if (f.properties.kind === 'photo') {{
    const [lon, lat] = f.geometry.coordinates;
    L.circleMarker([lat, lon], {{radius: 5, color: c, fillColor: f.properties.estimated ? '#fff' : c, fillOpacity: 1, weight: 2}}).addTo(map)
      .bindPopup(`<b>${{f.properties.file}}</b><br>${{f.properties.time}}${{f.properties.estimated ? '<br><i>position estimated from neighbours</i>' : ''}}<img class="thumb" src="${{f.properties.thumb}}">`);
    bounds.push([lat, lon]);
  }} else if (f.properties.kind === 'landmark') {{
    const [lon, lat] = f.geometry.coordinates;
    L.marker([lat, lon]).addTo(map).bindTooltip(`${{f.properties.name}} (${{f.properties.photos}} photo${{f.properties.photos === 1 ? '' : 's'}})`, {{permanent: f.properties.photos > 0, direction: 'right', offset: [8, -20]}});
    bounds.push([lat, lon]);
  }}
}}
if (bounds.length) map.fitBounds(bounds, {{padding: [30, 30]}}); else map.setView([0, 0], 2);
const legend = L.control({{position: 'bottomleft'}});
legend.onAdd = () => {{ const d = L.DomUtil.create('div', 'legend'); d.innerHTML = '<b>' + title + '</b><br>solid: GPS from the photo · dashed: position estimated from neighbours<br>pins: places along the route (Wikipedia geosearch) · dots: photos'; return d; }};
legend.addTo(map);
</script></body></html>
"#
    )
}
