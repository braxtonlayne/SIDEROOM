//! Orchestration: load context from repositories, run the pure rules, persist the plan.
//! No database dependency; generic over [`ChatRepository`] and an injected clock.

use crate::channel_rules::{self, NewChannel};
use crate::domain::{Channel, Guild, Member, Message, Role, Visibility};
use crate::error::{ChatError, ChatResult};
use crate::guild_rules::{self, GuildBlueprint};
use crate::limits::ChatLimits;
use crate::message_rules::{
    self, CreateContext, CreateMessageInput, DeletePlan, EditContext, EditMessageInput, EditPlan,
};
use crate::perms::{GuildActor, channel_permissions, dm_permissions};
use crate::repo::ChatRepository;
use crate::role_rules::{self, NewRole};
use sr_core::permissions::Permissions;
use sr_core::{
    AttachmentId, ChannelId, Clock, GuildId, MessageId, RoleId, SnowflakeGenerator, UserId,
};
use std::sync::Arc;

pub struct ChatService<R> {
    pub(crate) repo: R,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) ids: SnowflakeGenerator<Arc<dyn Clock>>,
    pub(crate) limits: ChatLimits,
}

/// Request body of "create message" with attachments already resolved to upload ids.
#[derive(Debug, Clone, Default)]
pub struct SendMessage {
    pub content: String,
    pub nonce: Option<String>,
    pub reply_to: Option<MessageId>,
    pub attachment_ids: Vec<AttachmentId>,
    pub embed_count: usize,
    pub sticker_ids: Vec<u64>,
    pub has_poll: bool,
    pub flags: u32,
}

/// Resolved view of a channel for one user.
pub(crate) struct Access {
    pub(crate) channel: Channel,
    pub(crate) perms: Permissions,
    pub(crate) roles: Vec<Role>,
}

impl<R: ChatRepository> ChatService<R> {
    pub fn new(
        repo: R,
        clock: Arc<dyn Clock>,
        ids: SnowflakeGenerator<Arc<dyn Clock>>,
        limits: ChatLimits,
    ) -> Self {
        Self {
            repo,
            clock,
            ids,
            limits,
        }
    }

    pub(crate) fn now(&self) -> i64 {
        self.clock.now_unix_ms()
    }

    pub(crate) async fn guild(&self, id: GuildId) -> ChatResult<Guild> {
        self.repo
            .get_guild(id)
            .await?
            .ok_or(ChatError::UnknownGuild)
    }

    /// Member and roles of a guild; non-members get `MissingAccess`.
    pub(crate) async fn guild_actor(
        &self,
        guild: &Guild,
        user: UserId,
    ) -> ChatResult<(Member, Vec<Role>)> {
        let member = self
            .repo
            .get_member(guild.id, user)
            .await?
            .ok_or(ChatError::MissingAccess)?;
        let roles = self.repo.list_roles(guild.id).await?;
        Ok((member, roles))
    }

    pub(crate) async fn access(&self, user: UserId, channel_id: ChannelId) -> ChatResult<Access> {
        let channel = self
            .repo
            .get_channel(channel_id)
            .await?
            .ok_or(ChatError::UnknownChannel)?;
        match channel.guild_id {
            None => {
                if !self.repo.is_recipient(channel.id, user).await? {
                    return Err(ChatError::MissingAccess);
                }
                Ok(Access {
                    channel,
                    perms: dm_permissions(),
                    roles: Vec::new(),
                })
            }
            Some(gid) => {
                let guild = self.guild(gid).await?;
                let (member, roles) = self.guild_actor(&guild, user).await?;
                let overwrites = self.repo.get_overwrites(channel.id).await?;
                let perms = channel_permissions(&guild, &member, &roles, &overwrites, self.now());
                Ok(Access {
                    channel,
                    perms,
                    roles,
                })
            }
        }
    }

    // --------------------------------------------------------- guilds

    pub async fn create_guild(&self, owner: UserId, name: &str) -> ChatResult<GuildBlueprint> {
        let bp = guild_rules::plan_new_guild(owner, name, self.now(), || self.ids.generate())?;
        self.repo.insert_guild(&bp).await?;
        Ok(bp)
    }

    pub async fn create_role(
        &self,
        actor: UserId,
        guild_id: GuildId,
        input: NewRole,
    ) -> ChatResult<Role> {
        let guild = self.guild(guild_id).await?;
        let (member, roles) = self.guild_actor(&guild, actor).await?;
        let a = GuildActor::new(&guild, &member, &roles);
        let role = role_rules::plan_create_role(
            &a,
            guild_id,
            RoleId::from(self.ids.generate()),
            roles.len(),
            input,
            &self.limits,
        )?;
        self.repo.insert_role(&role).await?;
        Ok(role)
    }

    pub async fn delete_role(
        &self,
        actor: UserId,
        guild_id: GuildId,
        role_id: RoleId,
    ) -> ChatResult<()> {
        let guild = self.guild(guild_id).await?;
        let (member, roles) = self.guild_actor(&guild, actor).await?;
        let a = GuildActor::new(&guild, &member, &roles);
        let role = roles
            .iter()
            .find(|r| r.id == role_id)
            .ok_or(ChatError::UnknownRole)?;
        role_rules::check_delete_role(&a, role)?;
        self.repo.delete_role(guild_id, role_id).await?;
        Ok(())
    }

    // ------------------------------------------------------- channels

    pub async fn create_channel(
        &self,
        actor: UserId,
        guild_id: GuildId,
        input: NewChannel,
    ) -> ChatResult<Channel> {
        let guild = self.guild(guild_id).await?;
        let (member, roles) = self.guild_actor(&guild, actor).await?;
        let a = GuildActor::new(&guild, &member, &roles);
        let existing = self.repo.list_guild_channels(guild_id).await?;
        let parent = input
            .parent_id
            .and_then(|pid| existing.iter().find(|c| c.id == pid));
        let channel = channel_rules::plan_create_channel(
            &a,
            guild_id,
            ChannelId::from(self.ids.generate()),
            existing.len(),
            parent,
            input,
            &self.limits,
        )?;
        self.repo.insert_channel(&channel, &[]).await?;
        Ok(channel)
    }

    // ------------------------------------------------------- messages

    pub async fn send_message(
        &self,
        author: UserId,
        channel_id: ChannelId,
        req: SendMessage,
    ) -> ChatResult<Message> {
        let acc = self.access(author, channel_id).await?;
        let reply_target = match req.reply_to {
            Some(id) => self.repo.get_message(channel_id, id).await?,
            None => None,
        };
        let last_sent = if acc.channel.rate_limit_per_user > 0 {
            self.repo.last_sent_ms(channel_id, author).await?
        } else {
            None
        };
        let input = CreateMessageInput {
            content: req.content,
            nonce: req.nonce,
            reply_to: req.reply_to,
            attachment_count: req.attachment_ids.len(),
            embed_count: req.embed_count,
            sticker_ids: req.sticker_ids.clone(),
            has_poll: req.has_poll,
            flags: req.flags,
        };
        let now = self.now();
        let v = message_rules::validate_create(
            &input,
            &CreateContext {
                author,
                channel: &acc.channel,
                perms: acc.perms,
                now_ms: now,
                reply_target: reply_target.as_ref(),
                guild_roles: &acc.roles,
                last_sent_by_author_ms: last_sent,
                limits: &self.limits,
            },
        )?;
        let message = Message {
            id: MessageId::from(self.ids.generate()),
            channel_id,
            guild_id: acc.channel.guild_id,
            author_id: author,
            kind: v.kind,
            content: v.content,
            flags: v.flags,
            edited_at_ms: None,
            reference: v.reference,
            attachment_ids: req.attachment_ids,
            embed_count: u8::try_from(req.embed_count).unwrap_or(u8::MAX),
            sticker_ids: req.sticker_ids,
            has_poll: req.has_poll,
            mention_everyone: v.mentions.everyone,
            mention_user_ids: v.mentions.users,
            mention_role_ids: v.mentions.roles,
            mention_channel_ids: v.mentions.channels,
            pinned: false,
            visibility: Visibility::Visible,
            deleted_at_ms: None,
            purge_after_ms: None,
            hold_id: None,
        };
        self.repo.insert_message(&message).await?;
        Ok(message)
    }

    async fn load_message(&self, channel: ChannelId, id: MessageId) -> ChatResult<Message> {
        self.repo
            .get_message(channel, id)
            .await?
            .ok_or(ChatError::UnknownMessage)
    }

    pub async fn edit_message(
        &self,
        editor: UserId,
        channel_id: ChannelId,
        message_id: MessageId,
        input: EditMessageInput,
    ) -> ChatResult<EditPlan> {
        let acc = self.access(editor, channel_id).await?;
        let message = self.load_message(channel_id, message_id).await?;
        let plan = message_rules::validate_edit(
            &input,
            &EditContext {
                editor,
                message: &message,
                channel_kind: acc.channel.kind,
                perms: acc.perms,
                now_ms: self.now(),
                guild_roles: &acc.roles,
                limits: &self.limits,
            },
        )?;
        self.repo.apply_edit(&plan).await?;
        Ok(plan)
    }

    pub async fn delete_message(
        &self,
        deleter: UserId,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> ChatResult<DeletePlan> {
        let acc = self.access(deleter, channel_id).await?;
        let message = self.load_message(channel_id, message_id).await?;
        let plan =
            message_rules::validate_delete(deleter, &message, acc.perms, self.now(), &self.limits)?;
        self.repo.apply_delete(&plan).await?;
        Ok(plan)
    }

    pub async fn bulk_delete(
        &self,
        actor: UserId,
        channel_id: ChannelId,
        ids: &[MessageId],
    ) -> ChatResult<Vec<DeletePlan>> {
        let acc = self.access(actor, channel_id).await?;
        let found = self.repo.get_messages(channel_id, ids).await?;
        let plans = message_rules::validate_bulk_delete(
            &acc.channel,
            acc.perms,
            ids,
            &found,
            self.now(),
            &self.limits,
        )?;
        for p in &plans {
            self.repo.apply_delete(p).await?;
        }
        Ok(plans)
    }

    pub async fn pin_message(
        &self,
        actor: UserId,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> ChatResult<()> {
        let acc = self.access(actor, channel_id).await?;
        let message = self.load_message(channel_id, message_id).await?;
        let count = self.repo.count_pins(channel_id).await?;
        message_rules::validate_pin(&acc.channel, &message, acc.perms, count, &self.limits)?;
        self.repo
            .set_pinned(channel_id, message_id, actor, true, self.now())
            .await?;
        Ok(())
    }

    pub async fn unpin_message(
        &self,
        actor: UserId,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> ChatResult<()> {
        let acc = self.access(actor, channel_id).await?;
        let message = self.load_message(channel_id, message_id).await?;
        message_rules::validate_unpin(&acc.channel, &message, acc.perms)?;
        self.repo
            .set_pinned(channel_id, message_id, actor, false, self.now())
            .await?;
        Ok(())
    }
}

#[cfg(test)]
impl<R> ChatService<R> {
    pub(crate) fn repo_for_tests(&self) -> &R {
        &self.repo
    }
}
