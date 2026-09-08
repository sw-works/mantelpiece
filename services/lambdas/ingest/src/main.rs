//! Lambda entry point for the ingest tasks. The state machine passes a [`Task`] as input.

use lambda_runtime::{run, service_fn, Error, LambdaEvent};
use memorylane_ingest_lambda::bus::EventBridgeBus;
use memorylane_ingest_lambda::vision::{FixedVision, RekognitionVision, Vision};
use memorylane_ingest_lambda::{Ingest, Task};
use memorylane_store::dynamo::DynamoStorage;
use memorylane_store::s3::S3Blobs;
use std::sync::Arc;
use time::OffsetDateTime;

fn env(name: &str) -> Result<String, Error> {
    std::env::var(name).map_err(|_| Error::from(format!("missing env {name}")))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let aws = aws_config::load_from_env().await;
    let vision: Arc<dyn Vision> = if std::env::var("VISION").is_ok_and(|v| v == "off") {
        Arc::new(FixedVision::default())
    } else {
        Arc::new(RekognitionVision::from_config(&aws))
    };
    let ingest = Arc::new(Ingest {
        storage: Arc::new(DynamoStorage::from_config(&aws, env("TABLE_NAME")?)),
        blobs: Arc::new(S3Blobs::from_config(&aws, env("MEDIA_BUCKET")?)),
        vision,
        bus: Arc::new(EventBridgeBus::from_config(&aws, env("EVENT_BUS")?)),
        clock: Arc::new(OffsetDateTime::now_utc),
    });
    run(service_fn(move |event: LambdaEvent<Task>| {
        let ingest = ingest.clone();
        async move {
            ingest
                .run(event.payload)
                .await
                .map_err(|e| Error::from(e.to_string()))
        }
    }))
    .await
}
