//! In-memory repository used by the service tests.

use crate::domain::{Channel, Guild, Member, Message, Role};
use crate::error::RepoError;
use crate::guild_rules::GuildBlueprint;
use crate::message_rules::{DeletePlan, EditPlan};
use crate::repo::{ChannelRepository, GuildRepository, MessageRepository, RepoResult};
use async_trait::async_trait;
use sr_core::permissions::Overwrite;
use sr_core::{ChannelId, GuildId, MessageId, RoleId, UserId};
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
}

#[derive(Default)]
pub struct MemoryRepo {
    pub store: Mutex<Store>,
}

impl MemoryRepo {
    fn lock(&self) -> RepoResult<std::sync::MutexGuard<'_, Store>> {
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
            .filter(|c| c.guild_id == Some(guild))
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
        s.channels.remove(&id);
        for c in s.channels.values_mut().filter(|c| c.parent_id == Some(id)) {
            c.parent_id = None;
        }
        Ok(())
    }

    async fn is_recipient(&self, channel: ChannelId, user: UserId) -> RepoResult<bool> {
        Ok(self.lock()?.recipients.contains(&(channel, user)))
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
        if let Some(m) = s.messages.iter_mut().find(|m| m.id == plan.message_id) {
            m.visibility = plan.visibility;
            m.deleted_at_ms = Some(plan.deleted_at_ms);
            m.purge_after_ms = plan.purge_after_ms;
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
