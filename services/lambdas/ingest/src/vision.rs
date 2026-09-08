//! Model-backed enrichment (F-202 to F-204). Rekognition for labels and faces.
//! Landmark escalation to Bedrock Nova on low confidence is a later step.

use async_trait::async_trait;
use memorylane_core::media::{Enrichment, Tag};

use crate::{IngestError, Result};

#[async_trait]
pub trait Vision: Send + Sync {
    /// `jpeg` is the TV rendition, never the original.
    async fn analyze(&self, jpeg: &[u8]) -> Result<Enrichment>;
}

/// Returns whatever it was configured with. For tests and for running the pipeline
/// without model spend.
#[derive(Debug, Default)]
pub struct FixedVision {
    pub enrichment: Enrichment,
}

#[async_trait]
impl Vision for FixedVision {
    async fn analyze(&self, _jpeg: &[u8]) -> Result<Enrichment> {
        Ok(self.enrichment.clone())
    }
}

/// Rekognition backend. Compile-checked only.
#[derive(Debug, Clone)]
pub struct RekognitionVision {
    client: aws_sdk_rekognition::Client,
}

impl RekognitionVision {
    pub const MIN_CONFIDENCE: f32 = 60.0;
    pub const MAX_LABELS: i32 = 20;

    pub fn from_config(config: &aws_config::SdkConfig) -> Self {
        Self {
            client: aws_sdk_rekognition::Client::new(config),
        }
    }
}

#[async_trait]
impl Vision for RekognitionVision {
    async fn analyze(&self, jpeg: &[u8]) -> Result<Enrichment> {
        use aws_sdk_rekognition::primitives::Blob;
        use aws_sdk_rekognition::types::{Attribute, Image};

        let image = || Image::builder().bytes(Blob::new(jpeg.to_vec())).build();
        let labels = self
            .client
            .detect_labels()
            .image(image())
            .max_labels(Self::MAX_LABELS)
            .min_confidence(Self::MIN_CONFIDENCE)
            .send()
            .await
            .map_err(|e| IngestError::Vision(format!("{e:?}")))?;
        let scene_tags: Vec<Tag> = labels
            .labels()
            .iter()
            .filter_map(|l| {
                Some(Tag {
                    name: l.name()?.to_owned(),
                    confidence: l.confidence().unwrap_or(0.0) / 100.0,
                })
            })
            .collect();

        let faces = self
            .client
            .detect_faces()
            .image(image())
            .attributes(Attribute::All)
            .send()
            .await
            .map_err(|e| IngestError::Vision(format!("{e:?}")))?;
        let details = faces.face_details();
        let face_count = u32::try_from(details.len()).unwrap_or(u32::MAX);
        let smile_count = details
            .iter()
            .filter(|f| {
                f.smile()
                    .is_some_and(aws_sdk_rekognition::types::Smile::value)
            })
            .count();

        Ok(Enrichment {
            scene_tags,
            landmark: None,
            face_count,
            smile_count: u32::try_from(smile_count).unwrap_or(u32::MAX),
            inherited_from: None,
        })
    }
}
