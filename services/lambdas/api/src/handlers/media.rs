//! Media reads, contributor self-removal (F-157), pin and hide (F-209).

use memorylane_core::confirmation::ConfirmedAction;
use memorylane_core::events::EventKind;
use memorylane_core::ids::{HouseholdId, MediaId};
use memorylane_core::media::{MediaItem, ProcessingState};
use memorylane_ingest::embed;
use serde::Deserialize;
use serde_json::json;

use crate::app::{App, Caller};
use crate::error::ApiError;
use crate::handlers::confirmations::{redeem, subject_of};
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
        // A paired phone's subject is its contributor id; the TV's is the household id.
        Caller::Owner { subject, .. } => subject.as_str(),
        Caller::Contributor(
            Session::Tv { household, .. }
            | Session::Phone { household, .. }
            | Session::Pairing { household, .. },
        ) => household.as_str(),
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
    app.emit(
        caller.household(),
        EventKind::MediaRemoved { media: vec![id] },
    )
    .await?;
    Resp::no_content()
}

/// A media item the viewing household can see: its own, or on a trip it joined. Returns
/// the owning household alongside.
async fn visible_media(
    app: &App,
    viewer: &HouseholdId,
    id: &MediaId,
) -> Result<Option<(HouseholdId, MediaItem)>, ApiError> {
    if let Some(m) = app.storage.get_media(viewer, id).await? {
        return Ok(Some((viewer.clone(), m)));
    }
    for membership in app.storage.list_memberships(viewer).await? {
        if !membership.is_active() {
            continue;
        }
        if let Some(m) = app
            .storage
            .get_media(&membership.owner_household, id)
            .await?
        {
            if m.trip.as_ref() == Some(&membership.trip) {
                return Ok(Some((membership.owner_household.clone(), m)));
            }
        }
    }
    Ok(None)
}

/// Pin or hide (F-209, F-163). The owner's flags shape the trip for everyone; a member
/// household's choices apply to its own view only.
pub async fn set_flag(app: &App, req: &Req, id: &str, flag: &str) -> Result<Resp, ApiError> {
    let viewer = app.owner(req)?;
    let id = MediaId::new(id);
    let value: bool = if req.body.is_empty() {
        true
    } else {
        req.json::<serde_json::Value>()?["value"]
            .as_bool()
            .unwrap_or(true)
    };
    let pin = match flag {
        "pin" => true,
        "hide" => false,
        _ => return Err(ApiError::bad_request("unknown flag")),
    };
    let (owner, mut media) = visible_media(app, &viewer, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown media"))?;
    media.set_flag_for(&viewer, pin, value);
    app.storage.put_media(&media).await?;
    Resp::ok(json!({
        "media_id": media.id,
        "pinned": media.pinned_for(&viewer),
        "hidden": media.hidden_for(&viewer),
        "owner_household": owner,
    }))
}

/// `POST /media/{media}/keeper` (F-201, F-164): make this item the keeper of its burst
/// group. Owner or co-editor of the trip. Non-keepers re-inherit tags and the embedding
/// index follows the keeper.
pub async fn set_keeper(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let id = MediaId::new(id);
    let caller = app.caller(req)?;
    let (household, media) = visible_media(app, caller.household(), &id)
        .await?
        .filter(|(_, m)| !m.is_removed())
        .ok_or_else(|| ApiError::not_found("unknown media"))?;
    match &media.trip {
        Some(trip) => {
            app.owner_or_co_editor(req, trip).await?;
        }
        None => {
            app.owner(req)?;
        }
    }
    let Some(group) = media.burst_group.clone() else {
        return Resp::ok(json!({ "keeper": media.id, "group": null, "members": 1 }));
    };
    let mut members: Vec<MediaItem> = app
        .storage
        .list_media(&household)
        .await?
        .into_iter()
        .filter(|m| m.burst_group.as_deref() == Some(group.as_str()) && !m.is_removed())
        .collect();
    let old_keeper = members.iter().find(|m| m.is_keeper).map(|m| m.id.clone());
    if old_keeper.as_ref() == Some(&id) {
        return Resp::ok(
            json!({ "keeper": id, "group": group, "members": members.len(), "changed": false }),
        );
    }
    let new_enrichment = media.enrichment.clone();
    let new_enriched = media.state == ProcessingState::Enriched;
    for m in &mut members {
        m.is_keeper = m.id == id;
        if !m.is_keeper && new_enriched {
            m.enrichment = new_enrichment.clone();
            m.enrichment.inherited_from = Some(id.clone());
        }
        if m.is_keeper {
            m.enrichment.inherited_from = None;
        }
        app.storage.put_media(m).await?;
    }
    if let Some(old) = &old_keeper {
        app.vectors.delete(&household, old).await?;
    }
    if new_enriched {
        let vector = embed::embed_media(
            &new_enrichment.scene_tags,
            new_enrichment.landmark.as_ref().map(|t| t.name.as_str()),
            media.text.as_deref(),
            media.description.as_ref().map(|d| d.text.as_str()),
        );
        app.vectors
            .upsert(&household, media.trip.as_ref(), &id, &vector)
            .await?;
    }
    Resp::ok(
        json!({ "keeper": id, "group": group, "members": members.len(), "changed": true, "previous": old_keeper }),
    )
}

/// Soft-remove a set of items with the cascade and one feed event. Ids that are unknown
/// or already removed are skipped.
async fn remove_many(
    app: &App,
    household: &HouseholdId,
    ids: &[MediaId],
) -> Result<Vec<MediaId>, ApiError> {
    let now = app.now();
    let mut removed = Vec::new();
    for id in ids {
        let Some(mut m) = app.storage.get_media(household, id).await? else {
            continue;
        };
        if m.is_removed() {
            continue;
        }
        m.remove(now);
        app.storage.put_media(&m).await?;
        cascade(app, household, &m).await?;
        removed.push(id.clone());
    }
    if !removed.is_empty() {
        app.emit(
            household,
            EventKind::MediaRemoved {
                media: removed.clone(),
            },
        )
        .await?;
    }
    Ok(removed)
}

/// `GET /media/cleanup` (the `list_cleanup_candidates` tool, F-705).
pub async fn cleanup_candidates(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let trip = req
        .query
        .get("trip")
        .map(|t| memorylane_core::ids::TripId::new(t.as_str()));
    let rows: Vec<serde_json::Value> = app
        .storage
        .list_media(&household)
        .await?
        .into_iter()
        .filter(|m| !m.is_removed() && m.cleanup_reason.is_some())
        .filter(|m| trip.as_ref().is_none_or(|t| m.trip.as_ref() == Some(t)))
        .map(|m| json!({ "media": m.id, "reason": m.cleanup_reason.map(memorylane_core::media::CleanupReason::as_str), "trip": m.trip, "filename": m.original_filename }))
        .collect();
    Resp::ok(rows)
}

#[derive(Deserialize)]
struct IdsBody {
    media_ids: Vec<String>,
}

#[derive(Deserialize)]
struct GroupsBody {
    group_ids: Vec<String>,
}

/// `POST /media/cleanup/apply` (the `apply_cleanup` tool): token required (F-504).
pub async fn apply_cleanup(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let body: IdsBody = req.json()?;
    redeem(
        app,
        req,
        &household,
        &ConfirmedAction::ApplyCleanup {
            media_ids: body.media_ids.clone(),
        },
    )
    .await?;
    let ids: Vec<MediaId> = body
        .media_ids
        .iter()
        .map(|s| MediaId::new(s.as_str()))
        .collect();
    let removed = remove_many(app, &household, &ids).await?;
    Resp::ok(json!({ "removed": removed.len(), "media": removed }))
}

/// `POST /media/dedup/apply` (the `apply_dedup` tool): removes the non-keepers of the
/// given burst groups; token required.
pub async fn apply_dedup(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let body: GroupsBody = req.json()?;
    redeem(
        app,
        req,
        &household,
        &ConfirmedAction::ApplyDedup {
            group_ids: body.group_ids.clone(),
        },
    )
    .await?;
    let ids: Vec<MediaId> = app
        .storage
        .list_media(&household)
        .await?
        .into_iter()
        .filter(|m| {
            !m.is_keeper
                && !m.is_removed()
                && m.burst_group
                    .as_ref()
                    .is_some_and(|g| body.group_ids.contains(g))
        })
        .map(|m| m.id)
        .collect();
    let removed = remove_many(app, &household, &ids).await?;
    Resp::ok(json!({ "removed": removed.len(), "media": removed }))
}

/// `POST /media/delete` (the `delete_media` tool): the owner's token, or a co-editor's
/// RemoveMedia token whose subject holds the role (F-164). Token required either way.
pub async fn delete_many(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let caller = app.caller(req)?;
    let household = caller.household().clone();
    let body: IdsBody = req.json()?;
    let subject = subject_of(app, req).await?;
    let by = match &caller {
        Caller::Owner { .. } => None,
        Caller::Contributor(Session::Upload { contributor, .. }) => Some(contributor.to_string()),
        Caller::Contributor(_) => return Err(ApiError::forbidden("owner only")),
    };
    let action = match by {
        None => ConfirmedAction::DeleteMedia {
            media_ids: body.media_ids.clone(),
        },
        Some(by) => {
            let c = app
                .storage
                .get_contributor(
                    &household,
                    &memorylane_core::ids::ContributorId::new(by.as_str()),
                )
                .await?
                .filter(memorylane_core::contributor::Contributor::can_edit)
                .ok_or_else(|| ApiError::forbidden("co-editors only"))?;
            if subject.as_deref() != Some(c.id.as_str()) {
                return Err(ApiError::forbidden(
                    "the token was not confirmed by this co-editor",
                ));
            }
            ConfirmedAction::RemoveMedia {
                media_ids: body.media_ids.clone(),
                by,
            }
        }
    };
    redeem(app, req, &household, &action).await?;
    let ids: Vec<MediaId> = body
        .media_ids
        .iter()
        .map(|s| MediaId::new(s.as_str()))
        .collect();
    let removed = remove_many(app, &household, &ids).await?;
    Resp::ok(json!({ "removed": removed.len(), "media": removed }))
}

/// `POST /media/{media}/restore`: undo a soft removal inside its window (US6).
pub async fn restore(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let id = MediaId::new(id);
    let mut media = app
        .storage
        .get_media(&household, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown media"))?;
    if !media.restore(app.now()) {
        return Err(ApiError::gone("the restore window has closed"));
    }
    app.storage.put_media(&media).await?;
    // Back into its trip's last day; organize re-places it properly on the next run.
    if let Some(trip_id) = &media.trip {
        if let Some(mut trip) = app.storage.get_trip(&household, trip_id).await? {
            if let Some(last) = trip.days.last_mut() {
                if !last.media.contains(&id) {
                    last.media.push(id.clone());
                }
            }
            app.storage.put_trip(&trip).await?;
        }
    }
    Resp::ok(json!({ "restored": id, "state": media.state }))
}

/// `POST /media/purge` (the `purge_expired` tool, nightly): delete originals, renditions
/// and records of items past their restore window.
pub async fn purge_expired(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let now = app.now();
    let mut purged = Vec::new();
    for m in app.storage.list_media(&household).await? {
        if !m.is_removed() || m.restore_until.is_none_or(|t| now < t) {
            continue;
        }
        let keys = vec![
            m.original_key.clone(),
            format!("renditions/{}/{}/thumb.jpg", m.household, m.id),
            format!("renditions/{}/{}/tv.jpg", m.household, m.id),
            format!("renditions/{}/{}/poster.jpg", m.household, m.id),
        ];
        app.blobs.delete(&keys).await?;
        app.vectors.delete(&household, &m.id).await?;
        app.storage.delete_media(&household, &m.id).await?;
        purged.push(m.id);
    }
    Resp::ok(json!({ "purged": purged.len(), "media": purged }))
}
