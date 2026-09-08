//! Object storage behind the API: presigned multipart uploads and reads.

use async_trait::async_trait;
use time::Duration;

use crate::error::StorageError;

pub type Result<T> = std::result::Result<T, StorageError>;

/// One completed multipart part as reported by the browser.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompletedPart {
    pub part_number: i32,
    pub etag: String,
}

pub const PRESIGN_TTL: Duration = Duration::hours(6);

#[async_trait]
pub trait Blobs: Send + Sync {
    async fn create_multipart(&self, key: &str, content_type: &str) -> Result<String>;
    async fn presign_part(&self, key: &str, upload_id: &str, part_number: i32) -> Result<String>;
    async fn complete_multipart(
        &self,
        key: &str,
        upload_id: &str,
        parts: &[CompletedPart],
    ) -> Result<()>;
    async fn abort_multipart(&self, key: &str, upload_id: &str) -> Result<()>;
    async fn presign_get(&self, key: &str) -> Result<String>;
    async fn delete(&self, keys: &[String]) -> Result<()>;
}
