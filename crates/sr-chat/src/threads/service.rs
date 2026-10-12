//! Thread use cases: start, join/leave, list, modify, delete, forum posts and the
//! activity bookkeeping behind auto-archive. Permission decisions come from
//! [`super::rules`]; this layer loads context and persists plans.

use super::access::{guild_channel_permissions, overwrite_source};
use super::model::ThreadMember;
use super::repo::{ActivityOutcome, ArchivedFilter, ThreadRepository};
use super::rules::{
    self, ForumPost, StartThread, ThreadPatch, plan_forum_post, plan_modify_thread,
    plan_standalone_thread, plan_thread_from_message,
};
use crate::domain::{Channel, ChannelType, Guild, Member, Message, MessageType, Role, Visibility};
use crate::error::{ChatError, ChatResult};
use crate::perms::channel_permissions;
use crate::repo::ChatRepository;
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, Clock, GuildId, MessageId, SnowflakeGenerator, UserId};
use std::collections::HashMap;
use std::sync::Arc;

/// Page size bounds for member and archived listings.
pub const MAX_LIST_PAGE: usize = 100;

pub struct ThreadService<R> {
    repo: R,
    clock: Arc<dyn Clock>,
    ids: SnowflakeGenerator<Arc<dyn Clock>>,
}

/// A thread with the acting user's membership.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadView {
    pub channel: Channel,
    pub member: Option<ThreadMember>,
}

/// Result of starting a thread. `messages` are system messages the service stored
/// (the "thread created" notice in the parent and the starter reference in the
/// thread); the caller announces them as `MESSAGE_CREATE`.
#[derive(Debug, Clone, PartialEq)]
pub struct StartedThread {
    pub view: ThreadView,
    pub parent: Channel,
    pub messages: Vec<Message>,
}

/// Outcome of a membership change. `changed` is false when it was a no-op.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberChange {
    pub thread: Channel,
    pub user: UserId,
    pub member: Option<ThreadMember>,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ThreadListing {
    pub threads: Vec<Channel>,
    /// The caller's membership rows among `threads`.
    pub members: Vec<ThreadMember>,
    pub has_more: bool,
}

/// What deleting a thread leaves for the event: the channel and who could see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedThread {
    pub channel: Channel,
    pub viewers: Vec<UserId>,
}

/// A thread resolved for one user.
pub struct ThreadAccess {
    pub thread: Channel,
    pub parent: Channel,
    pub perms: Permissions,
    pub is_member: bool,
    pub guild: Guild,
    pub member: Member,
    pub roles: Vec<Role>,
}

impl<R: ChatRepository + ThreadRepository> ThreadService<R> {
    pub fn new(repo: R, clock: Arc<dyn Clock>, ids: SnowflakeGenerator<Arc<dyn Clock>>) -> Self {
        Self { repo, clock, ids }
    }

    fn now(&self) -> i64 {
        self.clock.now_unix_ms()
    }

    async fn guild_ctx(
        &self,
        guild_id: GuildId,
        user: UserId,
    ) -> ChatResult<(Guild, Member, Vec<Role>)> {
        let guild = self
            .repo
            .get_guild(guild_id)
            .await?
            .ok_or(ChatError::UnknownGuild)?;
        let member = self
            .repo
            .get_member(guild_id, user)
            .await?
            .ok_or(ChatError::MissingAccess)?;
        let roles = self.repo.list_roles(guild_id).await?;
        Ok((guild, member, roles))
    }

    /// A non-thread guild channel and the user's permissions in it.
    async fn parent_access(
        &self,
        user: UserId,
        channel_id: ChannelId,
    ) -> ChatResult<(Channel, Permissions, Guild, Member, Vec<Role>)> {
        let channel = self
            .repo
            .get_channel(channel_id)
            .await?
            .ok_or(ChatError::UnknownChannel)?;
        let gid = channel.guild_id.ok_or(ChatError::NotInGuild)?;
        let (guild, member, roles) = self.guild_ctx(gid, user).await?;
        let overwrites = self.repo.get_overwrites(channel.id).await?;
        let perms = channel_permissions(&guild, &member, &roles, &overwrites, self.now());
        Ok((channel, perms, guild, member, roles))
    }

    /// Resolve a thread for `user`. Unknown ids and non-threads are 404; a thread the
    /// user cannot see is 403.
    pub async fn access(&self, user: UserId, thread_id: ChannelId) -> ChatResult<ThreadAccess> {
        let thread = self
            .repo
            .get_channel(thread_id)
            .await?
            .ok_or(ChatError::UnknownChannel)?;
        if !thread.kind.is_thread() || thread.thread.is_none() {
            return Err(ChatError::UnknownChannel);
        }
        let gid = thread.guild_id.ok_or(ChatError::NotInGuild)?;
        let parent_id = thread.parent_id.ok_or(ChatError::UnknownChannel)?;
        let parent = self
            .repo
            .get_channel(parent_id)
            .await?
            .ok_or(ChatError::UnknownChannel)?;
        let (guild, member, roles) = self.guild_ctx(gid, user).await?;
        let is_member = self.repo.is_thread_member(thread.id, user).await?;
        let perms =
            guild_channel_permissions(&self.repo, &guild, &member, &roles, &thread, self.now())
                .await?;
        Ok(ThreadAccess {
            thread,
            parent,
            perms,
            is_member,
            guild,
            member,
            roles,
        })
    }

    async fn fresh(&self, id: ChannelId) -> ChatResult<Channel> {
        self.repo
            .get_channel(id)
            .await?
            .ok_or(ChatError::UnknownChannel)
    }

    fn system_message(
        &self,
        kind: MessageType,
        channel: &Channel,
        author: UserId,
        content: String,
        reference: Option<crate::domain::MessageReference>,
    ) -> Message {
        Message {
            id: MessageId::from(self.ids.generate()),
            channel_id: channel.id,
            guild_id: channel.guild_id,
            author_id: author,
            kind,
            content,
            flags: 0,
            edited_at_ms: None,
            reference,
            attachment_ids: Vec::new(),
            embeds: Vec::new(),
            components: Vec::new(),
            sticker_ids: Vec::new(),
            poll: None,
            mention_everyone: false,
            mention_user_ids: Vec::new(),
            mention_role_ids: Vec::new(),
            mention_channel_ids: Vec::new(),
            pinned: false,
            visibility: Visibility::Visible,
            deleted_at_ms: None,
            purge_after_ms: None,
            hold_id: None,
        }
    }

    // ----------------------------------------------------------- starting

    /// Store the notices that accompany a new thread and return the finished view.
    async fn finish_start(
        &self,
        actor: UserId,
        thread: Channel,
        parent: Channel,
        starter: Option<&Message>,
    ) -> ChatResult<StartedThread> {
        let mut messages = Vec::new();
        if let Some(orig) = starter {
            // The thread's first entry points at the message it grew from.
            let m = self.system_message(
                MessageType::ThreadStarterMessage,
                &thread,
                orig.author_id,
                String::new(),
                Some(crate::domain::MessageReference {
                    message_id: orig.id,
                    channel_id: parent.id,
                }),
            );
            self.repo.insert_message(&m).await?;
            messages.push(m);
        }
        if thread.kind != ChannelType::PrivateThread {
            let m = self.system_message(
                MessageType::ThreadCreated,
                &parent,
                actor,
                thread.name.clone().unwrap_or_default(),
                Some(crate::domain::MessageReference {
                    message_id: MessageId::new(thread.id.get()),
                    channel_id: thread.id,
                }),
            );
            self.repo.insert_message(&m).await?;
            messages.push(m);
        }
        let channel = self.fresh(thread.id).await?;
        let member = self.repo.get_thread_member(thread.id, actor).await?;
        Ok(StartedThread {
            view: ThreadView { channel, member },
            parent: self.fresh(parent.id).await?,
            messages,
        })
    }

    /// `POST /channels/{parent}/messages/{message}/threads`; the thread id is the
    /// message id.
    pub async fn start_from_message(
        &self,
        actor: UserId,
        parent_id: ChannelId,
        message_id: MessageId,
        input: StartThread,
    ) -> ChatResult<StartedThread> {
        let (parent, perms, ..) = self.parent_access(actor, parent_id).await?;
        let message = self
            .repo
            .get_message(parent_id, message_id)
            .await?
            .filter(Message::is_visible)
            .ok_or(ChatError::UnknownMessage)?;
        let planned =
            plan_thread_from_message(actor, perms, &parent, &message, &input, self.now())?;
        if self.repo.get_channel(planned.id).await?.is_some() {
            return Err(ChatError::ThreadAlreadyCreated);
        }
        self.repo.insert_thread(&planned, actor, self.now()).await?;
        self.finish_start(actor, planned, parent, Some(&message))
            .await
    }

    /// `POST /channels/{parent}/threads` in a text or announcement channel.
    pub async fn start_standalone(
        &self,
        actor: UserId,
        parent_id: ChannelId,
        input: StartThread,
    ) -> ChatResult<StartedThread> {
        let (parent, perms, ..) = self.parent_access(actor, parent_id).await?;
        let id = ChannelId::from(self.ids.generate());
        let planned = plan_standalone_thread(id, actor, perms, &parent, &input, self.now())?;
        self.repo.insert_thread(&planned, actor, self.now()).await?;
        self.finish_start(actor, planned, parent, None).await
    }

    /// Create the thread of a forum or media post. The caller stores the starter
    /// message next and calls [`Self::discard`] if that fails.
    pub async fn start_forum_post(
        &self,
        actor: UserId,
        forum_id: ChannelId,
        input: ForumPost,
    ) -> ChatResult<ThreadView> {
        let (forum, perms, ..) = self.parent_access(actor, forum_id).await?;
        let id = ChannelId::from(self.ids.generate());
        let planned = plan_forum_post(id, actor, perms, &forum, &input, self.now())?;
        self.repo.insert_thread(&planned, actor, self.now()).await?;
        let channel = self.fresh(id).await?;
        let member = self.repo.get_thread_member(id, actor).await?;
        Ok(ThreadView { channel, member })
    }

    /// Remove a thread that never became visible (its starter message failed).
    pub async fn discard(&self, thread: ChannelId) -> ChatResult<()> {
        self.repo.delete_channel(thread).await?;
        Ok(())
    }

    // ------------------------------------------------------------ members

    pub async fn view(&self, actor: UserId, thread_id: ChannelId) -> ChatResult<ThreadView> {
        let a = self.access(actor, thread_id).await?;
        if !a.perms.has(Permissions::VIEW_CHANNEL) {
            return Err(ChatError::MissingAccess);
        }
        let member = self.repo.get_thread_member(thread_id, actor).await?;
        Ok(ThreadView {
            channel: a.thread,
            member,
        })
    }

    /// Add `target` (the actor themselves for `PUT .../@me`).
    pub async fn add_member(
        &self,
        actor: UserId,
        thread_id: ChannelId,
        target: UserId,
    ) -> ChatResult<MemberChange> {
        let a = self.access(actor, thread_id).await?;
        rules::check_add_member(actor, target, a.perms, a.is_member, &a.thread)?;
        if target != actor {
            // The invitee must be able to see the parent channel.
            let (g, m, r) = self.guild_ctx(a.guild.id, target).await.map_err(|e| {
                if matches!(e, ChatError::MissingAccess) {
                    ChatError::UnknownMember
                } else {
                    e
                }
            })?;
            let ow = self.repo.get_overwrites(a.parent.id).await?;
            if !channel_permissions(&g, &m, &r, &ow, self.now()).has(Permissions::VIEW_CHANNEL) {
                return Err(ChatError::MissingAccess);
            }
        }
        let changed = self
            .repo
            .add_thread_member(thread_id, target, self.now())
            .await?;
        let member = self.repo.get_thread_member(thread_id, target).await?;
        Ok(MemberChange {
            thread: self.fresh(thread_id).await?,
            user: target,
            member,
            changed,
        })
    }

    /// Remove `target` (the actor themselves for `DELETE .../@me`).
    pub async fn remove_member(
        &self,
        actor: UserId,
        thread_id: ChannelId,
        target: UserId,
    ) -> ChatResult<MemberChange> {
        let a = self.access(actor, thread_id).await?;
        rules::check_remove_member(actor, target, a.perms, &a.thread)?;
        let changed = self.repo.remove_thread_member(thread_id, target).await?;
        Ok(MemberChange {
            thread: self.fresh(thread_id).await?,
            user: target,
            member: None,
            changed,
        })
    }

    pub async fn get_member(
        &self,
        actor: UserId,
        thread_id: ChannelId,
        user: UserId,
    ) -> ChatResult<ThreadMember> {
        let a = self.access(actor, thread_id).await?;
        if !a.perms.has(Permissions::VIEW_CHANNEL) {
            return Err(ChatError::MissingAccess);
        }
        self.repo
            .get_thread_member(thread_id, user)
            .await?
            .ok_or(ChatError::UnknownMember)
    }

    pub async fn list_members(
        &self,
        actor: UserId,
        thread_id: ChannelId,
        after: Option<UserId>,
        limit: usize,
    ) -> ChatResult<Vec<ThreadMember>> {
        let a = self.access(actor, thread_id).await?;
        if !a.perms.has(Permissions::VIEW_CHANNEL) {
            return Err(ChatError::MissingAccess);
        }
        Ok(self
            .repo
            .list_thread_members(thread_id, after, limit.clamp(1, MAX_LIST_PAGE))
            .await?)
    }

    // ------------------------------------------------------------ listing

    /// Parent permissions per parent channel, cached for one listing.
    async fn visible(
        &self,
        threads: Vec<Channel>,
        guild: &Guild,
        member: &Member,
        roles: &[Role],
    ) -> ChatResult<Vec<Channel>> {
        let now = self.now();
        let mut parent_perms: HashMap<ChannelId, Permissions> = HashMap::new();
        let mut out = Vec::with_capacity(threads.len());
        for t in threads {
            let Some(src) = overwrite_source(&t) else {
                continue;
            };
            let base = match parent_perms.get(&src) {
                Some(p) => *p,
                None => {
                    let ow = self.repo.get_overwrites(src).await?;
                    let p = channel_permissions(guild, member, roles, &ow, now);
                    parent_perms.insert(src, p);
                    p
                }
            };
            if !base.has(Permissions::VIEW_CHANNEL) {
                continue;
            }
            let is_member = self.repo.is_thread_member(t.id, member.user_id).await?;
            if rules::thread_permissions(base, &t, is_member).has(Permissions::VIEW_CHANNEL) {
                out.push(t);
            }
        }
        Ok(out)
    }

    /// `GET /guilds/{id}/threads/active`: unarchived threads the user can see.
    pub async fn list_active(&self, actor: UserId, guild_id: GuildId) -> ChatResult<ThreadListing> {
        let (guild, member, roles) = self.guild_ctx(guild_id, actor).await?;
        let all = self.repo.list_active_threads(guild_id).await?;
        let threads = self.visible(all, &guild, &member, &roles).await?;
        let ids: Vec<ChannelId> = threads.iter().map(|t| t.id).collect();
        let members = self.repo.thread_memberships(actor, &ids).await?;
        Ok(ThreadListing {
            threads,
            members,
            has_more: false,
        })
    }

    /// Archived threads of `parent_id`. Public needs `READ_MESSAGE_HISTORY`; all private
    /// threads additionally need `MANAGE_THREADS`.
    pub async fn list_archived(
        &self,
        actor: UserId,
        parent_id: ChannelId,
        filter: ArchivedFilter,
        before_ms: Option<i64>,
        limit: usize,
    ) -> ChatResult<ThreadListing> {
        let (parent, perms, ..) = self.parent_access(actor, parent_id).await?;
        if !perms.has(Permissions::VIEW_CHANNEL) {
            return Err(ChatError::MissingAccess);
        }
        if !(parent.kind.hosts_threads() || parent.kind.is_forum_like()) {
            return Err(ChatError::invalid(
                "channel_id",
                "CHANNEL_TYPE_INVALID",
                "This channel has no threads.",
            ));
        }
        if !perms.has(Permissions::READ_MESSAGE_HISTORY) {
            return Err(ChatError::MissingPermissions);
        }
        if filter == ArchivedFilter::Private && !perms.has(Permissions::MANAGE_THREADS) {
            return Err(ChatError::MissingPermissions);
        }
        let limit = limit.clamp(1, MAX_LIST_PAGE);
        let mut threads = self
            .repo
            .list_archived_threads(parent_id, filter, before_ms, limit + 1)
            .await?;
        let has_more = threads.len() > limit;
        threads.truncate(limit);
        let ids: Vec<ChannelId> = threads.iter().map(|t| t.id).collect();
        let members = self.repo.thread_memberships(actor, &ids).await?;
        Ok(ThreadListing {
            threads,
            members,
            has_more,
        })
    }

    // ------------------------------------------------------ modify/delete

    pub async fn modify(
        &self,
        actor: UserId,
        thread_id: ChannelId,
        patch: ThreadPatch,
    ) -> ChatResult<ThreadView> {
        let a = self.access(actor, thread_id).await?;
        let planned = plan_modify_thread(
            actor,
            a.perms,
            &a.thread,
            Some(&a.parent),
            &patch,
            self.now(),
        )?;
        self.repo.update_thread(&planned).await?;
        let channel = self.fresh(thread_id).await?;
        let member = self.repo.get_thread_member(thread_id, actor).await?;
        Ok(ThreadView { channel, member })
    }

    /// Everyone who can currently see `thread`, for the delete event.
    pub async fn viewers(&self, thread: &Channel) -> ChatResult<Vec<UserId>> {
        let (Some(gid), Some(src)) = (thread.guild_id, overwrite_source(thread)) else {
            return Ok(Vec::new());
        };
        let guild = self
            .repo
            .get_guild(gid)
            .await?
            .ok_or(ChatError::UnknownGuild)?;
        let roles = self.repo.list_roles(gid).await?;
        let ow = self.repo.get_overwrites(src).await?;
        let private = thread.kind == ChannelType::PrivateThread;
        let members: Vec<UserId> = if private {
            self.repo
                .list_thread_members(thread.id, None, 1000)
                .await?
                .into_iter()
                .map(|m| m.user_id)
                .collect()
        } else {
            Vec::new()
        };
        let now = self.now();
        let mut out = Vec::new();
        let mut after = None;
        loop {
            let page = self.repo.page_members(gid, after, 1000).await?;
            for m in &page {
                let p = channel_permissions(&guild, m, &roles, &ow, now);
                let ok = p.has(Permissions::VIEW_CHANNEL)
                    && (!private
                        || members.contains(&m.user_id)
                        || p.has(Permissions::MANAGE_THREADS));
                if ok {
                    out.push(m.user_id);
                }
            }
            match page.last() {
                Some(last) if page.len() == 1000 => after = Some(last.user_id),
                _ => break,
            }
        }
        Ok(out)
    }

    pub async fn delete(&self, actor: UserId, thread_id: ChannelId) -> ChatResult<DeletedThread> {
        let a = self.access(actor, thread_id).await?;
        rules::check_delete_thread(actor, a.perms, &a.thread, Some(&a.parent))?;
        let viewers = self.viewers(&a.thread).await?;
        self.repo.delete_channel(thread_id).await?;
        Ok(DeletedThread {
            channel: a.thread,
            viewers,
        })
    }

    // ----------------------------------------------------------- activity

    /// Bookkeeping for a message just stored in `message.channel_id`. `None` when that
    /// channel is not a thread.
    pub async fn record_message(&self, message: &Message) -> ChatResult<Option<ActivityOutcome>> {
        Ok(self
            .repo
            .record_thread_activity(message.channel_id, message.author_id, self.now())
            .await?)
    }

    /// Archive threads idle past their auto-archive duration. Returns them as
    /// archived, so the caller can announce `THREAD_UPDATE`.
    pub async fn archive_idle(&self, limit: usize) -> ChatResult<Vec<Channel>> {
        let ids = self.repo.archive_idle_threads(self.now(), limit).await?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(c) = self.repo.get_channel(id).await? {
                out.push(c);
            }
        }
        Ok(out)
    }
}
