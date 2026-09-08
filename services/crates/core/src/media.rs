//! One uploaded photo or video and everything the pipeline learns about it.

use crate::geo::GeoPoint;
use crate::ids::{BatchId, ContributorId, HouseholdId, MediaId, TripId};
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, UtcOffset};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Photo,
    Video,
}

impl MediaKind {
    /// Classify by file extension. Unknown extensions are photos, which lets the
    /// decoder decide later.
    pub fn from_filename(name: &str) -> Self {
        let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        match ext.as_str() {
            "mp4" | "mov" | "m4v" | "webm" | "avi" | "mkv" => Self::Video,
            _ => Self::Photo,
        }
    }
}

/// Capture time as recorded by the camera, in the camera's zone when known.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureTime {
    /// Normalised to UTC (F-104).
    #[serde(with = "time::serde::rfc3339")]
    pub utc: OffsetDateTime,
    /// Original zone offset in whole seconds, when EXIF carried one.
    pub offset_seconds: Option<i32>,
    /// True when the EXIF timestamp had no zone and UTC was assumed.
    pub zone_assumed: bool,
}

impl CaptureTime {
    pub fn new(utc: OffsetDateTime, offset: Option<UtcOffset>) -> Self {
        Self {
            utc: utc.to_offset(UtcOffset::UTC),
            offset_seconds: offset.map(UtcOffset::whole_seconds),
            zone_assumed: offset.is_none(),
        }
    }

    /// Wall-clock time as the traveller saw it.
    pub fn local(&self) -> OffsetDateTime {
        match self
            .offset_seconds
            .and_then(|s| UtcOffset::from_whole_seconds(s).ok())
        {
            Some(off) => self.utc.to_offset(off),
            None => self.utc,
        }
    }
}

/// Signals computed before any model call (F-107).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Signals {
    /// 64-bit perceptual hash.
    pub phash: Option<u64>,
    /// Variance of the Laplacian; higher is sharper.
    pub sharpness: Option<f64>,
    /// Mean luminance 0.0 to 1.0.
    pub exposure: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// Model-derived enrichment. Non-keepers in a burst inherit the keeper's copy (F-202).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Enrichment {
    pub scene_tags: Vec<Tag>,
    pub landmark: Option<Tag>,
    pub face_count: u32,
    pub smile_count: u32,
    /// Id of the keeper this enrichment was copied from, if inherited.
    pub inherited_from: Option<MediaId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    /// 0.0 to 1.0
    pub confidence: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingState {
    Uploaded,
    Signalled,
    Enriched,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: MediaId,
    pub household: HouseholdId,
    pub contributor: ContributorId,
    pub batch: BatchId,
    pub kind: MediaKind,
    /// S3 key of the original: `originals/{household}/{contributor}/{media}.{ext}`.
    pub original_key: String,
    pub original_filename: String,
    pub bytes: u64,
    pub sha256: String,
    pub captured: Option<CaptureTime>,
    pub gps: Option<GeoPoint>,
    /// EXIF orientation 1 to 8.
    pub orientation: u8,
    pub signals: Signals,
    pub enrichment: Enrichment,
    /// 0 to 10 (F-204).
    pub quality_score: Option<f32>,
    pub burst_group: Option<String>,
    pub is_keeper: bool,
    pub trip: Option<TripId>,
    pub pinned: bool,
    pub hidden: bool,
    pub state: ProcessingState,
    #[serde(with = "time::serde::rfc3339")]
    pub uploaded_at: OffsetDateTime,
}

impl MediaItem {
    pub fn original_key(
        household: &HouseholdId,
        contributor: &ContributorId,
        id: &MediaId,
        filename: &str,
    ) -> String {
        let ext = filename
            .rsplit('.')
            .next()
            .filter(|e| e.len() <= 5)
            .unwrap_or("bin")
            .to_ascii_lowercase();
        format!("originals/{household}/{contributor}/{id}.{ext}")
    }

    /// Best-effort capture instant used for ordering. Falls back to upload time (F-155 wants
    /// capture time, never upload time, but a photo without EXIF still needs a place).
    pub fn sort_instant(&self) -> OffsetDateTime {
        self.captured.map_or(self.uploaded_at, |c| c.utc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn kind_from_filename() {
        assert_eq!(MediaKind::from_filename("IMG_0001.HEIC"), MediaKind::Photo);
        assert_eq!(MediaKind::from_filename("clip.MOV"), MediaKind::Video);
        assert_eq!(MediaKind::from_filename("noext"), MediaKind::Photo);
    }

    #[test]
    fn capture_time_normalises_to_utc_and_keeps_zone() {
        let local = datetime!(2026-04-03 14:30:00 +09:00);
        let ct = CaptureTime::new(local, Some(UtcOffset::from_hms(9, 0, 0).unwrap()));
        assert_eq!(ct.utc, datetime!(2026-04-03 05:30:00 UTC));
        assert_eq!(ct.offset_seconds, Some(9 * 3600));
        assert!(!ct.zone_assumed);
        assert_eq!(ct.local().hour(), 14);
    }

    #[test]
    fn capture_time_without_zone_is_flagged() {
        let ct = CaptureTime::new(datetime!(2026-04-03 14:30:00 UTC), None);
        assert!(ct.zone_assumed);
        assert_eq!(ct.local(), ct.utc);
    }

    #[test]
    fn original_key_layout() {
        let key = MediaItem::original_key(
            &HouseholdId::new("h1"),
            &ContributorId::new("c1"),
            &MediaId::new("m1"),
            "Photo.JPEG",
        );
        assert_eq!(key, "originals/h1/c1/m1.jpeg");
        let key = MediaItem::original_key(
            &HouseholdId::new("h1"),
            &ContributorId::new("c1"),
            &MediaId::new("m1"),
            "weird.averylongext",
        );
        assert_eq!(key, "originals/h1/c1/m1.bin");
    }
}
