//! System messages posted by other subsystems: "X joined the server" (LNG-GLD-008), a followed
//! announcement channel and a server-level contribution (LNG-MSG-016).

use super::deps::MessagesDeps;
use super::error::{MsgError, MsgResult};
use super::render::event_payload;
use sr_chat::{Message, MessageType, Visibility};
use sr_core::{GuildId, MessageId, UserId};
use sr_events::DomainEvent;

/// Post the join notice (message type 7) for `user` in the server's system channel.
/// Nothing happens when the server has no system channel or it is gone. The notice is
/// authored by the member who joined, like Discord's. Failures are logged and swallowed:
/// the membership already exists and a missing notice must not undo it.
pub async fn post_user_join(deps: &MessagesDeps, guild: GuildId, user: UserId) {
    if let Err(e) = try_post_user_join(deps, guild, user).await {
        tracing::warn!(
            guild = guild.get(),
            user = user.get(),
            "join notice failed: {e:?}"
        );
    }
}

async fn try_post_user_join(deps: &MessagesDeps, guild: GuildId, user: UserId) -> MsgResult<()> {
    let Some(g) = deps.repo.get_guild(guild).await? else {
        return Ok(());
    };
    let Some(channel_id) = g.system_channel_id else {
        return Ok(());
    };
    // "Send a random welcome message" is off (G-36 system_channel_flags bit 0).
    if g.system_channel_flags & sr_chat::domain::system_flags::SUPPRESS_JOIN_NOTIFICATIONS != 0 {
        return Ok(());
    }
    store_and_announce(deps, channel_id, user, MessageType::UserJoin, String::new()).await
}

/// The system message type for a server level (Discord's boost types: 8 for the first
/// contribution, 9 to 11 when it brings the server to level 1 to 3).
#[must_use]
pub fn support_type(level: Option<u8>) -> MessageType {
    match level {
        Some(1) => MessageType::ServerSupportLevel1,
        Some(2) => MessageType::ServerSupportLevel2,
        Some(3) => MessageType::ServerSupportLevel3,
        _ => MessageType::ServerSupport,
    }
}

/// Post "X supported this server" (LNG-MSG-016) in the system channel after a server-level
/// contribution was credited. `level` is the server's level after it. Nothing happens
/// without a system channel or when "boost" notices are switched off (G-36 flag bit 1).
/// Failures are logged: the payment is already credited.
pub async fn post_server_support(
    deps: &MessagesDeps,
    guild: GuildId,
    user: UserId,
    level: Option<u8>,
) {
    if let Err(e) = try_post_server_support(deps, guild, user, level).await {
        tracing::warn!(guild = guild.get(), "support notice failed: {e:?}");
    }
}

async fn try_post_server_support(
    deps: &MessagesDeps,
    guild: GuildId,
    user: UserId,
    level: Option<u8>,
) -> MsgResult<()> {
    let Some(g) = deps.repo.get_guild(guild).await? else {
        return Ok(());
    };
    let Some(channel_id) = g.system_channel_id else {
        return Ok(());
    };
    if g.system_channel_flags & sr_chat::domain::system_flags::SUPPRESS_PREMIUM_SUBSCRIPTIONS != 0 {
        return Ok(());
    }
    let content = level.map(|l| l.to_string()).unwrap_or_default();
    store_and_announce(deps, channel_id, user, support_type(level), content).await
}

/// Post "X added <channel> to this channel" (type 12) in the channel that now receives an
/// announcement channel's posts. `source_name` is the followed channel's name. Failures are
/// logged: the follow already exists.
pub async fn post_follow_added(
    deps: &MessagesDeps,
    target: sr_core::ChannelId,
    user: UserId,
    source_name: &str,
) {
    if let Err(e) = store_and_announce(
        deps,
        target,
        user,
        MessageType::ChannelFollowAdd,
        source_name.to_owned(),
    )
    .await
    {
        tracing::warn!(channel = target.get(), "follow notice failed: {e:?}");
    }
}

/// Store a system message of `kind` in `channel_id` and announce it like any message.
async fn store_and_announce(
    deps: &MessagesDeps,
    channel_id: sr_core::ChannelId,
    user: UserId,
    kind: MessageType,
    content: String,
) -> MsgResult<()> {
    let Some(ch) = deps.repo.get_channel(channel_id).await? else {
        return Ok(());
    };
    let msg = Message {
        id: MessageId::from(deps.next_id()),
        channel_id,
        guild_id: ch.guild_id,
        author_id: user,
        kind,
        content,
        flags: 0,
        edited_at_ms: None,
        reference: None,
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
    deps.repo.insert_message(&msg).await?;
    let data = event_payload(deps, &msg).await?;
    deps.publish(DomainEvent::MessageSent {
        guild: msg.guild_id,
        channel: channel_id,
        message_id: msg.id.get(),
        author: user,
        data,
    })
    .await;
    Ok(())
}

/// Post a card (type 24, authored by the system user) with embeds and components into a
/// guild channel: the AutoMod alerts and incident alerts of D-243/D-244. The system user is
/// not a member of the server, so this stores the message directly instead of going through
/// the send checks. Returns the stored message.
pub async fn post_system_card(
    deps: &MessagesDeps,
    channel_id: sr_core::ChannelId,
    embeds: Vec<sr_chat::message::Embed>,
    components: Vec<sr_chat::message::Component>,
) -> MsgResult<Message> {
    let ch = deps
        .repo
        .get_channel(channel_id)
        .await?
        .ok_or_else(|| MsgError::from(sr_core::ErrorCode::UnknownChannel))?;
    let user = UserId::new(sr_chat::SYSTEM_USER_ID);
    let msg = Message {
        id: MessageId::from(deps.next_id()),
        channel_id,
        guild_id: ch.guild_id,
        author_id: user,
        kind: MessageType::AutoModerationAction,
        content: String::new(),
        flags: 0,
        edited_at_ms: None,
        reference: None,
        attachment_ids: Vec::new(),
        embeds,
        components,
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
    deps.repo.insert_message(&msg).await?;
    let data = event_payload(deps, &msg).await?;
    deps.publish(DomainEvent::MessageSent {
        guild: msg.guild_id,
        channel: channel_id,
        message_id: msg.id.get(),
        author: user,
        data,
    })
    .await;
    Ok(msg)
}

/// Rewrite a system card in place (embeds and components) and tell the clients with
/// `MESSAGE_UPDATE`. `false` when the message is gone.
pub async fn rewrite_system_card(
    deps: &MessagesDeps,
    channel_id: sr_core::ChannelId,
    message_id: MessageId,
    embeds: &[sr_chat::message::Embed],
    components: &[sr_chat::message::Component],
) -> MsgResult<bool> {
    let replaced = deps
        .repo
        .replace_card(
            channel_id,
            message_id,
            "",
            embeds,
            components,
            deps.now_ms(),
        )
        .await?;
    if !replaced {
        return Ok(false);
    }
    let msg = deps.visible_message(channel_id, message_id).await?;
    let data = event_payload(deps, &msg).await?;
    deps.publish(DomainEvent::MessageEdited {
        guild: msg.guild_id,
        channel: channel_id,
        message_id: msg.id.get(),
        data,
    })
    .await;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn support_notices_follow_the_server_level() {
        let codes: Vec<i16> = [None, Some(0), Some(1), Some(2), Some(3)]
            .into_iter()
            .map(|l| support_type(l).code())
            .collect();
        assert_eq!(codes, [8, 8, 9, 10, 11]);
        for c in 8..=12 {
            assert_eq!(MessageType::from_code(c).map(MessageType::code), Some(c));
            assert!(MessageType::from_code(c).unwrap().is_system());
        }
    }
}
