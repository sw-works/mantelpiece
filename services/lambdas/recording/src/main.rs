//! Lambda entry point: `{ "key": ".../events/recording-ended.json" }` from the EventBridge
//! rule on the recordings bucket.

use lambda_runtime::{run, service_fn, Error, LambdaEvent};
use mantelpiece_recording::{FfmpegRemux, Input, Recording, SfnOrganizer, MAX_BYTES};
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
    let rec = Arc::new(Recording {
        storage: Arc::new(DynamoStorage::from_config(&aws, env("TABLE_NAME")?)),
        media: Arc::new(S3Blobs::from_config(&aws, env("MEDIA_BUCKET")?)),
        recordings: Arc::new(S3Blobs::from_config(&aws, env("RECORDINGS_BUCKET")?)),
        remux: Arc::new(FfmpegRemux::from_env()),
        organizer: Arc::new(SfnOrganizer {
            client: aws_sdk_sfn::Client::new(&aws),
            state_machine_arn: env("ORGANIZE_STATE_MACHINE_ARN")?,
        }),
        work_dir: std::env::temp_dir(),
        max_bytes: MAX_BYTES,
        clock: Arc::new(OffsetDateTime::now_utc),
    });
    run(service_fn(move |event: LambdaEvent<Input>| {
        let rec = rec.clone();
        async move {
            mantelpiece_recording::run(&rec, event.payload)
                .await
                .map_err(|e| Error::from(e.to_string()))
        }
    }))
    .await
}
