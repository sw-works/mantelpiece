//! Poster frames against in-memory backends and a scripted frame extractor.

#![allow(clippy::cast_possible_truncation)]

use mantelpiece_core::ids::{BatchId, ContributorId, HouseholdId, MediaId};
use mantelpiece_core::media::{Enrichment, MediaItem, MediaKind, ProcessingState, Signals};
use mantelpiece_poster::{poster_key, run, Input, MemoryFrames, Poster};
use mantelpiece_store::memory::{MemoryBlobs, MemoryStorage};
use mantelpiece_store::{Blobs, Storage};
use std::collections::BTreeSet;
use std::sync::Arc;
use time::macros::datetime;

fn jpeg(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, 40])
    });
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90)
        .encode_image(&img)
        .unwrap();
    out
}

fn media(id: &str, kind: MediaKind) -> MediaItem {
    let (household, contributor, mid) = (
        HouseholdId::new("house"),
        ContributorId::new("c"),
        MediaId::new(id),
    );
    MediaItem {
        original_key: format!("originals/house/c/{id}.mov"),
        id: mid,
        household,
        contributor,
        batch: BatchId::new("b"),
        kind,
        original_filename: format!("{id}.mov"),
        bytes: 3,
        sha256: "00".repeat(32),
        captured: None,
        gps: None,
        orientation: 6,
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
        state: ProcessingState::Signalled,
        uploaded_at: datetime!(2026-09-08 10:00:00 UTC),
    }
}

#[tokio::test]
async fn video_gets_poster_thumb_tv_and_signals_and_photos_are_skipped() {
    let storage = Arc::new(MemoryStorage::new());
    let blobs = Arc::new(MemoryBlobs::default());
    let poster = Poster {
        storage: storage.clone(),
        blobs: blobs.clone(),
        frames: Arc::new(MemoryFrames {
            jpeg: jpeg(1600, 900),
        }),
        clock: Arc::new(|| datetime!(2026-09-08 10:00:00 UTC)),
    };
    let h = HouseholdId::new("house");
    let clip = media("clip", MediaKind::Video);
    blobs
        .put(&clip.original_key, b"mov".to_vec(), "video/quicktime")
        .await
        .unwrap();
    storage.put_media(&clip).await.unwrap();

    let out = run(
        &poster,
        Input {
            household: h.clone(),
            media_id: MediaId::new("clip"),
        },
    )
    .await
    .unwrap();
    assert_eq!(out.skipped, None);
    assert_eq!(out.state, ProcessingState::Signalled);
    let poster_jpeg = blobs.get(&poster_key("house", "clip")).await.unwrap();
    let decoded = image::load_from_memory(&poster_jpeg).unwrap();
    // Orientation 6 rotates the 1600x900 frame upright.
    assert_eq!((decoded.width(), decoded.height()), (900, 1600));
    let thumb =
        image::load_from_memory(&blobs.get("renditions/house/clip/thumb.jpg").await.unwrap())
            .unwrap();
    assert_eq!(thumb.height(), 400);
    assert!(blobs.get("renditions/house/clip/tv.jpg").await.is_ok());
    let stored = storage
        .get_media(&h, &MediaId::new("clip"))
        .await
        .unwrap()
        .unwrap();
    assert!(stored.signals.phash.is_some());
    assert_eq!(stored.signals.width, Some(900));

    let photo = media("still", MediaKind::Photo);
    storage.put_media(&photo).await.unwrap();
    let skipped = run(
        &poster,
        Input {
            household: h.clone(),
            media_id: MediaId::new("still"),
        },
    )
    .await
    .unwrap();
    assert_eq!(skipped.skipped.as_deref(), Some("not a video"));
    assert!(run(
        &poster,
        Input {
            household: h,
            media_id: MediaId::new("nope")
        }
    )
    .await
    .is_err());
}
