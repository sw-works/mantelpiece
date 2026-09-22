//! Signal, organize and enrich against in-memory backends.

#![allow(clippy::cast_possible_truncation, clippy::too_many_lines)]

use memorylane_core::geo::GeoPoint;
use memorylane_core::ids::{BatchId, ContributorId, HouseholdId, MediaId};
use memorylane_core::media::{
    CaptureTime, Enrichment, MediaItem, MediaKind, ProcessingState, Signals, Tag,
};
use memorylane_ingest::checksum::sha256_hex;
use memorylane_ingest_lambda::{Ingest, Task};
use memorylane_store::bus::MemoryBus;
use memorylane_store::encyclopedia::MemoryEncyclopedia;
use memorylane_store::faces::{FaceMatch, MemoryFaces};
use memorylane_store::maps::MemoryMaps;
use memorylane_store::memory::{MemoryBlobs, MemoryStorage};
use memorylane_store::places::MemoryPlaces;
use memorylane_store::vectors::MemoryVectors;
use memorylane_store::vision::MemoryVision;
use memorylane_store::{Blobs, Faces, Storage, Vectors};
use std::collections::BTreeSet;
use std::sync::Arc;
use time::macros::datetime;
use time::{Duration, OffsetDateTime};

struct Harness {
    ingest: Ingest,
    storage: Arc<MemoryStorage>,
    blobs: Arc<MemoryBlobs>,
    bus: Arc<MemoryBus>,
    vectors: Arc<MemoryVectors>,
    maps: Arc<MemoryMaps>,
    faces: Arc<MemoryFaces>,
    encyclopedia: Arc<MemoryEncyclopedia>,
}

fn harness(enrichment: Enrichment) -> Harness {
    harness_with(MemoryVision::with_enrichment(enrichment))
}

fn harness_with(vision: MemoryVision) -> Harness {
    let storage = Arc::new(MemoryStorage::new());
    let blobs = Arc::new(MemoryBlobs::default());
    let bus = Arc::new(MemoryBus::default());
    let vectors = Arc::new(MemoryVectors::default());
    let maps = Arc::new(MemoryMaps::default());
    let faces = Arc::new(MemoryFaces::with_faces_per_image(1));
    let encyclopedia = Arc::new(MemoryEncyclopedia::with(vec![
        (
            "Kinkaku-ji",
            "Kinkaku-ji is a Zen Buddhist temple in Kyoto, Japan. Its top two floors are covered in gold leaf.",
            Some((35.0394, 135.7292)),
        ),
        (
            "Tōdai-ji",
            "Tōdai-ji is a Buddhist temple complex in Nara, Japan.",
            Some((34.6890, 135.8398)),
        ),
    ]));
    let ingest = Ingest {
        storage: storage.clone(),
        blobs: blobs.clone(),
        vision: Arc::new(vision),
        vectors: vectors.clone(),
        faces: faces.clone(),
        maps: maps.clone(),
        bus: bus.clone(),
        places: Arc::new(MemoryPlaces::with(vec![
            ("Kinkaku-ji", 35.0394, 135.7292),
            ("Fushimi Inari-taisha", 34.9671, 135.7727),
            ("Kyoto Station", 34.9858, 135.7588),
        ])),
        encyclopedia: encyclopedia.clone(),
        cluster: memorylane_trips::cluster::Params::default(),
        clock: Arc::new(|| datetime!(2026-09-08 10:00:00 UTC)),
    };
    Harness {
        ingest,
        storage,
        blobs,
        bus,
        vectors,
        maps,
        faces,
        encyclopedia,
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
        pinned_by: BTreeSet::default(),
        hidden_by: BTreeSet::default(),
        text: None,
        description: None,
        cleanup_reason: None,
        people: vec![],
        removed_at: None,
        restore_until: None,
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
async fn organize_names_places_along_the_route_and_positions_camera_shots() {
    let h = harness(Enrichment::default());
    let kinkakuji = Some((35.0394, 135.7292));
    let inari = Some((34.9671, 135.7727));
    let items = vec![
        signalled("p1", "owner", 0.0, kinkakuji, 0x1, 100.0),
        signalled("cam1", "owner", 0.5, None, 0x2, 300.0), // camera, no GPS: rides along
        signalled("p2", "owner", 1.0, inari, 0x3, 200.0),
        signalled("cam2", "owner", 9.0, None, 0x4, 150.0), // too far in time: unplaced
        signalled("p3", "owner", 25.0, inari, 0x5, 250.0),
        signalled("p4", "owner", 26.0, kinkakuji, 0x6, 220.0),
        signalled("p5", "owner", 27.0, kinkakuji, 0x7, 210.0),
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
    assert_eq!(out["new_trips"], 1, "{out}");
    assert_eq!(out["estimated_positions"], 1);
    let trip = &h.storage.list_trips(&HouseholdId::new(H)).await.unwrap()[0];
    let day1 = &trip.days[0];
    let names: Vec<&str> = day1.landmarks.iter().map(|l| l.name.as_str()).collect();
    // Kyoto Station comes from the region lookup but no photo was within its reach.
    assert_eq!(
        names,
        vec!["Kinkaku-ji", "Fushimi Inari-taisha", "Kyoto Station"],
        "{names:?}"
    );
    assert!(day1.landmarks[2].media.is_empty());
    assert_eq!(day1.landmarks[0].media, vec![MediaId::new("p1")]);
    assert_eq!(day1.title.as_deref(), Some("Day 1: Kinkaku-ji"));
    assert!(trip.name.starts_with("Kinkaku-ji,"), "{}", trip.name);
    let cam = day1
        .route
        .points
        .iter()
        .find(|p| p.media.as_str() == "cam1")
        .expect("camera shot placed on the route");
    assert!(cam.estimated);
    assert!((cam.at.lat - 35.0033).abs() < 0.001, "{}", cam.at.lat);
    assert!(!day1.route.points.iter().any(|p| p.media.as_str() == "cam2"));
    let all: Vec<String> = trip.landmarks().into_iter().map(|l| l.name).collect();
    assert_eq!(
        all,
        vec!["Kinkaku-ji", "Fushimi Inari-taisha", "Kyoto Station"]
    );
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
    // The Kyoto spot sits within the region lookup's reach of Kinkaku-ji in the fake.
    assert_eq!(trip.days[0].title.as_deref(), Some("Day 1: Kinkaku-ji"));
    assert_eq!(trip.contributors.len(), 2);
    assert!(
        trip.name.starts_with("Kinkaku-ji, April 2026"),
        "{}",
        trip.name
    );
    assert!(!trip.route.points.is_empty());
    // F-207: a route map for the trip and each day with GPS, drawn once.
    assert_eq!(
        trip.route.map_key.as_deref(),
        Some(format!("maps/{H}/{}.png", trip.id).as_str())
    );
    assert!(h
        .blobs
        .get(trip.route.map_key.as_ref().unwrap())
        .await
        .is_ok());
    assert!(trip
        .days
        .iter()
        .all(|d| d.route.points.is_empty() || d.route.map_key.is_some()));
    let drawn = h.maps.log.lock().unwrap().len();
    assert!(drawn >= 1);
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
    assert_eq!(
        h.maps.log.lock().unwrap().len(),
        drawn,
        "no new maps on a no-op re-run"
    );
    assert_eq!(h.bus.published.lock().unwrap().len(), 1);
    // F-106: a re-run over a completed batch writes no new records and no new events.
    assert_eq!(
        h.storage
            .list_trips(&HouseholdId::new(H))
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        h.storage
            .list_media(&HouseholdId::new(H))
            .await
            .unwrap()
            .len(),
        6
    );
    assert_eq!(
        h.storage
            .events_since(&HouseholdId::new(H), 0, 10)
            .await
            .unwrap()
            .len(),
        1
    );
    let manifest = again["keepers_manifest"].as_str().unwrap();
    assert!(
        manifest.starts_with("manifests/house/keepers-"),
        "{manifest}"
    );
    let listed: Vec<serde_json::Value> =
        serde_json::from_slice(&h.blobs.get(manifest).await.unwrap()).unwrap();
    assert_eq!(listed.len(), out["groups"].as_u64().unwrap() as usize);
    assert_eq!(listed[0]["household"], H);

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
            grace_until: None,
            role: memorylane_core::contributor::Role::Contributor,
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
    let h = harness_with(MemoryVision {
        enrichment: tags.clone(),
        text: vec!["Ichiran".into(), "ラーメン".into()],
        description: "Landmark: Todai-ji\nThe Great Buddha hall on a bright afternoon.".into(),
        prompts: std::sync::Mutex::new(vec![]),
    });
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
    assert_eq!(done["described"], true);
    assert!(
        done["cleanup_reason"].is_null(),
        "a sharp captured photo with two lines of text is kept"
    );
    let b = h
        .storage
        .get_media(&HouseholdId::new(H), &MediaId::new("b"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(b.text.as_deref(), Some("Ichiran ラーメン"));
    let description = b.description.as_ref().expect("description stored");
    assert_eq!(description.landmark.as_deref(), Some("Todai-ji"));
    assert_eq!(
        b.enrichment.landmark.as_ref().map(|t| t.name.as_str()),
        Some("Todai-ji")
    );
    let prompt = h.ingest.vision.describe(b"", "").await.unwrap();
    assert!(prompt.starts_with("Landmark:"));
    let prompts = h
        .storage
        .get_media(&HouseholdId::new(H), &MediaId::new("b"))
        .await
        .unwrap();
    assert!(prompts.is_some());
    // The keeper is searchable by its labels and text (F-303).
    let hits = h
        .vectors
        .query_household(
            &HouseholdId::new(H),
            &memorylane_ingest::embed::embed_query("temple ichiran"),
            5,
        )
        .await
        .unwrap();
    assert_eq!(hits.first().map(|x| x.media.as_str()), Some("b"));
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

#[tokio::test]
async fn people_are_grouped_only_when_enabled_and_faces_are_matched() {
    let h = harness(Enrichment {
        face_count: 1,
        ..Enrichment::default()
    });
    let house = HouseholdId::new(H);
    let mut items = vec![
        signalled("p1", "owner", 0.0, None, 0x1111_0000_0000_0000, 100.0),
        signalled("p2", "owner", 48.0, None, 0x2222_0000_0000_0000, 100.0),
    ];
    for m in &mut items {
        m.is_keeper = true;
        h.storage.put_media(m).await.unwrap();
        h.blobs
            .put(
                &format!("renditions/house/{}/tv.jpg", m.id),
                jpeg(64, 64, 5),
                "image/jpeg",
            )
            .await
            .unwrap();
    }
    // Off by default: nothing indexed.
    h.ingest
        .run(Task::Enrich {
            household: house.clone(),
            media_id: MediaId::new("p1"),
        })
        .await
        .unwrap();
    assert!(h.storage.list_people(&house).await.unwrap().is_empty());
    assert!(h.faces.log.lock().unwrap().is_empty());

    // On: the first face opens a person, a match on the second photo joins them.
    let mut household =
        memorylane_core::household::Household::new(house.clone(), "Living room", base());
    household.people_enabled = true;
    h.storage.put_household(&household).await.unwrap();
    h.faces.create_collection(&house).await.unwrap();
    let mut p1 = h
        .storage
        .get_media(&house, &MediaId::new("p1"))
        .await
        .unwrap()
        .unwrap();
    p1.state = ProcessingState::Signalled;
    h.storage.put_media(&p1).await.unwrap();
    h.ingest
        .run(Task::Enrich {
            household: house.clone(),
            media_id: MediaId::new("p1"),
        })
        .await
        .unwrap();
    let people = h.storage.list_people(&house).await.unwrap();
    assert_eq!(people.len(), 1);
    assert_eq!(people[0].face_ids, vec!["face-1"]);
    h.faces.matches.lock().unwrap().insert(
        "face-2".into(),
        vec![FaceMatch {
            face_id: "face-1".into(),
            similarity: 0.97,
        }],
    );
    h.ingest
        .run(Task::Enrich {
            household: house.clone(),
            media_id: MediaId::new("p2"),
        })
        .await
        .unwrap();
    let people = h.storage.list_people(&house).await.unwrap();
    assert_eq!(people.len(), 1, "the matched face joined the same person");
    assert_eq!(people[0].media_count, 2);
    assert_eq!(people[0].face_ids, vec!["face-1", "face-2"]);
    let p2 = h
        .storage
        .get_media(&house, &MediaId::new("p2"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(p2.people, vec![people[0].id.clone()]);
}

#[tokio::test]
async fn every_face_in_a_group_photo_finds_its_own_person() {
    let h = harness(Enrichment {
        face_count: 2,
        ..Enrichment::default()
    });
    let house = HouseholdId::new(H);
    let mut household =
        memorylane_core::household::Household::new(house.clone(), "Living room", base());
    household.people_enabled = true;
    h.storage.put_household(&household).await.unwrap();
    h.faces.create_collection(&house).await.unwrap();
    for (id, hash) in [
        ("solo-a", 0x1111_0000_0000_0000),
        ("solo-b", 0x2222_0000_0000_0000),
        ("both", 0x3333_0000_0000_0000),
    ] {
        let mut m = signalled(id, "owner", 0.0, None, hash, 100.0);
        m.is_keeper = true;
        h.storage.put_media(&m).await.unwrap();
        h.blobs
            .put(
                &format!("renditions/house/{id}/tv.jpg"),
                jpeg(64, 64, 5),
                "image/jpeg",
            )
            .await
            .unwrap();
    }
    let enrich = |id: &'static str| {
        h.ingest.run(Task::Enrich {
            household: house.clone(),
            media_id: MediaId::new(id),
        })
    };
    // Two portraits open two people: face-1 and face-2.
    *h.faces.faces_per_image.lock().unwrap() = 1;
    enrich("solo-a").await.unwrap();
    enrich("solo-b").await.unwrap();
    // The group photo indexes face-3 and face-4; the smaller face is the first person.
    *h.faces.faces_per_image.lock().unwrap() = 2;
    {
        let mut matches = h.faces.matches.lock().unwrap();
        let hit = |face: &str| {
            vec![FaceMatch {
                face_id: face.into(),
                similarity: 0.95,
            }]
        };
        matches.insert("face-3".into(), hit("face-2"));
        matches.insert("face-4".into(), hit("face-1"));
    }
    enrich("both").await.unwrap();

    let people = h.storage.list_people(&house).await.unwrap();
    assert_eq!(people.len(), 2, "nobody new: both faces were known");
    let of = |face: &str| people.iter().find(|p| p.face_ids[0] == face).unwrap();
    assert_eq!(of("face-1").face_ids, vec!["face-1", "face-4"]);
    assert_eq!(of("face-2").face_ids, vec!["face-2", "face-3"]);
    let both = h
        .storage
        .get_media(&house, &MediaId::new("both"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(both.people.len(), 2);
}

#[tokio::test]
async fn a_landmark_is_picked_from_the_sights_nearby_and_gets_its_facts() {
    let h = harness_with(MemoryVision {
        enrichment: Enrichment {
            scene_tags: vec![Tag {
                name: "Temple".into(),
                confidence: 0.9,
            }],
            ..Enrichment::default()
        },
        text: vec![],
        description: "Landmark: kinkaku-ji\nA golden pavilion over a still pond.".into(),
        prompts: std::sync::Mutex::new(vec![]),
    });
    let house = HouseholdId::new(H);
    let mut m = signalled(
        "gold",
        "owner",
        0.0,
        Some((35.0396, 135.7290)),
        0x1111_0000_0000_0000,
        100.0,
    );
    m.is_keeper = true;
    h.storage.put_media(&m).await.unwrap();
    h.blobs
        .put(
            "renditions/house/gold/tv.jpg",
            jpeg(64, 64, 5),
            "image/jpeg",
        )
        .await
        .unwrap();
    h.ingest
        .run(Task::Enrich {
            household: house.clone(),
            media_id: MediaId::new("gold"),
        })
        .await
        .unwrap();

    let gold = h
        .storage
        .get_media(&house, &MediaId::new("gold"))
        .await
        .unwrap()
        .unwrap();
    let tag = gold.enrichment.landmark.expect("landmark kept");
    assert_eq!(tag.name, "Kinkaku-ji", "the sight's own spelling");
    assert!((tag.confidence - 0.85).abs() < f32::EPSILON);
    let facts = h
        .storage
        .get_landmark_facts("kinkaku-ji")
        .await
        .unwrap()
        .expect("facts stored once");
    assert!(facts.summary.contains("gold leaf"));
    assert_eq!(h.encyclopedia.calls.lock().unwrap().len(), 1);
}
