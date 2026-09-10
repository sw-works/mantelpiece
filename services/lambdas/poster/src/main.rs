//! Lambda entry point: `{ "household": "...", "media_id": "..." }` from the signal state
//! machine after a video's signal step.

use lambda_runtime::{run, service_fn, Error, LambdaEvent};
use memorylane_poster::{FfmpegFrames, Input, Poster};
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
    let poster = Arc::new(Poster {
        storage: Arc::new(DynamoStorage::from_config(&aws, env("TABLE_NAME")?)),
        blobs: Arc::new(S3Blobs::from_config(&aws, env("MEDIA_BUCKET")?)),
        frames: Arc::new(FfmpegFrames::from_env()),
        clock: Arc::new(OffsetDateTime::now_utc),
    });
    run(service_fn(move |event: LambdaEvent<Input>| {
        let poster = poster.clone();
        async move {
            memorylane_poster::run(&poster, event.payload)
                .await
                .map_err(|e| Error::from(e.to_string()))
        }
    }))
    .await
}
