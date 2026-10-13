//! Permission resolution for channels that may be threads, shared by the chat
//! service and the message endpoints so a thread id behaves like any channel id.

use super::rules::thread_permissions;
use crate::domain::{Channel, Guild, Member, Role};
use crate::perms::channel_permissions;
use crate::repo::{ChannelRepository, RepoResult};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, UserId};

/// The channel whose overwrites govern `channel`: itself, or the parent of a thread.
pub fn overwrite_source(channel: &Channel) -> Option<ChannelId> {
    if channel.kind.is_thread() {
        channel.parent_id
    } else {
        Some(channel.id)
    }
}

/// Effective permissions of `member` in a guild `channel`, thread aware. Threads
/// inherit the parent's overwrites; private threads additionally need membership
/// or `MANAGE_THREADS`.
pub async fn guild_channel_permissions<R: ChannelRepository + ?Sized>(
    repo: &R,
    guild: &Guild,
    member: &Member,
    roles: &[Role],
    channel: &Channel,
    now_ms: i64,
) -> RepoResult<Permissions> {
    let Some(source) = overwrite_source(channel) else {
        // A thread whose parent is gone cannot be seen.
        return Ok(Permissions::empty());
    };
    let overwrites = repo.get_overwrites(source).await?;
    let perms = channel_permissions(guild, member, roles, &overwrites, now_ms);
    if !channel.kind.is_thread() {
        return Ok(perms);
    }
    let is_member = repo.is_thread_member(channel.id, member.user_id).await?;
    Ok(thread_permissions(perms, channel, is_member))
}

/// Whether `user` is a member of a private thread (always true for other channels).
pub async fn passes_thread_gate<R: ChannelRepository + ?Sized>(
    repo: &R,
    channel: &Channel,
    user: UserId,
) -> RepoResult<bool> {
    if channel.kind == crate::domain::ChannelType::PrivateThread {
        repo.is_thread_member(channel.id, user).await
    } else {
        Ok(true)
    }
}
