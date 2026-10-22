//! Announcement channels (LNG-CHN-004): publishing a message to the channels that follow it.
//!
//! * `POST /channels/{id}/messages/{id}/crosspost`: publish one message by hand.
//! * Auto-publish: an announcement channel with `channel_flags::AUTO_PUBLISH` publishes every
//!   message as soon as it is sent (see [`auto_publish`], called by `create_message`).
//!
//! Following is `POST /channels/{id}/followers` (settings module). A follower is a webhook of
//! kind 2 in the target channel that remembers the source channel.
//!
//! A published copy is a message in the follower's channel that carries `IS_CROSSPOST`, the
//! original author, the text and the embeds, and a reference to the source message. It pings
//! nobody and carries no attachments, stickers or polls (those would need copying files that
//! belong to another server).

use super::deps::MessagesDeps;
use super::error::{MsgResult, parse_id};
use super::model::MessageObject;
use super::render::{actor, event_payload, render_one};
use crate::routes::settings::SettingsDeps;
use crate::routes::{Ext, MaybeExt};
use crate::{Principal, openapi::ErrorBody};
use axum::{Json, extract::Path};
use sr_chat::settings::repo::WebhookRepository;
use sr_chat::threads::channel_flags;
use sr_chat::{ChannelType, Message, MessageReference, MessageType, Visibility, message_flags};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, ErrorCode, MessageId};
use sr_events::DomainEvent;

/// Whether `msg` may be published at all: a plain user message that was not published
/// before.
fn publishable(msg: &Message) -> Result<(), ErrorCode> {
    if !matches!(msg.kind, MessageType::Default | MessageType::Reply) {
        return Err(ErrorCode::CannotExecuteOnChannelType);
    }
    if msg.flags & message_flags::CROSSPOSTED != 0 || msg.flags & message_flags::IS_CROSSPOST != 0 {
        return Err(ErrorCode::MessageAlreadyCrossposted);
    }
    Ok(())
}

/// Copy `src` into every channel that follows its channel and mark it `CROSSPOSTED`.
/// Returns how many followers received it. A follower whose channel is gone or cannot take
/// messages is skipped; nothing here fails the caller (the source message already exists).
pub async fn deliver(messages: &MessagesDeps, settings: &SettingsDeps, src: &Message) -> usize {
    let followers = match settings
        .service
        .repo()
        .list_channel_followers(src.channel_id)
        .await
    {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(error = %e, "listing followers failed");
            return 0;
        }
    };
    let mut delivered = 0;
    let source_restricted = messages.channel_age_restricted(src.channel_id).await;
    for w in &followers {
        let Ok(Some(target)) = messages.repo.get_channel(w.channel_id).await else {
            continue;
        };
        if !target.kind.accepts_messages() || target.kind.is_dm() {
            continue;
        }
        // MOD-AGE-004: an age-restricted announcement channel's previews do not reach followers without the gate.
        let keep_embeds = !sr_chat::sensitive::embeds_stay_behind(
            source_restricted,
            messages.channel_age_restricted(target.id).await,
        );
        let copy = Message {
            id: MessageId::from(messages.next_id()),
            channel_id: target.id,
            guild_id: target.guild_id,
            author_id: src.author_id,
            kind: MessageType::Default,
            content: src.content.clone(),
            flags: message_flags::IS_CROSSPOST
                | (src.flags & message_flags::SUPPRESS_EMBEDS)
                | if keep_embeds {
                    0
                } else {
                    message_flags::SUPPRESS_EMBEDS
                },
            edited_at_ms: None,
            reference: Some(MessageReference {
                message_id: src.id,
                channel_id: src.channel_id,
                forward: None,
            }),
            attachment_ids: Vec::new(),
            embeds: if keep_embeds {
                src.embeds.clone()
            } else {
                Vec::new()
            },
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
        if let Err(e) = messages.repo.insert_message(&copy).await {
            tracing::warn!(error = %e, "storing a published copy failed");
            continue;
        }
        match event_payload(messages, &copy).await {
            Ok(data) => {
                messages
                    .publish(DomainEvent::MessageSent {
                        guild: copy.guild_id,
                        channel: copy.channel_id,
                        message_id: copy.id.get(),
                        author: copy.author_id,
                        data,
                    })
                    .await;
            }
            Err(_) => tracing::warn!("rendering a published copy failed"),
        }
        delivered += 1;
    }
    // The source learns it was published, also with no followers yet (Discord marks it too).
    let flags = src.flags | message_flags::CROSSPOSTED;
    if let Err(e) = messages
        .repo
        .set_message_flags(src.channel_id, src.id, flags)
        .await
    {
        tracing::warn!(error = %e, "marking a message as crossposted failed");
        return delivered;
    }
    let mut updated = src.clone();
    updated.flags = flags;
    if let Ok(data) = event_payload(messages, &updated).await {
        messages
            .publish(DomainEvent::MessageEdited {
                guild: updated.guild_id,
                channel: updated.channel_id,
                message_id: updated.id.get(),
                data,
            })
            .await;
    }
    delivered
}

/// Called after a message was stored: publish it when its channel is an announcement
/// channel set to auto-publish and the message is publishable.
pub async fn auto_publish(messages: &MessagesDeps, settings: &SettingsDeps, msg: &Message) {
    if publishable(msg).is_err() {
        return;
    }
    let Ok(Some(ch)) = messages.repo.get_channel(msg.channel_id).await else {
        return;
    };
    if ch.kind == ChannelType::Announcement && ch.flags & channel_flags::AUTO_PUBLISH != 0 {
        deliver(messages, settings, msg).await;
    }
}

/// Publish an announcement to the channels that follow this one. Needs `SEND_MESSAGES` for
/// your own message and `MANAGE_MESSAGES` for someone else's. Answers the updated message
/// (flag `CROSSPOSTED`).
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/messages/{message_id}/crosspost",
    params(
        ("channel_id" = String, Path, description = "Announcement channel snowflake"),
        ("message_id" = String, Path, description = "Message snowflake"),
    ),
    responses(
        (status = 200, body = MessageObject),
        (status = 400, body = ErrorBody, description = "Not an announcement channel (50024) or already published (40033)"),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn crosspost(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    MaybeExt(settings): MaybeExt<SettingsDeps>,
    Path((channel_id, message_id)): Path<(String, String)>,
) -> MsgResult<Json<MessageObject>> {
    let user = actor(&p);
    let channel_id: ChannelId = parse_id(&channel_id, ErrorCode::UnknownChannel)?;
    let message_id: MessageId = parse_id(&message_id, ErrorCode::UnknownMessage)?;
    let acc = deps.access(user, channel_id).await?;
    acc.require_view()?;
    if acc.channel.kind != ChannelType::Announcement {
        return Err(ErrorCode::CannotExecuteOnChannelType.into());
    }
    let msg = deps.visible_message(channel_id, message_id).await?;
    acc.require(if msg.author_id == user {
        Permissions::SEND_MESSAGES
    } else {
        Permissions::MANAGE_MESSAGES
    })?;
    publishable(&msg)?;
    match &settings {
        Some(s) => {
            deliver(&deps, s, &msg).await;
        }
        None => {
            tracing::error!("crosspost needs SettingsDeps registered on AppState");
            return Err(ErrorCode::SrInternal.into());
        }
    }
    let msg = deps.visible_message(channel_id, message_id).await?;
    Ok(Json(render_one(&deps, user, &msg).await?))
}
