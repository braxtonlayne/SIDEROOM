//! UTL-COL-001: tool rooms, the collaborative tier list and poll builder.
//!
//! * `POST   /channels/{id}/tool-rooms`            create (`SEND_MESSAGES`)
//! * `GET    /channels/{id}/tool-rooms`            the channel's live rooms (`VIEW_CHANNEL`)
//! * `GET    /channels/{id}/tool-rooms/{room_id}`  one room (`VIEW_CHANNEL`)
//! * `PATCH  /channels/{id}/tool-rooms/{room_id}`  edit under the version rule (`SEND_MESSAGES`); a stale
//!   `base_version` answers 409 with the current room in `current`
//! * `DELETE /channels/{id}/tool-rooms/{room_id}`  the creator, or `MANAGE_MESSAGES`
//!
//! Every change fans out as `TOOL_ROOM_UPDATE` to members who can view the channel (the gateway filters by
//! channel permission; DMs go to each recipient). Posting a poll or an image result is the existing message
//! endpoint, called by the client.

use crate::routes::Ext;
use crate::routes::messages::MessagesDeps;
use crate::routes::messages::error::{ApiJson, MsgError, invalid, parse_id};
use crate::routes::messages::render::actor;
use crate::state::AppState;
use crate::{AuthKind, Principal, openapi::ErrorBody};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sr_chat::tool_rooms::{
    self, Patch, RoomError, ToolRoom, ToolRoomStore, expires_at, validate_kind, validate_state,
    validate_title,
};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, ErrorCode};
use sr_events::DomainEvent;
use std::sync::Arc;
use utoipa::{OpenApi, ToSchema};

/// Collaborators of the tool room endpoints.
pub struct ToolRoomDeps {
    pub store: Arc<dyn ToolRoomStore>,
}

/// A tool room as the API shows it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ToolRoomObject {
    pub id: String,
    pub channel_id: String,
    pub guild_id: Option<String>,
    /// `tierlist` or `poll`.
    pub kind: String,
    pub title: String,
    /// A flat object of keys; images are references (`{"img": {"k": "mount", "mount", "key"}}`), never URLs.
    #[schema(value_type = Object)]
    pub state: Map<String, Value>,
    /// Bumps by one on every accepted edit.
    pub version: i64,
    pub created_by: Option<String>,
    /// Unix milliseconds.
    pub created_at: i64,
    /// Unix milliseconds; a room lives 7 days from creation.
    pub expires_at: i64,
}

impl From<&ToolRoom> for ToolRoomObject {
    fn from(r: &ToolRoom) -> Self {
        Self {
            id: r.id.to_string(),
            channel_id: r.channel_id.to_string(),
            guild_id: r.guild_id.map(|g| g.to_string()),
            kind: r.kind.clone(),
            title: r.title.clone(),
            state: r.state.clone(),
            version: r.version,
            created_by: r.created_by.map(|u| u.to_string()),
            created_at: r.created_ms,
            expires_at: r.expires_ms,
        }
    }
}

/// The 409 body of a stale edit.
#[derive(Debug, Serialize, ToSchema)]
pub struct StaleRoomBody {
    pub code: u32,
    pub message: String,
    /// The room as it is now: re-apply your keys on top of it and retry with its `version`.
    pub current: ToolRoomObject,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateRoomBody {
    /// `tierlist` or `poll`.
    pub kind: String,
    #[serde(default)]
    pub title: Option<String>,
    /// The starting keys.
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub state: Option<Map<String, Value>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PatchRoomBody {
    /// The version the client last saw.
    pub base_version: i64,
    /// Keys to set; `null` removes a key.
    #[serde(default)]
    #[schema(value_type = Object)]
    pub set: Map<String, Value>,
    #[serde(default)]
    pub title: Option<String>,
}

/// A failure of a room endpoint: any message error, or the stale-version 409 with the current room.
pub enum RoomFail {
    Msg(MsgError),
    Stale(Box<ToolRoom>),
}

impl From<MsgError> for RoomFail {
    fn from(e: MsgError) -> Self {
        Self::Msg(e)
    }
}

impl IntoResponse for RoomFail {
    fn into_response(self) -> Response {
        match self {
            Self::Msg(e) => e.into_response(),
            Self::Stale(room) => (
                StatusCode::CONFLICT,
                Json(json!({
                    "code": ErrorCode::SrConflict.code(),
                    "message": "The room changed since you last saw it.",
                    "current": ToolRoomObject::from(room.as_ref()),
                })),
            )
                .into_response(),
        }
    }
}

type Res<T> = Result<T, RoomFail>;

fn user_of(p: &Principal) -> Res<sr_core::UserId> {
    match p.kind {
        AuthKind::Bearer { .. } => Err(MsgError::from(ErrorCode::MissingAccess).into()),
        _ => Ok(actor(p)),
    }
}

fn room_err(e: RoomError) -> RoomFail {
    match e {
        RoomError::Invalid(m) => {
            let (field, msg) = m.split_once(": ").unwrap_or(("body", m.as_str()));
            invalid(field, "TOOL_ROOM_INVALID", msg).into()
        }
        RoomError::Stale(r) => RoomFail::Stale(r),
        RoomError::NotFound => MsgError::from(ErrorCode::SrNotFound).into(),
        RoomError::TooMany => MsgError::Api(sr_core::ApiError::with_message(
            ErrorCode::SrConflict,
            "This channel already has the most tool rooms it can hold; delete one first.",
        ))
        .into(),
        RoomError::Store(m) => {
            tracing::error!(error = %m, "tool room storage failure");
            MsgError::from(ErrorCode::SrInternal).into()
        }
    }
}

fn store_err(e: sr_chat::repo::StoreError) -> RoomFail {
    room_err(RoomError::Store(e.0))
}

fn send_perm(acc: &crate::routes::messages::deps::Access) -> Permissions {
    if acc.channel.kind.is_thread() {
        Permissions::SEND_MESSAGES_IN_THREADS
    } else {
        Permissions::SEND_MESSAGES
    }
}

async fn publish(
    m: &MessagesDeps,
    room: &ToolRoom,
    guild: Option<sr_core::GuildId>,
    actor_id: sr_core::UserId,
    extra: Value,
) {
    let mut payload = json!({
        "id": room.id.to_string(),
        "channel_id": room.channel_id.to_string(),
        "guild_id": guild.map(|g| g.get().to_string()),
        "kind": room.kind,
        "version": room.version,
        "title": room.title,
        "actor_id": actor_id.get().to_string(),
    });
    if let (Some(o), Some(e)) = (payload.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            o.insert(k.clone(), v.clone());
        }
    }
    m.publish(DomainEvent::ToolRoomUpdated {
        guild,
        channel: ChannelId::new(room.channel_id),
        payload,
    })
    .await;
}

fn room_id(raw: &str) -> Res<u64> {
    raw.parse::<u64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| MsgError::from(ErrorCode::SrNotFound).into())
}

/// Create a tool room in a channel or DM.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/tool-rooms",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    request_body = CreateRoomBody,
    responses(
        (status = 200, body = ToolRoomObject),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "The channel has too many rooms"),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create(
    p: Principal,
    Ext(d): Ext<ToolRoomDeps>,
    Ext(m): Ext<MessagesDeps>,
    State(app): State<AppState>,
    Path(channel): Path<String>,
    ApiJson(body): ApiJson<CreateRoomBody>,
) -> Res<Json<ToolRoomObject>> {
    let user = user_of(&p)?;
    let channel: ChannelId = parse_id(&channel, ErrorCode::UnknownChannel)?;
    let acc = m.access(user, channel).await?;
    acc.require(send_perm(&acc))?;
    validate_kind(&body.kind).map_err(room_err)?;
    // The kind is a tool of the drawer: its own switch applies (OPS-FLG-001).
    if let Some(flag) = sr_features::tool_flag_id(&body.kind)
        && !crate::module_gate::platform_on(&app, flag).await
    {
        return Err(MsgError::from(ErrorCode::SrFeatureDisabled).into());
    }
    let title = validate_title(body.title.as_deref().unwrap_or("")).map_err(room_err)?;
    let state = body.state.unwrap_or_default();
    validate_state(&state).map_err(room_err)?;
    let now = m.now_ms();
    let room = ToolRoom {
        id: m.next_id().get(),
        channel_id: channel.get(),
        guild_id: acc.channel.guild_id.map(|g| g.get()),
        kind: body.kind,
        title,
        state,
        version: 1,
        created_by: Some(user.get()),
        created_ms: now,
        updated_ms: now,
        expires_ms: expires_at(now),
    };
    let room = d.store.create(room, now).await.map_err(room_err)?;
    publish(
        &m,
        &room,
        acc.channel.guild_id,
        user,
        json!({"created": true, "state": room.state}),
    )
    .await;
    Ok(Json(ToolRoomObject::from(&room)))
}

/// The live tool rooms of a channel, newest first.
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/tool-rooms",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses(
        (status = 200, body = Vec<ToolRoomObject>),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list(
    p: Principal,
    Ext(d): Ext<ToolRoomDeps>,
    Ext(m): Ext<MessagesDeps>,
    Path(channel): Path<String>,
) -> Res<Json<Vec<ToolRoomObject>>> {
    let user = user_of(&p)?;
    let channel: ChannelId = parse_id(&channel, ErrorCode::UnknownChannel)?;
    m.access(user, channel).await?.require_view()?;
    let rooms = d
        .store
        .list(channel.get(), m.now_ms())
        .await
        .map_err(store_err)?;
    Ok(Json(rooms.iter().map(ToolRoomObject::from).collect()))
}

/// One tool room.
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/tool-rooms/{room_id}",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("room_id" = String, Path, description = "Room snowflake"),
    ),
    responses(
        (status = 200, body = ToolRoomObject),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_room(
    p: Principal,
    Ext(d): Ext<ToolRoomDeps>,
    Ext(m): Ext<MessagesDeps>,
    Path((channel, room)): Path<(String, String)>,
) -> Res<Json<ToolRoomObject>> {
    let user = user_of(&p)?;
    let channel: ChannelId = parse_id(&channel, ErrorCode::UnknownChannel)?;
    m.access(user, channel).await?.require_view()?;
    let room = d
        .store
        .get(channel.get(), room_id(&room)?, m.now_ms())
        .await
        .map_err(store_err)?
        .ok_or_else(|| room_err(RoomError::NotFound))?;
    Ok(Json(ToolRoomObject::from(&room)))
}

/// Edit a tool room: set or remove whole keys against the version the client last saw. A stale version is
/// 409 with the current room; the client re-applies its own keys on top and retries.
#[utoipa::path(
    patch,
    path = "/channels/{channel_id}/tool-rooms/{room_id}",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("room_id" = String, Path, description = "Room snowflake"),
    ),
    request_body = PatchRoomBody,
    responses(
        (status = 200, body = ToolRoomObject),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = StaleRoomBody, description = "Stale version; `current` is the room now"),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn patch_room(
    p: Principal,
    Ext(d): Ext<ToolRoomDeps>,
    Ext(m): Ext<MessagesDeps>,
    Path((channel, room)): Path<(String, String)>,
    ApiJson(body): ApiJson<PatchRoomBody>,
) -> Res<Json<ToolRoomObject>> {
    let user = user_of(&p)?;
    let channel: ChannelId = parse_id(&channel, ErrorCode::UnknownChannel)?;
    let acc = m.access(user, channel).await?;
    acc.require(send_perm(&acc))?;
    let patch = Patch {
        base_version: body.base_version,
        set: body.set,
        title: body.title,
    };
    let updated = d
        .store
        .patch(channel.get(), room_id(&room)?, &patch, m.now_ms())
        .await
        .map_err(room_err)?;
    publish(
        &m,
        &updated,
        acc.channel.guild_id,
        user,
        json!({"set": patch.set}),
    )
    .await;
    Ok(Json(ToolRoomObject::from(&updated)))
}

/// Delete a tool room: its creator, or a member with `MANAGE_MESSAGES`.
#[utoipa::path(
    delete,
    path = "/channels/{channel_id}/tool-rooms/{room_id}",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("room_id" = String, Path, description = "Room snowflake"),
    ),
    responses(
        (status = 204),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_room(
    p: Principal,
    Ext(d): Ext<ToolRoomDeps>,
    Ext(m): Ext<MessagesDeps>,
    Path((channel, room)): Path<(String, String)>,
) -> Res<StatusCode> {
    let user = user_of(&p)?;
    let channel: ChannelId = parse_id(&channel, ErrorCode::UnknownChannel)?;
    let acc = m.access(user, channel).await?;
    acc.require_view()?;
    let id = room_id(&room)?;
    let now = m.now_ms();
    let room = d
        .store
        .get(channel.get(), id, now)
        .await
        .map_err(store_err)?
        .ok_or_else(|| room_err(RoomError::NotFound))?;
    if room.created_by != Some(user.get()) {
        acc.require(Permissions::MANAGE_MESSAGES)?;
    }
    if d.store.delete(channel.get(), id).await.map_err(store_err)? {
        let mut gone = room;
        gone.version += 1;
        publish(
            &m,
            &gone,
            acc.channel.guild_id,
            user,
            json!({"deleted": true}),
        )
        .await;
    }
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/channels/{channel_id}/tool-rooms", get(list).post(create))
        .route(
            "/channels/{channel_id}/tool-rooms/{room_id}",
            get(get_room).patch(patch_room).delete(delete_room),
        )
}

#[derive(OpenApi)]
#[openapi(
    paths(create, list, get_room, patch_room, delete_room),
    components(schemas(
        ErrorBody,
        ToolRoomObject,
        StaleRoomBody,
        CreateRoomBody,
        PatchRoomBody
    ))
)]
pub struct ToolRoomsApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    ToolRoomsApiDoc::openapi()
}

// Keep the re-export used by the composition root next to the deps.
pub use tool_rooms::LIFE_MS as ROOM_LIFE_MS;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_messages_name_their_field() {
        let RoomFail::Msg(MsgError::Api(e)) =
            room_err(RoomError::Invalid("title: at most 100 characters".into()))
        else {
            panic!("expected a form error");
        };
        assert_eq!(e.code, ErrorCode::InvalidFormBody);
        assert_eq!(e.fields[0].0, "title");
    }

    #[test]
    fn stale_is_a_409_with_the_current_room() {
        let room = ToolRoom {
            id: 1,
            channel_id: 2,
            guild_id: None,
            kind: "poll".into(),
            title: String::new(),
            state: Map::new(),
            version: 4,
            created_by: None,
            created_ms: 0,
            updated_ms: 0,
            expires_ms: 1,
        };
        let resp = room_err(RoomError::Stale(Box::new(room))).into_response();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }
}
