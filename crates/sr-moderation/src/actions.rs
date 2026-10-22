//! Case actions performed through the chat services (N-21, D-330, D-331; spec 4.4). A moderator or staff member does
//! the thing from the case page; the services behind [`CaseActions`] write the guild audit row (or the platform
//! audit row for a suspension) exactly as the server's own tools do, with `case:<id>` in the reason, and the case
//! gets a timeline entry. `record_action` stays for work done elsewhere.
//!
//! The target of an action is always the case's own: the reported message, or the account the case is about in the
//! case's server. A request cannot name anything else.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use sr_admin::{AccessKind, Caller};
use sr_core::PlatformPermissions as P;
use utoipa::ToSchema;

use crate::error::{ModError, ModResult, conflict, invalid};
use crate::model::{Access, CaseState, CaseSummary, TargetKind, Who};
use crate::rules;
use crate::service::{Moderation, log_staff, push_event};

/// Why a service refused or could not do an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionError {
    /// This installation cannot do it from a case (no service behind it).
    Unsupported,
    /// The service refused: the acting person lacks the server bit, the hierarchy forbids it, and so on.
    Refused(String),
    /// The target is not in a state the action applies to (a message that is not deleted, an attachment that is not
    /// on the message); the text is shown to the person.
    Conflict(String),
    /// Something broke.
    Failed(String),
}

/// What the case page can perform. Guild actions are done as the acting person, whose own server permissions the
/// service re-checks; `SuspendAccount` is a platform action (`MANAGE_ACCOUNTS` through the admin service).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PerformedAction {
    DeleteMessage,
    RestoreMessage,
    RemoveAttachment,
    Timeout,
    Kick,
    GuildBan,
    SuspendAccount,
}

impl PerformedAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DeleteMessage => "delete_message",
            Self::RestoreMessage => "restore_message",
            Self::RemoveAttachment => "remove_attachment",
            Self::Timeout => "timeout",
            Self::Kick => "kick",
            Self::GuildBan => "guild_ban",
            Self::SuspendAccount => "suspend_account",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "delete_message" => Self::DeleteMessage,
            "restore_message" => Self::RestoreMessage,
            "remove_attachment" => Self::RemoveAttachment,
            "timeout" => Self::Timeout,
            "kick" => Self::Kick,
            "guild_ban" => Self::GuildBan,
            "suspend_account" => Self::SuspendAccount,
            _ => return None,
        })
    }

    /// Done inside the case's server by its moderator; the rest are platform actions.
    pub const fn is_guild(self) -> bool {
        !matches!(self, Self::SuspendAccount)
    }
}

/// The services behind the actions, implemented in `sr-server` over the message, guild-member and account
/// services so this crate gains no dependency on them. Each method gets the acting [`Caller`] and the case id and
/// returns the audit reference it wrote (for the timeline).
#[async_trait]
pub trait CaseActions: Send + Sync {
    async fn delete_message(
        &self,
        by: &Caller,
        case: i64,
        channel: i64,
        message: i64,
    ) -> Result<String, ActionError>;
    async fn restore_message(
        &self,
        by: &Caller,
        case: i64,
        channel: i64,
        message: i64,
    ) -> Result<String, ActionError>;
    /// Take one attachment off the case's message (it must belong to it).
    async fn remove_attachment(
        &self,
        by: &Caller,
        case: i64,
        channel: i64,
        message: i64,
        attachment: i64,
    ) -> Result<String, ActionError>;
    /// `until_ms` is when the timeout ends.
    async fn timeout_member(
        &self,
        by: &Caller,
        case: i64,
        guild: i64,
        user: i64,
        until_ms: i64,
    ) -> Result<String, ActionError>;
    async fn kick_member(
        &self,
        by: &Caller,
        case: i64,
        guild: i64,
        user: i64,
    ) -> Result<String, ActionError>;
    async fn guild_ban(
        &self,
        by: &Caller,
        case: i64,
        guild: i64,
        user: i64,
        reason: &str,
    ) -> Result<String, ActionError>;
    async fn suspend_account(
        &self,
        by: &Caller,
        case: i64,
        account: i64,
        reason: &str,
    ) -> Result<String, ActionError>;
}

/// A request to perform an action on a case.
#[derive(Debug, Clone)]
pub struct PerformRequest {
    pub action: PerformedAction,
    /// Why; goes on the audit row as `case:<id> <note>`.
    pub note: String,
    /// `Timeout` only: how long, in minutes (1 to 40320, four weeks).
    pub timeout_minutes: Option<i64>,
    /// `RemoveAttachment` only: which attachment of the case's message.
    pub attachment_id: Option<i64>,
}

impl Moderation {
    /// The services performing actions.
    pub fn with_case_actions(mut self, a: std::sync::Arc<dyn CaseActions>) -> Self {
        self.case_actions = Some(a);
        self
    }

    /// Whether this installation can perform actions from a case.
    pub fn can_perform(&self) -> bool {
        self.case_actions.is_some()
    }

    /// Perform `req` on the case's own target through [`CaseActions`] and record it on the timeline. Guild actions
    /// need `work` on a case routed to the server (the moderator's own server bits are re-checked by the service);
    /// a suspension needs `manage` (platform staff). A refusal leaves the case untouched.
    pub async fn perform_action(
        &self,
        who: &Who<'_>,
        id: i64,
        req: &PerformRequest,
    ) -> ModResult<CaseSummary> {
        let Some(actions) = self.case_actions.clone() else {
            return Err(conflict("Actions from a case are not available here"));
        };
        let note = req.note.trim();
        if note.chars().count() > 500 {
            return Err(invalid("note must be 500 characters or fewer"));
        }
        let case = self.get_case_for_action(who, id).await?;
        let need = if req.action.is_guild() {
            Access::Work
        } else {
            Access::Manage
        };
        if case.access < need {
            return Err(match who {
                Who::Staff(_) if need == Access::Manage => {
                    ModError::MissingPermission("MANAGE_CASES")
                }
                _ => ModError::Forbidden,
            });
        }
        let by = match who {
            Who::Staff(c) => (*c).clone(),
            Who::GuildMod { user_id, .. } => Caller {
                user_id: *user_id,
                permissions: P::empty(),
                is_owner: false,
                ip: None,
            },
        };
        let reason = if note.is_empty() {
            format!("case:{id}")
        } else {
            format!("case:{id} {note}")
        };
        let guild = case.guild_id;
        let account = case.subject_account;
        let need_guild = || guild.ok_or_else(|| conflict("This case is not about a server"));
        let need_account = || account.ok_or_else(|| conflict("This case has no account to act on"));
        let need_message = || match (case.target_kind, case.target_id, case.target_channel) {
            (TargetKind::Message, Some(m), Some(c)) => Ok((c, m)),
            _ => Err(conflict("This case is not about a message")),
        };
        let done = match req.action {
            PerformedAction::DeleteMessage => {
                let (c, m) = need_message()?;
                actions.delete_message(&by, id, c, m).await
            }
            PerformedAction::RestoreMessage => {
                let (c, m) = need_message()?;
                actions.restore_message(&by, id, c, m).await
            }
            PerformedAction::RemoveAttachment => {
                let (c, m) = need_message()?;
                let a = req
                    .attachment_id
                    .ok_or_else(|| invalid("attachment_id is required to remove an attachment"))?;
                actions.remove_attachment(&by, id, c, m, a).await
            }
            PerformedAction::Timeout => {
                let minutes = req
                    .timeout_minutes
                    .filter(|m| (1..=40_320).contains(m))
                    .ok_or_else(|| invalid("timeout_minutes must be between 1 and 40320"))?;
                let until = now_ms(&self.pool).await? + minutes * 60_000;
                actions
                    .timeout_member(&by, id, need_guild()?, need_account()?, until)
                    .await
            }
            PerformedAction::Kick => {
                actions
                    .kick_member(&by, id, need_guild()?, need_account()?)
                    .await
            }
            PerformedAction::GuildBan => {
                actions
                    .guild_ban(&by, id, need_guild()?, need_account()?, &reason)
                    .await
            }
            PerformedAction::SuspendAccount => {
                rules::reason(Some(note))?;
                actions
                    .suspend_account(&by, id, need_account()?, &reason)
                    .await
            }
        };
        let audit_ref = done.map_err(|e| match e {
            ActionError::Unsupported => conflict("That action is not available from a case here"),
            ActionError::Refused(m) => {
                let _ = m;
                ModError::Forbidden
            }
            ActionError::Conflict(m) => conflict(m),
            ActionError::Failed(m) => ModError::Storage(m),
        })?;

        let mut tx = self.pool.begin().await?;
        push_event(
            &mut tx,
            self.next_id(),
            id,
            Some(who.user_id()),
            "action_performed",
            json!({ "action": req.action.as_str(), "audit_ref": audit_ref, "note": note }),
        )
        .await?;
        if rules::can_transition(case.state, CaseState::Actioned) {
            sqlx::query(
                "UPDATE moderation_cases SET state = 'actioned', updated_at = now(), \
                 first_action_at = COALESCE(first_action_at, now()) WHERE id = $1",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
            push_event(
                &mut tx,
                self.next_id(),
                id,
                Some(who.user_id()),
                "state_changed",
                json!({ "from": case.state.as_str(), "to": "actioned" }),
            )
            .await?;
        }
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.case.perform",
            ("case", id),
            note,
            json!({ "action": req.action.as_str(), "audit_ref": audit_ref }),
        )
        .await?;
        tx.commit().await?;
        self.case_summary_for(who, id).await
    }
}

async fn now_ms(pool: &sqlx::PgPool) -> ModResult<i64> {
    Ok(
        sqlx::query_scalar("SELECT (extract(epoch FROM now()) * 1000)::bigint")
            .fetch_one(pool)
            .await?,
    )
}

/// What `perform_action` needs to know about a case.
pub(crate) struct ActionCase {
    pub access: Access,
    pub state: CaseState,
    pub guild_id: Option<i64>,
    pub target_kind: TargetKind,
    pub target_id: Option<i64>,
    pub target_channel: Option<i64>,
    pub subject_account: Option<i64>,
}

impl Moderation {
    async fn get_case_for_action(&self, who: &Who<'_>, id: i64) -> ModResult<ActionCase> {
        let row = sqlx::query(
            "SELECT state, routing, guild_id, target_kind, target_id, target_channel_id, subject_account_id \
             FROM moderation_cases WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(ModError::NotFound("case"))?;
        let target_kind = TargetKind::parse(&row.try_get::<String, _>("target_kind")?)
            .unwrap_or(TargetKind::Other);
        let head = rules::CaseHead {
            routing: crate::model::Routing::parse(&row.try_get::<String, _>("routing")?)
                .unwrap_or(crate::model::Routing::Platform),
            guild_id: row.try_get("guild_id")?,
            target_kind,
        };
        let access = rules::access(&head, who);
        if access == Access::None {
            return Err(ModError::NotFound("case"));
        }
        Ok(ActionCase {
            access,
            state: CaseState::parse(&row.try_get::<String, _>("state")?).unwrap_or(CaseState::Open),
            guild_id: head.guild_id,
            target_kind,
            target_id: row.try_get("target_id")?,
            target_channel: row.try_get("target_channel_id")?,
            subject_account: row.try_get("subject_account_id")?,
        })
    }
}
