//! Collaborators of the thread endpoints: the thread service behind an object-safe
//! trait, the event bus, and the helpers that turn state changes into gateway events.
//! Registered on the composition root as an [`AppState`](crate::AppState) extension.

use crate::routes::guilds::dto::{ChannelObject, ThreadMemberObject};
use async_trait::async_trait;
use serde_json::{Value, json};
use sr_chat::message_hooks::MessageHooks;
use sr_chat::repo::ChatRepository;
use sr_chat::settings::audit::AuditAction;
use sr_chat::threads::{
    ActivityOutcome, ArchivedFilter, DeletedThread, ForumPost, MemberChange, StartThread,
    StartedThread, ThreadListing, ThreadMember, ThreadPatch, ThreadRepository, ThreadService,
    ThreadView,
};
use sr_chat::{Channel, ChannelType, ChatResult, Message};
use sr_core::{ChannelId, MessageId, UserId};
use sr_events::{DomainEvent, EventBus};
use std::sync::Arc;

/// Object-safe view of [`ThreadService`].
#[async_trait]
pub trait ThreadOps: Send + Sync {
    async fn kind_of(&self, id: ChannelId) -> ChatResult<Option<ChannelType>>;
    async fn view(&self, actor: UserId, thread: ChannelId) -> ChatResult<ThreadView>;
    async fn start_from_message(
        &self,
        actor: UserId,
        parent: ChannelId,
        message: MessageId,
        input: StartThread,
    ) -> ChatResult<StartedThread>;
    async fn start_standalone(
        &self,
        actor: UserId,
        parent: ChannelId,
        input: StartThread,
    ) -> ChatResult<StartedThread>;
    async fn start_forum_post(
        &self,
        actor: UserId,
        forum: ChannelId,
        input: ForumPost,
    ) -> ChatResult<ThreadView>;
    async fn discard(&self, thread: ChannelId) -> ChatResult<()>;
    async fn add_member(
        &self,
        actor: UserId,
        thread: ChannelId,
        target: UserId,
    ) -> ChatResult<MemberChange>;
    async fn remove_member(
        &self,
        actor: UserId,
        thread: ChannelId,
        target: UserId,
    ) -> ChatResult<MemberChange>;
    async fn get_member(
        &self,
        actor: UserId,
        thread: ChannelId,
        user: UserId,
    ) -> ChatResult<ThreadMember>;
    async fn list_members(
        &self,
        actor: UserId,
        thread: ChannelId,
        after: Option<UserId>,
        limit: usize,
    ) -> ChatResult<Vec<ThreadMember>>;
    async fn list_active(
        &self,
        actor: UserId,
        guild: sr_core::GuildId,
    ) -> ChatResult<ThreadListing>;
    async fn list_archived(
        &self,
        actor: UserId,
        parent: ChannelId,
        filter: ArchivedFilter,
        before_ms: Option<i64>,
        limit: usize,
    ) -> ChatResult<ThreadListing>;
    async fn modify(
        &self,
        actor: UserId,
        thread: ChannelId,
        patch: ThreadPatch,
    ) -> ChatResult<ThreadView>;
    async fn delete(&self, actor: UserId, thread: ChannelId) -> ChatResult<DeletedThread>;
    async fn record_message(&self, message: &Message) -> ChatResult<Option<ActivityOutcome>>;
    async fn archive_idle(&self, limit: usize) -> ChatResult<Vec<Channel>>;
}

#[async_trait]
impl<R: ChatRepository + ThreadRepository + 'static> ThreadOps for ThreadService<R> {
    async fn kind_of(&self, id: ChannelId) -> ChatResult<Option<ChannelType>> {
        ThreadService::kind_of(self, id).await
    }
    async fn view(&self, actor: UserId, thread: ChannelId) -> ChatResult<ThreadView> {
        ThreadService::view(self, actor, thread).await
    }
    async fn start_from_message(
        &self,
        actor: UserId,
        parent: ChannelId,
        message: MessageId,
        input: StartThread,
    ) -> ChatResult<StartedThread> {
        ThreadService::start_from_message(self, actor, parent, message, input).await
    }
    async fn start_standalone(
        &self,
        actor: UserId,
        parent: ChannelId,
        input: StartThread,
    ) -> ChatResult<StartedThread> {
        ThreadService::start_standalone(self, actor, parent, input).await
    }
    async fn start_forum_post(
        &self,
        actor: UserId,
        forum: ChannelId,
        input: ForumPost,
    ) -> ChatResult<ThreadView> {
        ThreadService::start_forum_post(self, actor, forum, input).await
    }
    async fn discard(&self, thread: ChannelId) -> ChatResult<()> {
        ThreadService::discard(self, thread).await
    }
    async fn add_member(
        &self,
        actor: UserId,
        thread: ChannelId,
        target: UserId,
    ) -> ChatResult<MemberChange> {
        ThreadService::add_member(self, actor, thread, target).await
    }
    async fn remove_member(
        &self,
        actor: UserId,
        thread: ChannelId,
        target: UserId,
    ) -> ChatResult<MemberChange> {
        ThreadService::remove_member(self, actor, thread, target).await
    }
    async fn get_member(
        &self,
        actor: UserId,
        thread: ChannelId,
        user: UserId,
    ) -> ChatResult<ThreadMember> {
        ThreadService::get_member(self, actor, thread, user).await
    }
    async fn list_members(
        &self,
        actor: UserId,
        thread: ChannelId,
        after: Option<UserId>,
        limit: usize,
    ) -> ChatResult<Vec<ThreadMember>> {
        ThreadService::list_members(self, actor, thread, after, limit).await
    }
    async fn list_active(
        &self,
        actor: UserId,
        guild: sr_core::GuildId,
    ) -> ChatResult<ThreadListing> {
        ThreadService::list_active(self, actor, guild).await
    }
    async fn list_archived(
        &self,
        actor: UserId,
        parent: ChannelId,
        filter: ArchivedFilter,
        before_ms: Option<i64>,
        limit: usize,
    ) -> ChatResult<ThreadListing> {
        ThreadService::list_archived(self, actor, parent, filter, before_ms, limit).await
    }
    async fn modify(
        &self,
        actor: UserId,
        thread: ChannelId,
        patch: ThreadPatch,
    ) -> ChatResult<ThreadView> {
        ThreadService::modify(self, actor, thread, patch).await
    }
    async fn delete(&self, actor: UserId, thread: ChannelId) -> ChatResult<DeletedThread> {
        ThreadService::delete(self, actor, thread).await
    }
    async fn record_message(&self, message: &Message) -> ChatResult<Option<ActivityOutcome>> {
        ThreadService::record_message(self, message).await
    }
    async fn archive_idle(&self, limit: usize) -> ChatResult<Vec<Channel>> {
        ThreadService::archive_idle(self, limit).await
    }
}

/// Thread service plus the event bus.
pub struct ThreadsDeps {
    pub ops: Arc<dyn ThreadOps>,
    bus: Arc<dyn EventBus>,
    audit: Option<Arc<dyn crate::routes::guilds::backend::AuditSink>>,
}

/// Page size of the auto-archive job per run.
pub const AUTO_ARCHIVE_BATCH: usize = 200;

/// `{id, user_id, join_timestamp, flags, guild_id}`: the thread member as the gateway
/// sends it.
pub fn member_event_object(m: &ThreadMember, guild: Option<sr_core::GuildId>) -> Value {
    let mut v = serde_json::to_value(ThreadMemberObject::from(m)).unwrap_or(Value::Null);
    if let (Some(g), Some(o)) = (guild, v.as_object_mut()) {
        o.insert("guild_id".into(), json!(g.to_string()));
    }
    v
}

impl ThreadsDeps {
    pub fn new<R: ChatRepository + ThreadRepository + 'static>(
        service: ThreadService<R>,
        bus: Arc<dyn EventBus>,
    ) -> Self {
        Self {
            ops: Arc::new(service),
            bus,
            audit: None,
        }
    }

    /// Write thread creates, edits and deletes to the guild audit log (LNG-SSET-056).
    pub fn with_audit(mut self, sink: Arc<dyn crate::routes::guilds::backend::AuditSink>) -> Self {
        self.audit = Some(sink);
        self
    }

    async fn audit(&self, channel: &Channel, actor: UserId, action: AuditAction) {
        if let (Some(sink), Some(guild)) = (&self.audit, channel.guild_id) {
            let options = json!({ "parent_id": channel.parent_id.map(|p| p.to_string()) });
            sink.record_with_options(guild, actor, action, Some(channel.id.get()), options)
                .await;
        }
    }

    /// Publish; a bus failure is logged, never surfaced (the change is committed).
    async fn emit(&self, ev: DomainEvent) {
        for topic in ev.topics() {
            if let Err(e) = self.bus.publish(topic, ev.clone()).await {
                tracing::warn!(error = %e, "failed to publish domain event");
            }
        }
    }

    /// The thread as a Discord channel object (no per-viewer `member`).
    pub fn object(channel: &Channel) -> Value {
        serde_json::to_value(ChannelObject::new(channel, &[])).unwrap_or(Value::Null)
    }

    pub async fn publish_created(&self, channel: &Channel) {
        let Some(guild) = channel.guild_id else {
            return;
        };
        if let Some(owner) = channel.owner_id {
            self.audit(channel, owner, AuditAction::ThreadCreate).await;
        }
        let mut payload = Self::object(channel);
        if let Some(o) = payload.as_object_mut() {
            o.insert("newly_created".into(), json!(true));
        }
        self.emit(DomainEvent::ThreadCreated {
            guild,
            thread: channel.id,
            payload,
        })
        .await;
    }

    pub async fn publish_updated(&self, channel: &Channel) {
        let Some(guild) = channel.guild_id else {
            return;
        };
        self.emit(DomainEvent::ThreadUpdated {
            guild,
            thread: channel.id,
            payload: Self::object(channel),
        })
        .await;
    }

    pub async fn publish_deleted(&self, channel: &Channel, viewers: Vec<UserId>) {
        let Some(guild) = channel.guild_id else {
            return;
        };
        self.emit(DomainEvent::ThreadDeleted {
            guild,
            thread: channel.id,
            payload: json!({
                "id": channel.id.to_string(),
                "guild_id": guild.to_string(),
                "parent_id": channel.parent_id.map(|p| p.to_string()),
                "type": channel.kind.code(),
            }),
            viewers,
        })
        .await;
    }

    /// `THREAD_MEMBERS_UPDATE` for an add and/or remove, plus the self event for the
    /// added user. Removed users of a private thread still hear about it.
    pub async fn publish_members(
        &self,
        channel: &Channel,
        added: &[ThreadMember],
        removed: &[UserId],
    ) {
        let Some(guild) = channel.guild_id else {
            return;
        };
        let count = channel.thread.as_ref().map_or(0, |t| t.member_count);
        let mut payload = json!({
            "id": channel.id.to_string(),
            "guild_id": guild.to_string(),
            "member_count": count,
        });
        if let Some(o) = payload.as_object_mut() {
            if !added.is_empty() {
                o.insert(
                    "added_members".into(),
                    Value::Array(
                        added
                            .iter()
                            .map(|m| member_event_object(m, Some(guild)))
                            .collect(),
                    ),
                );
            }
            if !removed.is_empty() {
                o.insert(
                    "removed_member_ids".into(),
                    json!(removed.iter().map(UserId::to_string).collect::<Vec<_>>()),
                );
            }
        }
        let also = if channel.kind == ChannelType::PrivateThread {
            removed.to_vec()
        } else {
            Vec::new()
        };
        self.emit(DomainEvent::ThreadMembersUpdated {
            guild,
            thread: channel.id,
            payload,
            also,
        })
        .await;
        for m in added {
            self.emit(DomainEvent::ThreadMemberUpdated {
                user: m.user_id,
                payload: member_event_object(m, Some(guild)),
            })
            .await;
        }
    }

    /// Tell a user their own membership row (they were added to a thread).
    pub async fn publish_self_member(&self, channel: &Channel, member: &ThreadMember) {
        self.emit(DomainEvent::ThreadMemberUpdated {
            user: member.user_id,
            payload: member_event_object(member, channel.guild_id),
        })
        .await;
    }

    /// Edit a thread and announce `THREAD_UPDATE`.
    pub async fn modify(
        &self,
        actor: UserId,
        thread: ChannelId,
        patch: ThreadPatch,
    ) -> ChatResult<ChannelObject> {
        let view = self.ops.modify(actor, thread, patch).await?;
        self.publish_updated(&view.channel).await;
        self.audit(&view.channel, actor, AuditAction::ThreadUpdate)
            .await;
        Ok(ChannelObject::new(&view.channel, &[]).with_member(view.member.as_ref()))
    }

    /// Delete a thread and announce `THREAD_DELETE` to everyone who could see it.
    pub async fn delete(&self, actor: UserId, thread: ChannelId) -> ChatResult<ChannelObject> {
        let del = self.ops.delete(actor, thread).await?;
        self.publish_deleted(&del.channel, del.viewers).await;
        self.audit(&del.channel, actor, AuditAction::ThreadDelete)
            .await;
        Ok(ChannelObject::new(&del.channel, &[]))
    }

    /// Archive threads idle past their duration and announce each. Returns how many.
    pub async fn auto_archive(&self) -> ChatResult<usize> {
        let archived = self.ops.archive_idle(AUTO_ARCHIVE_BATCH).await?;
        for c in &archived {
            self.publish_updated(c).await;
        }
        Ok(archived.len())
    }

    /// Message hook that keeps thread activity current (counters, reopening archived
    /// threads, auto-joining posters) and announces what changed.
    pub fn activity_hook(self: &Arc<Self>) -> Arc<dyn MessageHooks> {
        Arc::new(ActivityHook(self.clone()))
    }
}

struct ActivityHook(Arc<ThreadsDeps>);

#[async_trait]
impl MessageHooks for ActivityHook {
    async fn message_sent(&self, message: &Message, _notify: &[UserId]) {
        match self.0.ops.record_message(message).await {
            Ok(Some(out)) => {
                if out.unarchived {
                    self.0.publish_updated(&out.thread).await;
                }
                if let Some(m) = out.joined {
                    self.0
                        .publish_members(&out.thread, std::slice::from_ref(&m), &[])
                        .await;
                }
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(error = %e, channel = %message.channel_id, "thread activity failed");
            }
        }
    }
}
