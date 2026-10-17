//! Render a day film from a folder of JPEGs with a local ffmpeg, through the same code the
//! Lambda runs (in-memory storage, real encoder). For checking the look and the timing
//! without AWS:
//!
//! ```sh
//! cargo run --release -p mantelpiece-film --example local -- <jpeg-dir> <music-dir> <out.mp4> [name]
//! ```
//!
//! `<music-dir>` holds the library's `.m4a` files (`infra/assets/music`, from
//! `make music`). Photos are taken in file-name order as one day.

use mantelpiece_core::ids::{BatchId, ContributorId, HouseholdId, MediaId, TripId};
use mantelpiece_core::media::{Enrichment, MediaItem, MediaKind, ProcessingState, Signals};
use mantelpiece_core::trip::{Route, Trip, TripDay};
use mantelpiece_film::{rendition_key, run, FfmpegEncoder, Film, Input, FPS, FRAME};
use mantelpiece_store::memory::{MemoryBlobs, MemoryStorage};
use mantelpiece_store::{Blobs, Storage};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use time::macros::{date, datetime};

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        return Err("usage: local <jpeg-dir> <music-dir> <out.mp4> [name]".into());
    }
    let (dir, music_dir, out) = (
        PathBuf::from(&args[1]),
        PathBuf::from(&args[2]),
        PathBuf::from(&args[3]),
    );
    let name = args.get(4).cloned().unwrap_or_else(|| "A day out".into());
    let household = HouseholdId::new("h");
    let storage = Arc::new(MemoryStorage::new());
    let blobs = Arc::new(MemoryBlobs::default());

    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("jpg") || x.eq_ignore_ascii_case("jpeg"))
        })
        .collect();
    files.sort();
    let mut ids = Vec::new();
    for (i, path) in files.iter().enumerate() {
        let bytes = std::fs::read(path)?;
        let (w, h) = image::image_dimensions(path)?;
        let id = MediaId::new(format!("m{i:03}"));
        storage
            .put_media(&MediaItem {
                id: id.clone(),
                household: household.clone(),
                contributor: ContributorId::new("c"),
                batch: BatchId::new("b"),
                kind: MediaKind::Photo,
                original_key: String::new(),
                original_filename: path.display().to_string(),
                bytes: 0,
                sha256: String::new(),
                captured: None,
                gps: None,
                orientation: 1,
                signals: Signals {
                    width: Some(w),
                    height: Some(h),
                    ..Signals::default()
                },
                enrichment: Enrichment::default(),
                quality_score: Some(6.0),
                burst_group: None,
                is_keeper: true,
                trip: Some(TripId::new("t")),
                pinned: false,
                hidden: false,
                pinned_by: BTreeSet::new(),
                hidden_by: BTreeSet::new(),
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
            })
            .await?;
        blobs
            .put(&rendition_key(&household, &id), bytes, "image/jpeg")
            .await?;
        ids.push(id);
    }
    storage
        .put_trip(&Trip {
            id: TripId::new("t"),
            household: household.clone(),
            name,
            start: datetime!(2026-03-25 10:00 UTC),
            end: datetime!(2026-03-25 20:00 UTC),
            days: vec![TripDay {
                index: 1,
                date: date!(2026 - 03 - 25),
                media: ids,
                route: Route::default(),
                title: None,
                story: None,
                undated: vec![],
                landmarks: vec![],
            }],
            route: Route::default(),
            highlights: vec![],
            contributors: vec![],
            clock_offsets: vec![],
            narrative: None,
            hero: None,
            members: vec![],
        })
        .await?;
    for track in mantelpiece_reels::library() {
        let file = music_dir.join(format!("{}.m4a", track.id));
        if let Ok(bytes) = std::fs::read(&file) {
            blobs.put(&track.file, bytes, "audio/mp4").await?;
        }
    }
    let mut encoder = FfmpegEncoder::from_env();
    if std::env::var("FFMPEG").is_err() {
        encoder.binary = PathBuf::from("ffmpeg");
    }
    let film = Film {
        storage,
        blobs: blobs.clone(),
        encoder: Arc::new(encoder),
        clock: Arc::new(time::OffsetDateTime::now_utc),
        frame: FRAME,
        fps: FPS,
    };
    let started = std::time::Instant::now();
    let output = run(
        &film,
        Input {
            household,
            trip: TripId::new("t"),
            day: 1,
        },
    )
    .await?;
    let key = output
        .key
        .ok_or_else(|| format!("skipped: {:?}", output.skipped))?;
    std::fs::write(&out, blobs.get(&key).await?)?;
    println!(
        "{} in {:.1}s",
        out.display(),
        started.elapsed().as_secs_f64()
    );
    Ok(())
}
