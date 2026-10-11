//! Repository traits. The domain logic depends only on these; Postgres
//! implementations live in `sr-db` (`chat_repo`).

use crate::domain::{Channel, Guild, Member, Message, Role};
use crate::error::RepoError;
use crate::guild_rules::GuildBlueprint;
use crate::message_rules::{DeletePlan, EditPlan};
use async_trait::async_trait;
use sr_core::permissions::Overwrite;
use sr_core::{ChannelId, GuildId, MessageId, RoleId, UserId};

pub type RepoResult<T> = Result<T, RepoError>;

#[async_trait]
pub trait GuildRepository: Send + Sync {
    /// Insert guild, `@everyone`, owner membership and default channels atomically.
    async fn insert_guild(&self, blueprint: &GuildBlueprint) -> RepoResult<()>;
    async fn get_guild(&self, id: GuildId) -> RepoResult<Option<Guild>>;
    async fn get_member(&self, guild: GuildId, user: UserId) -> RepoResult<Option<Member>>;
    /// All roles including `@everyone`.
    async fn list_roles(&self, guild: GuildId) -> RepoResult<Vec<Role>>;
    /// Insert a role, first shifting every role with `position >= role.position` up by one.
    async fn insert_role(&self, role: &Role) -> RepoResult<()>;
    async fn update_role(&self, role: &Role) -> RepoResult<()>;
    /// Delete the role and strip it from all members.
    async fn delete_role(&self, guild: GuildId, role: RoleId) -> RepoResult<()>;
}

#[async_trait]
pub trait ChannelRepository: Send + Sync {
    async fn get_channel(&self, id: ChannelId) -> RepoResult<Option<Channel>>;
    async fn list_guild_channels(&self, guild: GuildId) -> RepoResult<Vec<Channel>>;
    async fn get_overwrites(&self, channel: ChannelId) -> RepoResult<Vec<Overwrite>>;
    async fn insert_channel(&self, channel: &Channel, overwrites: &[Overwrite]) -> RepoResult<()>;
    async fn update_channel(&self, channel: &Channel) -> RepoResult<()>;
    /// Delete the channel; children of a category are orphaned, not deleted.
    async fn delete_channel(&self, id: ChannelId) -> RepoResult<()>;
    /// Whether `user` is a recipient of a DM or group DM.
    async fn is_recipient(&self, channel: ChannelId, user: UserId) -> RepoResult<bool>;
}

#[async_trait]
pub trait MessageRepository: Send + Sync {
    async fn get_message(&self, channel: ChannelId, id: MessageId) -> RepoResult<Option<Message>>;
    /// Messages of `channel` among `ids` (any visibility); missing ids are omitted.
    async fn get_messages(&self, channel: ChannelId, ids: &[MessageId])
    -> RepoResult<Vec<Message>>;
    /// Insert and bump `channels.last_message_id` in one transaction.
    async fn insert_message(&self, message: &Message) -> RepoResult<()>;
    /// Persist an edit and append a `message_edits` row when
    /// `plan.history_previous_content` is set.
    async fn apply_edit(&self, plan: &EditPlan) -> RepoResult<()>;
    async fn apply_delete(&self, plan: &DeletePlan) -> RepoResult<()>;
    /// Time the user last posted in the channel, for slowmode.
    async fn last_sent_ms(&self, channel: ChannelId, user: UserId) -> RepoResult<Option<i64>>;
    async fn count_pins(&self, channel: ChannelId) -> RepoResult<usize>;
    async fn set_pinned(
        &self,
        channel: ChannelId,
        message: MessageId,
        pinned_by: UserId,
        pinned: bool,
        now_ms: i64,
    ) -> RepoResult<()>;
}

/// Everything the service needs.
pub trait ChatRepository: GuildRepository + ChannelRepository + MessageRepository {}
impl<T: GuildRepository + ChannelRepository + MessageRepository> ChatRepository for T {}
