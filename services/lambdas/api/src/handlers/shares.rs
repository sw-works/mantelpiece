//! Expiring share links (F-601). `POST /shares` is the `share_album` tool: it needs a
//! `ShareAlbum` confirmation minted on the TV, publishes a playlist for seven days and
//! optionally emails the link. `GET /s/{share}` is public and read-only until expiry.

use memorylane_core::confirmation::ConfirmedAction;
use memorylane_core::ids::{HouseholdId, MediaId, PlaylistId, ShareLinkId};
use memorylane_core::share_link::{ShareLink, ShareTarget};
use memorylane_store::mailer::Mail;
use serde::Deserialize;
use serde_json::{json, Value};
use time::format_description::well_known::Rfc3339;

use crate::app::App;
use crate::error::ApiError;
use crate::req::{Req, Resp};

#[derive(Deserialize)]
struct CreateBody {
    /// The playlist (album) to publish. `media` and `name` on the tool describe it.
    playlist: String,
    name: Option<String>,
    email: Option<String>,
}

fn link_for(app: &App, id: &ShareLinkId) -> String {
    format!("{}/s.html?s={id}", app.config.upload_base)
}

fn view(app: &App, l: &ShareLink) -> Value {
    json!({
        "id": l.id,
        "name": l.name,
        "link": link_for(app, &l.id),
        "path": l.path(),
        "expires_at": l.expires_at.format(&Rfc3339).ok(),
        "revoked": l.revoked_at.is_some(),
        "email": l.email,
    })
}

pub async fn create(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let body: CreateBody = req.json()?;
    let playlist = app
        .storage
        .get_playlist(&household, &PlaylistId::new(&body.playlist))
        .await?
        .ok_or_else(|| ApiError::not_found("no such playlist"))?;
    let action = ConfirmedAction::ShareAlbum {
        album_id: body.playlist.clone(),
    };
    if let Some(email) = body.email.as_deref() {
        if !email.contains('@') {
            return Err(ApiError::bad_request("that is not an email address"));
        }
        if !app.mailer.configured() {
            return Err(ApiError::conflict(
                "email sending is not configured; copy the link",
            ));
        }
    }
    let token = crate::handlers::confirmations::redeem(app, req, &household, &action).await?;
    let name = body
        .name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| playlist.name.clone());
    let mut link = ShareLink::new(
        household.clone(),
        ShareTarget::Playlist { id: playlist.id },
        name.chars().take(80).collect::<String>(),
        token,
        app.now(),
        None,
    );
    if let Some(email) = body.email {
        let owner = app
            .storage
            .get_household(&household)
            .await?
            .map_or_else(|| "A Mantelpiece TV".to_owned(), |h| h.name);
        app.mailer
            .send(Mail {
                to: email.clone(),
                subject: format!("{owner} shared \"{}\" with you", link.name),
                text: format!(
                    "{owner} shared photos with you on Mantelpiece.\n\n{}\n\nThe link works for 7 days.",
                    link_for(app, &link.id)
                ),
            })
            .await?;
        link.email = Some(email);
    }
    app.storage.put_share_link(&link).await?;
    Resp::created(view(app, &link))
}

pub async fn list(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let links = app.storage.list_share_links(&household).await?;
    Resp::ok(links.iter().map(|l| view(app, l)).collect::<Vec<_>>())
}

pub async fn revoke(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let mut link = app
        .storage
        .get_share_link(&ShareLinkId::new(id))
        .await?
        .filter(|l| l.household == household)
        .ok_or_else(|| ApiError::not_found("no such share"))?;
    link.revoke(app.now());
    app.storage.put_share_link(&link).await?;
    Resp::ok(view(app, &link))
}

/// Public album: the playlist's live, unhidden items with presigned renditions. 404 for
/// an unknown id, 410 once expired or revoked, so the page can say which.
pub async fn open(app: &App, id: &str) -> Result<Resp, ApiError> {
    let link = app
        .storage
        .get_share_link(&ShareLinkId::new(id))
        .await?
        .ok_or_else(|| ApiError::not_found("no such share"))?;
    if !link.is_live(app.now()) {
        return Err(ApiError::gone("this link has expired"));
    }
    let household: &HouseholdId = &link.household;
    let ids: Vec<MediaId> = match &link.target {
        ShareTarget::Playlist { id } => app
            .storage
            .get_playlist(household, id)
            .await?
            .map(|p| p.media)
            .unwrap_or_default(),
        ShareTarget::Album { .. } => Vec::new(),
    };
    let mut items = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(m) = app.storage.get_media(household, &id).await? else {
            continue;
        };
        if m.is_removed() || m.hidden_for(household) {
            continue;
        }
        let key = format!("renditions/{}/{}/tv.jpg", m.household, m.id);
        items.push(json!({
            "id": m.id,
            "url": app.blobs.presign_get(&key).await?,
            "taken_at": m.captured.as_ref().map(|c| c.utc),
            "landmark": m.enrichment.landmark,
        }));
    }
    let owner = app.storage.get_household(household).await?.map(|h| h.name);
    Resp::ok(json!({
        "name": link.name,
        "shared_by": owner,
        "expires_at": link.expires_at.format(&Rfc3339).ok(),
        "items": items,
    }))
}
