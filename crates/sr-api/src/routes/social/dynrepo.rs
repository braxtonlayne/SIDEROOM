//! Type erasure for the social repository: the routes hold one concrete
//! `SocialService<DynRepo>` while the composition root picks Postgres and tests
//! pick the in-memory fake. Every method forwards to the wrapped repository.

use async_trait::async_trait;
use sr_chat::social::dm::{DmChannel, RequestState};
use sr_chat::social::notifications::{ChannelOverride, ScopeSettings};
use sr_chat::social::presence::PresencePrefs;
use sr_chat::social::profile::{Connection, Profile};
use sr_chat::social::read_state::ReadState;
use sr_chat::social::relationships::{RelOp, Relationship, RelationshipCounts};
use sr_chat::social::repo::*;
use sr_core::{ChannelId, GuildId, UserId};
use std::sync::Arc;

/// A shared, type-erased [`SocialRepository`].
#[derive(Clone)]
pub struct DynRepo(pub Arc<dyn SocialRepository>);

impl DynRepo {
    pub fn new<R: SocialRepository + 'static>(repo: R) -> Self {
        Self(Arc::new(repo))
    }
}

#[async_trait]
impl RelationshipRepository for DynRepo {
    async fn user_exists(&self, user: UserId) -> RepoResult<bool> {
        self.0.user_exists(user).await
    }
    async fn get_relationship(
        &self,
        user: UserId,
        target: UserId,
    ) -> RepoResult<Option<Relationship>> {
        self.0.get_relationship(user, target).await
    }
    async fn list_relationships(&self, user: UserId) -> RepoResult<Vec<Relationship>> {
        self.0.list_relationships(user).await
    }
    async fn counts(&self, user: UserId) -> RepoResult<RelationshipCounts> {
        self.0.counts(user).await
    }
    async fn apply(&self, ops: &[RelOp]) -> RepoResult<()> {
        self.0.apply(ops).await
    }
    async fn shares_guild(&self, a: UserId, b: UserId) -> RepoResult<bool> {
        self.0.shares_guild(a, b).await
    }
    async fn has_mutual_friend(&self, a: UserId, b: UserId) -> RepoResult<bool> {
        self.0.has_mutual_friend(a, b).await
    }
    async fn mutual_friends(&self, a: UserId, b: UserId) -> RepoResult<Vec<UserId>> {
        self.0.mutual_friends(a, b).await
    }
    async fn find_user_by_username(&self, username: &str) -> RepoResult<Option<UserId>> {
        self.0.find_user_by_username(username).await
    }
    async fn list_guild_memberships(&self, user: UserId) -> RepoResult<Vec<GuildMembership>> {
        self.0.list_guild_memberships(user).await
    }
    async fn guild_peers(&self, user: UserId) -> RepoResult<Vec<UserId>> {
        self.0.guild_peers(user).await
    }
    async fn get_privacy(&self, user: UserId) -> RepoResult<Privacy> {
        self.0.get_privacy(user).await
    }
    async fn put_privacy(&self, user: UserId, privacy: &Privacy) -> RepoResult<()> {
        self.0.put_privacy(user, privacy).await
    }
    async fn is_ignoring(&self, user: UserId, target: UserId) -> RepoResult<bool> {
        self.0.is_ignoring(user, target).await
    }
    async fn list_ignores(&self, user: UserId) -> RepoResult<Vec<UserId>> {
        self.0.list_ignores(user).await
    }
    async fn put_ignore(&self, user: UserId, target: UserId) -> RepoResult<()> {
        self.0.put_ignore(user, target).await
    }
    async fn delete_ignore(&self, user: UserId, target: UserId) -> RepoResult<()> {
        self.0.delete_ignore(user, target).await
    }
    async fn get_note(&self, user: UserId, target: UserId) -> RepoResult<Option<String>> {
        self.0.get_note(user, target).await
    }
    async fn set_note(&self, user: UserId, target: UserId, note: Option<&str>) -> RepoResult<()> {
        self.0.set_note(user, target, note).await
    }
}

#[async_trait]
impl DmRepository for DynRepo {
    async fn find_dm(&self, a: UserId, b: UserId) -> RepoResult<Option<DmChannel>> {
        self.0.find_dm(a, b).await
    }
    async fn insert_dm(&self, id: ChannelId, a: UserId, b: UserId) -> RepoResult<()> {
        self.0.insert_dm(id, a, b).await
    }
    async fn insert_group(
        &self,
        id: ChannelId,
        members: &[UserId],
        name: Option<&str>,
    ) -> RepoResult<()> {
        self.0.insert_group(id, members, name).await
    }
    async fn get_dm(&self, id: ChannelId) -> RepoResult<Option<DmChannel>> {
        self.0.get_dm(id).await
    }
    async fn add_recipients(&self, id: ChannelId, users: &[UserId]) -> RepoResult<()> {
        self.0.add_recipients(id, users).await
    }
    async fn remove_recipient(&self, id: ChannelId, user: UserId) -> RepoResult<()> {
        self.0.remove_recipient(id, user).await
    }
    async fn set_owner(&self, id: ChannelId, owner: UserId) -> RepoResult<()> {
        self.0.set_owner(id, owner).await
    }
    async fn update_group(
        &self,
        id: ChannelId,
        name: Option<&str>,
        icon_key: Option<&str>,
    ) -> RepoResult<()> {
        self.0.update_group(id, name, icon_key).await
    }
    async fn delete_dm(&self, id: ChannelId) -> RepoResult<()> {
        self.0.delete_dm(id).await
    }
    async fn set_hidden(&self, user: UserId, id: ChannelId, hidden: bool) -> RepoResult<bool> {
        self.0.set_hidden(user, id, hidden).await
    }
    async fn list_dms(&self, user: UserId, include_hidden: bool) -> RepoResult<Vec<DmChannel>> {
        self.0.list_dms(user, include_hidden).await
    }
    async fn count_groups(&self, user: UserId) -> RepoResult<usize> {
        self.0.count_groups(user).await
    }
    async fn get_request_state(
        &self,
        user: UserId,
        id: ChannelId,
    ) -> RepoResult<Option<RequestState>> {
        self.0.get_request_state(user, id).await
    }
    async fn set_request_state(
        &self,
        user: UserId,
        id: ChannelId,
        state: RequestState,
    ) -> RepoResult<()> {
        self.0.set_request_state(user, id, state).await
    }
}

#[async_trait]
impl ReadStateRepository for DynRepo {
    async fn get_read_state(
        &self,
        user: UserId,
        channel: ChannelId,
    ) -> RepoResult<Option<ReadState>> {
        self.0.get_read_state(user, channel).await
    }
    async fn put_read_state(&self, state: &ReadState) -> RepoResult<()> {
        self.0.put_read_state(state).await
    }
    async fn list_read_states(&self, user: UserId) -> RepoResult<Vec<ReadState>> {
        self.0.list_read_states(user).await
    }
    async fn add_mentions(&self, users: &[UserId], channel: ChannelId, n: u32) -> RepoResult<()> {
        self.0.add_mentions(users, channel, n).await
    }
}

#[async_trait]
impl NotificationRepository for DynRepo {
    async fn get_scope(
        &self,
        user: UserId,
        guild: Option<GuildId>,
    ) -> RepoResult<Option<ScopeSettings>> {
        self.0.get_scope(user, guild).await
    }
    async fn put_scope(&self, settings: &ScopeSettings) -> RepoResult<()> {
        self.0.put_scope(settings).await
    }
    async fn get_override(
        &self,
        user: UserId,
        channel: ChannelId,
    ) -> RepoResult<Option<ChannelOverride>> {
        self.0.get_override(user, channel).await
    }
    async fn put_override(&self, o: &ChannelOverride) -> RepoResult<()> {
        self.0.put_override(o).await
    }
    async fn delete_override(&self, user: UserId, channel: ChannelId) -> RepoResult<()> {
        self.0.delete_override(user, channel).await
    }
    async fn list_overrides(&self, user: UserId) -> RepoResult<Vec<ChannelOverride>> {
        self.0.list_overrides(user).await
    }
}

#[async_trait]
impl ProfileRepository for DynRepo {
    async fn get_profile(&self, user: UserId) -> RepoResult<Option<Profile>> {
        self.0.get_profile(user).await
    }
    async fn put_profile(&self, profile: &Profile) -> RepoResult<()> {
        self.0.put_profile(profile).await
    }
    async fn upsert_connection(&self, c: &Connection) -> RepoResult<()> {
        self.0.upsert_connection(c).await
    }
    async fn delete_connection(
        &self,
        user: UserId,
        provider: &str,
        external_id: &str,
    ) -> RepoResult<()> {
        self.0.delete_connection(user, provider, external_id).await
    }
    async fn count_connections(&self, user: UserId) -> RepoResult<usize> {
        self.0.count_connections(user).await
    }
}

#[async_trait]
impl PresenceRepository for DynRepo {
    async fn get_presence(&self, user: UserId) -> RepoResult<Option<PresencePrefs>> {
        self.0.get_presence(user).await
    }
    async fn put_presence(&self, prefs: &PresencePrefs) -> RepoResult<()> {
        self.0.put_presence(prefs).await
    }
}
