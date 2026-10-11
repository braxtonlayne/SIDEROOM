//! Collaborators of the message endpoints, injected as an axum `Extension`.
//!
//! `AppState` has no slot for chat services (it is outside this module's remit),
//! so [`MessagesDeps`] is layered on by [`super::router_with`].

use super::error::{MsgError, MsgResult};
use super::store::MessageStore;
use async_trait::async_trait;
use sr_chat::message_rules::{DeletePlan, EditMessageInput, EditPlan};
use sr_chat::perms::{channel_permissions, dm_permissions};
use sr_chat::repo::ChatRepository;
use sr_chat::{
    Channel, ChatError, ChatLimits, ChatResult, ChatService, Message, Role, SendMessage,
};
use sr_core::permissions::Permissions;
use sr_core::snowflake::SnowflakeError;
use sr_core::{
    AttachmentId, ChannelId, Clock, ErrorCode, MessageId, Snowflake, SnowflakeGenerator, UserId,
};
use sr_events::{DomainEvent, EventBus};
use sr_storage::ObjectStorage;
use sr_storage::upload::SizePolicy;
use std::sync::Arc;
use std::time::Duration;

/// Per-user upload size entitlement (donor perks, doc 08). The result is clamped
/// to `SizePolicy` bounds by the caller.
#[async_trait]
pub trait UploadEntitlements: Send + Sync {
    async fn max_upload_bytes(&self, user: UserId) -> u64;
}

/// The same limit for everyone.
#[derive(Debug, Clone, Copy)]
pub struct FixedEntitlements(pub u64);

#[async_trait]
impl UploadEntitlements for FixedEntitlements {
    async fn max_upload_bytes(&self, _user: UserId) -> u64 {
        self.0
    }
}

/// Object-safe view of the `ChatService` operations used here.
#[async_trait]
trait ChatOps: Send + Sync {
    async fn send(&self, a: UserId, c: ChannelId, r: SendMessage) -> ChatResult<Message>;
    async fn edit(
        &self,
        u: UserId,
        c: ChannelId,
        m: MessageId,
        i: EditMessageInput,
    ) -> ChatResult<EditPlan>;
    async fn delete(&self, u: UserId, c: ChannelId, m: MessageId) -> ChatResult<DeletePlan>;
    async fn bulk(&self, u: UserId, c: ChannelId, ids: &[MessageId])
    -> ChatResult<Vec<DeletePlan>>;
    async fn pin(&self, u: UserId, c: ChannelId, m: MessageId) -> ChatResult<()>;
    async fn unpin(&self, u: UserId, c: ChannelId, m: MessageId) -> ChatResult<()>;
}

#[async_trait]
impl<R: ChatRepository + 'static> ChatOps for ChatService<R> {
    async fn send(&self, a: UserId, c: ChannelId, r: SendMessage) -> ChatResult<Message> {
        self.send_message(a, c, r).await
    }
    async fn edit(
        &self,
        u: UserId,
        c: ChannelId,
        m: MessageId,
        i: EditMessageInput,
    ) -> ChatResult<EditPlan> {
        self.edit_message(u, c, m, i).await
    }
    async fn delete(&self, u: UserId, c: ChannelId, m: MessageId) -> ChatResult<DeletePlan> {
        self.delete_message(u, c, m).await
    }
    async fn bulk(
        &self,
        u: UserId,
        c: ChannelId,
        ids: &[MessageId],
    ) -> ChatResult<Vec<DeletePlan>> {
        self.bulk_delete(u, c, ids).await
    }
    async fn pin(&self, u: UserId, c: ChannelId, m: MessageId) -> ChatResult<()> {
        self.pin_message(u, c, m).await
    }
    async fn unpin(&self, u: UserId, c: ChannelId, m: MessageId) -> ChatResult<()> {
        self.unpin_message(u, c, m).await
    }
}

/// A channel resolved for one user.
pub struct Access {
    pub channel: Channel,
    pub perms: Permissions,
    pub roles: Vec<Role>,
}

impl Access {
    /// 403 `Missing Access` unless the channel is visible.
    pub fn require_view(&self) -> MsgResult<()> {
        if self.perms.has(Permissions::VIEW_CHANNEL) {
            Ok(())
        } else {
            Err(ChatError::MissingAccess.into())
        }
    }

    pub fn require(&self, needed: Permissions) -> MsgResult<()> {
        self.require_view()?;
        if self.perms.has(needed) {
            Ok(())
        } else {
            Err(ChatError::MissingPermissions.into())
        }
    }
}

pub struct MessagesDeps {
    chat: Arc<dyn ChatOps>,
    pub repo: Arc<dyn ChatRepository>,
    pub store: Arc<dyn MessageStore>,
    pub storage: Arc<dyn ObjectStorage>,
    bus: Arc<dyn EventBus>,
    clock: Arc<dyn Clock>,
    ids: SnowflakeGenerator<Arc<dyn Clock>>,
    entitlements: Arc<dyn UploadEntitlements>,
    pub limits: ChatLimits,
    pub size_policy: SizePolicy,
    /// How long a nonce keeps deduplicating (Discord: a few minutes).
    pub nonce_window_ms: i64,
    pub upload_url_ttl: Duration,
}

impl MessagesDeps {
    /// `repo` must be a cheap handle (`Clone`) because the chat service owns one copy
    /// and the routes keep another. `worker`/`process` identify this node's snowflake
    /// generator; attachment ids use `process + 1 (mod 32)` so they never collide with
    /// message ids minted by the chat service.
    #[allow(clippy::too_many_arguments)]
    pub fn new<R: ChatRepository + Clone + 'static>(
        repo: R,
        store: Arc<dyn MessageStore>,
        storage: Arc<dyn ObjectStorage>,
        bus: Arc<dyn EventBus>,
        entitlements: Arc<dyn UploadEntitlements>,
        clock: Arc<dyn Clock>,
        worker: u8,
        process: u8,
    ) -> Result<Self, SnowflakeError> {
        let limits = ChatLimits::default();
        let chat_ids = SnowflakeGenerator::new(clock.clone(), worker, process)?;
        let ids = SnowflakeGenerator::new(clock.clone(), worker, (process + 1) % 32)?;
        let chat = ChatService::new(repo.clone(), clock.clone(), chat_ids, limits);
        Ok(Self {
            chat: Arc::new(chat),
            repo: Arc::new(repo),
            store,
            storage,
            bus,
            clock,
            ids,
            entitlements,
            limits,
            size_policy: SizePolicy::default(),
            nonce_window_ms: 5 * 60 * 1000,
            upload_url_ttl: Duration::from_secs(15 * 60),
        })
    }

    pub fn now_ms(&self) -> i64 {
        self.clock.now_unix_ms()
    }

    pub fn next_id(&self) -> Snowflake {
        self.ids.generate()
    }

    pub fn next_attachment_id(&self) -> AttachmentId {
        AttachmentId::from(self.ids.generate())
    }

    pub async fn upload_limit(&self, user: UserId) -> u64 {
        self.entitlements.max_upload_bytes(user).await
    }

    /// Publish to every topic the event routes to. Delivery is best effort: the
    /// mutation already happened, so a bus failure is logged, not surfaced.
    pub async fn publish(&self, ev: DomainEvent) {
        for topic in ev.topics() {
            if let Err(e) = self.bus.publish(topic, ev.clone()).await {
                tracing::warn!(error = %e, "failed to publish domain event");
            }
        }
    }

    /// Resolve `channel` and the user's effective permissions in it.
    pub async fn access(&self, user: UserId, channel_id: ChannelId) -> MsgResult<Access> {
        let channel = self
            .repo
            .get_channel(channel_id)
            .await?
            .ok_or(ChatError::UnknownChannel)?;
        let Some(gid) = channel.guild_id else {
            if !self.repo.is_recipient(channel.id, user).await? {
                return Err(ChatError::MissingAccess.into());
            }
            return Ok(Access {
                channel,
                perms: dm_permissions(),
                roles: Vec::new(),
            });
        };
        let guild = self
            .repo
            .get_guild(gid)
            .await?
            .ok_or(ChatError::UnknownGuild)?;
        let member = self
            .repo
            .get_member(gid, user)
            .await?
            .ok_or(ChatError::MissingAccess)?;
        let roles = self.repo.list_roles(gid).await?;
        let overwrites = self.repo.get_overwrites(channel.id).await?;
        let perms = channel_permissions(&guild, &member, &roles, &overwrites, self.now_ms());
        Ok(Access {
            channel,
            perms,
            roles,
        })
    }

    /// A visible message of `channel`, or 404 10008.
    pub async fn visible_message(&self, channel: ChannelId, id: MessageId) -> MsgResult<Message> {
        self.repo
            .get_message(channel, id)
            .await?
            .filter(Message::is_visible)
            .ok_or_else(|| MsgError::from(ErrorCode::UnknownMessage))
    }

    pub async fn send(&self, a: UserId, c: ChannelId, r: SendMessage) -> ChatResult<Message> {
        self.chat.send(a, c, r).await
    }
    pub async fn edit(
        &self,
        u: UserId,
        c: ChannelId,
        m: MessageId,
        i: EditMessageInput,
    ) -> ChatResult<EditPlan> {
        self.chat.edit(u, c, m, i).await
    }
    pub async fn delete(&self, u: UserId, c: ChannelId, m: MessageId) -> ChatResult<DeletePlan> {
        self.chat.delete(u, c, m).await
    }
    pub async fn bulk_delete(
        &self,
        u: UserId,
        c: ChannelId,
        ids: &[MessageId],
    ) -> ChatResult<Vec<DeletePlan>> {
        self.chat.bulk(u, c, ids).await
    }
    pub async fn pin(&self, u: UserId, c: ChannelId, m: MessageId) -> ChatResult<()> {
        self.chat.pin(u, c, m).await
    }
    pub async fn unpin(&self, u: UserId, c: ChannelId, m: MessageId) -> ChatResult<()> {
        self.chat.unpin(u, c, m).await
    }
}
