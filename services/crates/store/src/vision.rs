//! Model-backed enrichment (F-202 to F-204, F-706, research R8): Rekognition for labels,
//! faces and text; a Bedrock multimodal model for descriptions of keepers.

use async_trait::async_trait;
use mantelpiece_core::field_note::{NormBox, Subject, SubjectKind};
use mantelpiece_core::media::{Enrichment, Tag};
use std::sync::Mutex;

use crate::{Result, StorageError};

#[async_trait]
pub trait Vision: Send + Sync {
    /// Labels and face counts. `jpeg` is the TV rendition, never the original.
    async fn analyze(&self, jpeg: &[u8]) -> Result<Enrichment>;
    /// Lines of text visible in the photo (F-706), with their boxes for field notes.
    async fn detect_text(&self, jpeg: &[u8]) -> Result<Text>;
    /// A short description answering `prompt` about the photo (research R8). The caller
    /// checks it against metadata before saving.
    async fn describe(&self, jpeg: &[u8], prompt: &str) -> Result<String>;
    /// The photo's main subjects as boxes, scenery included (a peak, a temple roof), for
    /// field-note placement (F-711). One model call; an unreadable answer is no boxes.
    async fn subjects(&self, jpeg: &[u8]) -> Result<Vec<Subject>>;
}

/// Text found in a photo: the lines, and where they are.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Text {
    pub lines: Vec<String>,
    pub boxes: Vec<Subject>,
}

/// The question put to the model for subject boxes.
pub const SUBJECTS_PROMPT: &str = "List the main subjects of this photo: people, animals, \
vehicles, buildings, monuments, statues, peaks, waterfalls, trees or anything else the photo \
is about. Do not list sky, clouds, plain water, plain ground or empty background. Answer with \
JSON only, an array of objects {\"label\": string, \"box\": [x1, y1, x2, y2]}, coordinates on \
a 0 to 1000 scale from the top left. Answer [] if nothing stands out.";

/// Parse the model's answer leniently: the first JSON array in the text, boxes on the 0 to
/// 1000 grid, anything malformed skipped.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn parse_subjects(answer: &str) -> Vec<Subject> {
    let (Some(start), Some(end)) = (answer.find('['), answer.rfind(']')) else {
        return vec![];
    };
    let Ok(items) =
        serde_json::from_str::<Vec<serde_json::Value>>(answer.get(start..=end).unwrap_or("[]"))
    else {
        return vec![];
    };
    items
        .iter()
        .filter_map(|v| {
            let b = v.get("box")?.as_array()?;
            let n: Vec<f32> = b
                .iter()
                .filter_map(serde_json::Value::as_f64)
                .map(|f| f as f32 / 1000.0)
                .collect();
            let [x1, y1, x2, y2] = n[..] else { return None };
            let label = v
                .get("label")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("subject");
            Subject::new(
                SubjectKind::Landscape,
                label,
                NormBox::new(x1.min(x2), y1.min(y2), (x2 - x1).abs(), (y2 - y1).abs())?,
                0.8,
            )
        })
        .collect()
}

/// A Rekognition box (ratios of width and height) as a frame box.
fn rek_box(b: Option<&aws_sdk_rekognition::types::BoundingBox>) -> Option<NormBox> {
    let b = b?;
    NormBox::new(b.left()?, b.top()?, b.width()?, b.height()?)
}

/// Returns whatever it was configured with. For tests and for running the pipeline
/// without model spend.
#[derive(Debug, Default)]
pub struct MemoryVision {
    pub enrichment: Enrichment,
    pub text: Vec<String>,
    pub subjects: Vec<Subject>,
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
    async fn detect_text(&self, _jpeg: &[u8]) -> Result<Text> {
        Ok(Text {
            lines: self.text.clone(),
            boxes: vec![],
        })
    }
    async fn describe(&self, _jpeg: &[u8], prompt: &str) -> Result<String> {
        self.prompts.lock().unwrap().push(prompt.to_owned());
        Ok(self.description.clone())
    }
    async fn subjects(&self, _jpeg: &[u8]) -> Result<Vec<Subject>> {
        self.prompts
            .lock()
            .unwrap()
            .push(SUBJECTS_PROMPT.to_owned());
        Ok(self.subjects.clone())
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
        let mut subjects: Vec<Subject> = labels
            .labels()
            .iter()
            .flat_map(|l| {
                let name = l.name().unwrap_or("Object").to_owned();
                let kind = if name == "Person" {
                    SubjectKind::Person
                } else {
                    SubjectKind::Object
                };
                l.instances().iter().filter_map(move |i| {
                    Subject::new(
                        kind,
                        name.clone(),
                        rek_box(i.bounding_box())?,
                        i.confidence().unwrap_or(0.0) / 100.0,
                    )
                })
            })
            .collect();
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
        subjects.extend(details.iter().filter_map(|f| {
            Subject::new(
                SubjectKind::Face,
                "Face",
                rek_box(f.bounding_box())?,
                f.confidence().unwrap_or(0.0) / 100.0,
            )
        }));
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
            subjects,
        })
    }

    async fn detect_text(&self, jpeg: &[u8]) -> Result<Text> {
        use aws_sdk_rekognition::types::TextTypes;

        let out = self
            .rekognition
            .detect_text()
            .image(Self::image(jpeg))
            .send()
            .await
            .map_err(backend)?;
        let lines: Vec<_> = out
            .text_detections()
            .iter()
            .filter(|t| t.r#type() == Some(&TextTypes::Line))
            .collect();
        Ok(Text {
            lines: lines
                .iter()
                .filter_map(|t| t.detected_text().map(str::to_owned))
                .collect(),
            boxes: lines
                .iter()
                .filter_map(|t| {
                    let b = rek_box(t.geometry().and_then(|g| g.bounding_box()))?;
                    Subject::new(
                        SubjectKind::Text,
                        t.detected_text().unwrap_or("text"),
                        b,
                        t.confidence().unwrap_or(0.0) / 100.0,
                    )
                })
                .collect(),
        })
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

    async fn subjects(&self, jpeg: &[u8]) -> Result<Vec<Subject>> {
        use aws_sdk_bedrockruntime::primitives::Blob;
        use aws_sdk_bedrockruntime::types::{
            ContentBlock, ConversationRole, ImageBlock, ImageFormat, ImageSource,
            InferenceConfiguration, Message,
        };

        let image = ImageBlock::builder()
            .format(ImageFormat::Jpeg)
            .source(ImageSource::Bytes(Blob::new(jpeg.to_vec())))
            .build()
            .map_err(backend)?;
        let message = Message::builder()
            .role(ConversationRole::User)
            .content(ContentBlock::Image(image))
            .content(ContentBlock::Text(SUBJECTS_PROMPT.to_owned()))
            .build()
            .map_err(backend)?;
        let out = self
            .bedrock
            .converse()
            .model_id(&self.model_id)
            .messages(message)
            .inference_config(
                InferenceConfiguration::builder()
                    .temperature(0.0)
                    .max_tokens(600)
                    .build(),
            )
            .send()
            .await
            .map_err(backend)?;
        let text = out
            .output()
            .and_then(|o| o.as_message().ok())
            .and_then(|m| m.content().iter().find_map(|c| c.as_text().ok()))
            .cloned()
            .unwrap_or_default();
        Ok(parse_subjects(&text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_answers_parse_leniently() {
        let a = "Here you go:\n```json\n[{\"label\": \"waterfall\", \"box\": [600, 50, 760, 900]}, {\"label\": \"bad\", \"box\": [1, 2]}, {\"box\": [900, 900, 800, 800]}]\n```";
        let s = parse_subjects(a);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].label, "waterfall");
        assert!((s[0].bbox.left - 0.6).abs() < 1e-6 && (s[0].bbox.height - 0.85).abs() < 1e-6);
        assert_eq!(s[1].label, "subject");
        assert!(
            (s[1].bbox.left - 0.8).abs() < 1e-6,
            "corners given in reverse still read"
        );
        assert!(parse_subjects("I cannot see anything.").is_empty());
        assert!(parse_subjects("[not json]").is_empty());
    }
}
