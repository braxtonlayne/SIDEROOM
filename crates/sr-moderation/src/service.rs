//! The handle every part of the moderation backend hangs off, and the helpers they share.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use sqlx::{PgConnection, PgPool, Row};
use sr_admin::{AccessKind, AuditEntry};

use crate::error::{ModError, ModResult};
use crate::model::{Limits, Who, keys};

/// Source of snowflake ids.
pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;

/// How the system user speaks to the person a case is about (warnings, notices of an action). Implemented by
/// the server over the message stack; absent in tests that do not care. It never names a reporter.
#[async_trait]
pub trait SubjectNotifier: Send + Sync {
    /// DM `to` from the system user. `true` when it was delivered.
    async fn system_dm(&self, to: i64, text: &str) -> bool;
}

/// The kinds of guild audit-log row a server moderator's case outcome writes (N-26, D-331).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuildAuditKind {
    /// `moderation_case_escalated`
    Escalated,
    /// `moderation_case_dismissed`
    Dismissed,
    /// `moderation_case_actioned`
    Actioned,
    /// `moderation_case_action_recorded`
    ActionRecorded,
}

/// What a guild audit row for a case may carry: the case, its category and the outcome line or the kind of
/// action recorded. Never the reporter and never note text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuildAuditRow {
    pub guild_id: i64,
    pub actor: i64,
    pub kind: GuildAuditKind,
    pub case_id: i64,
    pub category: String,
    pub detail: String,
}

/// Writes the guild audit-log row for a server moderator's case outcome. Implemented in `sr-server` over the
/// guild audit writer, like [`SubjectNotifier`]. A failure is logged by the implementation, never surfaced.
#[async_trait]
pub trait GuildAudit: Send + Sync {
    async fn record(&self, row: GuildAuditRow);
}

/// The moderation backend over one database. Cheap to clone.
#[derive(Clone)]
pub struct Moderation {
    pub(crate) pool: PgPool,
    pub(crate) ids: IdSource,
    pub(crate) notifier: Option<Arc<dyn SubjectNotifier>>,
    pub(crate) guild_audit: Option<Arc<dyn GuildAudit>>,
    pub(crate) case_actions: Option<Arc<dyn crate::actions::CaseActions>>,
}

impl Moderation {
    pub fn new(pool: PgPool, ids: IdSource) -> Self {
        Self {
            pool,
            ids,
            notifier: None,
            guild_audit: None,
            case_actions: None,
        }
    }

    /// The system user's voice for warnings and notices.
    pub fn with_notifier(mut self, n: Arc<dyn SubjectNotifier>) -> Self {
        self.notifier = Some(n);
        self
    }

    /// The guild audit log a server moderator's case outcomes are written to.
    pub fn with_guild_audit(mut self, a: Arc<dyn GuildAudit>) -> Self {
        self.guild_audit = Some(a);
        self
    }

    /// A server moderator's outcome on a case goes to their server's audit log; platform staff's go to the
    /// platform access log in the transaction. `claim` and notes write nothing.
    pub(crate) async fn audit_guild_hat(
        &self,
        who: &Who<'_>,
        kind: GuildAuditKind,
        case: &crate::model::CaseSummary,
        detail: &str,
    ) {
        let (Who::GuildMod { user_id, .. }, Some(a), Some(guild)) =
            (who, &self.guild_audit, case.guild_id)
        else {
            return;
        };
        a.record(GuildAuditRow {
            guild_id: guild,
            actor: *user_id,
            kind,
            case_id: case.id,
            category: case.category.as_str().to_owned(),
            detail: detail.to_owned(),
        })
        .await;
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub(crate) fn next_id(&self) -> i64 {
        (self.ids)()
    }

    /// The platform settings this backend reads, with their defaults for a setting nobody saved.
    pub async fn limits(&self) -> ModResult<Limits> {
        limits_in(&self.pool).await
    }

    pub(crate) async fn dm_subject(&self, to: i64, text: &str) -> bool {
        match &self.notifier {
            Some(n) => n.system_dm(to, text).await,
            None => false,
        }
    }
}

pub(crate) async fn limits_in<'e, E>(db: E) -> ModResult<Limits>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let rows = sqlx::query("SELECT key, value FROM platform_settings WHERE key = ANY($1)")
        .bind(vec![
            keys::REPORTS_PER_DAY.to_owned(),
            keys::HOLD_REVIEW_DAYS.to_owned(),
            keys::CASE_REOPEN_DAYS.to_owned(),
            keys::DELETED_MESSAGE_DAYS.to_owned(),
        ])
        .fetch_all(db)
        .await?;
    let mut l = Limits::default();
    for r in rows {
        let key: String = r.try_get("key")?;
        let v: Value = r.try_get("value")?;
        let Some(n) = v.as_i64() else { continue };
        match key.as_str() {
            keys::REPORTS_PER_DAY => l.reports_per_day = n,
            keys::HOLD_REVIEW_DAYS => l.hold_review_days = n,
            keys::CASE_REOPEN_DAYS => l.case_reopen_days = n,
            keys::DELETED_MESSAGE_DAYS => l.deleted_message_days = n,
            _ => {}
        }
    }
    Ok(l)
}

/// One row for the staff access log (D-125). Case staff actions are `admin_action` rows, reads of a reporter's
/// name are `read` rows and a DM snapshot is a `private_read` row with the case as the reason (D-214).
pub(crate) fn audit_entry(
    who: &Who<'_>,
    kind: AccessKind,
    action: &str,
    target: (&str, i64),
    reason: &str,
    detail: Value,
) -> Option<AuditEntry> {
    let Who::Staff(c) = who else { return None };
    Some(AuditEntry {
        actor_id: c.user_id,
        kind,
        action: action.to_owned(),
        target_kind: Some(target.0.to_owned()),
        target_id: Some(target.1),
        reason: reason.to_owned(),
        ip: c.ip.clone(),
        detail: detail.to_string(),
    })
}

/// Append to the hash-chained access log on this transaction. A no-op for server moderators, whose actions
/// are case events (the guild audit log has no case actions yet).
pub(crate) async fn log_staff(
    conn: &mut PgConnection,
    who: &Who<'_>,
    kind: AccessKind,
    action: &str,
    target: (&str, i64),
    reason: &str,
    detail: Value,
) -> ModResult<()> {
    if let Some(e) = audit_entry(who, kind, action, target, reason, detail) {
        sr_db::admin_repo::append_in(conn, &e).await?;
    }
    Ok(())
}

/// Append a case event (the timeline) on this transaction.
pub(crate) async fn push_event(
    conn: &mut PgConnection,
    id: i64,
    case_id: i64,
    actor: Option<i64>,
    kind: &str,
    detail: Value,
) -> ModResult<()> {
    sqlx::query(
        "INSERT INTO moderation_case_events (id, case_id, actor_id, kind, detail) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(case_id)
    .bind(actor)
    .bind(kind)
    .bind(detail)
    .execute(&mut *conn)
    .await?;
    sqlx::query("UPDATE moderation_cases SET updated_at = now() WHERE id = $1")
        .bind(case_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The platform-staff requirement of an action (D-213): the bit, or a `MissingPermission`.
pub(crate) fn require_staff(
    who: &Who<'_>,
    bit: sr_core::PlatformPermissions,
    name: &'static str,
) -> ModResult<()> {
    match who {
        Who::Staff(c) if c.permissions.has(bit) => Ok(()),
        Who::Staff(_) => Err(ModError::MissingPermission(name)),
        Who::GuildMod { .. } => Err(ModError::Forbidden),
    }
}
