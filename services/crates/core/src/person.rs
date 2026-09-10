//! Opt-in face groups (F-704). Exists only while the household has People enabled; deleted
//! with the face collection.

use crate::ids::{HouseholdId, MediaId, PersonId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub id: PersonId,
    pub household: HouseholdId,
    /// Named from the phone; unnamed clusters show a sample face only.
    pub name: Option<String>,
    /// Face ids in the household's Rekognition collection.
    pub face_ids: Vec<String>,
    pub media_count: u32,
    /// Up to a few media ids whose thumbnails represent the person.
    pub sample_media: Vec<MediaId>,
    /// Set when this cluster was merged into another; readers follow the pointer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_into: Option<PersonId>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

impl Person {
    pub const MAX_SAMPLES: usize = 4;

    pub fn new(
        household: HouseholdId,
        face_id: impl Into<String>,
        media: MediaId,
        now: OffsetDateTime,
    ) -> Self {
        Self {
            id: PersonId::random(),
            household,
            name: None,
            face_ids: vec![face_id.into()],
            media_count: 1,
            sample_media: vec![media],
            merged_into: None,
            created_at: now,
        }
    }

    /// Record another face of this person on `media`.
    pub fn add_face(&mut self, face_id: impl Into<String>, media: MediaId) {
        self.face_ids.push(face_id.into());
        self.media_count += 1;
        if self.sample_media.len() < Self::MAX_SAMPLES && !self.sample_media.contains(&media) {
            self.sample_media.push(media);
        }
    }

    /// Fold `other` into this person; `other` becomes a pointer.
    pub fn merge(&mut self, other: &mut Person) {
        self.face_ids.append(&mut other.face_ids);
        self.media_count += other.media_count;
        for m in other.sample_media.drain(..) {
            if self.sample_media.len() < Self::MAX_SAMPLES && !self.sample_media.contains(&m) {
                self.sample_media.push(m);
            }
        }
        if self.name.is_none() {
            self.name = other.name.take();
        }
        other.media_count = 0;
        other.merged_into = Some(self.id.clone());
    }

    pub fn is_merged(&self) -> bool {
        self.merged_into.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn merge_keeps_name_faces_and_samples() {
        let now = datetime!(2026-09-08 10:00:00 UTC);
        let mut a = Person::new(HouseholdId::new("h"), "f1", MediaId::new("m1"), now);
        a.add_face("f2", MediaId::new("m2"));
        let mut b = Person::new(HouseholdId::new("h"), "f3", MediaId::new("m3"), now);
        b.name = Some("Maya".into());
        a.merge(&mut b);
        assert_eq!(a.name.as_deref(), Some("Maya"));
        assert_eq!(a.face_ids, vec!["f1", "f2", "f3"]);
        assert_eq!(a.media_count, 3);
        assert_eq!(a.sample_media.len(), 3);
        assert!(b.is_merged());
        assert_eq!(b.merged_into, Some(a.id.clone()));
    }
}
