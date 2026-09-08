//! Per-keeper task: run the vision models on the TV rendition.

use memorylane_core::ids::{HouseholdId, MediaId};
use memorylane_core::media::ProcessingState;
use memorylane_ingest::renditions::Rendition;
use serde::Serialize;

use crate::{Ingest, IngestError, Result};

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Output {
    pub media_id: MediaId,
    pub state: ProcessingState,
    pub skipped: Option<String>,
}

pub async fn run(ingest: &Ingest, household: &HouseholdId, media_id: &MediaId) -> Result<Output> {
    let mut media = ingest
        .storage
        .get_media(household, media_id)
        .await?
        .ok_or_else(|| IngestError::Input(format!("no media {media_id}")))?;
    if media.state == ProcessingState::Enriched {
        return Ok(Output {
            media_id: media.id,
            state: media.state,
            skipped: Some("already enriched".into()),
        });
    }
    if !media.is_keeper {
        return Ok(Output {
            media_id: media.id,
            state: media.state,
            skipped: Some("not a keeper".into()),
        });
    }
    let key = Rendition::Tv.key(household.as_str(), media_id.as_str());
    let jpeg = ingest.blobs.get(&key).await?;
    media.enrichment = ingest.vision.analyze(&jpeg).await?;
    media.enrichment.inherited_from = None;
    media.state = ProcessingState::Enriched;
    ingest.storage.put_media(&media).await?;
    Ok(Output {
        media_id: media.id,
        state: media.state,
        skipped: None,
    })
}
