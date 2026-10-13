//! Read markers and mention badges (LNG-MSG-021, Q117, Q128).

use super::deps::SocialDeps;
use super::error::{ApiJson, MsgResult, invalid, parse_id};
use super::events::SocialEvent;
use super::model::{AckBody, AckResponse, BulkAckBody, ReadStateObject, ReadStatesResponse};
use super::util::{actor, channel_id, message_id, optional_json};
use crate::routes::messages::MessagesDeps;
use crate::{Principal, openapi::ErrorBody, routes::Ext};
use axum::{Json, body::Bytes, extract::Path, http::StatusCode};
use serde_json::json;
use sr_chat::social::read_state::ReadState;
use sr_core::{ChannelId, ErrorCode, GuildId, MessageId, UserId};

/// Most channels one bulk acknowledgement may name.
const MAX_BULK: usize = 100;

/// Acknowledge a message: the marker moves forward to it and the mention badge
/// resets. With `manual: true` ("mark as unread") the marker is set to exactly this
/// message and `mention_count` is restored.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/messages/{message_id}/ack",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("message_id" = String, Path, description = "Message snowflake")
    ),
    request_body(content = AckBody, description = "Optional"),
    responses((status = 200, body = AckResponse), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn ack_message(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    Ext(messages): Ext<MessagesDeps>,
    Path((channel, message)): Path<(String, String)>,
    body: Bytes,
) -> MsgResult<Json<AckResponse>> {
    let me = actor(&p)?;
    let cid = channel_id(&channel)?;
    let mid = message_id(&message)?;
    let body: AckBody = optional_json(&body)?;
    messages.access(me, cid).await?.require_view()?;
    let state = if body.manual.unwrap_or(false) {
        deps.service
            .mark_unread(me, cid, Some(mid), body.mention_count.unwrap_or(0))
            .await?
    } else {
        deps.service.ack(me, cid, mid).await?
    };
    announce(&deps, me, &state, mid).await;
    Ok(Json(AckResponse { token: None }))
}

/// Mark the channel's pins as seen.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/pins/ack",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 204, description = "Acknowledged"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn ack_pins(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    Ext(messages): Ext<MessagesDeps>,
    Path(channel): Path<String>,
) -> MsgResult<StatusCode> {
    let me = actor(&p)?;
    let cid = channel_id(&channel)?;
    messages.access(me, cid).await?.require_view()?;
    deps.service.ack_pins(me, cid).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Acknowledge many channels at once ("mark server as read").
#[utoipa::path(
    post,
    path = "/read-states/ack-bulk",
    request_body = BulkAckBody,
    responses((status = 204, description = "Acknowledged"), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn ack_bulk(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    Ext(messages): Ext<MessagesDeps>,
    ApiJson(body): ApiJson<BulkAckBody>,
) -> MsgResult<StatusCode> {
    let me = actor(&p)?;
    if body.read_states.len() > MAX_BULK {
        return Err(invalid(
            "read_states",
            "BASE_TYPE_MAX_ITEMS",
            "Too many read states.",
        ));
    }
    let mut targets: Vec<(ChannelId, MessageId)> = Vec::with_capacity(body.read_states.len());
    for e in &body.read_states {
        let cid = channel_id(&e.channel_id)?;
        let mid = message_id(&e.message_id)?;
        messages.access(me, cid).await?.require_view()?;
        targets.push((cid, mid));
    }
    for (cid, mid) in targets {
        let state = deps.service.ack(me, cid, mid).await?;
        announce(&deps, me, &state, mid).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Mark a whole guild as read: every channel the caller can view that has a message
/// is acknowledged up to its latest message (and its mention badge resets).
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/ack",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 204, description = "Acknowledged"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn ack_guild(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    Ext(messages): Ext<MessagesDeps>,
    Path(guild): Path<String>,
) -> MsgResult<StatusCode> {
    let me = actor(&p)?;
    let gid: GuildId = parse_id(&guild, ErrorCode::UnknownGuild)?;
    messages
        .repo
        .get_guild(gid)
        .await?
        .ok_or(ErrorCode::UnknownGuild)?;
    messages
        .repo
        .get_member(gid, me)
        .await?
        .ok_or(ErrorCode::MissingAccess)?;
    let mut channels = messages.repo.list_guild_channels(gid).await?;
    channels.extend(messages.repo.list_guild_threads(gid).await?);
    for ch in channels {
        let Some(last) = ch.last_message_id else {
            continue;
        };
        if messages.access(me, ch.id).await?.require_view().is_err() {
            continue;
        }
        let state = deps.service.ack(me, ch.id, last).await?;
        announce(&deps, me, &state, last).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Every read marker of the caller (the gateway READY payload carries the same list).
#[utoipa::path(
    get,
    path = "/users/@me/read-states",
    responses((status = 200, body = ReadStatesResponse)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list(p: Principal, Ext(deps): Ext<SocialDeps>) -> MsgResult<Json<ReadStatesResponse>> {
    let me = actor(&p)?;
    let states = deps.service.read_states(me).await?;
    Ok(Json(ReadStatesResponse {
        entries: states.iter().map(ReadStateObject::from).collect(),
    }))
}

/// `MESSAGE_ACK` to the caller's other sessions.
async fn announce(deps: &SocialDeps, me: UserId, state: &ReadState, message: MessageId) {
    deps.emit(SocialEvent::new(
        "MESSAGE_ACK",
        vec![me],
        json!({
            "channel_id": state.channel.to_string(),
            "message_id": message.to_string(),
            "mention_count": state.mention_count,
        }),
    ))
    .await;
}
