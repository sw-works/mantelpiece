//! Object storage behind the API: presigned multipart uploads and reads.

use async_trait::async_trait;
use time::Duration;

use crate::Result;

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
    /// Read a whole object. Originals are capped at 2 GB by the API, renditions are small.
    async fn get(&self, key: &str) -> Result<Vec<u8>>;
    async fn put(&self, key: &str, bytes: Vec<u8>, content_type: &str) -> Result<()>;
    async fn delete(&self, keys: &[String]) -> Result<()>;
}
