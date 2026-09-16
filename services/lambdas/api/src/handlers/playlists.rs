//! Playlists (F-304): the `create_playlist` tool target and what the TV plays.

use memorylane_core::events::EventKind;
use memorylane_core::ids::{MediaId, PlaylistId};
use memorylane_core::trip::Playlist;
use serde::Deserialize;

use crate::app::App;
use crate::error::ApiError;
use crate::req::{Req, Resp};

#[derive(Deserialize)]
struct CreateBody {
    name: String,
    media: Vec<MediaId>,
    source: String,
}

pub async fn create(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let body: CreateBody = req.json()?;
    if body.media.is_empty() {
        return Err(ApiError::bad_request("a playlist needs at least one item"));
    }
    let playlist = Playlist {
        id: PlaylistId::random(),
        household: household.clone(),
        name: body.name.chars().take(80).collect(),
        source: body.source,
        media: body.media,
        created_at: app.now(),
    };
    app.storage.put_playlist(&playlist).await?;
    app.emit(
        &household,
        EventKind::PlaylistCreated {
            playlist: playlist.id.clone(),
            name: playlist.name.clone(),
        },
    )
    .await?;
    Resp::created(playlist)
}

pub async fn list(app: &App, req: &Req) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    Resp::ok(app.storage.list_playlists(&household).await?)
}

#[derive(Deserialize, Default)]
struct PlayBody {
    #[serde(default)]
    source: Option<String>,
}

/// Ask the household's TV to play this playlist (`play.requested` on the feed).
pub async fn play(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let playlist = app
        .storage
        .get_playlist(&household, &PlaylistId::new(id))
        .await?
        .ok_or_else(|| ApiError::not_found("no such playlist"))?;
    let body: PlayBody = if req.body.is_empty() {
        PlayBody::default()
    } else {
        req.json()?
    };
    app.emit(
        &household,
        EventKind::PlayRequested {
            trip: None,
            playlist: Some(playlist.id.clone()),
            name: playlist.name.clone(),
            source: body.source.unwrap_or_else(|| "agent".into()),
            start: None,
        },
    )
    .await?;
    Resp::ok(
        serde_json::json!({ "requested": true, "playlist": playlist.id, "name": playlist.name }),
    )
}

/// One playlist for the TV: pinned items first, hidden ones dropped, removed ones gone
/// (F-406 for the ambient set, F-163 for every other).
pub async fn get(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let mut playlist = app
        .storage
        .get_playlist(&household, &PlaylistId::new(id))
        .await?
        .ok_or_else(|| ApiError::not_found("no such playlist"))?;
    let media = app.storage.list_media(&household).await?;
    let find = |id: &MediaId| media.iter().find(|m| &m.id == id);
    let mut pinned: Vec<MediaId> = playlist
        .media
        .iter()
        .filter(|id| find(id).is_some_and(|m| !m.is_removed() && m.pinned_for(&household)))
        .cloned()
        .collect();
    let rest: Vec<MediaId> = playlist
        .media
        .iter()
        .filter(|id| {
            !pinned.contains(id)
                && find(id).is_none_or(|m| !m.is_removed() && !m.hidden_for(&household))
        })
        .cloned()
        .collect();
    pinned.extend(rest);
    playlist.media = pinned;
    Resp::ok(playlist)
}
