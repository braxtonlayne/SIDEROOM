//! Pins (LNG-MSG-008): list, pin, unpin, plus the "pinned a message" system message.

use super::deps::MessagesDeps;
use super::error::{MsgResult, parse_id};
use super::model::{MessageObject, PinEntry, PinsPage, iso};
use super::render::{actor, event_payload, render_many};
use crate::routes::Ext;
use crate::routes::guilds::safety::MfaGate;
use crate::{Principal, openapi::ErrorBody};
use axum::{Json, extract::Path, http::StatusCode};
use sr_chat::settings::audit::AuditAction;
use sr_chat::{Message, MessageReference, MessageType, Visibility};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, ErrorCode, MessageId};
use sr_events::DomainEvent;

async fn pinned(
    deps: &MessagesDeps,
    p: &Principal,
    channel: &str,
) -> MsgResult<Vec<(MessageObject, i64)>> {
    let user = actor(p);
    let channel: ChannelId = parse_id(channel, ErrorCode::UnknownChannel)?;
    let acc = deps.access(user, channel).await?;
    acc.require(Permissions::READ_MESSAGE_HISTORY)?;
    let pins = deps.store.list_pins(channel).await?;
    let (msgs, at): (Vec<Message>, Vec<i64>) = pins.into_iter().unzip();
    let objs = render_many(deps, user, &msgs).await?;
    Ok(objs.into_iter().zip(at).collect())
}

/// Legacy pins list (array of messages).
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/pins",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = Vec<MessageObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_pins_legacy(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path(channel): Path<String>,
) -> MsgResult<Json<Vec<MessageObject>>> {
    Ok(Json(
        pinned(&deps, &p, &channel)
            .await?
            .into_iter()
            .map(|(m, _)| m)
            .collect(),
    ))
}

/// Pins with pin times (Discord's newer shape). At most 250 pins exist, so one page.
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/messages/pins",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = PinsPage), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_pins(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path(channel): Path<String>,
) -> MsgResult<Json<PinsPage>> {
    let items = pinned(&deps, &p, &channel)
        .await?
        .into_iter()
        .map(|(message, at)| PinEntry {
            pinned_at: iso(at),
            message,
        })
        .collect();
    Ok(Json(PinsPage {
        items,
        has_more: false,
    }))
}

fn gid(c: &sr_chat::Channel) -> Option<sr_events::GuildId> {
    c.guild_id
}

/// Pin a message (MANAGE_MESSAGES, max 250 per channel).
#[utoipa::path(
    put,
    path = "/channels/{channel_id}/pins/{message_id}",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("message_id" = String, Path, description = "Message snowflake"),
    ),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn pin(
    p: Principal,
    gate: MfaGate,
    Ext(deps): Ext<MessagesDeps>,
    Path((channel, message)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let user = actor(&p);
    let channel_id: ChannelId = parse_id(&channel, ErrorCode::UnknownChannel)?;
    let message_id: MessageId = parse_id(&message, ErrorCode::UnknownMessage)?;
    let author = deps
        .repo
        .get_message(channel_id, message_id)
        .await?
        .map(|m| m.author_id);
    let guild = deps
        .repo
        .get_channel(channel_id)
        .await?
        .and_then(|c| c.guild_id);
    deps.require_mfa(&gate, user, guild).await?;
    deps.pin(user, channel_id, message_id).await?;
    let now = deps.now_ms();
    let ch = deps.access(user, channel_id).await?.channel;

    // System message "X pinned a message to this channel".
    let sys = Message {
        id: MessageId::from(deps.next_id()),
        channel_id,
        guild_id: ch.guild_id,
        author_id: user,
        kind: MessageType::ChannelPinnedMessage,
        content: String::new(),
        flags: 0,
        edited_at_ms: None,
        reference: Some(MessageReference {
            message_id,
            channel_id,
            forward: None,
        }),
        attachment_ids: Vec::new(),
        embeds: Vec::new(),
        components: Vec::new(),
        sticker_ids: Vec::new(),
        poll: None,
        mention_everyone: false,
        mention_user_ids: Vec::new(),
        mention_role_ids: Vec::new(),
        mention_channel_ids: Vec::new(),
        pinned: false,
        visibility: Visibility::Visible,
        deleted_at_ms: None,
        purge_after_ms: None,
        hold_id: None,
    };
    deps.repo.insert_message(&sys).await?;
    let data = event_payload(&deps, &sys).await?;
    deps.publish(DomainEvent::MessageSent {
        guild: gid(&ch),
        channel: channel_id,
        message_id: sys.id.get(),
        author: user,
        data,
    })
    .await;
    deps.publish(DomainEvent::ChannelPinsUpdated {
        guild: gid(&ch),
        channel: channel_id,
        last_pin_ms: Some(now),
    })
    .await;
    deps.audit(
        ch.guild_id,
        user,
        AuditAction::MessagePin,
        author.map(|a| a.get()),
        serde_json::json!({ "channel_id": channel_id.to_string(), "message_id": message_id.to_string() }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Unpin a message.
#[utoipa::path(
    delete,
    path = "/channels/{channel_id}/pins/{message_id}",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("message_id" = String, Path, description = "Message snowflake"),
    ),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn unpin(
    p: Principal,
    gate: MfaGate,
    Ext(deps): Ext<MessagesDeps>,
    Path((channel, message)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let user = actor(&p);
    let channel_id: ChannelId = parse_id(&channel, ErrorCode::UnknownChannel)?;
    let message_id: MessageId = parse_id(&message, ErrorCode::UnknownMessage)?;
    let author = deps
        .repo
        .get_message(channel_id, message_id)
        .await?
        .map(|m| m.author_id);
    let guild = deps
        .repo
        .get_channel(channel_id)
        .await?
        .and_then(|c| c.guild_id);
    deps.require_mfa(&gate, user, guild).await?;
    deps.unpin(user, channel_id, message_id).await?;
    let ch = deps.access(user, channel_id).await?.channel;
    let last = deps
        .store
        .list_pins(channel_id)
        .await?
        .first()
        .map(|(_, at)| *at);
    deps.publish(DomainEvent::ChannelPinsUpdated {
        guild: gid(&ch),
        channel: channel_id,
        last_pin_ms: last,
    })
    .await;
    deps.audit(
        ch.guild_id,
        user,
        AuditAction::MessageUnpin,
        author.map(|a| a.get()),
        serde_json::json!({ "channel_id": channel_id.to_string(), "message_id": message_id.to_string() }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}
