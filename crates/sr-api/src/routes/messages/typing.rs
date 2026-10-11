//! `POST /channels/{id}/typing` (LNG-MSG-013): broadcast `TYPING_START`; clients expire
//! the indicator after 10 s.

use super::deps::MessagesDeps;
use super::error::{MsgResult, parse_id};
use super::render::actor;
use crate::routes::Ext;
use crate::{Principal, openapi::ErrorBody};
use axum::{extract::Path, http::StatusCode};
use sr_chat::ChatError;
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, ErrorCode};
use sr_events::DomainEvent;

/// Trigger the typing indicator.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/typing",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn trigger_typing(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path(channel): Path<String>,
) -> MsgResult<StatusCode> {
    let user = actor(&p);
    let channel_id: ChannelId = parse_id(&channel, ErrorCode::UnknownChannel)?;
    let acc = deps.access(user, channel_id).await?;
    acc.require(if acc.channel.kind.is_thread() {
        Permissions::SEND_MESSAGES_IN_THREADS
    } else {
        Permissions::SEND_MESSAGES
    })?;
    if !acc.channel.kind.accepts_messages() {
        return Err(ChatError::MissingPermissions.into());
    }
    deps.publish(DomainEvent::TypingStarted {
        guild: acc.channel.guild_id,
        channel: channel_id,
        user,
        timestamp: u64::try_from(deps.now_ms().div_euclid(1000)).unwrap_or(0),
    })
    .await;
    Ok(StatusCode::NO_CONTENT)
}
