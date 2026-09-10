//! One uploaded photo or video and everything the pipeline learns about it.

use crate::geo::GeoPoint;
use crate::ids::{BatchId, ContributorId, HouseholdId, MediaId, TripId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use time::{Duration, OffsetDateTime, UtcOffset};

/// How long a removed item can be restored before it is purged (US6).
pub const RESTORE_WINDOW: Duration = Duration::days(30);

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

/// Why quick cleanup flagged an item (F-705).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupReason {
    Screenshot,
    Document,
    Receipt,
    Blurry,
}

impl CleanupReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Screenshot => "screenshot",
            Self::Document => "document",
            Self::Receipt => "receipt",
            Self::Blurry => "blurry",
        }
    }
}

/// Sharpness (variance of the Laplacian) below this is a blurry frame.
pub const BLURRY_SHARPNESS: f64 = 20.0;
/// Text lines at or above this make a document.
pub const DOCUMENT_LINES: usize = 8;

/// Why quick cleanup would flag this item, from signals and detected text (F-705). Only
/// keepers are classified; screenshots are phone-shaped frames with no capture time.
pub fn cleanup_reason(
    captured: bool,
    width: Option<u32>,
    height: Option<u32>,
    sharpness: Option<f64>,
    text_lines: &[String],
) -> Option<CleanupReason> {
    let joined = text_lines.join(" ").to_lowercase();
    let receipt_words = [
        "total",
        "receipt",
        "subtotal",
        "tax",
        "change due",
        "cash",
        "visa",
    ];
    let receipt_hits = receipt_words.iter().filter(|w| joined.contains(*w)).count();
    if text_lines.len() >= 3 && (receipt_hits >= 2 || joined.contains("receipt")) {
        return Some(CleanupReason::Receipt);
    }
    if text_lines.len() >= DOCUMENT_LINES {
        return Some(CleanupReason::Document);
    }
    if let (false, Some(w), Some(h)) = (captured, width, height) {
        let (short, long) = (f64::from(w.min(h)), f64::from(w.max(h)));
        let ratio = long / short;
        // Phone screens run from 16:9 to about 22:9.
        if (1.7..=2.5).contains(&ratio) {
            return Some(CleanupReason::Screenshot);
        }
    }
    if sharpness.is_some_and(|s| s < BLURRY_SHARPNESS) {
        return Some(CleanupReason::Blurry);
    }
    None
}

/// A model-written description of a keeper, precomputed at ingest so `describe_photo`
/// answers from storage (research R8). Checked against metadata before it is saved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Description {
    pub text: String,
    pub landmark: Option<String>,
    pub model_id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub generated_at: OffsetDateTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingState {
    Uploaded,
    Signalled,
    Enriched,
    Failed,
    /// Soft-removed; restorable until `restore_until`, then purged.
    Removed,
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
    /// Owner's pin and hide (F-209). Per-household choices for joined trips live in
    /// `pinned_by` and `hidden_by` (F-163).
    pub pinned: bool,
    pub hidden: bool,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub pinned_by: BTreeSet<HouseholdId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub hidden_by: BTreeSet<HouseholdId>,
    /// Text detected in the photo (F-706).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup_reason: Option<CleanupReason>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub removed_at: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub restore_until: Option<OffsetDateTime>,
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

    /// Soft removal: the item leaves every view but can be restored for
    /// [`RESTORE_WINDOW`].
    pub fn remove(&mut self, now: OffsetDateTime) {
        self.state = ProcessingState::Removed;
        self.removed_at = Some(now);
        self.restore_until = Some(now + RESTORE_WINDOW);
    }

    /// Undo a soft removal while the window is open.
    pub fn restore(&mut self, now: OffsetDateTime) -> bool {
        if self.state != ProcessingState::Removed || self.restore_until.is_none_or(|t| now >= t) {
            return false;
        }
        self.state = if self.enrichment.scene_tags.is_empty() {
            ProcessingState::Signalled
        } else {
            ProcessingState::Enriched
        };
        self.removed_at = None;
        self.restore_until = None;
        true
    }

    pub fn is_removed(&self) -> bool {
        self.state == ProcessingState::Removed
    }

    /// Whether `household` sees this item pinned: the owner's flag for the owner, the
    /// per-household set for everyone else (F-163).
    pub fn pinned_for(&self, household: &HouseholdId) -> bool {
        if household == &self.household {
            self.pinned
        } else {
            self.pinned_by.contains(household)
        }
    }

    pub fn hidden_for(&self, household: &HouseholdId) -> bool {
        if household == &self.household {
            self.hidden
        } else {
            self.hidden_by.contains(household)
        }
    }

    /// Pin or hide for one household. Pinning clears hiding and vice versa.
    pub fn set_flag_for(&mut self, household: &HouseholdId, pin: bool, value: bool) {
        if household == &self.household {
            if pin {
                self.pinned = value;
                if value {
                    self.hidden = false;
                }
            } else {
                self.hidden = value;
                if value {
                    self.pinned = false;
                }
            }
            return;
        }
        let (set, other) = if pin {
            (&mut self.pinned_by, &mut self.hidden_by)
        } else {
            (&mut self.hidden_by, &mut self.pinned_by)
        };
        if value {
            set.insert(household.clone());
            other.remove(household);
        } else {
            set.remove(household);
        }
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

    fn item() -> MediaItem {
        MediaItem {
            id: MediaId::new("m1"),
            household: HouseholdId::new("h1"),
            contributor: ContributorId::new("c1"),
            batch: BatchId::new("b1"),
            kind: MediaKind::Photo,
            original_key: "originals/h1/c1/m1.jpg".into(),
            original_filename: "m1.jpg".into(),
            bytes: 1,
            sha256: "00".repeat(32),
            captured: None,
            gps: None,
            orientation: 1,
            signals: Signals::default(),
            enrichment: Enrichment::default(),
            quality_score: None,
            burst_group: None,
            is_keeper: true,
            trip: None,
            pinned: false,
            hidden: false,
            pinned_by: BTreeSet::new(),
            hidden_by: BTreeSet::new(),
            text: None,
            description: None,
            cleanup_reason: None,
            removed_at: None,
            restore_until: None,
            state: ProcessingState::Signalled,
            uploaded_at: datetime!(2026-09-08 10:00:00 UTC),
        }
    }

    #[test]
    fn soft_removal_restores_inside_the_window_only() {
        let mut m = item();
        let now = datetime!(2026-09-08 10:00:00 UTC);
        m.remove(now);
        assert!(m.is_removed());
        assert_eq!(m.restore_until, Some(now + RESTORE_WINDOW));
        assert!(!m.restore(now + RESTORE_WINDOW));
        assert!(m.restore(now + Duration::days(29)));
        assert_eq!(m.state, ProcessingState::Signalled);
        assert!(m.restore_until.is_none());
    }

    #[test]
    fn per_household_pins_do_not_touch_the_owner_flags() {
        let mut m = item();
        let owner = HouseholdId::new("h1");
        let member = HouseholdId::new("h2");
        m.set_flag_for(&member, true, true);
        assert!(m.pinned_for(&member));
        assert!(!m.pinned_for(&owner));
        m.set_flag_for(&member, false, true);
        assert!(m.hidden_for(&member) && !m.pinned_for(&member));
        m.set_flag_for(&owner, true, true);
        assert!(m.pinned && !m.hidden);
        let json = serde_json::to_value(&m).unwrap();
        assert_eq!(json["hidden_by"], serde_json::json!(["h2"]));
        assert!(json.get("pinned_by").is_none());
    }

    #[test]
    fn cleanup_reasons_from_signals_and_text() {
        let lines = |n: usize| (0..n).map(|i| format!("line {i}")).collect::<Vec<_>>();
        assert_eq!(
            cleanup_reason(false, Some(1170), Some(2532), Some(300.0), &[]),
            Some(CleanupReason::Screenshot)
        );
        assert_eq!(
            cleanup_reason(true, Some(1170), Some(2532), Some(300.0), &[]),
            None,
            "a captured photo keeps its shape"
        );
        assert_eq!(
            cleanup_reason(true, Some(4000), Some(3000), Some(5.0), &[]),
            Some(CleanupReason::Blurry)
        );
        assert_eq!(
            cleanup_reason(true, Some(4000), Some(3000), Some(300.0), &lines(9)),
            Some(CleanupReason::Document)
        );
        assert_eq!(
            cleanup_reason(
                true,
                None,
                None,
                None,
                &[
                    "Ichiran".into(),
                    "Subtotal 1200".into(),
                    "Total 1320".into()
                ]
            ),
            Some(CleanupReason::Receipt)
        );
        assert_eq!(
            cleanup_reason(true, Some(4000), Some(3000), Some(300.0), &[]),
            None
        );
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
