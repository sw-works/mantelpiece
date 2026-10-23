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
    /// `hint` names the landmark the photo is tied to, when known.
    async fn subjects(&self, jpeg: &[u8], hint: Option<&str>) -> Result<Vec<Subject>>;
}

/// Text found in a photo: the lines, and where they are.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Text {
    pub lines: Vec<String>,
    pub boxes: Vec<Subject>,
}

/// The question put to the model for subject boxes. Landscape subjects are named because
/// without them Nova Lite answers `[]` for sea stacks, cliffs and geysers (decision 37).
pub const SUBJECTS_PROMPT: &str = "Find the parts of this travel photo that a viewer looks \
at: people, animals, vehicles, buildings, monuments, statues, and landscape subjects such as \
rock formations, sea stacks, cliffs, peaks, waterfalls, geysers and steam, icebergs and trees. \
Box each one tightly; a large landform may get one big box. Never box sky, clouds, plain \
water, plain ground or empty background.";

/// The answer format, after the optional landmark hint.
const SUBJECTS_FORMAT: &str = "Answer with JSON only, an array of objects {\"label\": \
string, \"box\": [x1, y1, x2, y2]}, one per subject, coordinates as integers on a 0 to 1000 \
scale from the top left. Answer [] if nothing stands out.";

/// The whole subjects question, with the landmark the photo is tied to when known.
#[must_use]
pub fn subjects_prompt(hint: Option<&str>) -> String {
    match hint {
        Some(h) => format!(
            "{SUBJECTS_PROMPT} The photo was taken near {h}; if it is in view, box it. {SUBJECTS_FORMAT}"
        ),
        None => format!("{SUBJECTS_PROMPT} {SUBJECTS_FORMAT}"),
    }
}

/// Labels that name background, which the prompt asks the model to leave out but it
/// sometimes boxes anyway.
const BACKGROUND: &[&str] = &[
    "sky",
    "cloud",
    "clouds",
    "water",
    "sea",
    "ocean",
    "lake",
    "river",
    "grass",
    "grassland",
    "ground",
    "field",
    "background",
    "horizon",
];

/// Parse the model's answer leniently. Nova sometimes closes a box with `]]` or opens the
/// next object with `[`, so each `"label"` is read with the four numbers after it rather
/// than parsing the answer as JSON. Boxes on the 0 to 1000 grid; background labels and
/// anything short of four numbers are skipped.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn parse_subjects(answer: &str) -> Vec<Subject> {
    let parts: Vec<&str> = answer.split("\"label\"").collect();
    parts
        .iter()
        .skip(1)
        .filter_map(|part| {
            let rest = part.trim_start().strip_prefix(':')?.trim_start();
            let rest = rest.strip_prefix('"')?;
            let end = rest.find('"')?;
            let label = rest[..end].trim();
            if BACKGROUND.contains(&label.to_lowercase().as_str()) {
                return None;
            }
            let after = &rest[end + 1..];
            let open = after.find('[')?;
            let close = open + after[open..].find(']')?;
            let n: Vec<f32> = after[open + 1..close]
                .split(|c: char| !(c.is_ascii_digit() || c == '.'))
                .filter(|t| !t.is_empty())
                .take(4)
                .filter_map(|t| t.parse::<f32>().ok())
                .map(|f| f / 1000.0)
                .collect();
            let [x1, y1, x2, y2] = n[..] else { return None };
            Subject::new(
                SubjectKind::Landscape,
                if label.is_empty() { "subject" } else { label },
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
    async fn subjects(&self, _jpeg: &[u8], hint: Option<&str>) -> Result<Vec<Subject>> {
        self.prompts.lock().unwrap().push(subjects_prompt(hint));
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

    async fn subjects(&self, jpeg: &[u8], hint: Option<&str>) -> Result<Vec<Subject>> {
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
            .content(ContentBlock::Text(subjects_prompt(hint)))
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
        let a = "Here you go:\n```json\n[{\"label\": \"waterfall\", \"box\": [600, 50, 760, 900]}, {\"label\": \"bad\", \"box\": [1, 2]}, {\"label\": \"rock\", \"box\": [900, 900, 800, 800]}]\n```";
        let s = parse_subjects(a);
        assert_eq!(s.len(), 2, "a box short of four numbers is skipped");
        assert_eq!(s[0].label, "waterfall");
        assert!((s[0].bbox.left - 0.6).abs() < 1e-6 && (s[0].bbox.height - 0.85).abs() < 1e-6);
        assert!(
            (s[1].bbox.left - 0.8).abs() < 1e-6,
            "corners given in reverse still read"
        );
        assert_eq!(
            parse_subjects("I cannot see anything."),
            Vec::<Subject>::new()
        );
        assert_eq!(parse_subjects("[not json]"), Vec::<Subject>::new());
    }

    #[test]
    fn nova_answers_with_broken_brackets_still_give_their_boxes() {
        // Seen from Nova Lite on the Deildartunguhver photo, 2026-10-09.
        let a = r#"[{"label": "waterfall", "box": [0, 580, 749, 1000]], ["label": "flowers", [590, 290, 930, 410]]]"#;
        let s = parse_subjects(a);
        assert_eq!(s.len(), 2);
        assert_eq!(s[1].label, "flowers");
        assert!((s[1].bbox.top - 0.29).abs() < 1e-6);
        let b = r#"[ {"label": "person", "box": [500, 600, 620, 900] ]}"#;
        assert_eq!(parse_subjects(b).len(), 1);
    }

    #[test]
    fn background_labels_are_not_subjects() {
        let a = r#"[{"label": "Sky", "box": [0, 0, 1000, 200]}, {"label": "geyser", "box": [0, 520, 1000, 1000]}]"#;
        let s = parse_subjects(a);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].label, "geyser");
    }

    #[test]
    fn the_prompt_names_the_landmark_when_known() {
        assert!(subjects_prompt(Some("Dettifoss")).contains("near Dettifoss"));
        assert!(!subjects_prompt(None).contains("near"));
    }
}
