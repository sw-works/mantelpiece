//! Persistence and model backends behind every Lambda. Every trait has an AWS backend
//! and an in-memory backend, so `make test` runs without credentials (constitution VI):
//! [`Storage`] (DynamoDB), [`Blobs`] (S3), [`Vectors`] (DynamoDB cache and S3 Vectors),
//! [`Faces`] (Rekognition collections), [`Vision`] (Rekognition labels and text, Bedrock
//! descriptions), [`Maps`] (Amazon Location static maps) and [`Speech`] (Polly).

pub mod blobs;
pub mod bus;
pub mod dynamo;
pub mod encyclopedia;
pub mod faces;
pub mod feed;
pub mod mailer;
pub mod maps;
pub mod memory;
pub mod places;
pub mod s3;
pub mod speech;
pub mod storage;
pub mod vectors;
pub mod vision;

pub use blobs::{Blobs, CompletedPart};
pub use bus::Bus;
pub use encyclopedia::Encyclopedia;
pub use faces::Faces;
pub use mailer::Mailer;
pub use maps::Maps;
pub use places::Places;
pub use speech::Speech;
pub use storage::Storage;
use thiserror::Error;
pub use vectors::Vectors;
pub use vision::Vision;

/// Backend failures. Every storage and blob backend maps onto this.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("storage: {0}")]
    Backend(String),
    #[error("serialisation: {0}")]
    Serde(String),
    #[error("not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, StorageError>;
