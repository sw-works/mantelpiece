//! Lambda entry point for the ingest tasks. The state machine passes a [`Task`] as input.

use lambda_runtime::{run, service_fn, Error, LambdaEvent};
use mantelpiece_ingest_lambda::{Ingest, Task};
use mantelpiece_store::bus::EventBridgeBus;
use mantelpiece_store::dynamo::DynamoStorage;
use mantelpiece_store::faces::RekognitionFaces;
use mantelpiece_store::maps::LocationMaps;
use mantelpiece_store::s3::S3Blobs;
use mantelpiece_store::vectors::DynamoVectors;
use mantelpiece_store::vision::{AwsVision, MemoryVision};
use mantelpiece_store::Vision;
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
            Ok("wikipedia") => Arc::new(mantelpiece_store::places::WikipediaPlaces::default()),
            Ok("none") => Arc::new(mantelpiece_store::places::NoPlaces),
            _ => Arc::new(mantelpiece_store::places::AwsPlaces::from_config(&aws)),
        },
        encyclopedia: match std::env::var("ENCYCLOPEDIA").as_deref() {
            Ok("none") => Arc::new(mantelpiece_store::encyclopedia::NoEncyclopedia),
            _ => Arc::new(mantelpiece_store::encyclopedia::Wikipedia::default()),
        },
        geography: Arc::new(mantelpiece_store::geography::OpenGeography::default()),
        field_notes: !std::env::var("FIELD_NOTES").is_ok_and(|v| v == "off"),
        cluster: mantelpiece_trips::cluster::Params::default(),
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
