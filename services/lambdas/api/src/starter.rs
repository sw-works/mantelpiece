//! Kick off the per-household organize state machine when an upload batch completes.

use async_trait::async_trait;
use memorylane_core::ids::HouseholdId;
use memorylane_store::{Result, StorageError};
use std::sync::Mutex;

#[async_trait]
pub trait Starter: Send + Sync {
    async fn start_organize(&self, household: &HouseholdId) -> Result<()>;
}

#[derive(Debug, Default)]
pub struct MemoryStarter {
    pub started: Mutex<Vec<HouseholdId>>,
}

#[async_trait]
impl Starter for MemoryStarter {
    async fn start_organize(&self, household: &HouseholdId) -> Result<()> {
        self.started.lock().unwrap().push(household.clone());
        Ok(())
    }
}

/// Step Functions backend. Compile-checked only.
#[derive(Debug, Clone)]
pub struct SfnStarter {
    client: aws_sdk_sfn::Client,
    state_machine_arn: String,
}

impl SfnStarter {
    pub fn from_config(
        config: &aws_config::SdkConfig,
        state_machine_arn: impl Into<String>,
    ) -> Self {
        Self {
            client: aws_sdk_sfn::Client::new(config),
            state_machine_arn: state_machine_arn.into(),
        }
    }
}

#[async_trait]
impl Starter for SfnStarter {
    async fn start_organize(&self, household: &HouseholdId) -> Result<()> {
        let input = serde_json::json!({ "task": "organize", "household": household }).to_string();
        self.client
            .start_execution()
            .state_machine_arn(&self.state_machine_arn)
            .input(input)
            .send()
            .await
            .map_err(|e| StorageError::Backend(format!("{e:?}")))?;
        Ok(())
    }
}
