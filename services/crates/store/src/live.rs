//! Live from the event: a phone broadcasts, the TV watches. One IVS low-latency channel
//! per broadcast, made on demand and deleted after, so nothing sits idle between events.
//! BASIC channels pass the phone's own encode through (no transcoding), the cheapest input.

use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::{Result, StorageError};

/// A channel ready for one broadcast. `stream_key` goes to the broadcasting phone only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Channel {
    pub arn: String,
    /// Host the broadcast SDK connects to, e.g. `a1b2c3.global-contribute.live-video.net`.
    pub ingest_endpoint: String,
    pub stream_key: String,
    /// HLS URL the TV plays.
    pub playback_url: String,
}

#[async_trait]
pub trait Live: Send + Sync {
    async fn create(&self, name: &str, household: &str) -> Result<Channel>;
    /// Cut the stream if one is running. Fine when nothing is.
    async fn stop(&self, arn: &str) -> Result<()>;
    /// Delete the channel. `false` means IVS refused for now (the stream is still winding
    /// down) and the caller should try again later.
    async fn delete(&self, arn: &str) -> Result<bool>;
}

fn backend<E: std::fmt::Debug>(e: E) -> StorageError {
    StorageError::Backend(format!("{e:?}"))
}

/// Channels in a map. `busy` arns refuse deletion once, the way IVS does right after a stop.
#[derive(Debug, Default)]
pub struct MemoryLive {
    pub channels: Mutex<BTreeMap<String, Channel>>,
    pub stopped: Mutex<Vec<String>>,
    pub busy: Mutex<Vec<String>>,
}

#[async_trait]
impl Live for MemoryLive {
    async fn create(&self, name: &str, _household: &str) -> Result<Channel> {
        let mut channels = self.channels.lock().unwrap();
        let n = channels.len() + 1;
        let channel = Channel {
            arn: format!("arn:aws:ivs:us-west-2:1:channel/{name}-{n}"),
            ingest_endpoint: "ingest.example.live-video.net".into(),
            stream_key: format!("sk_{n}"),
            playback_url: format!("https://play.example.live-video.net/{name}-{n}.m3u8"),
        };
        channels.insert(channel.arn.clone(), channel.clone());
        Ok(channel)
    }

    async fn stop(&self, arn: &str) -> Result<()> {
        self.stopped.lock().unwrap().push(arn.to_owned());
        Ok(())
    }

    async fn delete(&self, arn: &str) -> Result<bool> {
        let mut busy = self.busy.lock().unwrap();
        if let Some(i) = busy.iter().position(|a| a == arn) {
            busy.remove(i);
            return Ok(false);
        }
        self.channels.lock().unwrap().remove(arn);
        Ok(true)
    }
}

#[derive(Debug, Clone)]
pub struct IvsLive {
    client: aws_sdk_ivs::Client,
}

impl IvsLive {
    pub fn from_config(config: &aws_config::SdkConfig) -> Self {
        Self {
            client: aws_sdk_ivs::Client::new(config),
        }
    }
}

#[async_trait]
impl Live for IvsLive {
    async fn create(&self, name: &str, household: &str) -> Result<Channel> {
        use aws_sdk_ivs::types::{ChannelLatencyMode, ChannelType};
        let out = self
            .client
            .create_channel()
            .name(name)
            .latency_mode(ChannelLatencyMode::LowLatency)
            .r#type(ChannelType::BasicChannelType)
            .tags("mantelpiece:household", household)
            .send()
            .await
            .map_err(backend)?;
        let channel = out
            .channel()
            .ok_or_else(|| StorageError::Backend("IVS returned no channel".into()))?;
        let key = out
            .stream_key()
            .and_then(|k| k.value())
            .ok_or_else(|| StorageError::Backend("IVS returned no stream key".into()))?;
        Ok(Channel {
            arn: channel.arn().unwrap_or_default().to_owned(),
            ingest_endpoint: channel.ingest_endpoint().unwrap_or_default().to_owned(),
            stream_key: key.to_owned(),
            playback_url: channel.playback_url().unwrap_or_default().to_owned(),
        })
    }

    async fn stop(&self, arn: &str) -> Result<()> {
        match self.client.stop_stream().channel_arn(arn).send().await {
            Ok(_) => Ok(()),
            Err(e) => {
                let service = e.as_service_error();
                if service.is_some_and(|s| {
                    s.is_channel_not_broadcasting() || s.is_resource_not_found_exception()
                }) {
                    Ok(())
                } else {
                    Err(backend(e))
                }
            }
        }
    }

    async fn delete(&self, arn: &str) -> Result<bool> {
        match self.client.delete_channel().arn(arn).send().await {
            Ok(_) => Ok(true),
            Err(e) => {
                let service = e.as_service_error();
                if service.is_some_and(aws_sdk_ivs::operation::delete_channel::DeleteChannelError::is_resource_not_found_exception) {
                    Ok(true)
                } else if service.is_some_and(aws_sdk_ivs::operation::delete_channel::DeleteChannelError::is_conflict_exception) {
                    Ok(false)
                } else {
                    Err(backend(e))
                }
            }
        }
    }
}
