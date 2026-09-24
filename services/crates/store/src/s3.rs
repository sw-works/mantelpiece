//! S3 backend: presigned multipart uploads and reads. Compile-checked, not yet run
//! against a bucket.

use async_trait::async_trait;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::types::{
    CompletedMultipartUpload, CompletedPart as S3Part, Delete, ObjectIdentifier,
};
use aws_sdk_s3::Client;

use crate::blobs::{Blobs, CompletedPart, PRESIGN_TTL};
use crate::{Result, StorageError};

#[derive(Debug, Clone)]
pub struct S3Blobs {
    client: Client,
    bucket: String,
}

fn backend<E: std::fmt::Debug>(e: E) -> StorageError {
    StorageError::Backend(format!("{e:?}"))
}

fn presign() -> Result<PresigningConfig> {
    PresigningConfig::expires_in(std::time::Duration::from_secs(
        PRESIGN_TTL.whole_seconds().unsigned_abs(),
    ))
    .map_err(backend)
}

impl S3Blobs {
    pub fn from_config(config: &aws_config::SdkConfig, bucket: impl Into<String>) -> Self {
        Self::new(Client::new(config), bucket)
    }

    pub fn new(client: Client, bucket: impl Into<String>) -> Self {
        Self {
            client,
            bucket: bucket.into(),
        }
    }
}

#[async_trait]
impl Blobs for S3Blobs {
    async fn create_multipart(&self, key: &str, content_type: &str) -> Result<String> {
        let out = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .send()
            .await
            .map_err(backend)?;
        out.upload_id
            .ok_or_else(|| StorageError::Backend("no upload id".into()))
    }

    async fn presign_part(&self, key: &str, upload_id: &str, part_number: i32) -> Result<String> {
        let req = self
            .client
            .upload_part()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(upload_id)
            .part_number(part_number)
            .presigned(presign()?)
            .await
            .map_err(backend)?;
        Ok(req.uri().to_string())
    }

    async fn complete_multipart(
        &self,
        key: &str,
        upload_id: &str,
        parts: &[CompletedPart],
    ) -> Result<()> {
        let parts: Vec<S3Part> = parts
            .iter()
            .map(|p| {
                S3Part::builder()
                    .part_number(p.part_number)
                    .e_tag(&p.etag)
                    .build()
            })
            .collect();
        self.client
            .complete_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(upload_id)
            .multipart_upload(
                CompletedMultipartUpload::builder()
                    .set_parts(Some(parts))
                    .build(),
            )
            .send()
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn abort_multipart(&self, key: &str, upload_id: &str) -> Result<()> {
        self.client
            .abort_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(upload_id)
            .send()
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn presign_get(&self, key: &str) -> Result<String> {
        let req = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(presign()?)
            .await
            .map_err(backend)?;
        Ok(req.uri().to_string())
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        let out = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| {
                if e.as_service_error()
                    .is_some_and(aws_sdk_s3::operation::get_object::GetObjectError::is_no_such_key)
                {
                    StorageError::NotFound(key.to_owned())
                } else {
                    backend(e)
                }
            })?;
        let bytes = out.body.collect().await.map_err(backend)?;
        Ok(bytes.into_bytes().to_vec())
    }

    async fn put(&self, key: &str, bytes: Vec<u8>, content_type: &str) -> Result<()> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .body(bytes.into())
            .send()
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn delete(&self, keys: &[String]) -> Result<()> {
        if keys.is_empty() {
            return Ok(());
        }
        let objects: std::result::Result<Vec<ObjectIdentifier>, _> = keys
            .iter()
            .map(|k| ObjectIdentifier::builder().key(k).build())
            .collect();
        let delete = Delete::builder()
            .set_objects(Some(objects.map_err(backend)?))
            .build()
            .map_err(backend)?;
        self.client
            .delete_objects()
            .bucket(&self.bucket)
            .delete(delete)
            .send()
            .await
            .map_err(backend)?;
        Ok(())
    }
}
