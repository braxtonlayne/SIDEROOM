//! Case actions through the chat services (N-21, D-331): [`sr_moderation::CaseActions`] over the message
//! service, the guild-member service and the admin service. Each runs as the acting person, so their own server
//! permissions are re-checked by the service and the audit row is written exactly as the server's own tool
//! writes it (guild audit log for guild actions, the platform audit for a suspension), tagged with the case.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use sr_admin::{AdminService, Caller};
use sr_api::routes::guilds::GuildBackend;
use sr_api::routes::messages::MessagesDeps;
use sr_api::routes::messages::render::event_payload;
use sr_chat::ChatError;
use sr_chat::admin_rules::MemberPatch;
use sr_chat::settings::audit::{AuditAction, AuditChange};
use sr_core::{AttachmentId, ChannelId, GuildId, MessageId, UserId};
use sr_events::DomainEvent;
use sr_moderation::{ActionError, CaseActions};

pub struct ChatCaseActions {
    pub guilds: Arc<dyn GuildBackend>,
    pub messages: Arc<MessagesDeps>,
    pub admin: AdminService,
}

fn uid(v: i64) -> Result<UserId, ActionError> {
    u64::try_from(v)
        .map(UserId::new)
        .map_err(|_| ActionError::Failed("bad id".into()))
}

fn gid(v: i64) -> Result<GuildId, ActionError> {
    u64::try_from(v)
        .map(GuildId::new)
        .map_err(|_| ActionError::Failed("bad id".into()))
}

fn refused(e: impl std::fmt::Display) -> ActionError {
    ActionError::Refused(e.to_string())
}

#[async_trait]
impl CaseActions for ChatCaseActions {
    async fn delete_message(
        &self,
        by: &Caller,
        case: i64,
        channel: i64,
        message: i64,
    ) -> Result<String, ActionError> {
        let (actor, channel, message) = (
            uid(by.user_id)?,
            u64::try_from(channel)
                .map(ChannelId::new)
                .map_err(|_| ActionError::Failed("bad id".into()))?,
            u64::try_from(message)
                .map(MessageId::new)
                .map_err(|_| ActionError::Failed("bad id".into()))?,
        );
        let guild = self
            .messages
            .repo
            .get_channel(channel)
            .await
            .map_err(refused)?
            .and_then(|c| c.guild_id);
        let author = self
            .messages
            .repo
            .get_message(channel, message)
            .await
            .map_err(refused)?
            .map(|m| m.author_id);
        self.messages
            .delete(actor, channel, message)
            .await
            .map_err(refused)?;
        self.messages
            .publish(DomainEvent::MessageDeleted {
                guild,
                channel,
                message_id: message.get(),
            })
            .await;
        // The same row the delete route writes when somebody else removes a message.
        self.messages
            .audit(
                guild,
                actor,
                AuditAction::MessageDelete,
                author.map(|a| a.get()),
                json!({ "channel_id": channel.to_string(), "count": 1, "case_id": case.to_string() }),
            )
            .await;
        Ok(format!("message_delete:{}", message.get()))
    }

    async fn restore_message(
        &self,
        by: &Caller,
        case: i64,
        channel: i64,
        message: i64,
    ) -> Result<String, ActionError> {
        let (actor, channel_id, message_id) = (uid(by.user_id)?, cid(channel)?, mid(message)?);
        // A message a platform member removed comes back only through platform staff; a server moderator's
        // `Caller` carries no platform permission.
        let by_platform = !by.permissions.is_empty();
        let plan = self
            .messages
            .restore(actor, channel_id, message_id, by_platform)
            .await
            .map_err(restore_error)?;
        let guild = self
            .messages
            .repo
            .get_channel(plan.channel_id)
            .await
            .map_err(refused)?
            .and_then(|c| c.guild_id);
        // Clients dropped the message when it was deleted; they get the full object again (MESSAGE_CREATE), the
        // search index takes it back.
        let restored = self
            .messages
            .repo
            .get_message(plan.channel_id, plan.message_id)
            .await
            .map_err(refused)?
            .ok_or_else(|| ActionError::Failed("restored message vanished".into()))?;
        let data = event_payload(&self.messages, &restored)
            .await
            .map_err(|_| ActionError::Failed("could not build the message".into()))?;
        self.messages
            .publish(DomainEvent::MessageRestored {
                guild,
                channel: plan.channel_id,
                message_id: plan.message_id.get(),
                data,
            })
            .await;
        self.messages
            .audit_with_reason(
                guild,
                actor,
                AuditAction::ModerationMessageRestored,
                Some(restored.author_id.get()),
                vec![AuditChange::new(
                    "message_id",
                    None,
                    Some(json!(plan.message_id.to_string())),
                )],
                &format!("case:{case}"),
            )
            .await;
        Ok(format!("message_restore:{}", plan.message_id.get()))
    }

    async fn remove_attachment(
        &self,
        by: &Caller,
        case: i64,
        channel: i64,
        message: i64,
        attachment: i64,
    ) -> Result<String, ActionError> {
        let (actor, channel_id, message_id) = (uid(by.user_id)?, cid(channel)?, mid(message)?);
        let attachment_id = u64::try_from(attachment)
            .map(AttachmentId::new)
            .map_err(|_| ActionError::Failed("bad id".into()))?;
        let msg = self
            .messages
            .moderate(actor, channel_id, message_id)
            .await
            .map_err(refused)?;
        let now_ms = self.messages.now_ms();
        let detached = self
            .messages
            .store
            .detach_attachment(attachment_id, message_id, case, actor, now_ms)
            .await
            .map_err(|e| ActionError::Failed(e.to_string()))?
            .ok_or_else(|| ActionError::Conflict("That attachment is not on the message".into()))?;
        // Not held: the stored object goes now (a missing one counts as gone) and then the row. Held (D-122):
        // object and row stay for the evidence and the purge job takes them once the hold is released.
        if !detached.held {
            match self.messages.storage.delete(&detached.key).await {
                Ok(()) | Err(sr_storage::StorageError::NotFound(_)) => {
                    let _ = self
                        .messages
                        .store
                        .forget_removed_attachment(attachment_id)
                        .await;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "deleting a removed attachment's object failed; the purge job retries")
                }
            }
        }
        if msg.is_visible() {
            let data = event_payload(&self.messages, &msg)
                .await
                .map_err(|_| ActionError::Failed("could not build the message".into()))?;
            self.messages
                .publish(DomainEvent::MessageEdited {
                    guild: msg.guild_id,
                    channel: channel_id,
                    message_id: message_id.get(),
                    data,
                })
                .await;
        }
        self.messages
            .audit_with_reason(
                msg.guild_id,
                actor,
                AuditAction::ModerationAttachmentRemoved,
                Some(msg.author_id.get()),
                vec![
                    AuditChange::new("message_id", None, Some(json!(message_id.to_string()))),
                    AuditChange::new(
                        "attachment_id",
                        None,
                        Some(json!(attachment_id.to_string())),
                    ),
                    AuditChange::new("held", None, Some(json!(detached.held))),
                ],
                &format!("case:{case}"),
            )
            .await;
        Ok(format!("attachment_remove:{attachment}"))
    }

    async fn timeout_member(
        &self,
        by: &Caller,
        case: i64,
        guild: i64,
        user: i64,
        until_ms: i64,
    ) -> Result<String, ActionError> {
        self.guilds
            .edit_member_with_reason(
                uid(by.user_id)?,
                gid(guild)?,
                uid(user)?,
                MemberPatch {
                    communication_disabled_until_ms: Some(Some(until_ms)),
                    ..MemberPatch::default()
                },
                Some(format!("case:{case}")),
            )
            .await
            .map_err(refused)?;
        Ok(format!("member_update:{user}"))
    }

    async fn kick_member(
        &self,
        by: &Caller,
        case: i64,
        guild: i64,
        user: i64,
    ) -> Result<String, ActionError> {
        self.guilds
            .kick_member_with_reason(
                uid(by.user_id)?,
                gid(guild)?,
                uid(user)?,
                Some(format!("case:{case}")),
            )
            .await
            .map_err(refused)?;
        Ok(format!("member_kick:{user}"))
    }

    async fn guild_ban(
        &self,
        by: &Caller,
        _case: i64,
        guild: i64,
        user: i64,
        reason: &str,
    ) -> Result<String, ActionError> {
        self.guilds
            .ban_member(
                uid(by.user_id)?,
                gid(guild)?,
                uid(user)?,
                Some(reason.to_owned()),
                0,
            )
            .await
            .map_err(refused)?;
        Ok(format!("member_ban_add:{user}"))
    }

    async fn suspend_account(
        &self,
        by: &Caller,
        _case: i64,
        account: i64,
        reason: &str,
    ) -> Result<String, ActionError> {
        self.admin
            .set_suspended(by, account, true, Some(reason))
            .await
            .map_err(refused)?;
        Ok(format!("account.suspend:{account}"))
    }
}

fn cid(v: i64) -> Result<ChannelId, ActionError> {
    u64::try_from(v)
        .map(ChannelId::new)
        .map_err(|_| ActionError::Failed("bad id".into()))
}

fn mid(v: i64) -> Result<MessageId, ActionError> {
    u64::try_from(v)
        .map(MessageId::new)
        .map_err(|_| ActionError::Failed("bad id".into()))
}

/// What a refused restore means to the person on the case page.
fn restore_error(e: ChatError) -> ActionError {
    match e {
        ChatError::UnknownMessage => ActionError::Conflict(
            "The message is gone for good or about to be purged, so it cannot be restored".into(),
        ),
        ChatError::Invalid {
            code: "MESSAGE_NOT_DELETED",
            ..
        } => ActionError::Conflict("The message is not deleted".into()),
        other => refused(other),
    }
}
