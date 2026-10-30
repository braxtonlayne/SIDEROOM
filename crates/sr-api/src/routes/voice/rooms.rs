//! REST for 24/7 listening rooms (MUS-JB-003's flag, built here with the jukebox routes).
//!
//! * `PUT /channels/{id}/listening-room`: make a voice channel a room on a server playlist (needs
//!   `MANAGE_CHANNELS`); sending another playlist id changes the playlist.
//! * `DELETE /channels/{id}/listening-room`: stop being a room (needs `MANAGE_CHANNELS`).
//! * `GET /channels/{id}/listening-room`: the cursor as `LISTENING_ROOM_UPDATE` carries it, for page load
//!   (needs `VIEW_CHANNEL` and `CONNECT`).
//!
//! The server never streams audio: a room is a cursor, and listeners presign each track from the shelf.

use super::jukebox::{JukeboxDeps, standing};
use crate::AppState;
use crate::auth::SessionOnly;
use crate::openapi::ErrorBody;
use crate::routes::Ext;
use crate::routes::messages::error::{ApiJson, MsgError, MsgResult, invalid};
use crate::routes::storage::uid;
use axum::{Json, Router, extract::Path, http::StatusCode, routing::get};
use serde::Deserialize;
use sr_chat::jukebox::ports::RoomError;
use sr_core::ErrorCode;
use utoipa::ToSchema;

/// The routes, relative to the API prefix.
pub fn router() -> Router<AppState> {
    Router::new().route(
        "/channels/{channel_id}/listening-room",
        get(cursor).put(enable).delete(disable),
    )
}

/// Body of `PUT /channels/{id}/listening-room`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct EnableRoomBody {
    /// A playlist of this server (`GET /guilds/{id}/music/playlists`).
    pub playlist_id: String,
}

fn failed(e: RoomError) -> MsgError {
    match e {
        RoomError::UnknownPlaylist => invalid(
            "playlist_id",
            "UNKNOWN_PLAYLIST",
            "Pick a playlist that belongs to this server.",
        ),
        RoomError::Store(m) => {
            tracing::error!(error = %m, "listening room store failed");
            ErrorCode::SrInternal.into()
        }
    }
}

fn row(raw: &str) -> MsgResult<i64> {
    raw.parse::<i64>().ok().filter(|v| *v > 0).ok_or_else(|| {
        MsgError::from(sr_core::ApiError::with_message(
            ErrorCode::SrNotFound,
            "Unknown channel or playlist",
        ))
    })
}

/// Make the voice channel a listening room on a server playlist.
#[utoipa::path(
    put,
    path = "/channels/{channel_id}/listening-room",
    params(("channel_id" = String, Path, description = "Voice channel snowflake")),
    request_body = EnableRoomBody,
    responses(
        (status = 204, description = "The channel is a listening room"),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn enable(
    SessionOnly(p): SessionOnly,
    Ext(j): Ext<JukeboxDeps>,
    Path(channel_id): Path<String>,
    ApiJson(body): ApiJson<EnableRoomBody>,
) -> MsgResult<StatusCode> {
    let user = uid(&p);
    let channel = row(&channel_id)?;
    let access = standing(&j, user, channel).await?;
    if !access.manage {
        return Err(ErrorCode::MissingPermissions.into());
    }
    let playlist = body
        .playlist_id
        .parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| invalid("playlist_id", "SNOWFLAKE_TYPE", "Not a playlist id."))?;
    j.rooms
        .enable(channel, access.guild_id, playlist, user)
        .await
        .map_err(failed)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Stop the channel being a listening room.
#[utoipa::path(
    delete,
    path = "/channels/{channel_id}/listening-room",
    params(("channel_id" = String, Path, description = "Voice channel snowflake")),
    responses(
        (status = 204, description = "The channel is an ordinary voice channel"),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn disable(
    SessionOnly(p): SessionOnly,
    Ext(j): Ext<JukeboxDeps>,
    Path(channel_id): Path<String>,
) -> MsgResult<StatusCode> {
    let user = uid(&p);
    let channel = row(&channel_id)?;
    let access = standing(&j, user, channel).await?;
    if !access.manage {
        return Err(ErrorCode::MissingPermissions.into());
    }
    j.rooms.disable(channel).await.map_err(failed)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The room's cursor now, in the shape of the `LISTENING_ROOM_UPDATE` event.
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/listening-room",
    params(("channel_id" = String, Path, description = "Voice channel snowflake")),
    responses(
        (status = 200, description = "The cursor: mode, seq, at_ms, position_ms, now_playing, next, skipped"),
        (status = 403, body = ErrorBody),
        (status = 404, description = "Not a listening room", body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn cursor(
    SessionOnly(p): SessionOnly,
    Ext(j): Ext<JukeboxDeps>,
    Path(channel_id): Path<String>,
) -> MsgResult<Json<serde_json::Value>> {
    let user = uid(&p);
    let channel = row(&channel_id)?;
    standing(&j, user, channel).await?;
    match j.rooms.cursor(channel).await.map_err(failed)? {
        Some(v) => Ok(Json(v)),
        None => Err(MsgError::from(sr_core::ApiError::with_message(
            ErrorCode::SrNotFound,
            "This channel is not a listening room",
        ))),
    }
}

/// OpenAPI fragment of the room routes.
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(enable, disable, cursor),
    components(schemas(ErrorBody, EnableRoomBody))
)]
pub struct RoomsApiDoc;

/// The OpenAPI fragment of the room routes.
pub fn openapi() -> utoipa::openapi::OpenApi {
    <RoomsApiDoc as utoipa::OpenApi>::openapi()
}
