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
use sr_chat::admin_rules::MemberPatch;
use sr_chat::settings::audit::AuditAction;
use sr_core::{ChannelId, GuildId, MessageId, UserId};
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
        _by: &Caller,
        _case: i64,
        _channel: i64,
        _message: i64,
    ) -> Result<String, ActionError> {
        // No service restores a deleted message today.
        Err(ActionError::Unsupported)
    }

    async fn remove_attachment(
        &self,
        _by: &Caller,
        _case: i64,
        _message: i64,
    ) -> Result<String, ActionError> {
        Err(ActionError::Unsupported)
    }

    async fn timeout_member(
        &self,
        by: &Caller,
        _case: i64,
        guild: i64,
        user: i64,
        until_ms: i64,
    ) -> Result<String, ActionError> {
        self.guilds
            .edit_member(
                uid(by.user_id)?,
                gid(guild)?,
                uid(user)?,
                MemberPatch {
                    communication_disabled_until_ms: Some(Some(until_ms)),
                    ..MemberPatch::default()
                },
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
