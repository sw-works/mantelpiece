//! Saved broadcasts: live from the event, kept in the household's memories.
//!
//! A phone that asked to keep its broadcast streams to a channel with the stack's IVS
//! recording configuration, and the API leaves a [`RecordingClaim`] beside the channel.
//! When IVS writes `events/recording-ended.json`, this Lambda reads the claim, copies the
//! highest rendition's HLS segments to local disk, remuxes them to one MP4 with ffmpeg
//! (stream copy, no re-encode) and writes it to `originals/` with a media record, exactly
//! as an upload would. The signal and poster steps then treat it like any phone video, and
//! the organize run started here places it in a trip. The capture time is the broadcast
//! start in the phone's zone, so it lands on the day it happened.
//!
//! Remuxing sits behind [`Remux`] so the pipeline test runs without ffmpeg.

use async_trait::async_trait;
use mantelpiece_core::household::RecordingClaim;
use mantelpiece_core::ids::{BatchId, HouseholdId, MediaId};
use mantelpiece_core::media::{
    CaptureTime, Enrichment, MediaItem, MediaKind, ProcessingState, Signals,
};
use mantelpiece_ingest::checksum;
use mantelpiece_store::{Blobs, Storage, StorageError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use time::format_description::well_known::Rfc3339;
use time::{OffsetDateTime, UtcOffset};

#[derive(Debug, Error)]
pub enum RecordingError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("bad input: {0}")]
    Input(String),
    #[error("ffmpeg: {0}")]
    Ffmpeg(String),
    #[error("disk: {0}")]
    Disk(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, RecordingError>;

/// The object that starts this Lambda, relative to the recording's S3 prefix.
pub const ENDED: &str = "events/recording-ended.json";

/// Longest recording kept, in bytes of HLS segments. Signal and poster read a video into
/// memory whole, so a saved broadcast stays well inside their Lambdas' memory; a longer
/// one keeps its first part (about 40 minutes at the web broadcast's 720p) and says so.
pub const MAX_BYTES: u64 = 768 * 1024 * 1024;

/// Copy an HLS rendition into an MP4.
pub trait Remux: Send + Sync {
    /// `playlist` is a local media playlist whose segments sit beside it.
    fn remux(&self, playlist: &Path, out: &Path, created: OffsetDateTime) -> Result<()>;
}

/// Concatenates the segment files the playlist names. For tests.
#[derive(Debug, Clone, Default)]
pub struct MemoryRemux;

impl Remux for MemoryRemux {
    fn remux(&self, playlist: &Path, out: &Path, _created: OffsetDateTime) -> Result<()> {
        let dir = playlist.parent().unwrap_or(Path::new("."));
        let mut mp4 = b"mp4:".to_vec();
        for name in segments(&std::fs::read_to_string(playlist)?) {
            mp4.extend(std::fs::read(dir.join(name))?);
        }
        std::fs::write(out, mp4)?;
        Ok(())
    }
}

/// Shells out to the static ffmpeg in the poster's layer (`/opt/bin/ffmpeg`). Stream copy
/// into MP4 with the index up front so the TV can start playing before it has the end.
/// Not exercised by tests.
#[derive(Debug, Clone)]
pub struct FfmpegRemux {
    pub binary: PathBuf,
}

impl FfmpegRemux {
    pub fn from_env() -> Self {
        Self {
            binary: std::env::var("FFMPEG")
                .map_or_else(|_| "/opt/bin/ffmpeg".into(), PathBuf::from),
        }
    }
}

impl Remux for FfmpegRemux {
    fn remux(&self, playlist: &Path, out: &Path, created: OffsetDateTime) -> Result<()> {
        let status = std::process::Command::new(&self.binary)
            .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
            .arg(playlist)
            .args([
                "-c",
                "copy",
                "-bsf:a",
                "aac_adtstoasc",
                "-movflags",
                "+faststart",
            ])
            .arg("-metadata")
            .arg(format!(
                "creation_time={}",
                created.format(&Rfc3339).unwrap_or_default()
            ))
            .arg(out)
            .status()
            .map_err(|e| RecordingError::Ffmpeg(format!("spawn {}: {e}", self.binary.display())))?;
        if status.success() {
            Ok(())
        } else {
            Err(RecordingError::Ffmpeg(format!("exit {status}")))
        }
    }
}

/// Starts the household's organize run, which places the new video in a trip.
#[async_trait]
pub trait Organizer: Send + Sync {
    async fn start(&self, household: &HouseholdId) -> Result<()>;
}

#[derive(Debug, Default)]
pub struct MemoryOrganizer {
    pub started: std::sync::Mutex<Vec<HouseholdId>>,
}

#[async_trait]
impl Organizer for MemoryOrganizer {
    async fn start(&self, household: &HouseholdId) -> Result<()> {
        self.started.lock().unwrap().push(household.clone());
        Ok(())
    }
}

/// The organize state machine, the same input the API sends. Compile-checked only.
#[derive(Debug, Clone)]
pub struct SfnOrganizer {
    pub client: aws_sdk_sfn::Client,
    pub state_machine_arn: String,
}

#[async_trait]
impl Organizer for SfnOrganizer {
    async fn start(&self, household: &HouseholdId) -> Result<()> {
        let input = serde_json::json!({ "task": "organize", "household": household });
        self.client
            .start_execution()
            .state_machine_arn(&self.state_machine_arn)
            .input(input.to_string())
            .send()
            .await
            .map_err(|e| StorageError::Backend(format!("{e:?}")))?;
        Ok(())
    }
}

pub type Clock = Arc<dyn Fn() -> OffsetDateTime + Send + Sync>;

pub struct Recording {
    pub storage: Arc<dyn Storage>,
    /// The media bucket: claims, and where the MP4 goes.
    pub media: Arc<dyn Blobs>,
    /// The bucket IVS records into.
    pub recordings: Arc<dyn Blobs>,
    pub remux: Arc<dyn Remux>,
    pub organizer: Arc<dyn Organizer>,
    pub work_dir: PathBuf,
    pub max_bytes: u64,
    pub clock: Clock,
}

impl std::fmt::Debug for Recording {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recording").finish_non_exhaustive()
    }
}

/// S3 key of a recording's `recording-ended.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Input {
    pub key: String,
}

#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub struct Output {
    pub media_id: Option<MediaId>,
    pub skipped: Option<String>,
    /// The recording ran past [`MAX_BYTES`]; only its start was kept.
    pub truncated: bool,
}

fn skipped(why: &str) -> Output {
    Output {
        skipped: Some(why.to_owned()),
        ..Output::default()
    }
}

/// The parts of IVS's recording metadata this needs.
#[derive(Debug, Deserialize)]
struct Ended {
    channel_arn: String,
    recording_status: String,
    #[serde(default, with = "time::serde::rfc3339::option")]
    recording_started_at: Option<OffsetDateTime>,
    media: EndedMedia,
}

#[derive(Debug, Deserialize)]
struct EndedMedia {
    hls: Hls,
}

#[derive(Debug, Deserialize)]
struct Hls {
    #[serde(default)]
    duration_ms: u64,
    path: String,
    renditions: Vec<Rendition>,
}

#[derive(Debug, Deserialize)]
struct Rendition {
    path: String,
    playlist: String,
    #[serde(default)]
    resolution_height: u32,
}

/// Segment names a media playlist lists, in order.
fn segments(playlist: &str) -> impl Iterator<Item = &str> {
    playlist
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
}

/// A segment name that stays inside the recording's folder.
fn plain(name: &str) -> bool {
    !name.contains('/') && !name.contains('\\') && !name.starts_with('.')
}

/// Deletes the working folder however `run` leaves.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Copy the rendition's segments in order into `dir` until the cap, and write a local
/// playlist that ends where the copy stopped, so a long broadcast keeps its start.
/// Returns the playlist, or `None` when nothing was copied, and whether it was cut short.
async fn copy_rendition(
    rec: &Recording,
    folder: &str,
    listed: &str,
    dir: &Path,
) -> Result<(Option<PathBuf>, bool)> {
    let mut kept = String::new();
    let mut pending = String::new();
    let mut bytes = 0u64;
    let mut truncated = false;
    for line in listed.lines() {
        let name = line.trim();
        if name.is_empty() || name.starts_with('#') {
            if name != "#EXT-X-ENDLIST" {
                pending.push_str(line);
                pending.push('\n');
            }
            continue;
        }
        if !plain(name) {
            return Err(RecordingError::Input(format!(
                "segment outside the folder: {name}"
            )));
        }
        let segment = rec.recordings.get(&format!("{folder}/{name}")).await?;
        bytes += segment.len() as u64;
        if bytes > rec.max_bytes {
            truncated = true;
            break;
        }
        std::fs::write(dir.join(name), segment)?;
        kept.push_str(&std::mem::take(&mut pending));
        kept.push_str(name);
        kept.push('\n');
    }
    if segments(&kept).next().is_none() {
        return Ok((None, truncated));
    }
    if !truncated {
        kept.push_str(&pending);
    }
    kept.push_str("#EXT-X-ENDLIST\n");
    let playlist = dir.join("playlist.m3u8");
    std::fs::write(&playlist, kept)?;
    Ok((Some(playlist), truncated))
}

/// The media record an upload of this MP4 would have made, captured at `started`.
fn media_item(
    claim: &RecordingClaim,
    id: &MediaId,
    mp4: &[u8],
    started: OffsetDateTime,
    now: OffsetDateTime,
) -> MediaItem {
    let filename = format!("Live from {}.mp4", claim.display_name);
    let offset = claim
        .save
        .utc_offset_seconds
        .and_then(|s| UtcOffset::from_whole_seconds(s).ok());
    MediaItem {
        id: id.clone(),
        household: claim.household.clone(),
        contributor: claim.contributor.clone(),
        batch: BatchId::new(format!("live-{}", claim.broadcast)),
        kind: MediaKind::Video,
        original_key: MediaItem::original_key(&claim.household, &claim.contributor, id, &filename),
        original_filename: filename,
        bytes: mp4.len() as u64,
        sha256: checksum::sha256_hex(mp4),
        captured: Some(CaptureTime::new(started, offset)),
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
        uploaded_at: now,
    }
}

pub async fn run(rec: &Recording, input: Input) -> Result<Output> {
    let prefix = input
        .key
        .strip_suffix(ENDED)
        .and_then(|p| p.strip_suffix('/'))
        .ok_or_else(|| RecordingError::Input(format!("not a recording end: {}", input.key)))?;
    let recording_id = prefix.rsplit('/').next().unwrap_or(prefix);
    let ended: Ended = serde_json::from_slice(&rec.recordings.get(&input.key).await?)
        .map_err(|e| RecordingError::Input(format!("recording-ended.json: {e}")))?;
    if ended.recording_status != "RECORDING_ENDED" || ended.media.hls.duration_ms == 0 {
        return Ok(skipped("nothing recorded"));
    }

    let claim_key = RecordingClaim::key(&ended.channel_arn);
    let claim: RecordingClaim = match rec.media.get(&claim_key).await {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| RecordingError::Input(format!("claim: {e}")))?,
        // Recorded, but nobody asked to keep it here (or it was saved already).
        Err(StorageError::NotFound(_)) => return Ok(skipped("not a saved broadcast")),
        Err(e) => return Err(e.into()),
    };
    let household = claim.household.clone();
    if rec
        .storage
        .get_household(&household)
        .await?
        .is_none_or(|h| h.is_deleting())
    {
        rec.media.delete(&[claim_key]).await?;
        return Ok(skipped("household gone"));
    }
    // One media id per recording, so a repeated event rewrites rather than duplicates.
    let id = MediaId::new(format!("live-{recording_id}"));
    if rec
        .storage
        .get_media(&household, &id)
        .await?
        .is_some_and(|m| m.state != ProcessingState::Uploaded)
    {
        rec.media.delete(&[claim_key]).await?;
        return Ok(skipped("already saved"));
    }

    let rendition = ended
        .media
        .hls
        .renditions
        .iter()
        .max_by_key(|r| r.resolution_height)
        .ok_or_else(|| RecordingError::Input("no renditions".into()))?;
    let folder = format!("{prefix}/{}/{}", ended.media.hls.path, rendition.path);
    let listed = String::from_utf8_lossy(
        &rec.recordings
            .get(&format!("{folder}/{}", rendition.playlist))
            .await?,
    )
    .into_owned();

    let scratch = Scratch(rec.work_dir.join(format!("recording-{recording_id}")));
    std::fs::create_dir_all(&scratch.0)?;
    let (playlist, truncated) = copy_rendition(rec, &folder, &listed, &scratch.0).await?;
    let Some(playlist) = playlist else {
        rec.media.delete(&[claim_key]).await?;
        return Ok(skipped("no segments"));
    };
    let started = ended.recording_started_at.unwrap_or(claim.started_at);
    let out = scratch.0.join("out.mp4");
    rec.remux.remux(&playlist, &out, started)?;
    let mp4 = std::fs::read(&out)?;
    drop(scratch);

    let media = media_item(&claim, &id, &mp4, started, (rec.clock)());
    // The record first: the object's arrival starts the signal step, which reads it.
    rec.storage.put_media(&media).await?;
    rec.media.put(&media.original_key, mp4, "video/mp4").await?;
    rec.organizer.start(&household).await?;
    rec.media.delete(&[claim_key]).await?;
    tracing::info!(media = %id, household = %household, truncated, "broadcast saved");
    Ok(Output {
        media_id: Some(id),
        skipped: None,
        truncated,
    })
}
