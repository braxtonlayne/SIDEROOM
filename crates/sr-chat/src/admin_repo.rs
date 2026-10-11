//! Repository surface for guild administration: guild edit/delete, member
//! management, role/channel ordering, permission overwrites and bans.
//! Kept separate from [`crate::repo`] so the message-side traits stay stable;
//! `PgChatRepo` in `sr-db` implements both.

use crate::domain::{Channel, Guild, Member};
use crate::repo::RepoResult;
use async_trait::async_trait;
use sr_core::permissions::{Overwrite, OverwriteKind};
use sr_core::{ChannelId, GuildId, RoleId, Snowflake, UserId};

/// A ban row (`guild_bans`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ban {
    pub guild_id: GuildId,
    pub user_id: UserId,
    pub reason: Option<String>,
    pub banned_by: Option<UserId>,
    pub created_at_ms: i64,
}

#[async_trait]
pub trait GuildAdminRepository: Send + Sync {
    /// Persist the editable guild columns.
    async fn update_guild(&self, guild: &Guild) -> RepoResult<()>;
    /// Delete the guild and everything under it.
    async fn delete_guild(&self, id: GuildId) -> RepoResult<()>;
    /// Guilds `user` is a member of, ordered by guild id.
    async fn list_user_guilds(&self, user: UserId) -> RepoResult<Vec<Guild>>;

    /// Members ordered by user id, strictly after `after`.
    async fn list_members(
        &self,
        guild: GuildId,
        after: Option<UserId>,
        limit: usize,
    ) -> RepoResult<Vec<Member>>;
    /// Persist nick, role assignments and timeout of an existing member.
    async fn update_member(&self, member: &Member) -> RepoResult<()>;
    /// Remove the membership row (and its role assignments).
    async fn remove_member(&self, guild: GuildId, user: UserId) -> RepoResult<()>;

    /// Set the `position` of the listed roles in one transaction.
    async fn set_role_positions(
        &self,
        guild: GuildId,
        positions: &[(RoleId, i32)],
    ) -> RepoResult<()>;
    /// Persist `position` and `parent_id` of the listed channels in one transaction.
    async fn update_channel_layout(&self, channels: &[Channel]) -> RepoResult<()>;

    /// Insert or replace one overwrite.
    async fn put_overwrite(&self, channel: ChannelId, overwrite: &Overwrite) -> RepoResult<()>;
    async fn delete_overwrite(
        &self,
        channel: ChannelId,
        target: Snowflake,
        kind: OverwriteKind,
    ) -> RepoResult<()>;
    /// Replace all overwrites of a channel ("sync with category").
    async fn replace_overwrites(
        &self,
        channel: ChannelId,
        overwrites: &[Overwrite],
    ) -> RepoResult<()>;

    /// Insert (or update the reason of) a ban and remove the membership, atomically.
    async fn insert_ban(&self, ban: &Ban) -> RepoResult<()>;
    async fn delete_ban(&self, guild: GuildId, user: UserId) -> RepoResult<bool>;
    async fn get_ban(&self, guild: GuildId, user: UserId) -> RepoResult<Option<Ban>>;
    /// Bans ordered by user id; `before`/`after` are exclusive user-id cursors.
    async fn list_bans(
        &self,
        guild: GuildId,
        before: Option<UserId>,
        after: Option<UserId>,
        limit: usize,
    ) -> RepoResult<Vec<Ban>>;
}
