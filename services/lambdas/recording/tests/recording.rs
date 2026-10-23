//! Saved broadcasts against in-memory backends and a remux that concatenates segments.

use mantelpiece_core::household::{Household, LiveSave, RecordingClaim};
use mantelpiece_core::ids::{ContributorId, HouseholdId, MediaId};
use mantelpiece_core::media::{MediaKind, ProcessingState};
use mantelpiece_recording::{run, Input, MemoryOrganizer, MemoryRemux, Output, Recording, ENDED};
use mantelpiece_store::memory::{MemoryBlobs, MemoryStorage};
use mantelpiece_store::{Blobs, Storage};
use std::sync::Arc;
use time::macros::datetime;

const ARN: &str = "arn:aws:ivs:us-west-2:123456789012:channel/AsXego4U6tnj";
const PREFIX: &str = "ivs/v1/123456789012/AsXego4U6tnj/2026/9/23/18/5/j8Z9O91ndcVs";

struct Rig {
    rec: Recording,
    storage: Arc<MemoryStorage>,
    media: Arc<MemoryBlobs>,
    recordings: Arc<MemoryBlobs>,
    organizer: Arc<MemoryOrganizer>,
    _dir: TempDir,
}

/// A folder under the system temp dir, removed on drop.
struct TempDir(std::path::PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn rig(max_bytes: u64) -> Rig {
    let storage = Arc::new(MemoryStorage::new());
    let media = Arc::new(MemoryBlobs::default());
    let recordings = Arc::new(MemoryBlobs::default());
    let organizer = Arc::new(MemoryOrganizer::default());
    let dir = std::env::temp_dir().join(format!("rec-test-{}", uuid_like()));
    std::fs::create_dir_all(&dir).unwrap();
    storage
        .put_household(&Household::new(
            HouseholdId::new("house"),
            "Grandma's TV",
            datetime!(2026-09-01 00:00 UTC),
        ))
        .await
        .unwrap();
    let rec = Recording {
        storage: storage.clone(),
        media: media.clone(),
        recordings: recordings.clone(),
        remux: Arc::new(MemoryRemux),
        organizer: organizer.clone(),
        work_dir: dir.clone(),
        max_bytes,
        clock: Arc::new(|| datetime!(2026-09-23 19:00 UTC)),
    };
    Rig {
        rec,
        storage,
        media,
        recordings,
        organizer,
        _dir: TempDir(dir),
    }
}

fn uuid_like() -> String {
    format!("{}-{:?}", std::process::id(), std::thread::current().id())
        .replace(|c: char| !c.is_ascii_alphanumeric(), "")
}

async fn claim(r: &Rig) {
    let claim = RecordingClaim {
        household: HouseholdId::new("house"),
        broadcast: "b1".into(),
        contributor: ContributorId::new("maya"),
        display_name: "Maya".into(),
        started_at: datetime!(2026-09-23 18:05 UTC),
        save: LiveSave {
            utc_offset_seconds: Some(-7 * 3600),
        },
    };
    r.media
        .put(
            &RecordingClaim::key(ARN),
            serde_json::to_vec(&claim).unwrap(),
            "application/json",
        )
        .await
        .unwrap();
}

/// What IVS leaves: metadata naming two renditions, and the 720p one's playlist and
/// three segments of `size` bytes each.
async fn recorded(r: &Rig, status: &str, size: usize) {
    let ended = serde_json::json!({
        "version": "v1",
        "channel_arn": ARN,
        "recording_started_at": "2026-09-23T18:05:02Z",
        "recording_ended_at": "2026-09-23T18:35:02Z",
        "recording_status": status,
        "media": { "hls": {
            "duration_ms": 1_800_000,
            "path": "media/hls",
            "playlist": "master.m3u8",
            "byte_range_playlist": "byte-range-multivariant.m3u8",
            "renditions": [
                { "path": "360p30", "playlist": "playlist.m3u8", "byte_range_playlist": "b.m3u8", "resolution_height": 360, "resolution_width": 640 },
                { "path": "720p30", "playlist": "playlist.m3u8", "byte_range_playlist": "b.m3u8", "resolution_height": 720, "resolution_width": 1280 }
            ]
        } }
    });
    let put = |key: String, bytes: Vec<u8>| {
        let recordings = r.recordings.clone();
        async move { recordings.put(&key, bytes, "x").await.unwrap() }
    };
    put(
        format!("{PREFIX}/{ENDED}"),
        serde_json::to_vec(&ended).unwrap(),
    )
    .await;
    put(
        format!("{PREFIX}/media/hls/720p30/playlist.m3u8"),
        b"#EXTM3U\n#EXT-X-TARGETDURATION:10\n#EXTINF:10.0,\n0.ts\n#EXTINF:10.0,\n1.ts\n#EXTINF:10.0,\n2.ts\n#EXT-X-ENDLIST\n".to_vec(),
    )
    .await;
    for (i, fill) in b"abc".iter().copied().enumerate() {
        put(
            format!("{PREFIX}/media/hls/720p30/{i}.ts"),
            vec![fill; size],
        )
        .await;
    }
}

fn input() -> Input {
    Input {
        key: format!("{PREFIX}/{ENDED}"),
    }
}

#[tokio::test]
async fn a_saved_broadcast_becomes_a_video_in_the_household() {
    let r = rig(1 << 20).await;
    claim(&r).await;
    recorded(&r, "RECORDING_ENDED", 4).await;

    let out = run(&r.rec, input()).await.unwrap();
    let id = MediaId::new("live-j8Z9O91ndcVs");
    assert_eq!(
        out,
        Output {
            media_id: Some(id.clone()),
            skipped: None,
            truncated: false
        }
    );
    let m = r
        .storage
        .get_media(&HouseholdId::new("house"), &id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(m.kind, MediaKind::Video);
    assert_eq!(m.state, ProcessingState::Uploaded);
    assert_eq!(m.contributor, ContributorId::new("maya"));
    assert_eq!(m.original_key, "originals/house/maya/live-j8Z9O91ndcVs.mp4");
    // Highest rendition, all segments, in order.
    let mp4 = r.media.get(&m.original_key).await.unwrap();
    assert_eq!(mp4, b"mp4:aaaabbbbcccc");
    assert_eq!(m.sha256, mantelpiece_ingest::checksum::sha256_hex(&mp4));
    // Captured when IVS started recording, in the phone's zone.
    let captured = m.captured.unwrap();
    assert_eq!(captured.utc, datetime!(2026-09-23 18:05:02 UTC));
    assert_eq!(captured.offset_seconds, Some(-7 * 3600));
    assert!(!captured.zone_assumed);
    assert_eq!(
        *r.organizer.started.lock().unwrap(),
        vec![HouseholdId::new("house")]
    );
    // The claim is used up, so a repeated event does nothing.
    assert!(r.media.get(&RecordingClaim::key(ARN)).await.is_err());
    let again = run(&r.rec, input()).await.unwrap();
    assert_eq!(again.skipped.as_deref(), Some("not a saved broadcast"));
}

#[tokio::test]
async fn a_long_broadcast_keeps_its_start() {
    let r = rig(9).await;
    claim(&r).await;
    recorded(&r, "RECORDING_ENDED", 4).await;
    let out = run(&r.rec, input()).await.unwrap();
    assert!(out.truncated);
    let m = r
        .storage
        .get_media(&HouseholdId::new("house"), &out.media_id.unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r.media.get(&m.original_key).await.unwrap(), b"mp4:aaaabbbb");
}

#[tokio::test]
async fn recordings_nobody_asked_to_keep_are_left_alone() {
    let r = rig(1 << 20).await;
    recorded(&r, "RECORDING_ENDED", 4).await;
    let out = run(&r.rec, input()).await.unwrap();
    assert_eq!(out.skipped.as_deref(), Some("not a saved broadcast"));
    assert_eq!(
        r.storage
            .list_media(&HouseholdId::new("house"))
            .await
            .unwrap(),
        Vec::<mantelpiece_core::media::MediaItem>::new()
    );
    assert!(r.organizer.started.lock().unwrap().is_empty());

    // Failed recordings and keys that are not a recording's end are not videos either.
    claim(&r).await;
    recorded(&r, "RECORDING_ENDED_WITH_FAILURE", 4).await;
    assert_eq!(
        run(&r.rec, input()).await.unwrap().skipped.as_deref(),
        Some("nothing recorded")
    );
    assert!(run(
        &r.rec,
        Input {
            key: format!("{PREFIX}/events/recording-started.json")
        }
    )
    .await
    .is_err());
}
