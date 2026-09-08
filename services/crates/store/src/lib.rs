//! Persistence behind every Lambda: one [`Storage`] trait and one [`Blobs`] trait, each
//! with an AWS backend and an in-memory backend for tests.

pub mod blobs;
pub mod dynamo;
pub mod memory;
pub mod s3;
pub mod storage;

pub use blobs::{Blobs, CompletedPart};
pub use storage::Storage;
use thiserror::Error;

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
