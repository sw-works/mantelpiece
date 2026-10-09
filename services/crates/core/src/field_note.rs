//! Field notes (F-711): a quiet map of where a highlight was taken, laid into the photo's
//! empty space as a separate layer. The photo itself is never changed. These are the
//! records; drawing lives in `mantelpiece-fieldnotes`.

use crate::geo::GeoPoint;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// A box in a photo's upright frame, as fractions of its width and height.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct NormBox {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl NormBox {
    /// Clamped to the frame; `None` when nothing of it is inside.
    #[must_use]
    pub fn new(left: f32, top: f32, width: f32, height: f32) -> Option<Self> {
        let (l, t) = (left.max(0.0), top.max(0.0));
        let (r, b) = ((left + width).min(1.0), (top + height).min(1.0));
        (r > l && b > t).then_some(Self {
            left: l,
            top: t,
            width: r - l,
            height: b - t,
        })
    }

    #[must_use]
    pub fn area(&self) -> f32 {
        self.width * self.height
    }

    #[must_use]
    pub fn right(&self) -> f32 {
        self.left + self.width
    }

    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.top + self.height
    }

    /// Grown by `dx` and `dy` on every side, still inside the frame.
    #[must_use]
    pub fn padded(&self, dx: f32, dy: f32) -> Self {
        Self::new(
            self.left - dx,
            self.top - dy,
            self.width + 2.0 * dx,
            self.height + 2.0 * dy,
        )
        .unwrap_or(*self)
    }

    #[must_use]
    pub fn intersects(&self, other: &Self) -> bool {
        self.left < other.right()
            && other.left < self.right()
            && self.top < other.bottom()
            && other.top < self.bottom()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectKind {
    Person,
    Face,
    Text,
    Object,
    /// Scenery the photo is about: a peak, a building, a monument, a lone tree.
    Landscape,
}

/// A main subject of a photo. A field note's map keeps clear of every one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Subject {
    pub kind: SubjectKind,
    pub label: String,
    pub bbox: NormBox,
    pub confidence: f32,
}

/// Boxes under this share of the frame are noise, except text, which is kept at any size.
pub const MIN_SUBJECT_AREA: f32 = 0.005;

impl Subject {
    /// A subject worth keeping, or `None` for a box outside the frame or too small.
    #[must_use]
    pub fn new(
        kind: SubjectKind,
        label: impl Into<String>,
        bbox: NormBox,
        confidence: f32,
    ) -> Option<Self> {
        let bbox = NormBox::new(bbox.left, bbox.top, bbox.width, bbox.height)?;
        if kind != SubjectKind::Text && bbox.area() < MIN_SUBJECT_AREA {
            return None;
        }
        Some(Self {
            kind,
            label: label.into(),
            bbox,
            confidence: confidence.clamp(0.0, 1.0),
        })
    }
}

/// The map's character, chosen from the geography in view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapMode {
    Coast,
    Mountains,
    Town,
    Trail,
}

/// How much is known about where the photo was taken. Unknown places get no note.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Certainty {
    /// The photo's own GPS, outside the household's home area: a dot.
    Precise { at: GeoPoint },
    /// Inferred from the day's route or a landmark, or a GPS point near home: a ring.
    Approximate { at: GeoPoint, radius_m: u32 },
}

impl Certainty {
    #[must_use]
    pub fn at(&self) -> GeoPoint {
        match self {
            Self::Precise { at } | Self::Approximate { at, .. } => *at,
        }
    }

    #[must_use]
    pub fn is_precise(&self) -> bool {
        matches!(self, Self::Precise { .. })
    }
}

/// Light ink on a dark region, dark ink on a light one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ink {
    Light,
    Dark,
}

/// One small line under the map. `coords` only for precise places, already rounded.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Caption {
    pub place: Option<String>,
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coords: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureKind {
    Landmark,
    Peak,
    Water,
    Place,
}

/// What the map is mainly about, for "This place": the trip's landmark for the photo, else
/// the most prominent named feature near the marker. Its description is looked up from
/// the encyclopedia by name and position, never written by a model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Feature {
    pub name: String,
    pub kind: FeatureKind,
    pub at: GeoPoint,
    /// "2,999 m" for a peak.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
}

/// The map layer for one highlight. The drawing is a display list in S3 at `drawing_key`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldNote {
    pub placement: NormBox,
    pub mode: MapMode,
    pub certainty: Certainty,
    pub ink: Ink,
    pub caption: Caption,
    /// `openstreetmap`.
    pub source: String,
    /// Shown wherever the note is shown.
    pub credit: String,
    pub drawing_key: String,
    /// What it was made from; a different fingerprint at organize means remake.
    pub fingerprint: String,
    #[serde(with = "time::serde::rfc3339")]
    pub made_at: OffsetDateTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<Feature>,
}

impl FieldNote {
    pub fn drawing_key(household: &str, media: &str) -> String {
        format!("renditions/{household}/{media}/fieldnote.json")
    }
    pub fn layer_key(household: &str, media: &str) -> String {
        format!("renditions/{household}/{media}/fieldnote.png")
    }
    pub fn public_layer_key(household: &str, media: &str) -> String {
        format!("renditions/{household}/{media}/fieldnote-public.png")
    }
    pub fn full_key(household: &str, media: &str) -> String {
        format!("renditions/{household}/{media}/fieldnote-full.jpg")
    }
    /// The map alone, large, for "This place" on the TV.
    pub fn map_key(household: &str, media: &str) -> String {
        format!("renditions/{household}/{media}/fieldnote-map.png")
    }
    pub fn public_map_key(household: &str, media: &str) -> String {
        format!("renditions/{household}/{media}/fieldnote-map-public.png")
    }
    /// Every object a field note leaves, for purge and account deletion.
    pub fn keys(household: &str, media: &str) -> [String; 6] {
        [
            Self::drawing_key(household, media),
            Self::layer_key(household, media),
            Self::public_layer_key(household, media),
            Self::full_key(household, media),
            Self::map_key(household, media),
            Self::public_map_key(household, media),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxes_clamp_to_the_frame_and_drop_when_outside() {
        let b = NormBox::new(-0.1, 0.9, 0.5, 0.5).unwrap();
        assert!((b.left - 0.0).abs() < 1e-6 && (b.width - 0.4).abs() < 1e-6);
        assert!((b.height - 0.1).abs() < 1e-6);
        assert!(NormBox::new(1.2, 0.0, 0.3, 0.3).is_none());
        let p = NormBox::new(0.4, 0.4, 0.2, 0.2).unwrap().padded(0.05, 0.05);
        assert!((p.left - 0.35).abs() < 1e-6 && (p.width - 0.3).abs() < 1e-6);
    }

    #[test]
    fn tiny_subjects_are_noise_except_text() {
        let tiny = NormBox::new(0.1, 0.1, 0.05, 0.05).unwrap();
        assert!(Subject::new(SubjectKind::Object, "Bird", tiny, 0.9).is_none());
        assert!(Subject::new(SubjectKind::Text, "EXIT", tiny, 0.9).is_some());
    }

    #[test]
    fn certainty_reads_back_with_its_kind() {
        let c = Certainty::Approximate {
            at: GeoPoint::new(35.0, 135.7),
            radius_m: 800,
        };
        let json = serde_json::to_value(c).unwrap();
        assert_eq!(json["kind"], "approximate");
        assert_eq!(serde_json::from_value::<Certainty>(json).unwrap(), c);
        assert!(!c.is_precise());
    }
}
