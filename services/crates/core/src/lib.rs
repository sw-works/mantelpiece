//! Domain types and pure logic shared by every MemoryLane service.
//!
//! Nothing in this crate talks to AWS. Storage, HTTP and model calls live in the
//! Lambda crates, which depend on this one.

pub mod codes;
pub mod confirmation;
pub mod contributor;
pub mod device_code;
pub mod events;
pub mod geo;
pub mod ids;
pub mod media;
pub mod trip;
pub mod trip_code;

pub use time::OffsetDateTime;
