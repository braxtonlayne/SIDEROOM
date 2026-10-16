//! Reactions: add, remove (own, someone's, all, per emoji) and list (LNG-MSG-009).

use super::deps::{Access, MessagesDeps};
use super::error::{ApiQuery, MsgError, MsgResult, invalid, parse_id};
use super::model::{ReactionTypeQuery, ReactionUsersQuery, UserListEntry};
use super::render::actor;
use super::store::EmojiKey;
use crate::routes::Ext;
use crate::{Principal, openapi::ErrorBody};
use axum::{Json, extract::Path, http::StatusCode};
use sr_chat::message_rules::{validate_add_reaction, validate_remove_reaction};
use sr_chat::{ChatError, Message};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, ErrorCode, MessageId, UserId};
use sr_events::DomainEvent;

struct Target {
    user: UserId,
    channel: ChannelId,
    message: Message,
    acc: Access,
}

async fn target(
    deps: &MessagesDeps,
    p: &Principal,
    channel: &str,
    message: &str,
) -> MsgResult<Target> {
    let user = actor(p);
    let channel: ChannelId = parse_id(channel, ErrorCode::UnknownChannel)?;
    let message_id: MessageId = parse_id(message, ErrorCode::UnknownMessage)?;
    let acc = deps.access(user, channel).await?;
    acc.require_view()?;
    let message = deps.visible_message(channel, message_id).await?;
    Ok(Target {
        user,
        channel,
        message,
        acc,
    })
}

fn emoji(raw: &str) -> MsgResult<EmojiKey> {
    EmojiKey::parse(raw).ok_or_else(|| MsgError::from(ErrorCode::UnknownEmoji))
}

fn guild_of(t: &Target) -> Option<sr_events::GuildId> {
    t.message.guild_id
}

/// Add the caller's reaction.
#[utoipa::path(
    put,
    path = "/channels/{channel_id}/messages/{message_id}/reactions/{emoji}/@me",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("message_id" = String, Path, description = "Message snowflake"),
        ("emoji" = String, Path, description = "Unicode emoji, or `name:id` for custom emoji (URL-encoded)"),
        ("type" = Option<u8>, Query, description = "Accepted for Discord parity and ignored: burst reactions are not built (D-231)"),
    ),
    responses(
        (status = 204),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn add_own(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path((channel, message, raw_emoji)): Path<(String, String, String)>,
    ApiQuery(_q): ApiQuery<ReactionTypeQuery>,
) -> MsgResult<StatusCode> {
    let t = target(&deps, &p, &channel, &message).await?;
    let emoji = emoji(&raw_emoji)?;
    let existing = deps
        .store
        .reactions_for(&[t.message.id], t.user)
        .await?
        .remove(&t.message.id)
        .unwrap_or_default();
    let present = existing.iter().any(|r| r.emoji == emoji);
    validate_add_reaction(
        &t.message,
        t.acc.perms,
        present,
        existing.len(),
        &deps.limits,
    )?;
    let added = deps
        .store
        .add_reaction(t.channel, t.message.id, t.user, &emoji, deps.now_ms())
        .await?;
    if added {
        deps.publish(DomainEvent::ReactionAdded {
            guild: guild_of(&t),
            channel: t.channel,
            message_id: t.message.id.get(),
            user: t.user,
            emoji: emoji.json(),
        })
        .await;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_for(
    deps: &MessagesDeps,
    t: &Target,
    owner: UserId,
    emoji: &EmojiKey,
) -> MsgResult<StatusCode> {
    validate_remove_reaction(t.user, owner, t.acc.perms)?;
    if !deps
        .store
        .remove_reaction(t.message.id, owner, emoji)
        .await?
    {
        return Err(ErrorCode::UnknownEmoji.into());
    }
    deps.publish(DomainEvent::ReactionRemoved {
        guild: guild_of(t),
        channel: t.channel,
        message_id: t.message.id.get(),
        user: owner,
        emoji: emoji.json(),
    })
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Remove the caller's reaction.
#[utoipa::path(
    delete,
    path = "/channels/{channel_id}/messages/{message_id}/reactions/{emoji}/@me",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("message_id" = String, Path, description = "Message snowflake"),
        ("emoji" = String, Path, description = "Unicode emoji, or `name:id`"),
        ("type" = Option<u8>, Query, description = "Accepted and ignored (D-231)"),
    ),
    responses((status = 204), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn remove_own(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path((channel, message, raw_emoji)): Path<(String, String, String)>,
    ApiQuery(_q): ApiQuery<ReactionTypeQuery>,
) -> MsgResult<StatusCode> {
    let t = target(&deps, &p, &channel, &message).await?;
    let emoji = emoji(&raw_emoji)?;
    remove_for(&deps, &t, t.user, &emoji).await
}

/// Remove another user's reaction (MANAGE_MESSAGES).
#[utoipa::path(
    delete,
    path = "/channels/{channel_id}/messages/{message_id}/reactions/{emoji}/{user_id}",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("message_id" = String, Path, description = "Message snowflake"),
        ("emoji" = String, Path, description = "Unicode emoji, or `name:id`"),
        ("user_id" = String, Path, description = "User snowflake"),
        ("type" = Option<u8>, Query, description = "Accepted and ignored (D-231)"),
    ),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn remove_user(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path((channel, message, raw_emoji, user)): Path<(String, String, String, String)>,
    ApiQuery(_q): ApiQuery<ReactionTypeQuery>,
) -> MsgResult<StatusCode> {
    let t = target(&deps, &p, &channel, &message).await?;
    let emoji = emoji(&raw_emoji)?;
    let owner: UserId = parse_id(&user, ErrorCode::UnknownUser)?;
    remove_for(&deps, &t, owner, &emoji).await
}

/// List the users who reacted with an emoji.
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/messages/{message_id}/reactions/{emoji}",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("message_id" = String, Path, description = "Message snowflake"),
        ("emoji" = String, Path, description = "Unicode emoji, or `name:id`"),
        ("limit" = Option<u32>, Query, description = "1-100, default 25"),
        ("after" = Option<String>, Query, description = "User id to start after"),
        ("type" = Option<u8>, Query, description = "0 or 1; 1 answers an empty list (burst reactions are not built, D-231)"),
    ),
    responses((status = 200, body = Vec<UserListEntry>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_users(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path((channel, message, raw_emoji)): Path<(String, String, String)>,
    ApiQuery(q): ApiQuery<ReactionUsersQuery>,
) -> MsgResult<Json<Vec<UserListEntry>>> {
    let t = target(&deps, &p, &channel, &message).await?;
    if !t.acc.perms.has(Permissions::READ_MESSAGE_HISTORY) {
        return Err(ChatError::MissingPermissions.into());
    }
    let emoji = emoji(&raw_emoji)?;
    if q.kind == Some(1) {
        return Ok(Json(Vec::new()));
    }
    let limit = match q.limit {
        None => 25,
        Some(l) if (1..=100).contains(&l) => l as usize,
        Some(_) => {
            return Err(invalid(
                "limit",
                "NUMBER_TYPE_MAX",
                "Must be between 1 and 100.",
            ));
        }
    };
    let after = q
        .after
        .as_deref()
        .map(|s| s.parse::<UserId>())
        .transpose()
        .map_err(|_| invalid("after", "NUMBER_TYPE_COERCE", "Value is not snowflake."))?;
    let users = deps
        .store
        .reaction_users(t.message.id, &emoji, after, limit)
        .await?;
    Ok(Json(
        users
            .into_iter()
            .map(|u| UserListEntry { id: u.to_string() })
            .collect(),
    ))
}

/// Remove every reaction from a message (MANAGE_MESSAGES).
#[utoipa::path(
    delete,
    path = "/channels/{channel_id}/messages/{message_id}/reactions",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("message_id" = String, Path, description = "Message snowflake"),
    ),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn clear_all(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path((channel, message)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let t = target(&deps, &p, &channel, &message).await?;
    t.acc.require(Permissions::MANAGE_MESSAGES)?;
    deps.store.clear_reactions(t.message.id).await?;
    deps.publish(DomainEvent::ReactionsCleared {
        guild: guild_of(&t),
        channel: t.channel,
        message_id: t.message.id.get(),
    })
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Remove all reactions of one emoji (MANAGE_MESSAGES).
#[utoipa::path(
    delete,
    path = "/channels/{channel_id}/messages/{message_id}/reactions/{emoji}",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("message_id" = String, Path, description = "Message snowflake"),
        ("emoji" = String, Path, description = "Unicode emoji, or `name:id`"),
    ),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn clear_emoji(
    p: Principal,
    Ext(deps): Ext<MessagesDeps>,
    Path((channel, message, raw_emoji)): Path<(String, String, String)>,
) -> MsgResult<StatusCode> {
    let t = target(&deps, &p, &channel, &message).await?;
    t.acc.require(Permissions::MANAGE_MESSAGES)?;
    let emoji = emoji(&raw_emoji)?;
    if !deps.store.clear_emoji(t.message.id, &emoji).await? {
        return Err(ErrorCode::UnknownEmoji.into());
    }
    deps.publish(DomainEvent::ReactionEmojiCleared {
        guild: guild_of(&t),
        channel: t.channel,
        message_id: t.message.id.get(),
        emoji: emoji.json(),
    })
    .await;
    Ok(StatusCode::NO_CONTENT)
}
