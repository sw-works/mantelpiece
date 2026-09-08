//! The backend API behind API Gateway (architecture section 2).
//!
//! Handlers are written against [`Req`] and [`Resp`] plus the [`storage::Storage`] and
//! [`blobs::Blobs`] traits, so the whole API runs in tests against in-memory backends.
//! `main.rs` adapts Lambda events onto [`Req`] and wires DynamoDB and S3.

pub mod app;
pub mod blobs;
pub mod dynamo;
pub mod error;
pub mod handlers;
pub mod memory;
pub mod req;
pub mod s3;
pub mod storage;
pub mod tokens;

pub use app::App;
pub use error::ApiError;
pub use req::{Method, Req, Resp};
