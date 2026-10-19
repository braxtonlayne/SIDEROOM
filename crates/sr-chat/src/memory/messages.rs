//! In-memory backend implementing both [`ChatRepository`] and
//! [`MessageStore`], for tests and local bring-up (open question 148).

use crate::repo::{
    Attachment, EmojiKey, MessageStore, Page, ReactionSummary, StoreError, StoreResult, UploadState,
};
use async_trait::async_trait;

use crate::admin_repo::{Ban, GuildAdminRepository, MemberFilter, PruneCriteria};
use crate::domain::{Channel, Guild, Member, Message, Role, Visibility};
use crate::guild_rules::GuildBlueprint;
use crate::message_rules::{DeletePlan, EditPlan};
use crate::repo::{ChannelRepository, GuildRepository, MessageRepository, RepoResult};
use sr_core::permissions::{Overwrite, OverwriteKind};
use sr_core::{AttachmentId, ChannelId, GuildId, MessageId, RoleId, Snowflake, UserId};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Default)]
pub struct Data {
    pub guilds: HashMap<GuildId, Guild>,
    pub members: Vec<Member>,
    pub roles: Vec<Role>,
    pub channels: HashMap<ChannelId, Channel>,
    pub overwrites: HashMap<ChannelId, Vec<Overwrite>>,
    pub recipients: HashSet<(ChannelId, UserId)>,
    pub messages: Vec<Message>,
    pub edits: Vec<(MessageId, String)>,
    pub pins: HashMap<MessageId, i64>,
    pub sent_at: HashMap<(ChannelId, UserId), i64>,
    nonces: HashMap<(ChannelId, UserId, String), (MessageId, i64)>,
    nonce_by_message: HashMap<MessageId, String>,
    /// (message, emoji key) -> reactions in insertion order.
    reactions: BTreeMap<(MessageId, String), (EmojiKey, Vec<UserId>)>,
    attachments: HashMap<AttachmentId, Attachment>,
    /// Multipart upload id and part size per large upload slot.
    multipart: HashMap<AttachmentId, (String, u64)>,
    /// Accounts for the user lookup (open question 157).
    pub users: HashMap<UserId, crate::repo::UserSummary>,
    /// Poll votes `(message, answer, user)` for [`crate::rich_repo::RichStore`].
    pub poll_votes: std::collections::BTreeSet<(MessageId, u32, UserId)>,
    /// Sticker rows for [`crate::rich_repo::RichStore`].
    pub sticker_rows: HashMap<u64, crate::rich_repo::StickerRecord>,
    /// Published sticker packs (tests).
    pub sticker_packs: Vec<crate::rich_repo::StickerPack>,
    /// Webhook author overrides (DEV-BOT-006).
    pub webhook_authors: HashMap<MessageId, crate::rich_repo::WebhookAuthor>,
    /// Application emoji id to owning application (test seed, D-237).
    pub app_emojis: HashMap<u64, i64>,
    /// Bot user to application (test seed, D-237).
    pub bot_apps: HashMap<UserId, i64>,
    /// Bans for [`GuildAdminRepository`].
    pub bans: Vec<crate::admin_repo::Ban>,
    /// How members joined (tests seed this).
    pub joins: HashMap<(GuildId, UserId), crate::admin_repo::MemberJoin>,
}

/// Cheap-to-clone handle over shared state.
#[derive(Clone, Default)]
pub struct MemoryBackend {
    data: Arc<Mutex<Data>>,
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self::default()
    }

    /// Direct access for test setup (members, DM recipients, ...).
    pub fn data(&self) -> MutexGuard<'_, Data> {
        self.data.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn lock_data(&self) -> RepoResult<MutexGuard<'_, Data>> {
        Ok(self.data())
    }
}

#[async_trait]
impl GuildRepository for MemoryBackend {
    async fn insert_guild(&self, bp: &GuildBlueprint) -> RepoResult<()> {
        let mut s = self.data();
        s.guilds.insert(bp.guild.id, bp.guild.clone());
        s.roles.push(bp.everyone_role.clone());
        s.members.push(bp.owner_member.clone());
        for c in &bp.channels {
            s.channels.insert(c.id, c.clone());
        }
        Ok(())
    }

    async fn user_guild_ids(&self, user: UserId) -> RepoResult<Vec<GuildId>> {
        let mut ids: Vec<GuildId> = self
            .data()
            .members
            .iter()
            .filter(|m| m.user_id == user)
            .map(|m| m.guild_id)
            .collect();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    async fn get_guild(&self, id: GuildId) -> RepoResult<Option<Guild>> {
        Ok(self.data().guilds.get(&id).cloned())
    }

    async fn guild_by_vanity(&self, code: &str) -> RepoResult<Option<GuildId>> {
        Ok(self
            .data()
            .guilds
            .values()
            .find(|g| {
                g.vanity_code
                    .as_deref()
                    .is_some_and(|v| v.eq_ignore_ascii_case(code))
            })
            .map(|g| g.id))
    }

    async fn page_members(
        &self,
        guild: GuildId,
        after: Option<UserId>,
        limit: usize,
    ) -> RepoResult<Vec<Member>> {
        let mut out: Vec<Member> = self
            .data()
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
            .data()
            .members
            .iter()
            .find(|m| m.guild_id == guild && m.user_id == user)
            .cloned())
    }

    async fn list_roles(&self, guild: GuildId) -> RepoResult<Vec<Role>> {
        Ok(self
            .data()
            .roles
            .iter()
            .filter(|r| r.guild_id == guild)
            .cloned()
            .collect())
    }

    async fn insert_role(&self, role: &Role) -> RepoResult<()> {
        let mut s = self.data();
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
        if let Some(r) = self.data().roles.iter_mut().find(|r| r.id == role.id) {
            *r = role.clone();
        }
        Ok(())
    }

    async fn delete_role(&self, guild: GuildId, role: RoleId) -> RepoResult<()> {
        let mut s = self.data();
        s.roles.retain(|r| r.id != role);
        for m in s.members.iter_mut().filter(|m| m.guild_id == guild) {
            m.role_ids.retain(|r| *r != role);
        }
        Ok(())
    }
}

#[async_trait]
impl ChannelRepository for MemoryBackend {
    async fn get_channel(&self, id: ChannelId) -> RepoResult<Option<Channel>> {
        Ok(self.data().channels.get(&id).cloned())
    }

    async fn list_guild_channels(&self, guild: GuildId) -> RepoResult<Vec<Channel>> {
        Ok(self
            .data()
            .channels
            .values()
            .filter(|c| c.guild_id == Some(guild))
            .cloned()
            .collect())
    }

    async fn get_overwrites(&self, channel: ChannelId) -> RepoResult<Vec<Overwrite>> {
        Ok(self
            .data()
            .overwrites
            .get(&channel)
            .cloned()
            .unwrap_or_default())
    }

    async fn insert_channel(&self, channel: &Channel, overwrites: &[Overwrite]) -> RepoResult<()> {
        let mut s = self.data();
        s.channels.insert(channel.id, channel.clone());
        s.overwrites.insert(channel.id, overwrites.to_vec());
        Ok(())
    }

    async fn update_channel(&self, channel: &Channel) -> RepoResult<()> {
        self.data().channels.insert(channel.id, channel.clone());
        Ok(())
    }

    async fn delete_channel(&self, id: ChannelId) -> RepoResult<()> {
        let mut s = self.data();
        s.channels.remove(&id);
        for c in s.channels.values_mut().filter(|c| c.parent_id == Some(id)) {
            c.parent_id = None;
        }
        Ok(())
    }

    async fn is_recipient(&self, channel: ChannelId, user: UserId) -> RepoResult<bool> {
        Ok(self.data().recipients.contains(&(channel, user)))
    }

    async fn list_recipients(&self, channel: ChannelId) -> RepoResult<Vec<UserId>> {
        let mut v: Vec<UserId> = self
            .data()
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
impl MessageRepository for MemoryBackend {
    async fn get_message(&self, channel: ChannelId, id: MessageId) -> RepoResult<Option<Message>> {
        Ok(self
            .data()
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
            .data()
            .messages
            .iter()
            .filter(|m| m.channel_id == channel && ids.contains(&m.id))
            .cloned()
            .collect())
    }

    async fn insert_message(&self, message: &Message) -> RepoResult<()> {
        let mut s = self.data();
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

    async fn hide_user_messages(
        &self,
        author: UserId,
        channels: &[ChannelId],
        since_ms: i64,
        now_ms: i64,
        purge_after_ms: i64,
    ) -> RepoResult<Vec<(ChannelId, Vec<MessageId>)>> {
        let mut guard = self.data();
        let d = &mut *guard;
        let mut out: Vec<(ChannelId, Vec<MessageId>)> =
            channels.iter().map(|c| (*c, Vec::new())).collect();
        for m in d.messages.iter_mut() {
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
            d.pins.remove(&m.id);
            slot.1.push(m.id);
        }
        Ok(out)
    }

    async fn replace_card(
        &self,
        channel: ChannelId,
        id: MessageId,
        content: &str,
        embeds: &[crate::message::Embed],
        components: &[crate::message::Component],
        now_ms: i64,
    ) -> RepoResult<bool> {
        let mut s = self.data();
        let Some(m) = s
            .messages
            .iter_mut()
            .find(|m| m.channel_id == channel && m.id == id)
        else {
            return Ok(false);
        };
        m.content = content.to_owned();
        m.embeds = embeds.to_vec();
        m.components = components.to_vec();
        m.edited_at_ms = Some(now_ms);
        Ok(true)
    }

    async fn apply_edit(&self, plan: &EditPlan) -> RepoResult<()> {
        let mut s = self.data();
        if let Some(prev) = &plan.history_previous_content {
            s.edits.push((plan.message_id, prev.clone()));
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
        let mut s = self.data();
        if let Some(m) = s.messages.iter_mut().find(|m| m.id == plan.message_id) {
            m.visibility = plan.visibility;
            m.deleted_at_ms = Some(plan.deleted_at_ms);
            m.purge_after_ms = plan.purge_after_ms;
            m.pinned = false;
        }
        s.pins.remove(&plan.message_id);
        Ok(())
    }

    async fn last_sent_ms(&self, channel: ChannelId, user: UserId) -> RepoResult<Option<i64>> {
        Ok(self.data().sent_at.get(&(channel, user)).copied())
    }

    async fn count_pins(&self, channel: ChannelId) -> RepoResult<usize> {
        let s = self.data();
        Ok(s.messages
            .iter()
            .filter(|m| m.channel_id == channel && s.pins.contains_key(&m.id))
            .count())
    }

    async fn set_message_flags(
        &self,
        _channel: ChannelId,
        message: MessageId,
        flags: u32,
    ) -> RepoResult<()> {
        if let Some(m) = self.data().messages.iter_mut().find(|m| m.id == message) {
            m.flags = flags;
        }
        Ok(())
    }

    async fn set_pinned(
        &self,
        _channel: ChannelId,
        message: MessageId,
        _pinned_by: UserId,
        pinned: bool,
        now_ms: i64,
    ) -> RepoResult<()> {
        let mut s = self.data();
        if pinned {
            s.pins.insert(message, now_ms);
        } else {
            s.pins.remove(&message);
        }
        if let Some(m) = s.messages.iter_mut().find(|m| m.id == message) {
            m.pinned = pinned;
        }
        Ok(())
    }
}

#[async_trait]
impl MessageStore for MemoryBackend {
    async fn fetch(&self, channel: ChannelId, page: Page) -> StoreResult<Vec<Message>> {
        let mut v: Vec<Message> = self
            .data()
            .messages
            .iter()
            .filter(|m| {
                m.channel_id == channel
                    && m.is_visible()
                    && page.before.is_none_or(|b| m.id < b)
                    && page.after.is_none_or(|a| m.id > a)
            })
            .cloned()
            .collect();
        v.sort_by_key(|m| m.id);
        if !page.ascending {
            v.reverse();
        }
        v.truncate(page.limit);
        Ok(v)
    }

    async fn list_pins(&self, channel: ChannelId) -> StoreResult<Vec<(Message, i64)>> {
        let s = self.data();
        let mut v: Vec<(Message, i64)> = s
            .messages
            .iter()
            .filter(|m| m.channel_id == channel && m.is_visible())
            .filter_map(|m| s.pins.get(&m.id).map(|at| (m.clone(), *at)))
            .collect();
        v.sort_by_key(|(m, at)| std::cmp::Reverse((*at, m.id)));
        Ok(v)
    }

    async fn find_nonce(
        &self,
        channel: ChannelId,
        author: UserId,
        nonce: &str,
        since_ms: i64,
    ) -> StoreResult<Option<MessageId>> {
        Ok(self
            .data()
            .nonces
            .get(&(channel, author, nonce.to_owned()))
            .filter(|(_, at)| *at >= since_ms)
            .map(|(id, _)| *id))
    }

    async fn put_nonce(
        &self,
        channel: ChannelId,
        author: UserId,
        nonce: &str,
        message: MessageId,
        now_ms: i64,
    ) -> StoreResult<()> {
        let mut s = self.data();
        s.nonces
            .insert((channel, author, nonce.to_owned()), (message, now_ms));
        s.nonce_by_message.insert(message, nonce.to_owned());
        Ok(())
    }

    async fn nonce_of(&self, message: MessageId) -> StoreResult<Option<String>> {
        Ok(self.data().nonce_by_message.get(&message).cloned())
    }

    async fn add_reaction(
        &self,
        _channel: ChannelId,
        message: MessageId,
        user: UserId,
        emoji: &EmojiKey,
        _now_ms: i64,
    ) -> StoreResult<bool> {
        let mut s = self.data();
        let entry = s
            .reactions
            .entry((message, emoji.key()))
            .or_insert_with(|| (emoji.clone(), Vec::new()));
        if entry.1.contains(&user) {
            return Ok(false);
        }
        entry.1.push(user);
        Ok(true)
    }

    async fn remove_reaction(
        &self,
        message: MessageId,
        user: UserId,
        emoji: &EmojiKey,
    ) -> StoreResult<bool> {
        let mut s = self.data();
        let key = (message, emoji.key());
        let Some((_, users)) = s.reactions.get_mut(&key) else {
            return Ok(false);
        };
        let before = users.len();
        users.retain(|u| *u != user);
        let removed = users.len() != before;
        if users.is_empty() {
            s.reactions.remove(&key);
        }
        Ok(removed)
    }

    async fn clear_reactions(&self, message: MessageId) -> StoreResult<()> {
        self.data().reactions.retain(|(m, _), _| *m != message);
        Ok(())
    }

    async fn clear_emoji(&self, message: MessageId, emoji: &EmojiKey) -> StoreResult<bool> {
        Ok(self
            .data()
            .reactions
            .remove(&(message, emoji.key()))
            .is_some())
    }

    async fn reactions_for(
        &self,
        messages: &[MessageId],
        viewer: UserId,
    ) -> StoreResult<HashMap<MessageId, Vec<ReactionSummary>>> {
        let s = self.data();
        let mut out: HashMap<MessageId, Vec<ReactionSummary>> = HashMap::new();
        for ((m, _), (emoji, users)) in &s.reactions {
            if messages.contains(m) {
                out.entry(*m).or_default().push(ReactionSummary {
                    emoji: emoji.clone(),
                    count: u32::try_from(users.len()).unwrap_or(u32::MAX),
                    me: users.contains(&viewer),
                });
            }
        }
        Ok(out)
    }

    async fn reaction_users(
        &self,
        message: MessageId,
        emoji: &EmojiKey,
        after: Option<UserId>,
        limit: usize,
    ) -> StoreResult<Vec<UserId>> {
        let s = self.data();
        let mut users: Vec<UserId> = s
            .reactions
            .get(&(message, emoji.key()))
            .map(|(_, u)| u.clone())
            .unwrap_or_default();
        users.sort();
        users.retain(|u| after.is_none_or(|a| *u > a));
        users.truncate(limit);
        Ok(users)
    }

    async fn insert_attachment(&self, attachment: &Attachment) -> StoreResult<()> {
        self.data()
            .attachments
            .insert(attachment.id, attachment.clone());
        Ok(())
    }

    async fn get_attachments(&self, ids: &[AttachmentId]) -> StoreResult<Vec<Attachment>> {
        let s = self.data();
        Ok(ids
            .iter()
            .filter_map(|id| s.attachments.get(id).cloned())
            .collect())
    }

    async fn update_attachment(&self, attachment: &Attachment) -> StoreResult<()> {
        let mut s = self.data();
        match s.attachments.get_mut(&attachment.id) {
            Some(a) => {
                *a = attachment.clone();
                Ok(())
            }
            None => Err(StoreError("unknown attachment".into())),
        }
    }

    async fn set_attachment_multipart(
        &self,
        id: AttachmentId,
        multipart_id: &str,
        part_size: u64,
    ) -> StoreResult<()> {
        self.data()
            .multipart
            .insert(id, (multipart_id.to_owned(), part_size));
        Ok(())
    }

    async fn attachment_multipart(&self, id: AttachmentId) -> StoreResult<Option<(String, u64)>> {
        Ok(self.data().multipart.get(&id).cloned())
    }

    async fn clear_attachment_multipart(&self, id: AttachmentId) -> StoreResult<()> {
        self.data().multipart.remove(&id);
        Ok(())
    }

    async fn attachments_of(
        &self,
        messages: &[MessageId],
    ) -> StoreResult<HashMap<MessageId, Vec<Attachment>>> {
        let s = self.data();
        let mut out: HashMap<MessageId, Vec<Attachment>> = HashMap::new();
        for a in s.attachments.values() {
            if let Some(m) = a.message_id.filter(|m| messages.contains(m))
                && a.state == UploadState::Attached
            {
                out.entry(m).or_default().push(a.clone());
            }
        }
        for v in out.values_mut() {
            v.sort_by_key(|a| a.id);
        }
        Ok(out)
    }
}

#[async_trait]
impl crate::repo::UserLookup for MemoryBackend {
    async fn users(&self, ids: &[UserId]) -> RepoResult<Vec<crate::repo::UserSummary>> {
        let s = self.data();
        Ok(ids
            .iter()
            .filter_map(|id| s.users.get(id).cloned())
            .collect())
    }
}

#[async_trait]
impl GuildAdminRepository for MemoryBackend {
    async fn member_joins(
        &self,
        guild: GuildId,
        users: &[UserId],
    ) -> RepoResult<Vec<crate::admin_repo::MemberJoin>> {
        let s = self.lock_data()?;
        Ok(users
            .iter()
            .map(|u| {
                s.joins
                    .get(&(guild, *u))
                    .cloned()
                    .unwrap_or(crate::admin_repo::MemberJoin {
                        user_id: *u,
                        source: None,
                        invite_code: None,
                        ref_id: None,
                        rejoined: false,
                    })
            })
            .collect())
    }

    async fn set_vanity(&self, guild: GuildId, code: Option<&str>) -> RepoResult<bool> {
        let mut s = self.lock_data()?;
        if let Some(c) = code
            && s.guilds.values().any(|g| {
                g.id != guild
                    && g.vanity_code
                        .as_deref()
                        .is_some_and(|v| v.eq_ignore_ascii_case(c))
            })
        {
            return Ok(false);
        }
        if let Some(g) = s.guilds.get_mut(&guild) {
            g.vanity_code = code.map(str::to_owned);
        }
        Ok(true)
    }

    async fn update_guild(&self, guild: &Guild) -> RepoResult<()> {
        self.lock_data()?.guilds.insert(guild.id, guild.clone());
        Ok(())
    }

    async fn delete_guild(&self, id: GuildId) -> RepoResult<()> {
        let mut s = self.lock_data()?;
        s.guilds.remove(&id);
        s.members.retain(|m| m.guild_id != id);
        s.roles.retain(|r| r.guild_id != id);
        s.channels.retain(|_, c| c.guild_id != Some(id));
        s.bans.retain(|b| b.guild_id != id);
        Ok(())
    }

    async fn list_user_guilds(&self, user: UserId) -> RepoResult<Vec<Guild>> {
        let s = self.lock_data()?;
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
        let s = self.lock_data()?;
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
        let s = self.lock_data()?;
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
        let mut s = self.lock_data()?;
        let before = s.members.len();
        s.members
            .retain(|m| !(m.guild_id == guild && users.contains(&m.user_id)));
        Ok((before - s.members.len()) as u64)
    }

    async fn update_member(&self, member: &Member) -> RepoResult<()> {
        let mut s = self.lock_data()?;
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
        self.lock_data()?
            .members
            .retain(|m| !(m.guild_id == guild && m.user_id == user));
        Ok(())
    }

    async fn set_role_positions(
        &self,
        guild: GuildId,
        positions: &[(RoleId, i32)],
    ) -> RepoResult<()> {
        let mut s = self.lock_data()?;
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
        let mut s = self.lock_data()?;
        for c in channels {
            if let Some(cur) = s.channels.get_mut(&c.id) {
                cur.position = c.position;
                cur.parent_id = c.parent_id;
            }
        }
        Ok(())
    }

    async fn put_overwrite(&self, channel: ChannelId, overwrite: &Overwrite) -> RepoResult<()> {
        let mut s = self.lock_data()?;
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
        if let Some(list) = self.lock_data()?.overwrites.get_mut(&channel) {
            list.retain(|o| !(o.id == target && o.kind == kind));
        }
        Ok(())
    }

    async fn replace_overwrites(
        &self,
        channel: ChannelId,
        overwrites: &[Overwrite],
    ) -> RepoResult<()> {
        self.lock_data()?
            .overwrites
            .insert(channel, overwrites.to_vec());
        Ok(())
    }

    async fn insert_ban(&self, ban: &Ban) -> RepoResult<()> {
        let mut s = self.lock_data()?;
        s.members
            .retain(|m| !(m.guild_id == ban.guild_id && m.user_id == ban.user_id));
        s.bans
            .retain(|b| !(b.guild_id == ban.guild_id && b.user_id == ban.user_id));
        s.bans.push(ban.clone());
        Ok(())
    }

    async fn delete_ban(&self, guild: GuildId, user: UserId) -> RepoResult<bool> {
        let mut s = self.lock_data()?;
        let before = s.bans.len();
        s.bans
            .retain(|b| !(b.guild_id == guild && b.user_id == user));
        Ok(s.bans.len() != before)
    }

    async fn get_ban(&self, guild: GuildId, user: UserId) -> RepoResult<Option<Ban>> {
        Ok(self
            .lock_data()?
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
            .lock_data()?
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
