use mantelpiece_store::StorageError;
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("{status}: {message}")]
pub struct ApiError {
    pub status: u16,
    pub message: String,
}

impl ApiError {
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
    pub fn bad_request(m: impl Into<String>) -> Self {
        Self::new(400, m)
    }
    pub fn unauthorized(m: impl Into<String>) -> Self {
        Self::new(401, m)
    }
    pub fn forbidden(m: impl Into<String>) -> Self {
        Self::new(403, m)
    }
    pub fn not_found(m: impl Into<String>) -> Self {
        Self::new(404, m)
    }
    pub fn conflict(m: impl Into<String>) -> Self {
        Self::new(409, m)
    }
    pub fn gone(m: impl Into<String>) -> Self {
        Self::new(410, m)
    }
    pub fn internal(m: impl Into<String>) -> Self {
        Self::new(500, m)
    }

    pub fn body(&self) -> serde_json::Value {
        json!({ "error": self.message })
    }
}

impl From<StorageError> for ApiError {
    fn from(e: StorageError) -> Self {
        match e {
            StorageError::NotFound(m) => Self::not_found(m),
            other => Self::internal(other.to_string()),
        }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        Self::bad_request(format!("invalid json: {e}"))
    }
}
