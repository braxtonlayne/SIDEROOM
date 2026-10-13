//! In-memory repository used by the service tests.

pub mod messages;
mod rich;
use crate::admin_repo::{Ban, GuildAdminRepository, MemberFilter, PruneCriteria};
use crate::domain::{Channel, Guild, Member, Message, Role, Visibility};
use crate::error::RepoError;
use crate::guild_rules::GuildBlueprint;
use crate::message_rules::{DeletePlan, EditPlan};
use crate::repo::{ChannelRepository, GuildRepository, MessageRepository, RepoResult};
use async_trait::async_trait;
pub use messages::{Data, MemoryBackend};
use sr_core::permissions::{Overwrite, OverwriteKind};
use sr_core::{ChannelId, GuildId, MessageId, RoleId, Snowflake, UserId};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

#[derive(Default)]
pub struct Store {
    pub guilds: HashMap<GuildId, Guild>,
    pub members: Vec<Member>,
    pub roles: Vec<Role>,
    pub channels: HashMap<ChannelId, Channel>,
    pub overwrites: HashMap<ChannelId, Vec<Overwrite>>,
    pub recipients: HashSet<(ChannelId, UserId)>,
    pub messages: Vec<Message>,
    pub history: Vec<(MessageId, String)>,
    pub pins: HashSet<(ChannelId, MessageId)>,
    pub sent_at: HashMap<(ChannelId, UserId), i64>,
    pub bans: Vec<Ban>,
    pub thread_members: Vec<crate::threads::model::ThreadMember>,
    /// Accounts for name search and the user lookup.
    pub users: HashMap<UserId, crate::repo::UserSummary>,
}

/// Cloning shares the same store, so tests can keep a handle while the service owns a clone.
#[derive(Default, Clone)]
pub struct MemoryRepo {
    pub store: std::sync::Arc<Mutex<Store>>,
}

impl MemoryRepo {
    pub(crate) fn lock(&self) -> RepoResult<std::sync::MutexGuard<'_, Store>> {
        self.store
            .lock()
            .map_err(|_| RepoError("store poisoned".into()))
    }
}

#[async_trait]
impl GuildRepository for MemoryRepo {
    async fn insert_guild(&self, bp: &GuildBlueprint) -> RepoResult<()> {
        let mut s = self.lock()?;
        s.guilds.insert(bp.guild.id, bp.guild.clone());
        s.roles.push(bp.everyone_role.clone());
        s.members.push(bp.owner_member.clone());
        for c in &bp.channels {
            s.channels.insert(c.id, c.clone());
        }
        Ok(())
    }

    async fn get_guild(&self, id: GuildId) -> RepoResult<Option<Guild>> {
        Ok(self.lock()?.guilds.get(&id).cloned())
    }

    async fn page_members(
        &self,
        guild: GuildId,
        after: Option<UserId>,
        limit: usize,
    ) -> RepoResult<Vec<Member>> {
        let mut out: Vec<Member> = self
            .lock()?
            .members
            .iter()
            .filter(|m| m.guild_id == guild && after.is_none_or(|a| m.user_id > a))
            .cloned()
            .collect();
        out.sort_by_key(|m| m.user_id);
        out.truncate(limit);
        Ok(out)
    }

    async fn get_member(&self, guild: GuildId, user: UserId) -> RepoResult<Option<Member>> {
        Ok(self
            .lock()?
            .members
            .iter()
            .find(|m| m.guild_id == guild && m.user_id == user)
            .cloned())
    }

    async fn list_roles(&self, guild: GuildId) -> RepoResult<Vec<Role>> {
        Ok(self
            .lock()?
            .roles
            .iter()
            .filter(|r| r.guild_id == guild)
            .cloned()
            .collect())
    }

    async fn insert_role(&self, role: &Role) -> RepoResult<()> {
        let mut s = self.lock()?;
        for r in s
            .roles
            .iter_mut()
            .filter(|r| r.guild_id == role.guild_id && r.position >= role.position)
        {
            r.position += 1;
        }
        s.roles.push(role.clone());
        Ok(())
    }

    async fn update_role(&self, role: &Role) -> RepoResult<()> {
        let mut s = self.lock()?;
        if let Some(r) = s.roles.iter_mut().find(|r| r.id == role.id) {
            *r = role.clone();
        }
        Ok(())
    }

    async fn delete_role(&self, guild: GuildId, role: RoleId) -> RepoResult<()> {
        let mut s = self.lock()?;
        s.roles.retain(|r| r.id != role);
        for m in s.members.iter_mut().filter(|m| m.guild_id == guild) {
            m.role_ids.retain(|r| *r != role);
        }
        Ok(())
    }
}

#[async_trait]
impl ChannelRepository for MemoryRepo {
    async fn get_channel(&self, id: ChannelId) -> RepoResult<Option<Channel>> {
        Ok(self.lock()?.channels.get(&id).cloned())
    }

    async fn list_guild_channels(&self, guild: GuildId) -> RepoResult<Vec<Channel>> {
        Ok(self
            .lock()?
            .channels
            .values()
            .filter(|c| c.guild_id == Some(guild) && !c.kind.is_thread())
            .cloned()
            .collect())
    }

    async fn list_guild_threads(&self, guild: GuildId) -> RepoResult<Vec<Channel>> {
        let mut v: Vec<Channel> = self
            .lock()?
            .channels
            .values()
            .filter(|c| c.guild_id == Some(guild) && c.kind.is_thread())
            .cloned()
            .collect();
        v.sort_by_key(|c| c.id);
        Ok(v)
    }

    async fn get_threads(&self, ids: &[ChannelId]) -> RepoResult<Vec<Channel>> {
        let s = self.lock()?;
        Ok(ids
            .iter()
            .filter_map(|id| s.channels.get(id))
            .filter(|c| c.kind.is_thread())
            .cloned()
            .collect())
    }

    async fn get_overwrites(&self, channel: ChannelId) -> RepoResult<Vec<Overwrite>> {
        Ok(self
            .lock()?
            .overwrites
            .get(&channel)
            .cloned()
            .unwrap_or_default())
    }

    async fn insert_channel(&self, channel: &Channel, overwrites: &[Overwrite]) -> RepoResult<()> {
        let mut s = self.lock()?;
        s.channels.insert(channel.id, channel.clone());
        s.overwrites.insert(channel.id, overwrites.to_vec());
        Ok(())
    }

    async fn update_channel(&self, channel: &Channel) -> RepoResult<()> {
        self.lock()?.channels.insert(channel.id, channel.clone());
        Ok(())
    }

    async fn delete_channel(&self, id: ChannelId) -> RepoResult<()> {
        let mut s = self.lock()?;
        let doomed: Vec<ChannelId> = s
            .channels
            .values()
            .filter(|c| c.kind.is_thread() && c.parent_id == Some(id))
            .map(|c| c.id)
            .collect();
        for t in doomed {
            s.channels.remove(&t);
            s.thread_members.retain(|m| m.thread_id != t);
        }
        s.thread_members.retain(|m| m.thread_id != id);
        s.channels.remove(&id);
        for c in s.channels.values_mut().filter(|c| c.parent_id == Some(id)) {
            c.parent_id = None;
        }
        Ok(())
    }

    async fn is_recipient(&self, channel: ChannelId, user: UserId) -> RepoResult<bool> {
        Ok(self.lock()?.recipients.contains(&(channel, user)))
    }

    async fn is_thread_member(&self, channel: ChannelId, user: UserId) -> RepoResult<bool> {
        Ok(self
            .lock()?
            .thread_members
            .iter()
            .any(|m| m.thread_id == channel && m.user_id == user))
    }

    async fn list_thread_member_ids(
        &self,
        channel: ChannelId,
        after: Option<UserId>,
        limit: usize,
    ) -> RepoResult<Vec<UserId>> {
        let mut v: Vec<UserId> = self
            .lock()?
            .thread_members
            .iter()
            .filter(|m| m.thread_id == channel && after.is_none_or(|a| m.user_id > a))
            .map(|m| m.user_id)
            .collect();
        v.sort();
        v.truncate(limit);
        Ok(v)
    }

    async fn list_recipients(&self, channel: ChannelId) -> RepoResult<Vec<UserId>> {
        let mut v: Vec<UserId> = self
            .lock()?
            .recipients
            .iter()
            .filter(|(c, _)| *c == channel)
            .map(|(_, u)| *u)
            .collect();
        v.sort();
        Ok(v)
    }
}

#[async_trait]
impl MessageRepository for MemoryRepo {
    async fn get_message(&self, channel: ChannelId, id: MessageId) -> RepoResult<Option<Message>> {
        Ok(self
            .lock()?
            .messages
            .iter()
            .find(|m| m.channel_id == channel && m.id == id)
            .cloned())
    }

    async fn get_messages(
        &self,
        channel: ChannelId,
        ids: &[MessageId],
    ) -> RepoResult<Vec<Message>> {
        Ok(self
            .lock()?
            .messages
            .iter()
            .filter(|m| m.channel_id == channel && ids.contains(&m.id))
            .cloned()
            .collect())
    }

    async fn insert_message(&self, message: &Message) -> RepoResult<()> {
        let mut s = self.lock()?;
        s.messages.push(message.clone());
        if crate::domain::counts_for_slowmode(message.kind) {
            s.sent_at.insert(
                (message.channel_id, message.author_id),
                message.id.snowflake().unix_ms(),
            );
        }
        if let Some(c) = s.channels.get_mut(&message.channel_id) {
            c.last_message_id = Some(message.id);
        }
        Ok(())
    }

    async fn apply_edit(&self, plan: &EditPlan) -> RepoResult<()> {
        let mut s = self.lock()?;
        if let Some(prev) = &plan.history_previous_content {
            s.history.push((plan.message_id, prev.clone()));
        }
        if let Some(m) = s.messages.iter_mut().find(|m| m.id == plan.message_id) {
            m.content.clone_from(&plan.content);
            m.flags = plan.flags;
            m.edited_at_ms = Some(plan.edited_at_ms);
            m.mention_everyone = plan.mentions.everyone;
            m.mention_user_ids.clone_from(&plan.mentions.users);
            m.mention_role_ids.clone_from(&plan.mentions.roles);
            m.mention_channel_ids.clone_from(&plan.mentions.channels);
        }
        Ok(())
    }

    async fn apply_delete(&self, plan: &DeletePlan) -> RepoResult<()> {
        let mut s = self.lock()?;
        let mut was_visible_in = None;
        if let Some(m) = s.messages.iter_mut().find(|m| m.id == plan.message_id) {
            if m.is_visible() {
                was_visible_in = Some(m.channel_id);
            }
            m.visibility = plan.visibility;
            m.deleted_at_ms = Some(plan.deleted_at_ms);
            m.purge_after_ms = plan.purge_after_ms;
        }
        // A deleted message no longer counts towards its thread's message_count.
        if let Some(ch) = was_visible_in
            && let Some(t) = s.channels.get_mut(&ch).and_then(|c| c.thread.as_mut())
        {
            t.message_count = t.message_count.saturating_sub(1);
        }
        Ok(())
    }

    async fn last_sent_ms(&self, channel: ChannelId, user: UserId) -> RepoResult<Option<i64>> {
        Ok(self.lock()?.sent_at.get(&(channel, user)).copied())
    }

    async fn count_pins(&self, channel: ChannelId) -> RepoResult<usize> {
        Ok(self
            .lock()?
            .pins
            .iter()
            .filter(|(c, _)| *c == channel)
            .count())
    }

    async fn hide_user_messages(
        &self,
        author: UserId,
        channels: &[ChannelId],
        since_ms: i64,
        now_ms: i64,
        purge_after_ms: i64,
    ) -> RepoResult<Vec<(ChannelId, Vec<MessageId>)>> {
        let mut s = self.lock()?;
        let s = &mut *s;
        let mut out: Vec<(ChannelId, Vec<MessageId>)> =
            channels.iter().map(|c| (*c, Vec::new())).collect();
        for m in s.messages.iter_mut() {
            if m.author_id != author || !m.is_visible() || m.id.snowflake().unix_ms() < since_ms {
                continue;
            }
            let Some(slot) = out.iter_mut().find(|(c, _)| *c == m.channel_id) else {
                continue;
            };
            m.visibility = Visibility::UserDeleted;
            m.deleted_at_ms = Some(now_ms);
            m.purge_after_ms = m.hold_id.is_none().then_some(purge_after_ms);
            m.pinned = false;
            s.pins.remove(&(m.channel_id, m.id));
            slot.1.push(m.id);
        }
        Ok(out)
    }

    async fn set_pinned(
        &self,
        channel: ChannelId,
        message: MessageId,
        _pinned_by: UserId,
        pinned: bool,
        _now_ms: i64,
    ) -> RepoResult<()> {
        let mut s = self.lock()?;
        if pinned {
            s.pins.insert((channel, message));
        } else {
            s.pins.remove(&(channel, message));
        }
        if let Some(m) = s.messages.iter_mut().find(|m| m.id == message) {
            m.pinned = pinned;
        }
        Ok(())
    }
}

#[async_trait]
impl GuildAdminRepository for MemoryRepo {
    async fn update_guild(&self, guild: &Guild) -> RepoResult<()> {
        self.lock()?.guilds.insert(guild.id, guild.clone());
        Ok(())
    }

    async fn delete_guild(&self, id: GuildId) -> RepoResult<()> {
        let mut s = self.lock()?;
        s.guilds.remove(&id);
        s.members.retain(|m| m.guild_id != id);
        s.roles.retain(|r| r.guild_id != id);
        s.channels.retain(|_, c| c.guild_id != Some(id));
        s.bans.retain(|b| b.guild_id != id);
        Ok(())
    }

    async fn list_user_guilds(&self, user: UserId) -> RepoResult<Vec<Guild>> {
        let s = self.lock()?;
        let mut out: Vec<Guild> = s
            .members
            .iter()
            .filter(|m| m.user_id == user)
            .filter_map(|m| s.guilds.get(&m.guild_id).cloned())
            .collect();
        out.sort_by_key(|g| g.id);
        Ok(out)
    }

    async fn list_members(
        &self,
        guild: GuildId,
        filter: &MemberFilter,
        after: Option<UserId>,
        limit: usize,
    ) -> RepoResult<Vec<Member>> {
        let s = self.lock()?;
        let needle = filter.query.as_deref().map(str::to_lowercase);
        let hit = |m: &Member| {
            let role_ok = filter
                .role
                .is_none_or(|r| r.get() == guild.get() || m.role_ids.contains(&r));
            let query_ok = needle.as_deref().is_none_or(|q| {
                let has = |v: Option<&str>| v.is_some_and(|v| v.to_lowercase().contains(q));
                let user = s.users.get(&m.user_id);
                has(m.nick.as_deref())
                    || has(user.map(|u| u.username.as_str()))
                    || has(user.and_then(|u| u.display_name.as_deref()))
            });
            role_ok && query_ok
        };
        let mut out: Vec<Member> = s
            .members
            .iter()
            .filter(|m| m.guild_id == guild && after.is_none_or(|a| m.user_id > a) && hit(m))
            .cloned()
            .collect();
        out.sort_by_key(|m| m.user_id);
        out.truncate(limit);
        Ok(out)
    }

    async fn prune_candidates(
        &self,
        guild: GuildId,
        criteria: &PruneCriteria,
        limit: usize,
    ) -> RepoResult<Vec<Member>> {
        let s = self.lock()?;
        let owner = s.guilds.get(&guild).map(|g| g.owner_id);
        let mut out: Vec<Member> = s
            .members
            .iter()
            .filter(|m| m.guild_id == guild && Some(m.user_id) != owner)
            .filter(|m| m.joined_at_ms < criteria.joined_before_ms)
            .filter(|m| {
                m.role_ids
                    .iter()
                    .all(|r| criteria.include_roles.contains(r))
            })
            .filter(|m| {
                !s.messages.iter().any(|msg| {
                    msg.guild_id == Some(guild)
                        && msg.author_id == m.user_id
                        && msg.id.snowflake().unix_ms() >= criteria.active_since_ms
                })
            })
            .cloned()
            .collect();
        out.sort_by_key(|m| m.user_id);
        out.truncate(limit);
        Ok(out)
    }

    async fn remove_members(&self, guild: GuildId, users: &[UserId]) -> RepoResult<u64> {
        let mut s = self.lock()?;
        let before = s.members.len();
        s.members
            .retain(|m| !(m.guild_id == guild && users.contains(&m.user_id)));
        Ok((before - s.members.len()) as u64)
    }

    async fn update_member(&self, member: &Member) -> RepoResult<()> {
        let mut s = self.lock()?;
        if let Some(m) = s
            .members
            .iter_mut()
            .find(|m| m.guild_id == member.guild_id && m.user_id == member.user_id)
        {
            *m = member.clone();
        }
        Ok(())
    }

    async fn remove_member(&self, guild: GuildId, user: UserId) -> RepoResult<()> {
        self.lock()?
            .members
            .retain(|m| !(m.guild_id == guild && m.user_id == user));
        Ok(())
    }

    async fn set_role_positions(
        &self,
        guild: GuildId,
        positions: &[(RoleId, i32)],
    ) -> RepoResult<()> {
        let mut s = self.lock()?;
        for (id, pos) in positions {
            if let Some(r) = s
                .roles
                .iter_mut()
                .find(|r| r.id == *id && r.guild_id == guild)
            {
                r.position = *pos;
            }
        }
        Ok(())
    }

    async fn update_channel_layout(&self, channels: &[Channel]) -> RepoResult<()> {
        let mut s = self.lock()?;
        for c in channels {
            if let Some(cur) = s.channels.get_mut(&c.id) {
                cur.position = c.position;
                cur.parent_id = c.parent_id;
            }
        }
        Ok(())
    }

    async fn put_overwrite(&self, channel: ChannelId, overwrite: &Overwrite) -> RepoResult<()> {
        let mut s = self.lock()?;
        let list = s.overwrites.entry(channel).or_default();
        list.retain(|o| !(o.id == overwrite.id && o.kind == overwrite.kind));
        list.push(*overwrite);
        Ok(())
    }

    async fn delete_overwrite(
        &self,
        channel: ChannelId,
        target: Snowflake,
        kind: OverwriteKind,
    ) -> RepoResult<()> {
        if let Some(list) = self.lock()?.overwrites.get_mut(&channel) {
            list.retain(|o| !(o.id == target && o.kind == kind));
        }
        Ok(())
    }

    async fn replace_overwrites(
        &self,
        channel: ChannelId,
        overwrites: &[Overwrite],
    ) -> RepoResult<()> {
        self.lock()?.overwrites.insert(channel, overwrites.to_vec());
        Ok(())
    }

    async fn insert_ban(&self, ban: &Ban) -> RepoResult<()> {
        let mut s = self.lock()?;
        s.members
            .retain(|m| !(m.guild_id == ban.guild_id && m.user_id == ban.user_id));
        s.bans
            .retain(|b| !(b.guild_id == ban.guild_id && b.user_id == ban.user_id));
        s.bans.push(ban.clone());
        Ok(())
    }

    async fn delete_ban(&self, guild: GuildId, user: UserId) -> RepoResult<bool> {
        let mut s = self.lock()?;
        let before = s.bans.len();
        s.bans
            .retain(|b| !(b.guild_id == guild && b.user_id == user));
        Ok(s.bans.len() != before)
    }

    async fn get_ban(&self, guild: GuildId, user: UserId) -> RepoResult<Option<Ban>> {
        Ok(self
            .lock()?
            .bans
            .iter()
            .find(|b| b.guild_id == guild && b.user_id == user)
            .cloned())
    }

    async fn list_bans(
        &self,
        guild: GuildId,
        before: Option<UserId>,
        after: Option<UserId>,
        limit: usize,
    ) -> RepoResult<Vec<Ban>> {
        let mut out: Vec<Ban> = self
            .lock()?
            .bans
            .iter()
            .filter(|b| {
                b.guild_id == guild
                    && before.is_none_or(|x| b.user_id < x)
                    && after.is_none_or(|x| b.user_id > x)
            })
            .cloned()
            .collect();
        out.sort_by_key(|b| b.user_id);
        out.truncate(limit);
        Ok(out)
    }
}

#[async_trait]
impl crate::repo::UserLookup for MemoryRepo {
    async fn users(&self, ids: &[UserId]) -> RepoResult<Vec<crate::repo::UserSummary>> {
        let s = self.lock()?;
        Ok(ids
            .iter()
            .filter_map(|id| s.users.get(id).cloned())
            .collect())
    }
}
