//! Media reads, contributor self-removal (F-157), pin and hide (F-209).

use memorylane_core::events::EventKind;
use memorylane_core::ids::MediaId;
use serde_json::json;

use crate::app::{App, Caller};
use crate::error::ApiError;
use crate::req::{Req, Resp};
use crate::tokens::Session;

pub async fn url(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let caller = app.caller(req)?;
    let media = app
        .storage
        .get_media(caller.household(), &MediaId::new(id))
        .await?
        .ok_or_else(|| ApiError::not_found("unknown media"))?;
    let rendition = req.query.get("rendition").map_or("tv", String::as_str);
    let key = match rendition {
        "original" => media.original_key.clone(),
        "thumb" => format!("renditions/{}/{}/thumb.jpg", media.household, media.id),
        "tv" => format!("renditions/{}/{}/tv.jpg", media.household, media.id),
        _ => {
            return Err(ApiError::bad_request(
                "rendition must be thumb, tv or original",
            ))
        }
    };
    if rendition == "original"
        && !matches!(caller, Caller::Owner { .. })
        && media.contributor.as_str() != contributor_id(&caller)
    {
        return Err(ApiError::forbidden(
            "originals are visible to the owner and the uploader",
        ));
    }
    Resp::ok(
        json!({ "url": app.blobs.presign_get(&key).await?, "expires_in": crate::blobs::PRESIGN_TTL.whole_seconds() }),
    )
}

fn contributor_id(caller: &Caller) -> &str {
    match caller {
        Caller::Owner { household, .. } | Caller::Contributor(Session::Tv { household, .. }) => {
            household.as_str()
        }
        Caller::Contributor(Session::Upload { contributor, .. }) => contributor.as_str(),
    }
}

/// Owner deletes anything; a contributor deletes their own upload. Cascade to renditions
/// happens here; embeddings, albums and playlists are cleaned by the `media.removed`
/// event consumer.
pub async fn delete(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let caller = app.caller(req)?;
    let id = MediaId::new(id);
    let media = app
        .storage
        .get_media(caller.household(), &id)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown media"))?;
    if !matches!(caller, Caller::Owner { .. })
        && media.contributor.as_str() != contributor_id(&caller)
    {
        return Err(ApiError::forbidden("not your upload"));
    }
    let keys = vec![
        media.original_key.clone(),
        format!("renditions/{}/{}/thumb.jpg", media.household, media.id),
        format!("renditions/{}/{}/tv.jpg", media.household, media.id),
    ];
    app.blobs.delete(&keys).await?;
    app.storage.delete_media(caller.household(), &id).await?;
    app.storage
        .append_event(
            caller.household(),
            app.now(),
            EventKind::MediaRemoved { media: vec![id] },
        )
        .await?;
    Resp::no_content()
}

pub async fn set_flag(app: &App, req: &Req, id: &str, flag: &str) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let id = MediaId::new(id);
    let mut media = app
        .storage
        .get_media(&household, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown media"))?;
    let value: bool = if req.body.is_empty() {
        true
    } else {
        req.json::<serde_json::Value>()?["value"]
            .as_bool()
            .unwrap_or(true)
    };
    match flag {
        "pin" => {
            media.pinned = value;
            if value {
                media.hidden = false;
            }
        }
        "hide" => {
            media.hidden = value;
            if value {
                media.pinned = false;
            }
        }
        _ => return Err(ApiError::bad_request("unknown flag")),
    }
    app.storage.put_media(&media).await?;
    Resp::ok(json!({ "media_id": media.id, "pinned": media.pinned, "hidden": media.hidden }))
}
