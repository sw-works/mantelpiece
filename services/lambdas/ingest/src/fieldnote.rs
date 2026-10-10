//! Per-highlight task: make the field note (F-711). One subject call, then map data and
//! drawing on a blocking thread; the display list and two layers (owner and public form)
//! go beside the TV rendition. The photo and its renditions are never written.

use mantelpiece_core::field_note::{FieldNote, Subject};
use mantelpiece_core::ids::{HouseholdId, MediaId};
use mantelpiece_fieldnotes::geo::TileId;
use mantelpiece_fieldnotes::{fingerprint, make, raster, Input, Tiles};
use mantelpiece_ingest::decode;
use mantelpiece_ingest::renditions::Rendition;
use mantelpiece_store::Geography;
use serde::Serialize;
use std::sync::Arc;

use crate::landmarks::ensure_facts;
use crate::{Ingest, IngestError, Result};

/// Width of the large map in "This place", in pixels.
const MAP_WIDTH: u32 = 1600;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Output {
    pub media_id: MediaId,
    pub made: bool,
    pub skipped: Option<String>,
}

fn skipped(media_id: MediaId, why: &str) -> Output {
    Output {
        media_id,
        made: false,
        skipped: Some(why.to_owned()),
    }
}

/// The store's map data as the drawing crate's tile source.
struct GeoTiles(Arc<dyn Geography>);

impl Tiles for GeoTiles {
    fn vector(&self, id: TileId) -> std::result::Result<Option<Vec<u8>>, String> {
        self.0
            .vector_tile(id.z, id.x, id.y)
            .map_err(|e| e.to_string())
    }
    fn terrain(&self, id: TileId) -> std::result::Result<Option<Vec<u8>>, String> {
        self.0
            .terrain_tile(id.z, id.x, id.y)
            .map_err(|e| e.to_string())
    }
}

/// Delete a note's objects and clear the record, for a photo that should no longer have one.
pub async fn drop_note(
    ingest: &Ingest,
    media: &mut mantelpiece_core::media::MediaItem,
) -> Result<()> {
    if media.field_note.take().is_some() {
        let keys = FieldNote::keys(media.household.as_str(), media.id.as_str()).to_vec();
        ingest.blobs.delete(&keys).await?;
        ingest.storage.put_media(media).await?;
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
pub async fn run(ingest: &Ingest, household: &HouseholdId, media_id: &MediaId) -> Result<Output> {
    let mut media = ingest
        .storage
        .get_media(household, media_id)
        .await?
        .ok_or_else(|| IngestError::Input(format!("no media {media_id}")))?;
    let Some(trip_id) = media.trip.clone() else {
        drop_note(ingest, &mut media).await?;
        return Ok(skipped(media.id, "not a highlight"));
    };
    let trip = ingest
        .storage
        .get_trip(household, &trip_id)
        .await?
        .ok_or_else(|| IngestError::Input(format!("no trip {trip_id}")))?;
    if !trip.highlights.contains(&media.id) || media.is_removed() {
        drop_note(ingest, &mut media).await?;
        return Ok(skipped(media.id, "not a highlight"));
    }
    let home = ingest
        .storage
        .get_household(household)
        .await?
        .and_then(|h| h.home);
    let print = fingerprint(&media, &trip, home);
    if media
        .field_note
        .as_ref()
        .is_some_and(|n| n.fingerprint == print)
    {
        return Ok(skipped(media.id, "up to date"));
    }

    let jpeg = ingest
        .blobs
        .get(&Rendition::Tv.key(household.as_str(), media_id.as_str()))
        .await?;
    // Scenery the Rekognition boxes miss; a failed call leaves the boxes we have.
    let mut subjects: Vec<Subject> = media.enrichment.subjects.clone();
    // The landmark the trip tied this photo to, so the model boxes it when it is in view.
    let hint = trip
        .days
        .iter()
        .flat_map(|d| &d.landmarks)
        .find(|l| l.media.contains(&media.id))
        .map(|l| l.name.clone());
    match ingest.vision.subjects(&jpeg, hint.as_deref()).await {
        Ok(found) => subjects.extend(found),
        Err(e) => tracing::warn!(error = %e, media = %media_id, "subject call failed"),
    }

    let tiles = GeoTiles(ingest.geography.clone());
    let (m, t) = (media.clone(), trip.clone());
    let made = tokio::task::spawn_blocking(move || {
        let photo =
            decode::decode(&jpeg).map_err(|e| IngestError::Input(format!("rendition: {e}")))?;
        let (w, h) = (photo.width(), photo.height());
        let input = Input {
            media: &m,
            trip: &t,
            home,
            photo: &photo,
            subjects: &subjects,
            heading: m.heading,
        };
        Ok::<_, IngestError>(make(&input, &tiles).map(|made| {
            let public_drawing = made.drawing.public(made.width_m);
            let owner = raster::png(&raster::layer(&made.drawing, made.placement, w, h));
            let public = raster::png(&raster::layer(&public_drawing, made.placement, w, h));
            let maps = (
                raster::png(&raster::map_only(&made.drawing, MAP_WIDTH)),
                raster::png(&raster::map_only(&public_drawing, MAP_WIDTH)),
            );
            (made, owner, public, maps)
        }))
    })
    .await
    .map_err(|e| IngestError::Input(format!("field note task: {e}")))??;

    let (made, owner, public, (map, public_map)) = match made {
        Ok(m) => m,
        Err(skip) => {
            // A stale note for a place that is now unknown or unmappable goes.
            drop_note(ingest, &mut media).await?;
            return Ok(skipped(media.id, skip.reason()));
        }
    };
    let (h, id) = (household.as_str(), media_id.as_str());
    let drawing =
        serde_json::to_vec(&made.drawing).map_err(|e| IngestError::Input(e.to_string()))?;
    ingest
        .blobs
        .put(&FieldNote::drawing_key(h, id), drawing, "application/json")
        .await?;
    ingest
        .blobs
        .put(&FieldNote::layer_key(h, id), owner, "image/png")
        .await?;
    ingest
        .blobs
        .put(&FieldNote::public_layer_key(h, id), public, "image/png")
        .await?;
    ingest
        .blobs
        .put(&FieldNote::map_key(h, id), map, "image/png")
        .await?;
    ingest
        .blobs
        .put(&FieldNote::public_map_key(h, id), public_map, "image/png")
        .await?;
    // A share made from the old note no longer matches it.
    ingest.blobs.delete(&[FieldNote::full_key(h, id)]).await?;
    if let Some(f) = &made.feature {
        // Warm the encyclopedia cache so "This place" opens with the description at once.
        let _ = ensure_facts(ingest, &f.name, Some(f.at)).await;
    }
    media.field_note = Some(FieldNote {
        placement: made.placement,
        mode: made.mode,
        certainty: made.certainty,
        ink: made.ink,
        caption: made.caption,
        source: "openstreetmap".into(),
        credit: made.credit,
        drawing_key: FieldNote::drawing_key(h, id),
        fingerprint: print,
        made_at: ingest.now(),
        feature: made.feature,
    });
    ingest.storage.put_media(&media).await?;
    Ok(Output {
        media_id: media.id,
        made: true,
        skipped: None,
    })
}
