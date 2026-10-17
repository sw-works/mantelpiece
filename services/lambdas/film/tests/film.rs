//! Day films against in-memory backends and a counting encoder (no ffmpeg).

#![allow(clippy::cast_possible_truncation)]

use mantelpiece_core::events::EventKind;
use mantelpiece_core::film::FilmKeys;
use mantelpiece_core::ids::{BatchId, ContributorId, HouseholdId, MediaId, TripId};
use mantelpiece_core::media::{Enrichment, MediaItem, MediaKind, ProcessingState, Signals};
use mantelpiece_core::trip::{Route, Trip, TripDay};
use mantelpiece_film::{rendition_key, run, Film, Input, MemoryEncoder};
use mantelpiece_reels::plan::Frame;
use mantelpiece_store::memory::{MemoryBlobs, MemoryStorage};
use mantelpiece_store::{Blobs, Storage};
use std::collections::BTreeSet;
use std::sync::Arc;
use time::macros::{date, datetime};

const SMALL: Frame = Frame {
    width: 64,
    height: 36,
};

fn jpeg(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, 200])
    });
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85)
        .encode_image(&img)
        .unwrap();
    out
}

fn photo(id: &str) -> MediaItem {
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
            width: Some(160),
            height: Some(120),
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
    }
}

fn trip(day1: &[&str]) -> Trip {
    Trip {
        id: TripId::new("t"),
        household: HouseholdId::new("h"),
        name: "Florence".into(),
        start: datetime!(2026-03-25 10:00 UTC),
        end: datetime!(2026-03-25 20:00 UTC),
        days: vec![TripDay {
            index: 1,
            date: date!(2026 - 03 - 25),
            media: day1.iter().map(|s| MediaId::new(*s)).collect(),
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
    }
}

async fn setup(
    ids: &[&str],
) -> (
    Film,
    Arc<MemoryStorage>,
    Arc<MemoryBlobs>,
    Arc<MemoryEncoder>,
) {
    let storage = Arc::new(MemoryStorage::new());
    let blobs = Arc::new(MemoryBlobs::default());
    let encoder = Arc::new(MemoryEncoder::default());
    storage.put_trip(&trip(ids)).await.unwrap();
    for id in ids {
        storage.put_media(&photo(id)).await.unwrap();
        blobs
            .put(
                &rendition_key(&HouseholdId::new("h"), &MediaId::new(*id)),
                jpeg(160, 120),
                "image/jpeg",
            )
            .await
            .unwrap();
    }
    for track in mantelpiece_reels::library() {
        blobs
            .put(&track.file, b"m4a".to_vec(), "audio/mp4")
            .await
            .unwrap();
    }
    let film = Film {
        storage: storage.clone(),
        blobs: blobs.clone(),
        encoder: encoder.clone(),
        clock: Arc::new(|| datetime!(2026-03-25 21:00 UTC)),
        frame: SMALL,
        fps: 10,
    };
    (film, storage, blobs, encoder)
}

fn input() -> Input {
    Input {
        household: HouseholdId::new("h"),
        trip: TripId::new("t"),
        day: 1,
    }
}

#[test]
fn a_day_is_rendered_stored_and_announced_then_skipped_when_unchanged() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let (film, storage, blobs, encoder) = setup(&["m0", "m1", "m2", "m3", "m4"]).await;
        let out = run(&film, input()).await.unwrap();
        let keys = FilmKeys::new(&HouseholdId::new("h"), &TripId::new("t"));
        assert_eq!(out.key.as_deref(), Some(keys.film(1).as_str()));
        assert_eq!(blobs.get(&keys.film(1)).await.unwrap(), b"mp4");
        let (fingerprint, duration_ms, made_at) =
            FilmKeys::read_marker(&blobs.get(&keys.marker(1)).await.unwrap());
        assert_eq!(made_at, "2026-03-25T21:00:00Z");
        assert!(!fingerprint.is_empty());
        assert_eq!(*encoder.frames.lock().unwrap(), duration_ms * 10 / 1000);
        // The first frame is black: the film fades in.
        let first = encoder.first.lock().unwrap().clone().unwrap();
        assert_eq!(first.len(), 64 * 36 * 4);
        assert!(first.chunks(4).all(|p| p[0] < 8 && p[1] < 8 && p[2] < 8));
        let events = storage
            .events_since(&HouseholdId::new("h"), 0, 10)
            .await
            .unwrap();
        assert!(events
            .iter()
            .any(|e| matches!(&e.kind, EventKind::FilmReady { day: 1, .. })));

        let again = run(&film, input()).await.unwrap();
        assert_eq!(again.skipped.as_deref(), Some("up to date"));
        assert!(!blobs.log.lock().unwrap().is_empty());
    });
}

#[test]
fn too_few_photos_or_no_day_is_skipped_without_a_film() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let (film, _storage, blobs, _encoder) = setup(&["m0", "m1"]).await;
        let out = run(&film, input()).await.unwrap();
        assert_eq!(out.skipped.as_deref(), Some("too few photos"));
        let keys = FilmKeys::new(&HouseholdId::new("h"), &TripId::new("t"));
        assert!(blobs.get(&keys.film(1)).await.is_err());
        let none = run(&film, Input { day: 7, ..input() }).await.unwrap();
        assert_eq!(none.skipped.as_deref(), Some("no such day"));
    });
}
