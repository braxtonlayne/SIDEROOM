//! System messages posted by other subsystems: "X joined the server" (LNG-GLD-008).

use super::deps::MessagesDeps;
use super::error::MsgResult;
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
    let Some(ch) = deps.repo.get_channel(channel_id).await? else {
        return Ok(());
    };
    let msg = Message {
        id: MessageId::from(deps.next_id()),
        channel_id,
        guild_id: ch.guild_id,
        author_id: user,
        kind: MessageType::UserJoin,
        content: String::new(),
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
