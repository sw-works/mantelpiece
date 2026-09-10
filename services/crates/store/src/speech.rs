//! Story narration (F-709, research R10) through Amazon Polly. Behind a household flag
//! until the Vega audio spike passes.

use async_trait::async_trait;
use std::sync::Mutex;

use crate::{Result, StorageError};

#[async_trait]
pub trait Speech: Send + Sync {
    /// MP3 bytes for `text` in `voice`. The caller stores them under a `narration/` key.
    async fn synthesize(&self, text: &str, voice: &str) -> Result<Vec<u8>>;
}

/// Records calls; the "audio" is the text itself so tests can assert on it.
#[derive(Debug, Default)]
pub struct MemorySpeech {
    pub log: Mutex<Vec<String>>,
}

#[async_trait]
impl Speech for MemorySpeech {
    async fn synthesize(&self, text: &str, voice: &str) -> Result<Vec<u8>> {
        self.log
            .lock()
            .unwrap()
            .push(format!("{voice}: {} chars", text.chars().count()));
        Ok(text.as_bytes().to_vec())
    }
}

fn backend<E: std::fmt::Debug>(e: E) -> StorageError {
    StorageError::Backend(format!("{e:?}"))
}

/// Polly neural voices. Compile-checked only.
#[derive(Debug, Clone)]
pub struct PollySpeech {
    client: aws_sdk_polly::Client,
}

impl PollySpeech {
    pub const DEFAULT_VOICE: &'static str = "Joanna";
    /// Polly rejects text over 3000 characters per request.
    pub const MAX_CHARS: usize = 3000;

    pub fn from_config(config: &aws_config::SdkConfig) -> Self {
        Self {
            client: aws_sdk_polly::Client::new(config),
        }
    }
}

#[async_trait]
impl Speech for PollySpeech {
    async fn synthesize(&self, text: &str, voice: &str) -> Result<Vec<u8>> {
        use aws_sdk_polly::types::{Engine, OutputFormat, VoiceId};

        let text: String = text.chars().take(Self::MAX_CHARS).collect();
        let out = self
            .client
            .synthesize_speech()
            .engine(Engine::Neural)
            .output_format(OutputFormat::Mp3)
            .voice_id(VoiceId::from(voice))
            .text(text)
            .send()
            .await
            .map_err(backend)?;
        let bytes = out.audio_stream.collect().await.map_err(backend)?;
        Ok(bytes.into_bytes().to_vec())
    }
}
