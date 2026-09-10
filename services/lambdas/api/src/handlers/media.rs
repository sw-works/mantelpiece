//! Media reads, contributor self-removal (F-157), pin and hide (F-209).

use memorylane_core::events::EventKind;
use memorylane_core::ids::{HouseholdId, MediaId};
use memorylane_core::media::MediaItem;
use serde_json::json;

use crate::app::{App, Caller};
use crate::error::ApiError;
use crate::req::{Req, Resp};
use crate::tokens::Session;

pub async fn url(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    // The read-only web view resolves thumbnails by trip code instead of a session.
    let (household, caller) = match app.caller(req) {
        Ok(c) => (c.household().clone(), Some(c)),
        Err(e) if e.status == 401 && req.query.contains_key("code") => {
            let (household, trip) = app.viewer(req).await?;
            let media = app
                .storage
                .get_media(&household, &MediaId::new(id))
                .await?
                .filter(|m| !m.is_removed())
                .ok_or_else(|| ApiError::not_found("unknown media"))?;
            if media.trip.as_ref() != Some(&trip) {
                return Err(ApiError::forbidden("not on this trip"));
            }
            (household, None)
        }
        Err(e) => return Err(e),
    };
    let media = app
        .storage
        .get_media(&household, &MediaId::new(id))
        .await?
        .filter(|m| !m.is_removed())
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
    let may_see_original = match &caller {
        Some(Caller::Owner { .. }) => true,
        Some(c) => media.contributor.as_str() == contributor_id(c),
        None => false,
    };
    if rendition == "original" && !may_see_original {
        return Err(ApiError::forbidden(
            "originals are visible to the owner and the uploader",
        ));
    }
    Resp::ok(
        json!({ "url": app.blobs.presign_get(&key).await?, "expires_in": memorylane_store::blobs::PRESIGN_TTL.whole_seconds() }),
    )
}

fn contributor_id(caller: &Caller) -> &str {
    match caller {
        Caller::Owner { household, .. }
        | Caller::Contributor(Session::Tv { household, .. } | Session::Phone { household, .. }) => {
            household.as_str()
        }
        Caller::Contributor(Session::Upload { contributor, .. }) => contributor.as_str(),
    }
}

/// Take a removed item out of every view: its trip's days, highlights and hero, every
/// playlist, and the embedding index (F-157). Originals and renditions stay until the
/// restore window closes and `purge_expired` deletes them (US6).
pub async fn cascade(
    app: &App,
    household: &HouseholdId,
    media: &MediaItem,
) -> Result<(), ApiError> {
    if let Some(trip_id) = &media.trip {
        if let Some(mut trip) = app.storage.get_trip(household, trip_id).await? {
            for day in &mut trip.days {
                day.media.retain(|m| m != &media.id);
            }
            trip.highlights.retain(|m| m != &media.id);
            if trip.hero.as_ref() == Some(&media.id) {
                trip.hero = trip.highlights.first().cloned();
            }
            app.storage.put_trip(&trip).await?;
        }
    }
    for mut playlist in app.storage.list_playlists(household).await? {
        if playlist.media.contains(&media.id) {
            playlist.media.retain(|m| m != &media.id);
            app.storage.put_playlist(&playlist).await?;
        }
    }
    app.vectors.delete(household, &media.id).await?;
    Ok(())
}

/// Owner deletes anything; a contributor deletes their own upload. Soft removal: the item
/// leaves every view now and can be restored for 30 days (US6).
pub async fn delete(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let caller = app.caller(req)?;
    let id = MediaId::new(id);
    let mut media = app
        .storage
        .get_media(caller.household(), &id)
        .await?
        .filter(|m| !m.is_removed())
        .ok_or_else(|| ApiError::not_found("unknown media"))?;
    if !matches!(caller, Caller::Owner { .. })
        && media.contributor.as_str() != contributor_id(&caller)
    {
        return Err(ApiError::forbidden("not your upload"));
    }
    media.remove(app.now());
    app.storage.put_media(&media).await?;
    cascade(app, caller.household(), &media).await?;
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
