//! Per-map task: draw a trip's sketch map or one day's (F-712). Runs in the SketchMaps
//! Distributed Map after field notes; one item per map, so a cold-tile draw stays well
//! inside the Lambda's time and parallel items never write the trip record. A map is up to
//! date when the fingerprint marker beside its PNGs matches.

use mantelpiece_core::ids::{HouseholdId, TripId};
use mantelpiece_core::sketch::{SketchKeys, Which};
use mantelpiece_fieldnotes::sketch;
use serde::Serialize;

use crate::fieldnote::GeoTiles;
use crate::{Ingest, IngestError, Result};

/// Days past the end of a trip whose maps a trip item clears, after days were merged away.
const STALE_DAYS: u32 = 10;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Output {
    pub trip: TripId,
    pub day: Option<u32>,
    pub made: bool,
    pub skipped: Option<String>,
}

pub async fn run(
    ingest: &Ingest,
    household: &HouseholdId,
    trip_id: &TripId,
    day: Option<u32>,
) -> Result<Output> {
    let out = |made: bool, why: Option<&str>| Output {
        trip: trip_id.clone(),
        day,
        made,
        skipped: why.map(str::to_owned),
    };
    let which = day.map_or(Which::Trip, Which::Day);
    let keys = SketchKeys::new(household, trip_id);
    let Some(trip) = ingest.storage.get_trip(household, trip_id).await? else {
        return Ok(out(false, Some("no trip")));
    };
    if let Which::Day(n) = which {
        if !trip.days.iter().any(|d| d.index == n) {
            ingest.blobs.delete(&keys.of(which)).await?;
            return Ok(out(false, Some("no such day")));
        }
    } else {
        #[allow(clippy::cast_possible_truncation)]
        let days = trip.days.len() as u32;
        let stale: Vec<String> = (days + 1..=days + STALE_DAYS)
            .flat_map(|n| keys.of(Which::Day(n)))
            .collect();
        ingest.blobs.delete(&stale).await?;
    }
    let home = ingest
        .storage
        .get_household(household)
        .await?
        .and_then(|h| h.home);
    let print = sketch::fingerprint(&trip, which, home);
    let current = ingest.blobs.get(&keys.marker(which)).await.ok();
    if current
        .as_deref()
        .map(SketchKeys::read_marker)
        .is_some_and(|(f, _)| f == print)
    {
        return Ok(out(false, Some("up to date")));
    }
    let tiles = GeoTiles(ingest.geography.clone());
    let drawn = tokio::task::spawn_blocking(move || sketch::draw(&trip, which, home, &tiles))
        .await
        .map_err(|e| IngestError::Input(format!("sketch task: {e}")))?;
    match drawn {
        Ok(s) => {
            ingest
                .blobs
                .put(&keys.png(which), s.owner, "image/png")
                .await?;
            ingest
                .blobs
                .put(&keys.public_png(which), s.public, "image/png")
                .await?;
            ingest
                .blobs
                .put(
                    &keys.marker(which),
                    SketchKeys::marker_body(&s.fingerprint, &s.credit),
                    "text/plain",
                )
                .await?;
            Ok(out(true, None))
        }
        Err(mantelpiece_fieldnotes::Skip::MapUnavailable) => Err(IngestError::Input(
            "map data unavailable; the step retries".into(),
        )),
        Err(skip) => {
            // A day that lost its places loses its map.
            ingest.blobs.delete(&keys.of(which)).await?;
            Ok(out(false, Some(skip.reason())))
        }
    }
}
