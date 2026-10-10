//! Ingest pipeline tasks (architecture section 2, Step Functions "Ingest").
//!
//! One binary, three tasks, dispatched on the `task` field of the state input:
//!
//! - `signal`: per file. Verify checksum, read EXIF, compute pre-model signals, write
//!   renditions. Cheap, runs for every upload.
//! - `organize`: per household. Burst groups and keepers, scores, clock alignment,
//!   trips, days, routes, highlights. Idempotent; runs after every batch.
//! - `enrich`: per keeper. Labels, faces and text through the store's `Vision` trait.
//!   Only keepers pay for models; `organize` copies a keeper's enrichment onto the rest
//!   of its burst.

#![allow(clippy::cast_precision_loss)]

pub mod enrich;
pub mod fieldnote;
pub mod landmarks;
pub mod organize;
pub mod signal;
pub mod sketch;

use mantelpiece_core::ids::{HouseholdId, MediaId, TripId};
use mantelpiece_store::{
    Blobs, Bus, Faces, Geography, Maps, Storage, StorageError, Vectors, Vision,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use time::OffsetDateTime;

#[derive(Debug, Error)]
pub enum IngestError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("bad input: {0}")]
    Input(String),
    #[error("vision: {0}")]
    Vision(String),
}

pub type Result<T> = std::result::Result<T, IngestError>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "task", rename_all = "snake_case")]
pub enum Task {
    /// S3 key of a freshly completed original.
    Signal {
        key: String,
    },
    Enrich {
        household: HouseholdId,
        media_id: MediaId,
    },
    Organize {
        household: HouseholdId,
    },
    /// Make or refresh one highlight's field note (F-711).
    FieldNote {
        household: HouseholdId,
        media_id: MediaId,
    },
    /// Draw or refresh one sketch map (F-712): the trip's when `day` is absent.
    SketchMap {
        household: HouseholdId,
        trip: TripId,
        #[serde(default)]
        day: Option<u32>,
    },
}

pub type Clock = Arc<dyn Fn() -> OffsetDateTime + Send + Sync>;

pub struct Ingest {
    pub storage: Arc<dyn Storage>,
    pub blobs: Arc<dyn Blobs>,
    pub vision: Arc<dyn Vision>,
    pub vectors: Arc<dyn Vectors>,
    pub faces: Arc<dyn Faces>,
    pub maps: Arc<dyn Maps>,
    pub bus: Arc<dyn Bus>,
    /// Named places along routes (F-206); `NoPlaces` turns lookups off.
    pub places: Arc<dyn mantelpiece_store::Places>,
    /// What is known about a landmark (F-210); `NoEncyclopedia` leaves them plain names.
    pub encyclopedia: Arc<dyn mantelpiece_store::Encyclopedia>,
    /// Map data for field notes (F-711).
    pub geography: Arc<dyn Geography>,
    /// Field notes on or off; off skips the manifest and every map fetch.
    pub field_notes: bool,
    /// Clustering thresholds; the defaults are the product rules, tools may relax them.
    pub cluster: mantelpiece_trips::cluster::Params,
    pub clock: Clock,
}

impl std::fmt::Debug for Ingest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ingest").finish_non_exhaustive()
    }
}

impl Ingest {
    pub fn now(&self) -> OffsetDateTime {
        (self.clock)()
    }

    pub async fn run(&self, task: Task) -> Result<serde_json::Value> {
        match task {
            Task::Signal { key } => {
                let out = signal::run(self, &key).await?;
                Ok(serde_json::to_value(out).expect("serialisable"))
            }
            Task::Enrich {
                household,
                media_id,
            } => {
                let out = enrich::run(self, &household, &media_id).await?;
                Ok(serde_json::to_value(out).expect("serialisable"))
            }
            Task::Organize { household } => {
                let out = organize::run(self, &household).await?;
                Ok(serde_json::to_value(out).expect("serialisable"))
            }
            Task::FieldNote {
                household,
                media_id,
            } => {
                let out = fieldnote::run(self, &household, &media_id).await?;
                Ok(serde_json::to_value(out).expect("serialisable"))
            }
            Task::SketchMap {
                household,
                trip,
                day,
            } => {
                let out = sketch::run(self, &household, &trip, day).await?;
                Ok(serde_json::to_value(out).expect("serialisable"))
            }
        }
    }
}

/// Parse `originals/{household}/{contributor}/{media}.{ext}`.
pub fn parse_original_key(key: &str) -> Option<(HouseholdId, MediaId)> {
    let mut parts = key.split('/');
    if parts.next()? != "originals" {
        return None;
    }
    let household = parts.next()?;
    let _contributor = parts.next()?;
    let file = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let media = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    Some((HouseholdId::new(household), MediaId::new(media)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_original_keys() {
        let (h, m) = parse_original_key("originals/h1/c1/m1.jpeg").unwrap();
        assert_eq!((h.as_str(), m.as_str()), ("h1", "m1"));
        assert!(parse_original_key("renditions/h1/m1/thumb.jpg").is_none());
        assert!(parse_original_key("originals/h1/c1").is_none());
    }

    #[test]
    fn task_wire_format() {
        let t: Task = serde_json::from_str(r#"{"task":"organize","household":"h"}"#).unwrap();
        assert_eq!(
            t,
            Task::Organize {
                household: HouseholdId::new("h")
            }
        );
    }
}
