//! Queue a day film (F-714): invoke the film Lambda asynchronously.

use async_trait::async_trait;
use mantelpiece_core::ids::{HouseholdId, TripId};
use mantelpiece_store::{Result, StorageError};
use std::sync::Mutex;

#[async_trait]
pub trait Films: Send + Sync {
    /// Queue a render of one day. Returns without waiting for it.
    async fn queue(&self, household: &HouseholdId, trip: &TripId, day: u32) -> Result<()>;
}

/// Records what was queued.
#[derive(Debug, Default)]
pub struct MemoryFilms {
    pub queued: Mutex<Vec<(HouseholdId, TripId, u32)>>,
}

#[async_trait]
impl Films for MemoryFilms {
    async fn queue(&self, household: &HouseholdId, trip: &TripId, day: u32) -> Result<()> {
        self.queued
            .lock()
            .unwrap()
            .push((household.clone(), trip.clone(), day));
        Ok(())
    }
}

/// No film function configured (`FILM_FUNCTION_NAME` empty): every request is refused.
#[derive(Debug, Default)]
pub struct NoFilms;

#[async_trait]
impl Films for NoFilms {
    async fn queue(&self, _: &HouseholdId, _: &TripId, _: u32) -> Result<()> {
        Err(StorageError::Backend(
            "films are not set up on this stack".into(),
        ))
    }
}

/// Lambda backend: an `Event` (asynchronous) invocation of the film function. Compile-checked
/// only.
#[derive(Debug, Clone)]
pub struct LambdaFilms {
    client: aws_sdk_lambda::Client,
    function: String,
}

impl LambdaFilms {
    pub fn from_config(config: &aws_config::SdkConfig, function: impl Into<String>) -> Self {
        Self {
            client: aws_sdk_lambda::Client::new(config),
            function: function.into(),
        }
    }
}

#[async_trait]
impl Films for LambdaFilms {
    async fn queue(&self, household: &HouseholdId, trip: &TripId, day: u32) -> Result<()> {
        let payload =
            serde_json::json!({ "household": household, "trip": trip, "day": day }).to_string();
        self.client
            .invoke()
            .function_name(&self.function)
            .invocation_type(aws_sdk_lambda::types::InvocationType::Event)
            .payload(aws_sdk_lambda::primitives::Blob::new(payload))
            .send()
            .await
            .map_err(|e| StorageError::Backend(format!("{e:?}")))?;
        Ok(())
    }
}
