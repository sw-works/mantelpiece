//! Model-backed enrichment (F-202 to F-204, F-706, research R8): Rekognition for labels,
//! faces and text; a Bedrock multimodal model for descriptions of keepers.

use async_trait::async_trait;
use mantelpiece_core::media::{Enrichment, Tag};
use std::sync::Mutex;

use crate::{Result, StorageError};

#[async_trait]
pub trait Vision: Send + Sync {
    /// Labels and face counts. `jpeg` is the TV rendition, never the original.
    async fn analyze(&self, jpeg: &[u8]) -> Result<Enrichment>;
    /// Lines of text visible in the photo (F-706).
    async fn detect_text(&self, jpeg: &[u8]) -> Result<Vec<String>>;
    /// A short description answering `prompt` about the photo (research R8). The caller
    /// checks it against metadata before saving.
    async fn describe(&self, jpeg: &[u8], prompt: &str) -> Result<String>;
}

/// Returns whatever it was configured with. For tests and for running the pipeline
/// without model spend.
#[derive(Debug, Default)]
pub struct MemoryVision {
    pub enrichment: Enrichment,
    pub text: Vec<String>,
    pub description: String,
    pub prompts: Mutex<Vec<String>>,
}

impl MemoryVision {
    pub fn with_enrichment(enrichment: Enrichment) -> Self {
        Self {
            enrichment,
            ..Self::default()
        }
    }
}

#[async_trait]
impl Vision for MemoryVision {
    async fn analyze(&self, _jpeg: &[u8]) -> Result<Enrichment> {
        Ok(self.enrichment.clone())
    }
    async fn detect_text(&self, _jpeg: &[u8]) -> Result<Vec<String>> {
        Ok(self.text.clone())
    }
    async fn describe(&self, _jpeg: &[u8], prompt: &str) -> Result<String> {
        self.prompts.lock().unwrap().push(prompt.to_owned());
        Ok(self.description.clone())
    }
}

fn backend<E: std::fmt::Debug>(e: E) -> StorageError {
    StorageError::Backend(format!("{e:?}"))
}

/// Rekognition plus Bedrock. Compile-checked only.
#[derive(Debug, Clone)]
pub struct AwsVision {
    rekognition: aws_sdk_rekognition::Client,
    bedrock: aws_sdk_bedrockruntime::Client,
    model_id: String,
}

impl AwsVision {
    pub const MIN_CONFIDENCE: f32 = 60.0;
    pub const MAX_LABELS: i32 = 20;
    pub const DEFAULT_MODEL: &'static str = "amazon.nova-lite-v1:0";

    pub fn from_config(config: &aws_config::SdkConfig, model_id: Option<String>) -> Self {
        Self {
            rekognition: aws_sdk_rekognition::Client::new(config),
            bedrock: aws_sdk_bedrockruntime::Client::new(config),
            model_id: model_id.unwrap_or_else(|| Self::DEFAULT_MODEL.to_owned()),
        }
    }

    fn image(jpeg: &[u8]) -> aws_sdk_rekognition::types::Image {
        aws_sdk_rekognition::types::Image::builder()
            .bytes(aws_sdk_rekognition::primitives::Blob::new(jpeg.to_vec()))
            .build()
    }
}

#[async_trait]
impl Vision for AwsVision {
    async fn analyze(&self, jpeg: &[u8]) -> Result<Enrichment> {
        use aws_sdk_rekognition::types::Attribute;

        let labels = self
            .rekognition
            .detect_labels()
            .image(Self::image(jpeg))
            .max_labels(Self::MAX_LABELS)
            .min_confidence(Self::MIN_CONFIDENCE)
            .send()
            .await
            .map_err(backend)?;
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
            .rekognition
            .detect_faces()
            .image(Self::image(jpeg))
            .attributes(Attribute::All)
            .send()
            .await
            .map_err(backend)?;
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

    async fn detect_text(&self, jpeg: &[u8]) -> Result<Vec<String>> {
        use aws_sdk_rekognition::types::TextTypes;

        let out = self
            .rekognition
            .detect_text()
            .image(Self::image(jpeg))
            .send()
            .await
            .map_err(backend)?;
        Ok(out
            .text_detections()
            .iter()
            .filter(|t| t.r#type() == Some(&TextTypes::Line))
            .filter_map(|t| t.detected_text().map(str::to_owned))
            .collect())
    }

    async fn describe(&self, jpeg: &[u8], prompt: &str) -> Result<String> {
        use aws_sdk_bedrockruntime::primitives::Blob;
        use aws_sdk_bedrockruntime::types::{
            ContentBlock, ConversationRole, ImageBlock, ImageFormat, ImageSource, Message,
        };

        let image = ImageBlock::builder()
            .format(ImageFormat::Jpeg)
            .source(ImageSource::Bytes(Blob::new(jpeg.to_vec())))
            .build()
            .map_err(backend)?;
        let message = Message::builder()
            .role(ConversationRole::User)
            .content(ContentBlock::Image(image))
            .content(ContentBlock::Text(prompt.to_owned()))
            .build()
            .map_err(backend)?;
        let out = self
            .bedrock
            .converse()
            .model_id(&self.model_id)
            .messages(message)
            .send()
            .await
            .map_err(backend)?;
        let text = out
            .output()
            .and_then(|o| o.as_message().ok())
            .and_then(|m| m.content().iter().find_map(|c| c.as_text().ok()))
            .cloned()
            .unwrap_or_default();
        Ok(text)
    }
}
