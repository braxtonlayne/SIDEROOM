//! Orchestration: load context from repositories, run the pure rules, persist the plan.
//! No database dependency; generic over [`ChatRepository`] and an injected clock.

use crate::channel_rules::{self, NewChannel};
use crate::domain::{Channel, Guild, Member, Message, Role, Visibility};
use crate::error::{ChatError, ChatResult};
use crate::guild_rules::{self, GuildBlueprint};
use crate::limits::ChatLimits;
use crate::message::{Component, Embed, Poll};
use crate::message_hooks::MessageHooks;
use crate::message_rules::{
    self, AllowedMentions, CreateContext, CreateMessageInput, DeletePlan, EditContext,
    EditMessageInput, EditPlan,
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
    pub(crate) hooks: Option<Arc<dyn MessageHooks>>,
}

/// Request body of "create message" with attachments already resolved to upload ids.
#[derive(Debug, Clone, Default)]
pub struct SendMessage {
    pub content: String,
    pub nonce: Option<String>,
    pub reply_to: Option<MessageId>,
    pub attachment_ids: Vec<AttachmentId>,
    pub embeds: Vec<Embed>,
    pub components: Vec<Component>,
    pub sticker_ids: Vec<u64>,
    pub poll: Option<Poll>,
    pub flags: u32,
    /// `None` means Discord's default: every parsed mention may ping.
    pub allowed_mentions: Option<AllowedMentions>,
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
            hooks: None,
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

    /// Install the post-send hooks (reopen DMs, mention counts; open questions 114
    /// and 117). Without hooks a message is only stored.
    pub fn with_hooks(mut self, hooks: Arc<dyn MessageHooks>) -> Self {
        self.hooks = Some(hooks);
        self
    }

    /// Mentioned users who may view the channel (open questions 98 and 117). DMs
    /// count recipients; guild channels count members with VIEW_CHANNEL.
    async fn eligible_mentions(
        &self,
        guild: Option<&Guild>,
        channel: &Channel,
        users: Vec<UserId>,
    ) -> ChatResult<Vec<UserId>> {
        let mut out = Vec::with_capacity(users.len());
        let now = self.now();
        for u in users {
            let ok = match guild {
                None => self.repo.is_recipient(channel.id, u).await?,
                Some(g) => match self.repo.get_member(g.id, u).await? {
                    Some(member) => {
                        let roles = self.repo.list_roles(g.id).await?;
                        let overwrites = self.repo.get_overwrites(channel.id).await?;
                        channel_permissions(g, &member, &roles, &overwrites, now)
                            .has(Permissions::VIEW_CHANNEL)
                    }
                    None => false,
                },
            };
            if ok {
                out.push(u);
            }
        }
        Ok(out)
    }

    /// Everyone an `@everyone`/`@here` or role mention notifies: guild members
    /// (paged) who hold a pinged role, or all of them for `@everyone`, and who have
    /// VIEW_CHANNEL (open question 117). Explicit user mentions are handled by
    /// [`Self::eligible_mentions`].
    async fn fanout_recipients(
        &self,
        guild: &Guild,
        channel: &Channel,
        mentions: &message_rules::ResolvedMentions,
    ) -> ChatResult<Vec<UserId>> {
        if !mentions.everyone && mentions.roles.is_empty() {
            return Ok(Vec::new());
        }
        const PAGE: usize = 1000;
        const MAX_PAGES: usize = 100;
        let roles = self.repo.list_roles(guild.id).await?;
        let overwrites = self.repo.get_overwrites(channel.id).await?;
        let now = self.now();
        let mut out = Vec::new();
        let mut after = None;
        for _ in 0..MAX_PAGES {
            let page = self.repo.page_members(guild.id, after, PAGE).await?;
            for m in &page {
                let pinged =
                    mentions.everyone || m.role_ids.iter().any(|r| mentions.roles.contains(r));
                if pinged
                    && channel_permissions(guild, m, &roles, &overwrites, now)
                        .has(Permissions::VIEW_CHANNEL)
                {
                    out.push(m.user_id);
                }
            }
            match page.last() {
                Some(last) if page.len() == PAGE => after = Some(last.user_id),
                _ => break,
            }
        }
        Ok(out)
    }

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
            embed_count: req.embeds.len(),
            sticker_ids: req.sticker_ids.clone(),
            has_poll: req.poll.is_some(),
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

        // Mentions: the request's allowed_mentions gate the pings (open question 152),
        // then only users who can see the channel are kept (open questions 98, 117).
        let mut mentions = match &req.allowed_mentions {
            Some(allowed) => message_rules::apply_allowed_mentions(
                v.mentions,
                allowed,
                reply_target.as_ref().map(|m| m.author_id),
            ),
            None => v.mentions,
        };
        let guild = match acc.channel.guild_id {
            Some(gid) => Some(self.guild(gid).await?),
            None => None,
        };
        mentions.users = self
            .eligible_mentions(guild.as_ref(), &acc.channel, mentions.users)
            .await?;
        let mut notify = mentions.users.clone();
        if let Some(g) = &guild {
            notify.extend(self.fanout_recipients(g, &acc.channel, &mentions).await?);
        }

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
            embeds: req.embeds,
            components: req.components,
            sticker_ids: req.sticker_ids,
            poll: req.poll,
            mention_everyone: mentions.everyone,
            mention_user_ids: mentions.users,
            mention_role_ids: mentions.roles,
            mention_channel_ids: mentions.channels,
            pinned: false,
            visibility: Visibility::Visible,
            deleted_at_ms: None,
            purge_after_ms: None,
            hold_id: None,
        };
        self.repo.insert_message(&message).await?;
        if let Some(hooks) = &self.hooks {
            hooks.message_sent(&message, &notify).await;
        }
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
