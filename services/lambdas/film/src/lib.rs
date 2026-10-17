//! Day films (F-714).
//!
//! Invoked asynchronously, by the API (`POST /trips/{trip}/film`) or by a one-time
//! EventBridge Scheduler schedule set at the end of a trip day. Plans the day's reel for a
//! 1920 x 1080 frame (the same plan the TV plays, F-713), skips when the marker beside the
//! stored film matches the plan's fingerprint, else draws every frame with
//! `mantelpiece_reels::render`, encodes them with the day's music through [`Encoder`],
//! stores the MP4 under `films/` (never `originals/`, so it does not re-enter ingest) and
//! emits `film.ready`.

use mantelpiece_core::events::EventKind;
use mantelpiece_core::film::FilmKeys;
use mantelpiece_core::ids::{HouseholdId, MediaId, TripId};
use mantelpiece_reels::plan::{self, Frame, PlanError, Reel, Request};
use mantelpiece_reels::render::{self, Layers};
use mantelpiece_store::{feed, Blobs, Storage, StorageError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use thiserror::Error;
use time::OffsetDateTime;
use tiny_skia::Pixmap;

pub const FPS: u32 = 30;
pub const FRAME: Frame = Frame::TV;
/// Frames drawn per batch across threads before they are handed to the encoder in order.
const BATCH: usize = 24;

#[derive(Debug, Error)]
pub enum FilmError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("bad input: {0}")]
    Input(String),
    #[error("encode: {0}")]
    Encode(String),
}

pub type Result<T> = std::result::Result<T, FilmError>;

/// What the encoder is asked to make.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spec {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub duration_ms: u64,
    pub fade_out_ms: u64,
}

/// Turns RGBA frames and the music into an MP4.
pub trait Encoder: Send + Sync {
    /// `frames` yields `spec.width * spec.height * 4` RGBA bytes per frame, in order.
    ///
    /// # Errors
    /// When encoding fails.
    fn encode(
        &self,
        spec: &Spec,
        music: &[u8],
        frames: &mut dyn Iterator<Item = Vec<u8>>,
    ) -> Result<Vec<u8>>;
}

/// Counts frames and returns a fixed body. For tests and for running without the layer.
#[derive(Debug, Default)]
pub struct MemoryEncoder {
    pub frames: std::sync::Mutex<u64>,
    pub first: std::sync::Mutex<Option<Vec<u8>>>,
}

impl Encoder for MemoryEncoder {
    fn encode(
        &self,
        _spec: &Spec,
        _music: &[u8],
        frames: &mut dyn Iterator<Item = Vec<u8>>,
    ) -> Result<Vec<u8>> {
        let mut n = 0;
        for f in frames {
            if n == 0 {
                *self.first.lock().expect("lock") = Some(f);
            }
            n += 1;
        }
        *self.frames.lock().expect("lock") = n;
        Ok(b"mp4".to_vec())
    }
}

/// Pipes raw frames into a static ffmpeg (Lambda layer at `/opt/bin/ffmpeg`): H.264
/// (yuv420p, CRF 20) and AAC 192k, the music trimmed to the film, faded out with the
/// picture, normalised to -14 LUFS and limited at -3 dBFS so the true peak stays under
/// -1 dBTP after AAC, which overshoots a limiter by up to 1.3 dB (measured on the travel
/// POC). Not exercised by tests.
#[derive(Debug, Clone)]
pub struct FfmpegEncoder {
    pub binary: PathBuf,
    pub work_dir: PathBuf,
}

impl FfmpegEncoder {
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            binary: std::env::var("FFMPEG")
                .map_or_else(|_| PathBuf::from("/opt/bin/ffmpeg"), PathBuf::from),
            work_dir: std::env::temp_dir(),
        }
    }
}

impl Encoder for FfmpegEncoder {
    fn encode(
        &self,
        spec: &Spec,
        music: &[u8],
        frames: &mut dyn Iterator<Item = Vec<u8>>,
    ) -> Result<Vec<u8>> {
        let stamp = OffsetDateTime::now_utc().unix_timestamp_nanos();
        let audio = self.work_dir.join(format!("film-{stamp}.m4a"));
        let out = self.work_dir.join(format!("film-{stamp}.mp4"));
        std::fs::write(&audio, music).map_err(|e| FilmError::Encode(format!("music: {e}")))?;
        #[allow(clippy::cast_precision_loss)]
        let secs = |ms: u64| format!("{:.3}", ms as f64 / 1000.0);
        let fade_start = spec.duration_ms.saturating_sub(spec.fade_out_ms);
        let audio_chain = format!(
            "[1:a]atrim=0:{d},asetpts=N/SR/TB,afade=t=in:st=0:d=0.05,afade=t=out:st={fs}:d={fd},loudnorm=I=-14:TP=-3:LRA=11,alimiter=limit=0.708:level=0[a]",
            d = secs(spec.duration_ms),
            fs = secs(fade_start),
            fd = secs(spec.fade_out_ms),
        );
        let mut child = Command::new(&self.binary)
            .args(["-y", "-hide_banner", "-loglevel", "error"])
            .args(["-f", "rawvideo", "-pix_fmt", "rgba"])
            .args(["-s", &format!("{}x{}", spec.width, spec.height)])
            .args(["-r", &spec.fps.to_string(), "-i", "-"])
            .arg("-i")
            .arg(&audio)
            .args(["-filter_complex", &audio_chain])
            .args(["-map", "0:v", "-map", "[a]"])
            .args([
                "-c:v", "libx264", "-preset", "veryfast", "-crf", "20", "-pix_fmt", "yuv420p",
            ])
            .args(["-c:a", "aac", "-b:a", "192k", "-ar", "48000"])
            .args(["-movflags", "+faststart", "-t", &secs(spec.duration_ms)])
            .arg(&out)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| FilmError::Encode(format!("spawn {}: {e}", self.binary.display())))?;
        {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| FilmError::Encode("no stdin".into()))?;
            for frame in frames {
                if let Err(e) = stdin.write_all(&frame) {
                    // ffmpeg exited early; its stderr says why.
                    tracing::warn!(error = %e, "ffmpeg stopped reading frames");
                    break;
                }
            }
        }
        let result = child
            .wait_with_output()
            .map_err(|e| FilmError::Encode(e.to_string()))?;
        let _ = std::fs::remove_file(&audio);
        if !result.status.success() {
            let _ = std::fs::remove_file(&out);
            return Err(FilmError::Encode(
                String::from_utf8_lossy(&result.stderr).trim().to_owned(),
            ));
        }
        let bytes = std::fs::read(&out).map_err(|e| FilmError::Encode(e.to_string()));
        let _ = std::fs::remove_file(&out);
        bytes
    }
}

pub type Clock = Arc<dyn Fn() -> OffsetDateTime + Send + Sync>;

pub struct Film {
    pub storage: Arc<dyn Storage>,
    pub blobs: Arc<dyn Blobs>,
    pub encoder: Arc<dyn Encoder>,
    pub clock: Clock,
    /// [`FRAME`] and [`FPS`] in production; tests draw small frames.
    pub frame: Frame,
    pub fps: u32,
}

impl std::fmt::Debug for Film {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Film").finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Input {
    pub household: HouseholdId,
    pub trip: TripId,
    pub day: u32,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Output {
    pub day: u32,
    pub key: Option<String>,
    pub skipped: Option<String>,
}

fn skipped(day: u32, why: &str) -> Output {
    Output {
        day,
        key: None,
        skipped: Some(why.to_owned()),
    }
}

/// The TV rendition of a photo or video poster, which every reel slide is drawn from.
#[must_use]
pub fn rendition_key(household: &HouseholdId, media: &MediaId) -> String {
    format!("renditions/{household}/{media}/tv.jpg")
}

/// Plan the day's reel as its owner sees it.
///
/// # Errors
/// Storage failures.
pub async fn day_reel(
    storage: &dyn Storage,
    household: &HouseholdId,
    trip: &TripId,
    day: u32,
    frame: Frame,
) -> Result<std::result::Result<Reel, PlanError>> {
    let Some(t) = storage.get_trip(household, trip).await? else {
        return Ok(Err(PlanError::NoSuchDay));
    };
    let media: Vec<_> = storage
        .list_media(household)
        .await?
        .into_iter()
        .filter(|m| m.trip.as_ref() == Some(trip))
        .collect();
    Ok(plan::plan(&Request {
        trip: &t,
        day: Some(day),
        media: &media,
        viewer: household,
        frame,
    }))
}

/// Render and store one day film, or skip it.
///
/// # Errors
/// Storage, decoding and encoding failures; the invocation is retried.
pub async fn run(film: &Film, input: Input) -> Result<Output> {
    let Input {
        household,
        trip,
        day,
    } = input;
    let reel = match day_reel(film.storage.as_ref(), &household, &trip, day, film.frame).await? {
        Ok(r) => r,
        Err(PlanError::NoSuchDay) => return Ok(skipped(day, "no such day")),
        Err(PlanError::TooFewPhotos) => return Ok(skipped(day, "too few photos")),
    };
    let keys = FilmKeys::new(&household, &trip);
    let fingerprint = plan::fingerprint(&reel);
    if let Ok(body) = film.blobs.get(&keys.marker(day)).await {
        if FilmKeys::read_marker(&body).0 == fingerprint {
            return Ok(skipped(day, "up to date"));
        }
    }

    let mut pictures = HashMap::new();
    for slide in &reel.slides {
        if pictures.contains_key(&slide.media) {
            continue;
        }
        let jpeg = film
            .blobs
            .get(&rendition_key(&household, &slide.media))
            .await?;
        let pic = tokio::task::spawn_blocking(move || render::decode(&jpeg))
            .await
            .map_err(|e| FilmError::Input(e.to_string()))?
            .map_err(|e| FilmError::Input(format!("picture {}: {e}", slide.media)))?;
        pictures.insert(slide.media.clone(), pic);
    }
    let music = film.blobs.get(&reel.track.url).await?;

    let (frame, fps) = (film.frame, film.fps);
    let spec = Spec {
        width: frame.width,
        height: frame.height,
        fps,
        duration_ms: reel.duration_ms,
        fade_out_ms: reel.fade_out_ms,
    };
    let encoder = film.encoder.clone();
    let reel_for_frames = reel.clone();
    let bytes = tokio::task::spawn_blocking(move || {
        let layers = Layers::new(&reel_for_frames, frame);
        let mut frames = FrameStream::new(&reel_for_frames, &pictures, &layers, frame, fps);
        encoder.encode(&spec, &music, &mut frames)
    })
    .await
    .map_err(|e| FilmError::Encode(e.to_string()))??;

    let key = keys.film(day);
    film.blobs.put(&key, bytes, "video/mp4").await?;
    film.blobs
        .put(
            &keys.marker(day),
            FilmKeys::marker_body(
                &fingerprint,
                reel.duration_ms,
                &(film.clock)()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
            ),
            "text/plain",
        )
        .await?;
    feed::emit(
        film.storage.as_ref(),
        &household,
        (film.clock)(),
        EventKind::FilmReady {
            trip: trip.clone(),
            day,
            key: key.clone(),
        },
    )
    .await?;
    tracing::info!(%household, %trip, day, slides = reel.slides.len(), duration_ms = reel.duration_ms, "film stored");
    Ok(Output {
        day,
        key: Some(key),
        skipped: None,
    })
}

/// Frames drawn in batches across the machine's cores, yielded in order.
struct FrameStream<'a> {
    frame: Frame,
    reel: &'a Reel,
    pictures: &'a HashMap<MediaId, Pixmap>,
    layers: &'a Layers,
    times: Vec<u64>,
    next: usize,
    ready: std::collections::VecDeque<Vec<u8>>,
    threads: usize,
}

impl<'a> FrameStream<'a> {
    fn new(
        reel: &'a Reel,
        pictures: &'a HashMap<MediaId, Pixmap>,
        layers: &'a Layers,
        frame: Frame,
        fps: u32,
    ) -> Self {
        Self {
            frame,
            reel,
            pictures,
            layers,
            times: render::frame_times(reel.duration_ms, fps).collect(),
            next: 0,
            ready: std::collections::VecDeque::new(),
            threads: std::thread::available_parallelism().map_or(2, std::num::NonZero::get),
        }
    }

    fn fill(&mut self) {
        let end = (self.next + BATCH).min(self.times.len());
        let batch = &self.times[self.next..end];
        let per = batch.len().div_ceil(self.threads).max(1);
        let (reel, pictures, layers, frame) = (self.reel, self.pictures, self.layers, self.frame);
        let drawn: Vec<Vec<Vec<u8>>> = std::thread::scope(|s| {
            let handles: Vec<_> = batch
                .chunks(per)
                .map(|chunk| {
                    s.spawn(move || {
                        let mut canvas =
                            Pixmap::new(frame.width, frame.height).expect("frame size");
                        chunk
                            .iter()
                            .map(|&t| {
                                render::draw(&mut canvas, reel, t, pictures, layers, frame);
                                canvas.data().to_vec()
                            })
                            .collect()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("frame thread"))
                .collect()
        });
        self.ready.extend(drawn.into_iter().flatten());
        self.next = end;
    }
}

impl Iterator for FrameStream<'_> {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Vec<u8>> {
        if self.ready.is_empty() && self.next < self.times.len() {
            self.fill();
        }
        self.ready.pop_front()
    }
}
