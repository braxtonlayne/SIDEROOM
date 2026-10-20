//! The guild audit-log rows of a server moderator's case outcomes (N-26, D-331): the real
//! [`sr_moderation::GuildAudit`] over the guild audit writer the server's own tools use.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use sr_api::routes::guilds::backend::AuditSink;
use sr_chat::settings::audit::AuditAction;
use sr_core::{GuildId, UserId};
use sr_moderation::{GuildAudit, GuildAuditKind, GuildAuditRow};

pub struct GuildAuditAdapter(pub Arc<dyn AuditSink>);

/// The Discord-style audit action each kind is written as.
pub fn action_of(kind: GuildAuditKind) -> AuditAction {
    match kind {
        GuildAuditKind::Escalated => AuditAction::ModerationCaseEscalated,
        GuildAuditKind::Dismissed => AuditAction::ModerationCaseDismissed,
        GuildAuditKind::Actioned => AuditAction::ModerationCaseActioned,
        GuildAuditKind::ActionRecorded => AuditAction::ModerationCaseActionRecorded,
    }
}

#[async_trait]
impl GuildAudit for GuildAuditAdapter {
    async fn record(&self, row: GuildAuditRow) {
        let (Ok(g), Ok(a), Ok(c)) = (
            u64::try_from(row.guild_id),
            u64::try_from(row.actor),
            u64::try_from(row.case_id),
        ) else {
            return;
        };
        // The case, its category and the outcome line or the kind of action: never the reporter, never a note.
        let mut options = json!({ "case_id": row.case_id.to_string(), "category": row.category });
        if !row.detail.is_empty() {
            options["outcome"] = json!(row.detail);
        }
        self.0
            .record_with_options(
                GuildId::new(g),
                UserId::new(a),
                action_of(row.kind),
                Some(c),
                options,
            )
            .await;
    }
}
