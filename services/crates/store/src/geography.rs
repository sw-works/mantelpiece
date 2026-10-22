//! Map data for field notes (F-711, research R1, R2): OpenStreetMap vector tiles from
//! OpenFreeMap and Terrarium elevation tiles from AWS Open Data. Both may be kept as
//! drawings with credit, which Amazon Location's terms do not allow. Blocking calls: the
//! caller runs them on a blocking thread.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::{Result, StorageError};

/// One tile of map data. `Ok(None)` is a tile that does not exist (open sea, outside
/// coverage); `Err` is a failure worth retrying later.
pub trait Geography: Send + Sync {
    fn vector_tile(&self, z: u8, x: u32, y: u32) -> Result<Option<Vec<u8>>>;
    fn terrain_tile(&self, z: u8, x: u32, y: u32) -> Result<Option<Vec<u8>>>;
}

/// Tiles from fixtures keyed `vector/z/x/y` or `terrain/z/x/y`; missing keys are `None`.
#[derive(Debug, Default)]
pub struct MemoryGeography {
    pub tiles: HashMap<String, Vec<u8>>,
    pub fetched: Mutex<Vec<String>>,
    /// Every call fails, for the "map data unavailable" path.
    pub failing: bool,
}

impl MemoryGeography {
    fn get(&self, kind: &str, z: u8, x: u32, y: u32) -> Result<Option<Vec<u8>>> {
        let key = format!("{kind}/{z}/{x}/{y}");
        self.fetched.lock().unwrap().push(key.clone());
        if self.failing {
            return Err(StorageError::Backend("map data unavailable".into()));
        }
        Ok(self.tiles.get(&key).cloned())
    }
}

impl Geography for MemoryGeography {
    fn vector_tile(&self, z: u8, x: u32, y: u32) -> Result<Option<Vec<u8>>> {
        self.get("vector", z, x, y)
    }
    fn terrain_tile(&self, z: u8, x: u32, y: u32) -> Result<Option<Vec<u8>>> {
        self.get("terrain", z, x, y)
    }
}

/// OpenFreeMap (no key) and AWS Terrain Tiles over HTTPS. Compile-checked; exercised by
/// the field-notes spike, not by `make test`.
#[derive(Debug)]
pub struct OpenGeography {
    tilejson: String,
    terrain: String,
    user_agent: String,
    /// Vector tile URL template from the TileJSON, read once: it names the current build.
    template: OnceLock<String>,
}

impl Default for OpenGeography {
    fn default() -> Self {
        Self {
            tilejson: "https://tiles.openfreemap.org/planet".into(),
            terrain: "https://s3.amazonaws.com/elevation-tiles-prod/terrarium".into(),
            user_agent: "Mantelpiece field notes (https://github.com/sw-works/mantelpiece)".into(),
            template: OnceLock::new(),
        }
    }
}

fn backend<E: std::fmt::Display>(e: E) -> StorageError {
    StorageError::Backend(e.to_string())
}

impl OpenGeography {
    fn fetch(&self, url: &str) -> Result<Option<Vec<u8>>> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(10)))
            .build()
            .into();
        match agent.get(url).header("User-Agent", &self.user_agent).call() {
            Ok(mut r) => r
                .body_mut()
                .with_config()
                .limit(10 << 20)
                .read_to_vec()
                .map(Some)
                .map_err(backend),
            Err(ureq::Error::StatusCode(404 | 204)) => Ok(None),
            Err(e) => Err(backend(e)),
        }
    }

    fn template(&self) -> Result<&str> {
        if let Some(t) = self.template.get() {
            return Ok(t);
        }
        let body = self
            .fetch(&self.tilejson)?
            .ok_or_else(|| StorageError::Backend("no TileJSON".into()))?;
        let json: serde_json::Value = serde_json::from_slice(&body).map_err(backend)?;
        let t = json["tiles"][0]
            .as_str()
            .ok_or_else(|| StorageError::Backend("TileJSON has no tiles".into()))?
            .to_owned();
        Ok(self.template.get_or_init(|| t))
    }
}

impl Geography for OpenGeography {
    fn vector_tile(&self, z: u8, x: u32, y: u32) -> Result<Option<Vec<u8>>> {
        let url = self
            .template()?
            .replace("{z}", &z.to_string())
            .replace("{x}", &x.to_string())
            .replace("{y}", &y.to_string());
        self.fetch(&url)
    }
    fn terrain_tile(&self, z: u8, x: u32, y: u32) -> Result<Option<Vec<u8>>> {
        self.fetch(&format!("{}/{z}/{x}/{y}.png", self.terrain))
    }
}
