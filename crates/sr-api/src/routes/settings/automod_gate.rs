//! The seam between message create and AutoMod (LNG-SSET-059/060/062).
//!
//! `routes/messages` is not edited by this module. When it is ready to enforce
//! AutoMod it takes an [`AutoModGateHandle`] from the state (`Ext`) and calls
//! [`AutoModGate::evaluate`] before storing the message:
//!
//! ```ignore
//! let verdict = gate.0.evaluate(&MessageCheck { guild, channel, author, content, .. }).await?;
//! if let Some(block) = &verdict.block {
//!     // reject with `ErrorCode::AutoModBlocked`-style 400, show block.custom_message
//! }
//! // after storing: post `verdict.alerts`. `verdict.timeout_secs` is already applied
//! // by `evaluate` through the wired `MemberTimeouter`, blocked or not.
//! ```
//!
//! [`SettingsDeps`] implements the trait: it resolves the author's roles and
//! bypass, runs the cached rules and the per-member spam window, writes the audit
//! entries for blocked messages and returns what the caller must execute.

use super::deps::{SettingsDeps, re, se};
use super::events::SettingsEvent;
use crate::HttpError;
use async_trait::async_trait;
use serde_json::json;
use sr_chat::settings::audit::{AuditAction, AuditEntry};
use sr_chat::settings::automod::{Action, MessageContext, RuleMatch};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, GuildId, UserId};
use std::sync::Arc;

/// One message about to be created.
#[derive(Debug, Clone, Default)]
pub struct MessageCheck {
    pub guild: GuildId,
    pub channel: ChannelId,
    pub author: UserId,
    pub content: String,
    pub mentioned_user_ids: Vec<UserId>,
    pub mentioned_role_ids: Vec<sr_core::RoleId>,
}

/// The block action to apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub rule_id: u64,
    pub rule_name: String,
    /// Shown to the author instead of the default text.
    pub custom_message: Option<String>,
}

/// An alert to post into `channel_id` (LNG-SSET-062).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub channel_id: ChannelId,
    pub rule_id: u64,
    pub rule_name: String,
    pub trigger: sr_chat::settings::automod::TriggerKind,
    pub matched: String,
    pub user_id: UserId,
    pub source_channel_id: ChannelId,
    pub content: String,
}

/// What the caller must do about a message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AutoModVerdict {
    pub block: Option<Block>,
    pub alerts: Vec<Alert>,
    /// Longest timeout any matching rule asks for.
    pub timeout_secs: Option<u32>,
    pub matches: Vec<RuleMatch>,
}

impl AutoModVerdict {
    pub fn is_clean(&self) -> bool {
        self.matches.is_empty()
    }
}

#[async_trait]
pub trait AutoModGate: Send + Sync {
    async fn evaluate(&self, msg: &MessageCheck) -> Result<AutoModVerdict, HttpError>;
}

/// Registered on the state so `routes/messages` can take it with `Ext` later.
#[derive(Clone)]
pub struct AutoModGateHandle(pub Arc<dyn AutoModGate>);

/// Fold rule matches into the actions to execute.
pub fn verdict_of(msg: &MessageCheck, matches: Vec<RuleMatch>) -> AutoModVerdict {
    let mut v = AutoModVerdict::default();
    for m in &matches {
        for a in &m.actions {
            match a {
                Action::Block { custom_message } => {
                    if v.block.is_none() {
                        v.block = Some(Block {
                            rule_id: m.rule_id,
                            rule_name: m.rule_name.clone(),
                            custom_message: custom_message.clone(),
                        });
                    }
                }
                Action::SendAlert { channel_id } => v.alerts.push(Alert {
                    channel_id: *channel_id,
                    rule_id: m.rule_id,
                    rule_name: m.rule_name.clone(),
                    trigger: m.trigger,
                    matched: m.matched.clone(),
                    user_id: msg.author,
                    source_channel_id: msg.channel,
                    content: msg.content.clone(),
                }),
                Action::Timeout { duration_secs } => {
                    v.timeout_secs = Some(
                        v.timeout_secs
                            .map_or(*duration_secs, |t| t.max(*duration_secs)),
                    );
                }
            }
        }
    }
    v.matches = matches;
    v
}

#[async_trait]
impl AutoModGate for SettingsDeps {
    async fn evaluate(&self, msg: &MessageCheck) -> Result<AutoModVerdict, HttpError> {
        // Most servers have no rule: then nothing can match, and the channel and member lookups below (four
        // queries on every message sent) are skipped (RDY-PERF-002).
        if !self.service.has_rules(msg.guild).await.map_err(se)? {
            return Ok(verdict_of(msg, Vec::new()));
        }
        let channel = self.chat.get_channel(msg.channel).await.map_err(re)?;
        let acc = self.guild_access(msg.author, msg.guild).await?;
        let ctx = MessageContext {
            author_id: msg.author,
            channel_id: msg.channel,
            parent_channel_id: channel.and_then(|c| c.parent_id),
            author_role_ids: acc.member.role_ids.clone(),
            // Administrators and server managers are never filtered (Discord).
            bypass: acc.perms.has(Permissions::ADMINISTRATOR)
                || acc.perms.has(Permissions::MANAGE_GUILD),
            content: msg.content.clone(),
            mentioned_user_ids: msg.mentioned_user_ids.clone(),
            mentioned_role_ids: msg.mentioned_role_ids.clone(),
        };
        let matches = self
            .service
            .check_message(msg.guild, &ctx)
            .await
            .map_err(se)?;
        let verdict = verdict_of(msg, matches);
        // The timeout applies whether or not the message is blocked (Discord).
        // Best effort like the audit entry: the verdict stands if it fails.
        if let Some(secs) = verdict.timeout_secs {
            let until = self.now_ms().saturating_add(i64::from(secs) * 1000);
            if let Err(e) = self
                .timeouter()
                .timeout_member(msg.guild, msg.author, until)
                .await
            {
                tracing::warn!(error = ?e, "could not apply an AutoMod timeout");
            }
        }
        for m in &verdict.matches {
            if !m.actions.iter().any(|a| matches!(a, Action::Block { .. })) {
                continue;
            }
            // Audit and gateway notification are best effort: the verdict stands.
            let entry = AuditEntry::new(
                self.service.new_id(),
                msg.guild,
                msg.author,
                AuditAction::AutoModerationBlockMessage,
                Some(m.rule_id),
                None,
                self.now_ms(),
            )
            .map(|e| {
                e.with_options(json!({
                    "auto_moderation_rule_name": m.rule_name,
                    "channel_id": msg.channel.to_string(),
                }))
            });
            if let Ok(entry) = entry
                && let Err(e) = self.service.store_audit(&entry).await
            {
                tracing::warn!(error = %e, "could not record an AutoMod block");
            }
            self.publish(SettingsEvent::AutoModActionExecuted {
                guild: msg.guild,
                payload: json!({
                    "guild_id": msg.guild.to_string(),
                    "rule_id": m.rule_id.to_string(),
                    "user_id": msg.author.to_string(),
                    "channel_id": msg.channel.to_string(),
                    "matched_keyword": m.matched,
                    "content": msg.content,
                    "action": { "type": 1 },
                }),
            })
            .await;
        }
        Ok(verdict)
    }
}

/// `AUTO_MODERATION_ACTION_EXECUTION` body for a `send_alert_message` action
/// (Discord action type 2). `message_id` is absent when the message was blocked.
pub fn alert_execution_payload(
    guild: GuildId,
    alert: &Alert,
    message_id: Option<sr_core::MessageId>,
) -> serde_json::Value {
    let mut v = json!({
        "guild_id": guild.to_string(),
        "action": { "type": 2, "metadata": { "channel_id": alert.channel_id.to_string() } },
        "rule_id": alert.rule_id.to_string(),
        "user_id": alert.user_id.to_string(),
        "channel_id": alert.source_channel_id.to_string(),
        "content": alert.content,
        "matched_keyword": alert.matched,
        "matched_content": alert.matched,
    });
    if let (Some(id), Some(o)) = (message_id, v.as_object_mut()) {
        o.insert("message_id".into(), json!(id.to_string()));
    }
    v
}

#[cfg(test)]
mod alert_payload_tests {
    use super::*;

    #[test]
    fn alert_payload_is_discord_shaped() {
        let alert = Alert {
            channel_id: ChannelId::new(9),
            rule_id: 7,
            rule_name: "r".into(),
            trigger: sr_chat::settings::automod::TriggerKind::Keyword,
            matched: "bad".into(),
            user_id: UserId::new(2),
            source_channel_id: ChannelId::new(3),
            content: "a bad word".into(),
        };
        let v = alert_execution_payload(GuildId::new(1), &alert, Some(sr_core::MessageId::new(5)));
        assert_eq!(v["action"]["type"], 2);
        assert_eq!(v["action"]["metadata"]["channel_id"], "9");
        assert_eq!(v["rule_id"], "7");
        assert_eq!(v["message_id"], "5");
        let blocked = alert_execution_payload(GuildId::new(1), &alert, None);
        assert!(blocked.get("message_id").is_none());
    }
}
