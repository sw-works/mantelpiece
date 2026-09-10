//! Outbound events. Every event goes to the household feed (storage, for the TV's
//! long-poll) and to EventBridge (for the Concierge's workflows). Shared by the API and
//! the ingest Lambda.

use async_trait::async_trait;
use memorylane_core::events::Event;
use std::sync::Mutex;

use crate::{Result, StorageError};

#[async_trait]
pub trait Bus: Send + Sync {
    async fn publish(&self, event: &Event) -> Result<()>;
}

#[derive(Debug, Default)]
pub struct MemoryBus {
    pub published: Mutex<Vec<Event>>,
}

#[async_trait]
impl Bus for MemoryBus {
    async fn publish(&self, event: &Event) -> Result<()> {
        self.published.lock().unwrap().push(event.clone());
        Ok(())
    }
}

/// EventBridge backend. Compile-checked only.
#[derive(Debug, Clone)]
pub struct EventBridgeBus {
    client: aws_sdk_eventbridge::Client,
    bus_name: String,
}

impl EventBridgeBus {
    pub const SOURCE: &'static str = "memorylane";

    pub fn from_config(config: &aws_config::SdkConfig, bus_name: impl Into<String>) -> Self {
        Self {
            client: aws_sdk_eventbridge::Client::new(config),
            bus_name: bus_name.into(),
        }
    }
}

#[async_trait]
impl Bus for EventBridgeBus {
    async fn publish(&self, event: &Event) -> Result<()> {
        let entry = aws_sdk_eventbridge::types::PutEventsRequestEntry::builder()
            .event_bus_name(&self.bus_name)
            .source(Self::SOURCE)
            .detail_type(event.detail_type())
            .detail(serde_json::to_string(event).expect("event serialises"))
            .build();
        self.client
            .put_events()
            .entries(entry)
            .send()
            .await
            .map_err(|e| StorageError::Backend(format!("{e:?}")))?;
        Ok(())
    }
}
