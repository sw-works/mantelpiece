//! Lambda entry point for the ingest tasks. The state machine passes a [`Task`] as input.

use lambda_runtime::{run, service_fn, Error, LambdaEvent};
use memorylane_ingest_lambda::{Ingest, Task};
use memorylane_store::bus::EventBridgeBus;
use memorylane_store::dynamo::DynamoStorage;
use memorylane_store::faces::RekognitionFaces;
use memorylane_store::maps::LocationMaps;
use memorylane_store::s3::S3Blobs;
use memorylane_store::vectors::DynamoVectors;
use memorylane_store::vision::{AwsVision, MemoryVision};
use memorylane_store::Vision;
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
        Arc::new(MemoryVision::default())
    } else {
        Arc::new(AwsVision::from_config(
            &aws,
            std::env::var("DESCRIBE_MODEL_ID").ok(),
        ))
    };
    let ingest = Arc::new(Ingest {
        storage: Arc::new(DynamoStorage::from_config(&aws, env("TABLE_NAME")?)),
        blobs: Arc::new(S3Blobs::from_config(&aws, env("MEDIA_BUCKET")?)),
        vision,
        vectors: Arc::new(DynamoVectors::from_config(&aws, env("TABLE_NAME")?)),
        faces: Arc::new(RekognitionFaces::from_config(&aws)),
        maps: Arc::new(LocationMaps::from_config(&aws)),
        bus: Arc::new(EventBridgeBus::from_config(&aws, env("EVENT_BUS")?)),
        places: match std::env::var("PLACES_PROVIDER").as_deref() {
            Ok("wikipedia") => Arc::new(memorylane_store::places::WikipediaPlaces::default()),
            Ok("none") => Arc::new(memorylane_store::places::NoPlaces),
            _ => Arc::new(memorylane_store::places::AwsPlaces::from_config(&aws)),
        },
        encyclopedia: match std::env::var("ENCYCLOPEDIA").as_deref() {
            Ok("none") => Arc::new(memorylane_store::encyclopedia::NoEncyclopedia),
            _ => Arc::new(memorylane_store::encyclopedia::Wikipedia::default()),
        },
        cluster: memorylane_trips::cluster::Params::default(),
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
