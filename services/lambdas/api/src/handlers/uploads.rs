//! Resumable multipart uploads from the phone browser (F-101, F-103, F-106).

use memorylane_core::events::EventKind;
use memorylane_core::ids::{BatchId, ContributorId, MediaId};
use memorylane_core::media::{Enrichment, MediaItem, MediaKind, ProcessingState, Signals};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::app::{App, Caller};
use crate::blobs::CompletedPart;
use crate::error::ApiError;
use crate::req::{Req, Resp};
use crate::tokens::Session;

#[derive(Deserialize)]
struct InitBody {
    filename: String,
    bytes: u64,
    sha256: String,
    content_type: Option<String>,
    /// Client-side batch grouping; one Step Functions execution per batch.
    batch_id: Option<String>,
}

#[derive(Serialize)]
struct Initiated {
    media_id: MediaId,
    upload_id: String,
    key: String,
    part_size: u64,
    part_urls: Vec<String>,
}

/// The contributor id an upload is credited to.
fn contributor_of(caller: &Caller) -> Result<ContributorId, ApiError> {
    match caller {
        Caller::Owner { household, .. } => Ok(ContributorId::new(household.as_str())),
        Caller::Contributor(Session::Upload { contributor, .. }) => Ok(contributor.clone()),
        Caller::Contributor(Session::Tv { .. }) => {
            Err(ApiError::forbidden("TV sessions cannot upload"))
        }
    }
}

pub async fn init(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let caller = app.caller(req)?;
    let household = caller.household().clone();
    let contributor = contributor_of(&caller)?;
    if let Some(c) = app
        .storage
        .get_contributor(&household, &contributor)
        .await?
    {
        if !c.is_active() {
            return Err(ApiError::forbidden("removed from this household"));
        }
    }
    if let Caller::Contributor(Session::Upload { code, .. }) = &caller {
        let tc = app
            .storage
            .get_trip_code(code)
            .await?
            .ok_or_else(|| ApiError::gone("code revoked"))?;
        if !tc.accepts_uploads(app.now()) {
            return Err(ApiError::gone("this code no longer accepts uploads"));
        }
    }

    let body: InitBody = req.json()?;
    if body.bytes == 0 || body.bytes > app.config.max_file_bytes {
        return Err(ApiError::bad_request("file size out of range"));
    }
    if body.sha256.len() != 64 || !body.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ApiError::bad_request("sha256 must be 64 hex characters"));
    }
    let sha256 = body.sha256.to_ascii_lowercase();
    if let Some(existing) = app
        .storage
        .find_media_by_sha256(&household, &sha256)
        .await?
    {
        return Err(ApiError::new(409, format!("duplicate of {}", existing.id)));
    }

    let id = MediaId::random();
    let key = MediaItem::original_key(&household, &contributor, &id, &body.filename);
    let content_type = body
        .content_type
        .unwrap_or_else(|| "application/octet-stream".into());
    let upload_id = app.blobs.create_multipart(&key, &content_type).await?;
    let parts = body.bytes.div_ceil(app.config.part_size);
    let parts = i32::try_from(parts).map_err(|_| ApiError::bad_request("too many parts"))?;
    let mut part_urls = Vec::with_capacity(usize::try_from(parts).unwrap_or_default());
    for n in 1..=parts {
        part_urls.push(app.blobs.presign_part(&key, &upload_id, n).await?);
    }

    let media = MediaItem {
        id: id.clone(),
        household,
        contributor,
        batch: body.batch_id.map_or_else(BatchId::random, BatchId::new),
        kind: MediaKind::from_filename(&body.filename),
        original_key: key.clone(),
        original_filename: body.filename,
        bytes: body.bytes,
        sha256,
        captured: None,
        gps: None,
        orientation: 1,
        signals: Signals::default(),
        enrichment: Enrichment::default(),
        quality_score: None,
        burst_group: None,
        is_keeper: false,
        trip: None,
        pinned: false,
        hidden: false,
        state: ProcessingState::Uploaded,
        uploaded_at: app.now(),
    };
    app.storage.put_media(&media).await?;
    Resp::created(Initiated {
        media_id: id,
        upload_id,
        key,
        part_size: app.config.part_size,
        part_urls,
    })
}

#[derive(Deserialize)]
struct CompleteBody {
    media_id: MediaId,
    upload_id: String,
    parts: Vec<CompletedPart>,
    /// Progress for the TV ticker (F-102): how many files of this batch are done.
    batch_done: Option<u32>,
    batch_total: Option<u32>,
}

async fn owned_media(app: &App, req: &Req, id: &MediaId) -> Result<(Caller, MediaItem), ApiError> {
    let caller = app.caller(req)?;
    let media = app
        .storage
        .get_media(caller.household(), id)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown media"))?;
    if let Caller::Contributor(Session::Upload { contributor, .. }) = &caller {
        if &media.contributor != contributor {
            return Err(ApiError::forbidden("not your upload"));
        }
    }
    Ok((caller, media))
}

pub async fn complete(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let body: CompleteBody = req.json()?;
    let (caller, mut media) = owned_media(app, req, &body.media_id).await?;
    if body.parts.is_empty() {
        return Err(ApiError::bad_request("no parts"));
    }
    app.blobs
        .complete_multipart(&media.original_key, &body.upload_id, &body.parts)
        .await?;
    media.uploaded_at = app.now();
    app.storage.put_media(&media).await?;

    if let (Some(done), Some(total)) = (body.batch_done, body.batch_total) {
        let contributor = contributor_of(&caller)?;
        app.storage
            .append_event(
                caller.household(),
                app.now(),
                EventKind::UploadProgress {
                    contributor,
                    done,
                    total,
                },
            )
            .await?;
    }
    Resp::ok(json!({ "media_id": media.id, "state": media.state }))
}

#[derive(Deserialize)]
struct AbortBody {
    media_id: MediaId,
    upload_id: String,
}

pub async fn abort(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let body: AbortBody = req.json()?;
    let (caller, media) = owned_media(app, req, &body.media_id).await?;
    app.blobs
        .abort_multipart(&media.original_key, &body.upload_id)
        .await?;
    app.storage
        .delete_media(caller.household(), &media.id)
        .await?;
    Resp::no_content()
}
