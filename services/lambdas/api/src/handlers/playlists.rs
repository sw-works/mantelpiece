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
    app.storage
        .append_event(
            &household,
            app.now(),
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

pub async fn get(app: &App, req: &Req, id: &str) -> Result<Resp, ApiError> {
    let household = app.owner(req)?;
    let playlist = app
        .storage
        .get_playlist(&household, &PlaylistId::new(id))
        .await?
        .ok_or_else(|| ApiError::not_found("no such playlist"))?;
    Resp::ok(playlist)
}
