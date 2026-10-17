//! Lambda entry point: `{ "household": "...", "trip": "...", "day": 2 }`, invoked
//! asynchronously by the API or by an end-of-day schedule (F-714).

use lambda_runtime::{run, service_fn, Error, LambdaEvent};
use mantelpiece_film::{FfmpegEncoder, Film, Input};
use mantelpiece_store::dynamo::DynamoStorage;
use mantelpiece_store::s3::S3Blobs;
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
    let film = Arc::new(Film {
        storage: Arc::new(DynamoStorage::from_config(&aws, env("TABLE_NAME")?)),
        blobs: Arc::new(S3Blobs::from_config(&aws, env("MEDIA_BUCKET")?)),
        encoder: Arc::new(FfmpegEncoder::from_env()),
        clock: Arc::new(OffsetDateTime::now_utc),
        frame: mantelpiece_film::FRAME,
        fps: mantelpiece_film::FPS,
    });
    run(service_fn(move |event: LambdaEvent<Input>| {
        let film = film.clone();
        async move {
            mantelpiece_film::run(&film, event.payload)
                .await
                .map_err(|e| Error::from(e.to_string()))
        }
    }))
    .await
}
