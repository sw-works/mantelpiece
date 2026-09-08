//! Per-file task: checksum, EXIF, signals, renditions.

use memorylane_core::ids::MediaId;
use memorylane_core::media::{MediaKind, ProcessingState};
use memorylane_ingest::renditions::{self, Rendition};
use memorylane_ingest::{checksum, decode, exif, quality};
use serde::Serialize;

use crate::{parse_original_key, Ingest, IngestError, Result};

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Output {
    pub household: String,
    pub media_id: MediaId,
    pub state: ProcessingState,
    pub reason: Option<String>,
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
        });
    }
    media.bytes = bytes.len() as u64;

    let meta = exif::parse(&bytes);
    media.captured = meta.captured;
    media.gps = meta.gps;
    media.orientation = meta.orientation;

    let reason = match media.kind {
        MediaKind::Video => Some("video poster frames need the ffmpeg layer".to_owned()),
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
    Ok(Output {
        household: household.to_string(),
        media_id,
        state: media.state,
        reason,
    })
}
