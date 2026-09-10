//! Poster frames for videos (F-105, research R5).
//!
//! The signal task leaves videos as `Signalled` with no renditions; this Lambda, run next
//! by the signal state machine, extracts one frame with ffmpeg from a layer and writes the
//! same thumb and TV renditions a photo gets, plus `poster.jpg`, so every screen can treat
//! a video like a still. Frame extraction sits behind [`Frames`] so the pipeline test runs
//! without ffmpeg.

use memorylane_core::ids::{HouseholdId, MediaId};
use memorylane_core::media::{MediaKind, ProcessingState};
use memorylane_ingest::renditions::{self, Rendition};
use memorylane_ingest::{decode, quality};
use memorylane_store::{Blobs, Storage, StorageError};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use time::OffsetDateTime;

#[derive(Debug, Error)]
pub enum PosterError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("bad input: {0}")]
    Input(String),
    #[error("ffmpeg: {0}")]
    Ffmpeg(String),
}

pub type Result<T> = std::result::Result<T, PosterError>;

/// One JPEG frame from a video.
pub trait Frames: Send + Sync {
    fn extract(&self, video: &[u8]) -> Result<Vec<u8>>;
}

/// Always returns the configured JPEG. For tests and for running without the layer.
#[derive(Debug, Clone)]
pub struct MemoryFrames {
    pub jpeg: Vec<u8>,
}

impl Frames for MemoryFrames {
    fn extract(&self, _video: &[u8]) -> Result<Vec<u8>> {
        Ok(self.jpeg.clone())
    }
}

/// Shells out to a static ffmpeg (Lambda layer at `/opt/bin/ffmpeg`, built by
/// `scripts/ffmpeg-layer.sh`). Seeks one second in so black lead-in frames are skipped.
/// Not exercised by tests.
#[derive(Debug, Clone)]
pub struct FfmpegFrames {
    pub binary: PathBuf,
    pub seek_seconds: f32,
    pub work_dir: PathBuf,
}

impl FfmpegFrames {
    pub fn from_env() -> Self {
        Self {
            binary: std::env::var("FFMPEG")
                .map_or_else(|_| "/opt/bin/ffmpeg".into(), PathBuf::from),
            seek_seconds: 1.0,
            work_dir: std::env::temp_dir(),
        }
    }
}

impl Frames for FfmpegFrames {
    fn extract(&self, video: &[u8]) -> Result<Vec<u8>> {
        let stamp = OffsetDateTime::now_utc().unix_timestamp_nanos();
        let input = self.work_dir.join(format!("poster-{stamp}.in"));
        let output = self.work_dir.join(format!("poster-{stamp}.jpg"));
        std::fs::write(&input, video).map_err(|e| PosterError::Ffmpeg(e.to_string()))?;
        let status = std::process::Command::new(&self.binary)
            .args(["-y", "-hide_banner", "-loglevel", "error", "-ss"])
            .arg(format!("{}", self.seek_seconds))
            .arg("-i")
            .arg(&input)
            .args(["-frames:v", "1", "-q:v", "3"])
            .arg(&output)
            .status()
            .map_err(|e| PosterError::Ffmpeg(format!("spawn {}: {e}", self.binary.display())))?;
        let frame = if status.success() {
            std::fs::read(&output).map_err(|e| PosterError::Ffmpeg(e.to_string()))
        } else {
            Err(PosterError::Ffmpeg(format!("exit {status}")))
        };
        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&output);
        frame
    }
}

pub type Clock = Arc<dyn Fn() -> OffsetDateTime + Send + Sync>;

pub struct Poster {
    pub storage: Arc<dyn Storage>,
    pub blobs: Arc<dyn Blobs>,
    pub frames: Arc<dyn Frames>,
    pub clock: Clock,
}

impl std::fmt::Debug for Poster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Poster").finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Input {
    pub household: HouseholdId,
    pub media_id: MediaId,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Output {
    pub media_id: MediaId,
    pub state: ProcessingState,
    pub skipped: Option<String>,
}

pub fn poster_key(household: &str, media: &str) -> String {
    format!("renditions/{household}/{media}/poster.jpg")
}

pub async fn run(poster: &Poster, input: Input) -> Result<Output> {
    let Input {
        household,
        media_id,
    } = input;
    let mut media = poster
        .storage
        .get_media(&household, &media_id)
        .await?
        .ok_or_else(|| PosterError::Input(format!("no media {media_id}")))?;
    if media.kind != MediaKind::Video {
        return Ok(Output {
            media_id: media.id,
            state: media.state,
            skipped: Some("not a video".into()),
        });
    }
    let video = poster.blobs.get(&media.original_key).await?;
    let frame = poster.frames.extract(&video)?;
    let img = decode::decode(&frame).map_err(|e| PosterError::Ffmpeg(format!("frame: {e}")))?;
    let img = renditions::upright(img, media.orientation);
    media.signals = quality::signals(&img);
    let full = renditions::encode(&img, Rendition::Tv)
        .map_err(|e| PosterError::Input(format!("encode: {e}")))?;
    poster
        .blobs
        .put(
            &poster_key(household.as_str(), media_id.as_str()),
            full,
            "image/jpeg",
        )
        .await?;
    for rendition in [Rendition::Thumb, Rendition::Tv] {
        let jpeg = renditions::encode(&img, rendition)
            .map_err(|e| PosterError::Input(format!("encode: {e}")))?;
        poster
            .blobs
            .put(
                &rendition.key(household.as_str(), media_id.as_str()),
                jpeg,
                "image/jpeg",
            )
            .await?;
    }
    media.state = ProcessingState::Signalled;
    poster.storage.put_media(&media).await?;
    Ok(Output {
        media_id: media.id,
        state: media.state,
        skipped: None,
    })
}
