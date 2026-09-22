//! Opt-in face matching (F-704, research R9). One Rekognition collection per household,
//! created when People is switched on and deleted when it is switched off or the account
//! goes.

use async_trait::async_trait;
use mantelpiece_core::ids::{HouseholdId, MediaId};
use std::collections::HashMap;
use std::sync::Mutex;

use crate::{Result, StorageError};

#[derive(Clone, Debug, PartialEq)]
pub struct FaceMatch {
    pub face_id: String,
    /// 0.0 to 1.0
    pub similarity: f32,
}

#[async_trait]
pub trait Faces: Send + Sync {
    async fn create_collection(&self, household: &HouseholdId) -> Result<()>;
    async fn delete_collection(&self, household: &HouseholdId) -> Result<()>;
    /// Index every face on the keeper rendition; returns the new face ids.
    async fn index(
        &self,
        household: &HouseholdId,
        media: &MediaId,
        jpeg: &[u8],
    ) -> Result<Vec<String>>;
    /// Other faces in the collection that match the indexed face `face_id`, best first.
    /// Asked once per face, so every person in a group photo is matched on their own face.
    async fn search(&self, household: &HouseholdId, face_id: &str) -> Result<Vec<FaceMatch>>;
}

/// Scripted fake: `index` hands out sequential face ids, `search` answers from a map keyed
/// by the face asked about, so a test decides which faces match.
#[derive(Debug, Default)]
pub struct MemoryFaces {
    pub collections: Mutex<Vec<HouseholdId>>,
    pub faces_per_image: Mutex<usize>,
    pub matches: Mutex<HashMap<String, Vec<FaceMatch>>>,
    next: Mutex<u32>,
    pub log: Mutex<Vec<String>>,
}

impl MemoryFaces {
    pub fn with_faces_per_image(n: usize) -> Self {
        let f = Self::default();
        *f.faces_per_image.lock().unwrap() = n;
        f
    }
}

#[async_trait]
impl Faces for MemoryFaces {
    async fn create_collection(&self, household: &HouseholdId) -> Result<()> {
        self.collections.lock().unwrap().push(household.clone());
        self.log.lock().unwrap().push(format!("create {household}"));
        Ok(())
    }
    async fn delete_collection(&self, household: &HouseholdId) -> Result<()> {
        self.collections.lock().unwrap().retain(|h| h != household);
        self.log.lock().unwrap().push(format!("delete {household}"));
        Ok(())
    }
    async fn index(
        &self,
        household: &HouseholdId,
        media: &MediaId,
        _jpeg: &[u8],
    ) -> Result<Vec<String>> {
        if !self.collections.lock().unwrap().contains(household) {
            return Err(StorageError::NotFound(format!("collection {household}")));
        }
        let n = *self.faces_per_image.lock().unwrap();
        let mut next = self.next.lock().unwrap();
        let ids: Vec<String> = (0..n)
            .map(|_| {
                *next += 1;
                format!("face-{}", *next)
            })
            .collect();
        self.log
            .lock()
            .unwrap()
            .push(format!("index {media} {}", ids.len()));
        Ok(ids)
    }
    async fn search(&self, _household: &HouseholdId, face_id: &str) -> Result<Vec<FaceMatch>> {
        Ok(self
            .matches
            .lock()
            .unwrap()
            .get(face_id)
            .cloned()
            .unwrap_or_default())
    }
}

fn backend<E: std::fmt::Debug>(e: E) -> StorageError {
    StorageError::Backend(format!("{e:?}"))
}

/// Rekognition collections. Compile-checked only.
#[derive(Debug, Clone)]
pub struct RekognitionFaces {
    client: aws_sdk_rekognition::Client,
}

impl RekognitionFaces {
    pub const MATCH_THRESHOLD: f32 = 85.0;
    pub const MAX_FACES: i32 = 5;

    pub fn from_config(config: &aws_config::SdkConfig) -> Self {
        Self {
            client: aws_sdk_rekognition::Client::new(config),
        }
    }

    fn collection_id(household: &HouseholdId) -> String {
        let safe: String = household
            .as_str()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        format!("mantelpiece-{safe}")
    }

    fn image(jpeg: &[u8]) -> aws_sdk_rekognition::types::Image {
        aws_sdk_rekognition::types::Image::builder()
            .bytes(aws_sdk_rekognition::primitives::Blob::new(jpeg.to_vec()))
            .build()
    }
}

#[async_trait]
impl Faces for RekognitionFaces {
    async fn create_collection(&self, household: &HouseholdId) -> Result<()> {
        self.client
            .create_collection()
            .collection_id(Self::collection_id(household))
            .send()
            .await
            .map_err(backend)?;
        Ok(())
    }
    async fn delete_collection(&self, household: &HouseholdId) -> Result<()> {
        self.client
            .delete_collection()
            .collection_id(Self::collection_id(household))
            .send()
            .await
            .map_err(backend)?;
        Ok(())
    }
    async fn index(
        &self,
        household: &HouseholdId,
        media: &MediaId,
        jpeg: &[u8],
    ) -> Result<Vec<String>> {
        let out = self
            .client
            .index_faces()
            .collection_id(Self::collection_id(household))
            .image(Self::image(jpeg))
            .external_image_id(media.as_str())
            .max_faces(Self::MAX_FACES)
            .send()
            .await
            .map_err(backend)?;
        Ok(out
            .face_records()
            .iter()
            .filter_map(|r| r.face()?.face_id().map(str::to_owned))
            .collect())
    }
    async fn search(&self, household: &HouseholdId, face_id: &str) -> Result<Vec<FaceMatch>> {
        let out = self
            .client
            .search_faces()
            .collection_id(Self::collection_id(household))
            .face_id(face_id)
            .face_match_threshold(Self::MATCH_THRESHOLD)
            .max_faces(Self::MAX_FACES)
            .send()
            .await
            .map_err(backend)?;
        Ok(out
            .face_matches()
            .iter()
            .filter_map(|m| {
                Some(FaceMatch {
                    face_id: m.face()?.face_id()?.to_owned(),
                    similarity: m.similarity().unwrap_or(0.0) / 100.0,
                })
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_faces_need_a_collection_and_replay_matches() {
        let f = MemoryFaces::with_faces_per_image(2);
        let h = HouseholdId::new("h");
        assert!(f.index(&h, &MediaId::new("m"), b"jpg").await.is_err());
        f.create_collection(&h).await.unwrap();
        assert_eq!(
            f.index(&h, &MediaId::new("m"), b"jpg").await.unwrap(),
            ["face-1", "face-2"]
        );
        f.matches.lock().unwrap().insert(
            "face-2".into(),
            vec![FaceMatch {
                face_id: "face-1".into(),
                similarity: 0.97,
            }],
        );
        assert_eq!(f.search(&h, "face-2").await.unwrap().len(), 1);
        assert!(f.search(&h, "face-1").await.unwrap().is_empty());
        assert_eq!(
            RekognitionFaces::collection_id(&HouseholdId::new("a.b|c")),
            "mantelpiece-a_b_c"
        );
    }
}
