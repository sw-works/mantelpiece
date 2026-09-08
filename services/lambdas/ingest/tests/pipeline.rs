//! Signal, organize and enrich against in-memory backends.

#![allow(clippy::cast_possible_truncation, clippy::too_many_lines)]

use memorylane_core::geo::GeoPoint;
use memorylane_core::ids::{BatchId, ContributorId, HouseholdId, MediaId};
use memorylane_core::media::{
    CaptureTime, Enrichment, MediaItem, MediaKind, ProcessingState, Signals, Tag,
};
use memorylane_ingest::checksum::sha256_hex;
use memorylane_ingest_lambda::bus::MemoryBus;
use memorylane_ingest_lambda::vision::FixedVision;
use memorylane_ingest_lambda::{Ingest, Task};
use memorylane_store::memory::{MemoryBlobs, MemoryStorage};
use memorylane_store::{Blobs, Storage};
use std::sync::Arc;
use time::macros::datetime;
use time::{Duration, OffsetDateTime};

struct Harness {
    ingest: Ingest,
    storage: Arc<MemoryStorage>,
    blobs: Arc<MemoryBlobs>,
    bus: Arc<MemoryBus>,
}

fn harness(enrichment: Enrichment) -> Harness {
    let storage = Arc::new(MemoryStorage::new());
    let blobs = Arc::new(MemoryBlobs::default());
    let bus = Arc::new(MemoryBus::default());
    let ingest = Ingest {
        storage: storage.clone(),
        blobs: blobs.clone(),
        vision: Arc::new(FixedVision { enrichment }),
        bus: bus.clone(),
        clock: Arc::new(|| datetime!(2026-09-08 10:00:00 UTC)),
    };
    Harness {
        ingest,
        storage,
        blobs,
        bus,
    }
}

const H: &str = "house";

fn base() -> OffsetDateTime {
    datetime!(2026-04-03 01:00:00 UTC)
}

fn media(id: &str, contributor: &str, filename: &str, sha: &str) -> MediaItem {
    let (household, contributor, mid) = (
        HouseholdId::new(H),
        ContributorId::new(contributor),
        MediaId::new(id),
    );
    MediaItem {
        original_key: MediaItem::original_key(&household, &contributor, &mid, filename),
        id: mid,
        household,
        contributor,
        batch: BatchId::new("b"),
        kind: MediaKind::from_filename(filename),
        original_filename: filename.into(),
        bytes: 0,
        sha256: sha.into(),
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
        state: ProcessingState::Uploaded,
        uploaded_at: base(),
    }
}

fn jpeg(w: u32, h: u32, seed: u8) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, seed])
    });
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90)
        .encode_image(&img)
        .unwrap();
    out
}

/// A signalled photo with the fields organize reads, as if `signal` had run.
fn signalled(
    id: &str,
    contributor: &str,
    hours: f64,
    gps: Option<(f64, f64)>,
    phash: u64,
    sharp: f64,
) -> MediaItem {
    let mut m = media(id, contributor, &format!("{id}.jpg"), "");
    m.captured = Some(CaptureTime::new(
        base() + Duration::seconds((hours * 3600.0) as i64),
        None,
    ));
    m.gps = gps.map(|(a, b)| GeoPoint::new(a, b));
    m.signals = Signals {
        phash: Some(phash),
        sharpness: Some(sharp),
        exposure: Some(0.5),
        width: Some(10),
        height: Some(10),
    };
    m.state = ProcessingState::Signalled;
    m
}

#[tokio::test]
async fn signal_computes_signals_and_writes_renditions() {
    let h = harness(Enrichment::default());
    let bytes = jpeg(1200, 800, 1);
    let m = media("m1", "owner", "IMG_1.jpg", &sha256_hex(&bytes));
    h.blobs
        .put(&m.original_key, bytes.clone(), "image/jpeg")
        .await
        .unwrap();
    h.storage.put_media(&m).await.unwrap();

    let out = h
        .ingest
        .run(Task::Signal {
            key: m.original_key.clone(),
        })
        .await
        .unwrap();
    assert_eq!(out["state"], "signalled", "{out}");
    let stored = h
        .storage
        .get_media(&m.household, &m.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.state, ProcessingState::Signalled);
    assert_eq!(
        (stored.signals.width, stored.signals.height),
        (Some(1200), Some(800))
    );
    assert!(stored.signals.phash.is_some());
    assert_eq!(stored.bytes, bytes.len() as u64);
    let thumb = h.blobs.get("renditions/house/m1/thumb.jpg").await.unwrap();
    let decoded = image::load_from_memory(&thumb).unwrap();
    assert_eq!(decoded.width(), 400);
    assert!(h.blobs.get("renditions/house/m1/tv.jpg").await.is_ok());
}

#[tokio::test]
async fn signal_rejects_checksum_mismatch_and_unknown_keys() {
    let h = harness(Enrichment::default());
    let bytes = jpeg(100, 100, 2);
    let m = media("m1", "owner", "a.jpg", &"0".repeat(64));
    h.blobs
        .put(&m.original_key, bytes, "image/jpeg")
        .await
        .unwrap();
    h.storage.put_media(&m).await.unwrap();
    let out = h
        .ingest
        .run(Task::Signal {
            key: m.original_key.clone(),
        })
        .await
        .unwrap();
    assert_eq!(out["state"], "failed");
    assert_eq!(out["reason"], "checksum mismatch");
    assert!(h
        .ingest
        .run(Task::Signal {
            key: "renditions/x/y/thumb.jpg".into()
        })
        .await
        .is_err());
    assert!(h
        .ingest
        .run(Task::Signal {
            key: "originals/house/owner/missing.jpg".into()
        })
        .await
        .is_err());
}

#[tokio::test]
async fn organize_groups_bursts_builds_trip_and_is_idempotent() {
    let h = harness(Enrichment::default());
    let kyoto = Some((35.0116, 135.7681));
    let items = vec![
        signalled("a", "owner", 0.0, kyoto, 0xFF00_FF00_FF00_FF00, 100.0),
        signalled("b", "owner", 0.01, kyoto, 0xFF00_FF00_FF00_FF01, 400.0), // burst with a, sharper
        signalled("c", "maya", 0.02, kyoto, 0xFF00_FF00_FF00_FF03, 50.0), // same moment, other phone
        signalled(
            "d",
            "owner",
            5.0,
            Some((35.0094, 135.6667)),
            0x0000_0000_0000_0000,
            200.0,
        ),
        signalled("e", "owner", 26.0, kyoto, 0x1234_5678_9ABC_DEF0, 300.0),
        signalled("f", "maya", 27.0, kyoto, 0x0F0F_0F0F_0F0F_0F0F, 150.0),
    ];
    for m in &items {
        h.storage.put_media(m).await.unwrap();
    }

    let out = h
        .ingest
        .run(Task::Organize {
            household: HouseholdId::new(H),
        })
        .await
        .unwrap();
    assert_eq!(out["media"], 6, "{out}");
    assert_eq!(out["groups"], 4);
    assert_eq!(out["new_trips"], 1);
    let keepers: Vec<&str> = out["keepers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap())
        .collect();
    assert!(keepers.contains(&"b") && !keepers.contains(&"a") && !keepers.contains(&"c"));

    let trips = h.storage.list_trips(&HouseholdId::new(H)).await.unwrap();
    assert_eq!(trips.len(), 1);
    let trip = &trips[0];
    assert_eq!(trip.days.len(), 2);
    assert_eq!(trip.days[0].media.len(), 4);
    assert_eq!(trip.days[0].title.as_deref(), Some("Day 1"));
    assert_eq!(trip.contributors.len(), 2);
    assert!(
        trip.name.starts_with("Trip, April 3 to 4, 2026"),
        "{}",
        trip.name
    );
    assert!(!trip.route.points.is_empty());
    assert_eq!(trip.hero, trip.highlights.first().cloned());
    assert!(
        !trip.highlights.contains(&MediaId::new("a")),
        "non-keeper of a burst is not a highlight"
    );

    let a = h
        .storage
        .get_media(&HouseholdId::new(H), &MediaId::new("a"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(a.trip, Some(trip.id.clone()));
    assert_eq!(a.burst_group.as_deref(), Some("b"));
    assert!(!a.is_keeper);
    assert!(a.quality_score.is_some());

    let published = h.bus.published.lock().unwrap().clone();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].detail_type(), "trip.detected");
    let feed = h
        .storage
        .events_since(&HouseholdId::new(H), 0, 10)
        .await
        .unwrap();
    assert_eq!(feed.len(), 1);

    // Second run: same trip, nothing new, no events.
    let again = h
        .ingest
        .run(Task::Organize {
            household: HouseholdId::new(H),
        })
        .await
        .unwrap();
    assert_eq!(again["new_trips"], 0);
    assert_eq!(
        h.storage.list_trips(&HouseholdId::new(H)).await.unwrap()[0].id,
        trip.id
    );
    assert_eq!(h.bus.published.lock().unwrap().len(), 1);

    // A new contributor's photos land in the same trip and announce themselves.
    let g = signalled("g", "ken", 2.0, kyoto, 0xAAAA_BBBB_CCCC_DDDD, 120.0);
    h.storage.put_media(&g).await.unwrap();
    h.storage
        .put_contributor(&memorylane_core::contributor::Contributor {
            id: ContributorId::new("ken"),
            household: HouseholdId::new(H),
            display_name: "Ken".into(),
            kind: memorylane_core::contributor::ContributorKind::Guest,
            joined_at: base(),
            removed_at: None,
        })
        .await
        .unwrap();
    let third = h
        .ingest
        .run(Task::Organize {
            household: HouseholdId::new(H),
        })
        .await
        .unwrap();
    assert_eq!(third["new_trips"], 0);
    let published = h.bus.published.lock().unwrap().clone();
    assert_eq!(published.len(), 3);
    assert_eq!(published[1].detail_type(), "trip.detected");
    let v = serde_json::to_value(&published[2]).unwrap();
    assert_eq!(v["type"], "contribution.received");
    assert_eq!(v["display_name"], "Ken");
    assert_eq!(v["count"], 1);
}

#[tokio::test]
async fn enrich_runs_on_keepers_and_organize_propagates_to_burst() {
    let tags = Enrichment {
        scene_tags: vec![Tag {
            name: "Temple".into(),
            confidence: 0.9,
        }],
        face_count: 2,
        smile_count: 2,
        ..Enrichment::default()
    };
    let h = harness(tags.clone());
    let kyoto = Some((35.0116, 135.7681));
    let mut items = vec![
        signalled("a", "owner", 0.0, kyoto, 0xFF00_FF00_FF00_FF00, 100.0),
        signalled("b", "owner", 0.01, kyoto, 0xFF00_FF00_FF00_FF01, 400.0),
        signalled("z", "owner", 30.0, kyoto, 0x0000_0000_0000_0000, 100.0),
    ];
    for m in &mut items {
        h.storage.put_media(m).await.unwrap();
        h.blobs
            .put(
                &format!("renditions/house/{}/tv.jpg", m.id),
                jpeg(64, 64, 3),
                "image/jpeg",
            )
            .await
            .unwrap();
    }
    h.ingest
        .run(Task::Organize {
            household: HouseholdId::new(H),
        })
        .await
        .unwrap();

    let skipped = h
        .ingest
        .run(Task::Enrich {
            household: HouseholdId::new(H),
            media_id: MediaId::new("a"),
        })
        .await
        .unwrap();
    assert_eq!(skipped["skipped"], "not a keeper");
    let done = h
        .ingest
        .run(Task::Enrich {
            household: HouseholdId::new(H),
            media_id: MediaId::new("b"),
        })
        .await
        .unwrap();
    assert_eq!(done["state"], "enriched");
    let again = h
        .ingest
        .run(Task::Enrich {
            household: HouseholdId::new(H),
            media_id: MediaId::new("b"),
        })
        .await
        .unwrap();
    assert_eq!(again["skipped"], "already enriched");

    h.ingest
        .run(Task::Organize {
            household: HouseholdId::new(H),
        })
        .await
        .unwrap();
    let a = h
        .storage
        .get_media(&HouseholdId::new(H), &MediaId::new("a"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(a.enrichment.scene_tags, tags.scene_tags);
    assert_eq!(a.enrichment.inherited_from, Some(MediaId::new("b")));
    let b = h
        .storage
        .get_media(&HouseholdId::new(H), &MediaId::new("b"))
        .await
        .unwrap()
        .unwrap();
    assert!(
        b.quality_score.unwrap() > a.quality_score.unwrap(),
        "keeper with faces outscores its burst sibling"
    );
    assert_eq!(b.enrichment.inherited_from, None);
}
