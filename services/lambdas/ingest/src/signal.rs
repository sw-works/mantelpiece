//! Per-file task: checksum, EXIF, signals, renditions.

use mantelpiece_core::events::{EventKind, ARRIVAL_MEDIA};
use mantelpiece_core::ids::MediaId;
use mantelpiece_core::media::{MediaItem, MediaKind, ProcessingState};
use mantelpiece_ingest::renditions::{self, Rendition};
use mantelpiece_ingest::{checksum, decode, exif, quality};
use serde::Serialize;

use crate::{parse_original_key, Ingest, IngestError, Result};

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Output {
    pub household: String,
    pub media_id: MediaId,
    pub state: ProcessingState,
    pub reason: Option<String>,
    /// True for videos: the signal state machine runs the poster-frame Lambda next.
    pub poster: bool,
}

pub async fn run(ingest: &Ingest, key: &str) -> Result<Output> {
    let (household, media_id) = parse_original_key(key)
        .ok_or_else(|| IngestError::Input(format!("not an original key: {key}")))?;
    let mut media = ingest
        .storage
        .get_media(&household, &media_id)
        .await?
        .ok_or_else(|| IngestError::Input(format!("no media record for {key}")))?;

    let bytes = ingest.blobs.get(key).await?;
    let sha = checksum::sha256_hex(&bytes);
    if sha != media.sha256 {
        media.state = ProcessingState::Failed;
        ingest.storage.put_media(&media).await?;
        return Ok(Output {
            household: household.to_string(),
            media_id,
            state: ProcessingState::Failed,
            reason: Some("checksum mismatch".into()),
            poster: false,
        });
    }
    media.bytes = bytes.len() as u64;

    let meta = exif::parse(&bytes);
    media.captured = meta.captured;
    media.gps = meta.gps;
    media.orientation = meta.orientation;

    let reason = match media.kind {
        MediaKind::Video => Some("poster frame pending".to_owned()),
        MediaKind::Photo => match decode::decode(&bytes) {
            Ok(img) => {
                let img = renditions::upright(img, media.orientation);
                media.signals = quality::signals(&img);
                for rendition in [Rendition::Thumb, Rendition::Tv] {
                    let jpeg = renditions::encode(&img, rendition)
                        .map_err(|e| IngestError::Input(format!("encode: {e}")))?;
                    ingest
                        .blobs
                        .put(
                            &rendition.key(household.as_str(), media_id.as_str()),
                            jpeg,
                            "image/jpeg",
                        )
                        .await?;
                }
                None
            }
            Err(e) => Some(e.to_string()),
        },
    };

    media.state = if reason.is_some() && media.kind == MediaKind::Photo {
        ProcessingState::Failed
    } else {
        ProcessingState::Signalled
    };
    ingest.storage.put_media(&media).await?;
    if media.state == ProcessingState::Signalled && media.kind == MediaKind::Photo {
        announce_arrival(ingest, &media).await?;
    }
    Ok(Output {
        household: household.to_string(),
        media_id,
        state: media.state,
        reason,
        poster: media.kind == MediaKind::Video,
    })
}

/// Family channel: tell the TV which of this batch's photos can be shown now. One event
/// per photo for the first [`ARRIVAL_MEDIA`], then one per dozen, so a 5,000-photo archive
/// upload does not flood the feed. Signal runs per file in parallel, so two executions may
/// announce the same count; the TV keys on the batch and keeps the larger one.
async fn announce_arrival(ingest: &Ingest, media: &MediaItem) -> Result<()> {
    let mut batch: Vec<MediaItem> = ingest
        .storage
        .list_media_by_contributor(&media.household, &media.contributor)
        .await?
        .into_iter()
        .filter(|m| {
            m.batch == media.batch
                && m.kind == MediaKind::Photo
                && matches!(
                    m.state,
                    ProcessingState::Signalled | ProcessingState::Enriched
                )
        })
        .collect();
    let count = batch.len();
    if count == 0 || (count > ARRIVAL_MEDIA && !count.is_multiple_of(ARRIVAL_MEDIA)) {
        return Ok(());
    }
    batch.sort_by_key(|m| m.uploaded_at);
    let shown: Vec<MediaId> = batch
        .iter()
        .rev()
        .take(ARRIVAL_MEDIA)
        .rev()
        .map(|m| m.id.clone())
        .collect();
    let display_name = ingest
        .storage
        .get_contributor(&media.household, &media.contributor)
        .await?
        .map_or_else(|| "Someone".to_owned(), |c| c.display_name);
    let event = mantelpiece_store::feed::emit(
        ingest.storage.as_ref(),
        &media.household,
        ingest.now(),
        EventKind::PhotosArrived {
            contributor: media.contributor.clone(),
            display_name,
            batch: media.batch.clone(),
            media: shown,
            count: u32::try_from(count).unwrap_or(u32::MAX),
        },
    )
    .await?;
    ingest.bus.publish(&event).await?;
    Ok(())
}
