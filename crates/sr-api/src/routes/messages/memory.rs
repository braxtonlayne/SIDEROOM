//! In-memory backend implementing both `sr_chat::repo::ChatRepository` and
//! [`MessageStore`], for tests and local bring-up before the Postgres versions land.

use super::store::{
    Attachment, EmojiKey, MessageStore, Page, ReactionSummary, StoreError, StoreResult, UploadState,
};
use async_trait::async_trait;
use sr_chat::error::RepoError;
use sr_chat::guild_rules::GuildBlueprint;
use sr_chat::message_rules::{DeletePlan, EditPlan};
use sr_chat::repo::{ChannelRepository, GuildRepository, MessageRepository, RepoResult};
use sr_chat::{Channel, Guild, Member, Message, Role};
use sr_core::permissions::Overwrite;
use sr_core::{AttachmentId, ChannelId, GuildId, MessageId, RoleId, UserId};
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

    async fn get_guild(&self, id: GuildId) -> RepoResult<Option<Guild>> {
        Ok(self.data().guilds.get(&id).cloned())
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
        s.sent_at.insert(
            (message.channel_id, message.author_id),
            message.id.snowflake().unix_ms(),
        );
        if let Some(c) = s.channels.get_mut(&message.channel_id) {
            c.last_message_id = Some(message.id);
        }
        Ok(())
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

impl From<RepoError> for StoreError {
    fn from(e: RepoError) -> Self {
        Self(e.0)
    }
}
