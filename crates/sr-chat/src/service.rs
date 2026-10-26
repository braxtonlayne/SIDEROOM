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
    /// Set for a forward: the source message and the copy of it (LNG-MSG-007). The
    /// caller has already checked the forwarder may read the source.
    pub forward: Option<ForwardRequest>,
}

/// What a forward points at and carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardRequest {
    pub source_message: MessageId,
    pub source_channel: ChannelId,
    pub snapshot: crate::message::ForwardSnapshot,
}

/// A channel resolved for one author, handed to [`ChatService::send_resolved`] by a caller that already
/// checked access (see there).
#[derive(Debug, Clone)]
pub struct ResolvedChannel {
    pub channel: Channel,
    pub perms: Permissions,
    pub roles: Vec<Role>,
    /// The guild row of a guild channel; `None` for a DM (or to have it read).
    pub guild: Option<Guild>,
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
                let perms = crate::threads::access::guild_channel_permissions(
                    &self.repo,
                    &guild,
                    &member,
                    &roles,
                    &channel,
                    self.now(),
                )
                .await?;
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

    /// Create a guild from a template snapshot in one repository call (D-180): the
    /// roles, channels and overwrites it carries replace the default layout.
    pub async fn create_guild_from_snapshot(
        &self,
        owner: UserId,
        name: &str,
        snapshot: &crate::settings::templates::TemplateSnapshot,
    ) -> ChatResult<GuildBlueprint> {
        let bp = guild_rules::plan_guild_from_snapshot(owner, name, self.now(), snapshot, || {
            self.ids.generate()
        })?;
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
        mut input: NewChannel,
    ) -> ChatResult<Channel> {
        let guild = self.guild(guild_id).await?;
        let (member, roles) = self.guild_actor(&guild, actor).await?;
        let a = GuildActor::new(&guild, &member, &roles);
        input.forum.assign_tag_ids(|| self.ids.generate().get());
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
                        crate::threads::access::guild_channel_permissions(
                            &self.repo, g, &member, &roles, channel, now,
                        )
                        .await?
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

    /// Guild members (paged by user id) for whom `pick` holds and who have
    /// VIEW_CHANNEL in `channel`.
    async fn viewers_where(
        &self,
        guild: &Guild,
        channel: &Channel,
        pick: impl Fn(&Member) -> bool,
    ) -> ChatResult<Vec<UserId>> {
        const PAGE: usize = 1000;
        const MAX_PAGES: usize = 100;
        let roles = self.repo.list_roles(guild.id).await?;
        let overwrites = match crate::threads::access::overwrite_source(channel) {
            Some(src) => self.repo.get_overwrites(src).await?,
            None => return Ok(Vec::new()),
        };
        let private_members: Option<Vec<UserId>> =
            if channel.kind == crate::domain::ChannelType::PrivateThread {
                let mut all = Vec::new();
                let mut cursor = None;
                loop {
                    let page = self
                        .repo
                        .list_thread_member_ids(channel.id, cursor, 1000)
                        .await?;
                    all.extend(page.iter().copied());
                    match page.last() {
                        Some(last) if page.len() == 1000 => cursor = Some(*last),
                        _ => break,
                    }
                }
                Some(all)
            } else {
                None
            };
        let now = self.now();
        let mut out = Vec::new();
        let mut after = None;
        for _ in 0..MAX_PAGES {
            let page = self.repo.page_members(guild.id, after, PAGE).await?;
            for m in &page {
                let perms = channel_permissions(guild, m, &roles, &overwrites, now);
                let gate = private_members.as_ref().is_none_or(|ms| {
                    ms.contains(&m.user_id) || perms.has(Permissions::MANAGE_THREADS)
                });
                if pick(m) && gate && perms.has(Permissions::VIEW_CHANNEL) {
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

    /// Everyone who can currently see `channel` (guild members with VIEW_CHANNEL,
    /// or the recipients of a DM). Producers compute this before a delete so the
    /// gateway can restrict `CHANNEL_DELETE` (open question 159).
    pub async fn channel_viewers(&self, channel_id: ChannelId) -> ChatResult<Vec<UserId>> {
        let channel = self
            .repo
            .get_channel(channel_id)
            .await?
            .ok_or(ChatError::UnknownChannel)?;
        match channel.guild_id {
            None => Ok(self.repo.list_recipients(channel_id).await?),
            Some(gid) => {
                let guild = self.guild(gid).await?;
                self.viewers_where(&guild, &channel, |_| true).await
            }
        }
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
        self.viewers_where(guild, channel, |m| {
            mentions.everyone || m.role_ids.iter().any(|r| mentions.roles.contains(r))
        })
        .await
    }

    pub async fn send_message(
        &self,
        author: UserId,
        channel_id: ChannelId,
        req: SendMessage,
    ) -> ChatResult<Message> {
        let acc = self.access(author, channel_id).await?;
        let guild = match acc.channel.guild_id {
            Some(gid) => Some(self.guild(gid).await?),
            None => None,
        };
        self.send_resolved(
            author,
            ResolvedChannel {
                channel: acc.channel,
                perms: acc.perms,
                roles: acc.roles,
                guild,
            },
            req,
        )
        .await
    }

    /// [`Self::send_message`] into a channel the caller has already resolved for `author` (RDY-PERF-002: the
    /// message endpoint resolved channel, guild, member, roles and overwrites, then this service resolved them
    /// again, five of the statements of every message sent). `resolved` must be what [`Self::send_message`]
    /// would compute: the channel, the author's effective permissions and the guild's roles there, and the
    /// guild row of a guild channel.
    pub async fn send_resolved(
        &self,
        author: UserId,
        resolved: ResolvedChannel,
        req: SendMessage,
    ) -> ChatResult<Message> {
        let channel_id = resolved.channel.id;
        let acc = Access {
            channel: resolved.channel,
            perms: resolved.perms,
            roles: resolved.roles,
        };
        if acc.channel.kind.is_thread() {
            crate::threads::rules::check_can_send(&acc.channel, acc.perms)?;
        }
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
            has_forward: req.forward.is_some(),
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
        let guild = match (acc.channel.guild_id, resolved.guild) {
            (Some(_), Some(g)) => Some(g),
            (Some(gid), None) => Some(self.guild(gid).await?),
            (None, _) => None,
        };
        mentions.users = self
            .eligible_mentions(guild.as_ref(), &acc.channel, mentions.users)
            .await?;
        let mut notify = mentions.users.clone();
        if let Some(g) = &guild {
            notify.extend(self.fanout_recipients(g, &acc.channel, &mentions).await?);
        }

        let mut message = Message {
            id: MessageId::from(self.ids.generate()),
            channel_id,
            guild_id: acc.channel.guild_id,
            author_id: author,
            kind: v.kind,
            content: v.content,
            flags: v.flags,
            edited_at_ms: None,
            reference: match req.forward {
                Some(f) => Some(crate::domain::MessageReference {
                    message_id: f.source_message,
                    channel_id: f.source_channel,
                    forward: Some(f.snapshot),
                }),
                None => v.reference,
            },
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
        if let Some(hooks) = &self.hooks {
            message.flags |= hooks.extra_flags(&message).await;
        }
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
        let mut plan = plan;
        // PAR-FU-002: a content edit re-screens the message; the server-set bits are recomputed from the new text
        // (a flags-only edit keeps them).
        if let Some(hooks) = &self.hooks
            && plan.content != message.content
        {
            let edited = Message {
                content: plan.content.clone(),
                ..message.clone()
            };
            plan.flags = (plan.flags & !crate::domain::message_flags::LIKELY_SPAM)
                | hooks.extra_flags(&edited).await;
        }
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

    /// Restore a deleted message (N-21): the acting moderator needs `MANAGE_MESSAGES` in the channel. See
    /// [`message_rules::validate_restore`] for what may come back (D-121, D-122).
    pub async fn restore_message(
        &self,
        restorer: UserId,
        channel_id: ChannelId,
        message_id: MessageId,
        by_platform: bool,
    ) -> ChatResult<message_rules::RestorePlan> {
        let acc = self.access(restorer, channel_id).await?;
        let message = self.load_message(channel_id, message_id).await?;
        let plan = message_rules::validate_restore(&message, acc.perms, by_platform, self.now())?;
        self.repo.apply_restore(&plan).await?;
        Ok(plan)
    }

    /// Whether `actor` may moderate other people's messages in the channel (`MANAGE_MESSAGES`), and the message
    /// exists there, hidden or not.
    pub async fn authorize_moderation(
        &self,
        actor: UserId,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> ChatResult<Message> {
        let acc = self.access(actor, channel_id).await?;
        let message = self.load_message(channel_id, message_id).await?;
        message_rules::validate_moderate(acc.perms)?;
        Ok(message)
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
