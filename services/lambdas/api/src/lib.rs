//! The backend API behind API Gateway (architecture section 2).
//!
//! Handlers are written against [`Req`] and [`Resp`] plus the `Storage` and `Blobs`
//! traits from `memorylane-store`, so the whole API runs in tests against in-memory backends.
//! `main.rs` adapts Lambda events onto [`Req`] and wires DynamoDB and S3.

pub mod app;
pub mod error;
pub mod handlers;
pub mod req;
pub mod starter;
pub mod tokens;

pub use app::App;
pub use error::ApiError;
pub use req::{Method, Req, Resp};
