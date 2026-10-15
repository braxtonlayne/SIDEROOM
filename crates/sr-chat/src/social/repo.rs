//! Repository traits for the social domain. Postgres implementations live in
//! `sr-db` (`social_repo`); [`super::memory::MemoryRepo`] backs the tests.

use super::dm::{DmChannel, DmPrivacy, GroupInvite, RequestState};
use super::notifications::{ChannelOverride, NotificationLevel, ScopeSettings};
use super::presence::PresencePrefs;
use super::profile::{Connection, Profile};
use super::read_state::ReadState;
use super::relationships::{
    FriendRequestSources, RelOp, Relationship, RelationshipCounts, RelationshipKind,
};
use super::user_settings::UserSettings;
use crate::error::RepoError;
use async_trait::async_trait;
use sr_core::{ChannelId, GuildId, UserId};

pub type RepoResult<T> = Result<T, RepoError>;

/// One guild membership of a user, as the social layer needs it (mutual guilds,
/// guild-wide acknowledgement, presence fan-out).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuildMembership {
    pub guild: GuildId,
    pub nick: Option<String>,
    pub joined_at_ms: i64,
    /// Per-server profile overrides (LNG-PRF-001): avatar and banner content hashes and
    /// the about me the member set for this server only.
    pub avatar_key: Option<String>,
    pub banner_key: Option<String>,
    pub bio: Option<String>,
}

/// Per-user privacy preferences relevant to relationships and DMs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Privacy {
    pub friend_requests: FriendRequestSources,
    pub dms: DmPrivacy,
}

#[async_trait]
pub trait RelationshipRepository: Send + Sync {
    async fn user_exists(&self, user: UserId) -> RepoResult<bool>;
    async fn get_relationship(
        &self,
        user: UserId,
        target: UserId,
    ) -> RepoResult<Option<Relationship>>;
    async fn list_relationships(&self, user: UserId) -> RepoResult<Vec<Relationship>>;
    async fn counts(&self, user: UserId) -> RepoResult<RelationshipCounts>;
    /// Apply all ops atomically, in order.
    async fn apply(&self, ops: &[RelOp]) -> RepoResult<()>;
    async fn shares_guild(&self, a: UserId, b: UserId) -> RepoResult<bool>;
    async fn has_mutual_friend(&self, a: UserId, b: UserId) -> RepoResult<bool>;
    /// Users who are friends of both `a` and `b`, ascending by id.
    async fn mutual_friends(&self, a: UserId, b: UserId) -> RepoResult<Vec<UserId>>;
    /// Active account with this username (case-insensitive), if any.
    async fn find_user_by_username(&self, username: &str) -> RepoResult<Option<UserId>>;
    /// Every guild `user` belongs to, ascending by guild id.
    async fn list_guild_memberships(&self, user: UserId) -> RepoResult<Vec<GuildMembership>>;
    /// Users sharing at least one guild with `user` (excluding `user`), ascending.
    async fn guild_peers(&self, user: UserId) -> RepoResult<Vec<UserId>>;
    async fn get_privacy(&self, user: UserId) -> RepoResult<Privacy>;
    async fn put_privacy(&self, user: UserId, privacy: &Privacy) -> RepoResult<()>;
    /// Whether `user` has ignored `target` (independent of friendship).
    async fn is_ignoring(&self, user: UserId, target: UserId) -> RepoResult<bool>;
    /// Users `user` ignores.
    async fn list_ignores(&self, user: UserId) -> RepoResult<Vec<UserId>>;
    async fn put_ignore(&self, user: UserId, target: UserId) -> RepoResult<()>;
    async fn delete_ignore(&self, user: UserId, target: UserId) -> RepoResult<()>;
    /// `user`'s private note on `target`; `None` removes it.
    async fn get_note(&self, user: UserId, target: UserId) -> RepoResult<Option<String>>;
    async fn set_note(&self, user: UserId, target: UserId, note: Option<&str>) -> RepoResult<()>;
}

#[async_trait]
pub trait DmRepository: Send + Sync {
    /// The 1:1 DM between the pair, if any.
    async fn find_dm(&self, a: UserId, b: UserId) -> RepoResult<Option<DmChannel>>;
    /// Create channel, recipients and pair key atomically.
    async fn insert_dm(&self, id: ChannelId, a: UserId, b: UserId) -> RepoResult<()>;
    /// Create a group DM; `members[0]` is the owner. Members are stored in order.
    async fn insert_group(
        &self,
        id: ChannelId,
        members: &[UserId],
        name: Option<&str>,
    ) -> RepoResult<()>;
    async fn get_dm(&self, id: ChannelId) -> RepoResult<Option<DmChannel>>;
    async fn add_recipients(&self, id: ChannelId, users: &[UserId]) -> RepoResult<()>;
    async fn remove_recipient(&self, id: ChannelId, user: UserId) -> RepoResult<()>;
    async fn set_owner(&self, id: ChannelId, owner: UserId) -> RepoResult<()>;
    async fn update_group(
        &self,
        id: ChannelId,
        name: Option<&str>,
        icon_key: Option<&str>,
    ) -> RepoResult<()>;
    async fn delete_dm(&self, id: ChannelId) -> RepoResult<()>;
    /// Hide (close) or reopen a conversation for one user.
    async fn set_hidden(&self, user: UserId, id: ChannelId, hidden: bool) -> RepoResult<bool>;
    /// Conversations of `user`, most recent message first; hidden ones only on request.
    async fn list_dms(&self, user: UserId, include_hidden: bool) -> RepoResult<Vec<DmChannel>>;
    async fn count_groups(&self, user: UserId) -> RepoResult<usize>;
    /// `user`'s Message Request state in a conversation (`None` if not a recipient).
    async fn get_request_state(
        &self,
        user: UserId,
        id: ChannelId,
    ) -> RepoResult<Option<RequestState>>;
    async fn set_request_state(
        &self,
        user: UserId,
        id: ChannelId,
        state: RequestState,
    ) -> RepoResult<()>;
    /// Group DM invites (G-47).
    async fn insert_group_invite(&self, invite: &GroupInvite) -> RepoResult<()>;
    async fn get_group_invite(&self, code: &str) -> RepoResult<Option<GroupInvite>>;
    /// Mark one revoked; `false` when no such invite.
    async fn revoke_group_invite(&self, code: &str) -> RepoResult<bool>;
    /// Invites of a group, newest first (revoked and expired ones included).
    async fn list_group_invites(&self, channel: ChannelId) -> RepoResult<Vec<GroupInvite>>;
}

#[async_trait]
pub trait ReadStateRepository: Send + Sync {
    async fn get_read_state(
        &self,
        user: UserId,
        channel: ChannelId,
    ) -> RepoResult<Option<ReadState>>;
    async fn put_read_state(&self, state: &ReadState) -> RepoResult<()>;
    async fn list_read_states(&self, user: UserId) -> RepoResult<Vec<ReadState>>;
    /// Atomically add `n` mentions for each user (creating missing rows).
    async fn add_mentions(&self, users: &[UserId], channel: ChannelId, n: u32) -> RepoResult<()>;
}

#[async_trait]
pub trait NotificationRepository: Send + Sync {
    async fn get_scope(
        &self,
        user: UserId,
        guild: Option<GuildId>,
    ) -> RepoResult<Option<ScopeSettings>>;
    async fn put_scope(&self, settings: &ScopeSettings) -> RepoResult<()>;
    async fn get_override(
        &self,
        user: UserId,
        channel: ChannelId,
    ) -> RepoResult<Option<ChannelOverride>>;
    async fn put_override(&self, o: &ChannelOverride) -> RepoResult<()>;
    async fn delete_override(&self, user: UserId, channel: ChannelId) -> RepoResult<()>;
    async fn list_overrides(&self, user: UserId) -> RepoResult<Vec<ChannelOverride>>;
}

#[async_trait]
pub trait ProfileRepository: Send + Sync {
    /// Profile fields plus connections.
    async fn get_profile(&self, user: UserId) -> RepoResult<Option<Profile>>;
    /// Persist bio, pronouns, banner and accent (connections are separate).
    async fn put_profile(&self, profile: &Profile) -> RepoResult<()>;
    async fn upsert_connection(&self, c: &Connection) -> RepoResult<()>;
    async fn delete_connection(
        &self,
        user: UserId,
        provider: &str,
        external_id: &str,
    ) -> RepoResult<()>;
    async fn count_connections(&self, user: UserId) -> RepoResult<usize>;
    /// The account that holds a *verified* connection to this provider identity, if any.
    async fn connection_owner(
        &self,
        provider: &str,
        external_id: &str,
    ) -> RepoResult<Option<UserId>> {
        let _ = (provider, external_id);
        Ok(None)
    }
    /// Verified connections that show their stats and were refreshed before `before_ms`
    /// (or never), for the listed providers, oldest first.
    async fn stale_connections(
        &self,
        before_ms: i64,
        providers: &[&str],
        limit: usize,
    ) -> RepoResult<Vec<Connection>> {
        let _ = (before_ms, providers, limit);
        Ok(Vec::new())
    }
    /// Store refreshed public stats of one connection.
    async fn set_connection_stats(
        &self,
        user: UserId,
        provider: &str,
        external_id: &str,
        stats: &serde_json::Value,
        now_ms: i64,
    ) -> RepoResult<()> {
        let _ = (user, provider, external_id, stats, now_ms);
        Ok(())
    }
}

#[async_trait]
pub trait PresenceRepository: Send + Sync {
    async fn get_presence(&self, user: UserId) -> RepoResult<Option<PresencePrefs>>;
    async fn put_presence(&self, prefs: &PresencePrefs) -> RepoResult<()>;
}

#[async_trait]
pub trait UserSettingsRepository: Send + Sync {
    /// The synced settings blob; `None` until the first save.
    async fn get_user_settings(&self, user: UserId) -> RepoResult<Option<UserSettings>>;
    /// Save the blob if the stored version is still `expected_version` (0 = never
    /// saved). Returns the new version, or `None` when another session won the race.
    async fn put_user_settings(
        &self,
        user: UserId,
        data: &serde_json::Value,
        expected_version: i64,
    ) -> RepoResult<Option<i64>>;
    async fn get_default_notification_level(
        &self,
        user: UserId,
    ) -> RepoResult<Option<NotificationLevel>>;
    async fn put_default_notification_level(
        &self,
        user: UserId,
        level: Option<NotificationLevel>,
    ) -> RepoResult<()>;
}

/// Everything the service needs.
pub trait SocialRepository:
    RelationshipRepository
    + DmRepository
    + ReadStateRepository
    + NotificationRepository
    + ProfileRepository
    + PresenceRepository
    + UserSettingsRepository
{
}
impl<
    T: RelationshipRepository
        + DmRepository
        + ReadStateRepository
        + NotificationRepository
        + ProfileRepository
        + PresenceRepository
        + UserSettingsRepository,
> SocialRepository for T
{
}

/// Convenience used by services: the kind of a possibly-absent relationship.
pub fn kind_of(r: &Option<Relationship>) -> Option<RelationshipKind> {
    r.as_ref().map(|r| r.kind)
}
