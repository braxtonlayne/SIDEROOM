//! Appeals (spec 4.6, D-215): minimal at launch. One appeal per decision, where a decision is a platform ban;
//! reviewed by a staff member other than the one who made the ban (the Owner when nobody else holds the bit);
//! no second appeal. A banned account has no session, so the route proves the account with its credentials.

use serde_json::json;
use sqlx::{PgConnection, Row, postgres::PgRow};
use sr_admin::AccessKind;
use sr_core::PlatformPermissions as P;

use crate::error::{ModError, ModResult, conflict};
use crate::model::{AppealDecision, AppealView, ReportCategory, Who};
use crate::rules;
use crate::service::{Moderation, log_staff, push_event, require_staff};

const APPEAL_COLS: &str = "a.id, a.ban_id, a.case_id, a.appellant_account_id, a.message, a.state, \
    b.banned_by, b.person_id, a.decided_by, a.decision_reason, \
    (extract(epoch FROM a.created_at) * 1000)::bigint AS created_ms, \
    (extract(epoch FROM a.decided_at) * 1000)::bigint AS decided_ms";

pub(crate) const APPEAL_SELECT_BY_CASE: &str = "SELECT a.id, a.ban_id, a.case_id, a.appellant_account_id, a.message, a.state, \
    b.banned_by, b.person_id, a.decided_by, a.decision_reason, \
    (extract(epoch FROM a.created_at) * 1000)::bigint AS created_ms, \
    (extract(epoch FROM a.decided_at) * 1000)::bigint AS decided_ms \
    FROM moderation_appeals a JOIN person_bans b ON b.id = a.ban_id WHERE a.case_id = $1";

pub(crate) fn appeal_from(r: &PgRow) -> ModResult<AppealView> {
    Ok(AppealView {
        id: r.try_get("id")?,
        ban_id: r.try_get("ban_id")?,
        case_id: r.try_get("case_id")?,
        appellant_account_id: r.try_get("appellant_account_id")?,
        message: r.try_get("message")?,
        state: r.try_get("state")?,
        banned_by: r.try_get("banned_by")?,
        person_id: r.try_get("person_id")?,
        decided_by: r.try_get("decided_by")?,
        decision_reason: r.try_get("decision_reason")?,
        created_at_ms: r.try_get("created_ms")?,
        decided_at_ms: r.try_get("decided_ms")?,
    })
}

async fn appeal_by_id(conn: &mut PgConnection, id: i64, lock: bool) -> ModResult<AppealView> {
    let sql = format!(
        "SELECT {APPEAL_COLS} FROM moderation_appeals a JOIN person_bans b ON b.id = a.ban_id WHERE a.id = $1{}",
        if lock { " FOR UPDATE OF a" } else { "" }
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(ModError::NotFound("appeal"))?;
    appeal_from(&row)
}

impl Moderation {
    /// File an appeal for the open platform ban of the account's Person. The route has verified the
    /// credentials. The appeal joins the case that made the ban, or opens one so it reaches the queue.
    /// `Conflict` when the ban was already appealed (there is no second appeal) or the account is not banned.
    pub async fn create_appeal(&self, account_id: i64, message: &str) -> ModResult<AppealView> {
        let message = rules::required_text("message", message, 2000)?;
        let mut tx = self.pool.begin().await?;
        let ban = sqlx::query(
            "SELECT b.id, b.person_id, b.category::text AS category, b.reason_category::text AS reason_category \
             FROM person_bans b JOIN accounts a ON a.person_id = b.person_id \
             WHERE a.id = $1 AND b.lifted_at IS NULL FOR UPDATE OF b",
        )
        .bind(account_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| conflict("This account is not banned"))?;
        let ban_id: i64 = ban.try_get("id")?;
        let person: i64 = ban.try_get("person_id")?;
        let serious = ban.try_get::<String, _>("category")? == "serious";
        let category = ban
            .try_get::<Option<String>, _>("reason_category")?
            .as_deref()
            .and_then(ReportCategory::parse)
            .unwrap_or(ReportCategory::Other);

        let appeal_id = self.next_id();
        let inserted = sqlx::query(
            "INSERT INTO moderation_appeals (id, ban_id, appellant_account_id, message) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (ban_id) DO NOTHING",
        )
        .bind(appeal_id)
        .bind(ban_id)
        .bind(account_id)
        .bind(&message)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if inserted == 0 {
            return Err(conflict("This decision was already appealed"));
        }

        // The case that made the ban, if the ban came from a case.
        let known: Option<i64> = sqlx::query_scalar(
            "SELECT case_id FROM moderation_case_events WHERE kind = 'platform_ban' AND detail ->> 'ban_id' = $1 \
             ORDER BY id DESC LIMIT 1",
        )
        .bind(ban_id.to_string())
        .fetch_optional(&mut *tx)
        .await?;
        let case_id = match known {
            Some(c) => {
                let closed: bool = sqlx::query_scalar(
                    "SELECT state = 'closed' FROM moderation_cases WHERE id = $1",
                )
                .bind(c)
                .fetch_one(&mut *tx)
                .await?;
                if closed {
                    sqlx::query(
                        "UPDATE moderation_cases SET state = 'open', closed_at = NULL, updated_at = now() WHERE id = $1",
                    )
                    .bind(c)
                    .execute(&mut *tx)
                    .await?;
                }
                c
            }
            None => {
                let c = self.next_id();
                let sev = if serious { "serious" } else { "standard" };
                sqlx::query(
                    "INSERT INTO moderation_cases (id, category, severity, routing, summary, target_kind, target_id, \
                     subject_person_id, subject_account_id, opened_by_kind, opened_by_id) \
                     VALUES ($1, $2, $3, 'platform', 'Appeal of a platform ban', 'person', $4, $4, $5, 'appeal', $5)",
                )
                .bind(c)
                .bind(category.as_str())
                .bind(sev)
                .bind(person)
                .bind(account_id)
                .execute(&mut *tx)
                .await?;
                c
            }
        };
        sqlx::query("UPDATE moderation_appeals SET case_id = $2 WHERE id = $1")
            .bind(appeal_id)
            .bind(case_id)
            .execute(&mut *tx)
            .await?;
        push_event(
            &mut tx,
            self.next_id(),
            case_id,
            None,
            "appeal_filed",
            json!({ "appeal_id": appeal_id.to_string(), "ban_id": ban_id.to_string() }),
        )
        .await?;
        let view = appeal_by_id(&mut tx, appeal_id, false).await?;
        tx.commit().await?;
        Ok(view)
    }

    /// Appeals, open first (`MANAGE_CASES`).
    pub async fn list_appeals(
        &self,
        who: &Who<'_>,
        open_only: bool,
        limit: i64,
    ) -> ModResult<Vec<AppealView>> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let sql = format!(
            "SELECT {APPEAL_COLS} FROM moderation_appeals a JOIN person_bans b ON b.id = a.ban_id \
             WHERE ($1 = FALSE OR a.state = 'open') ORDER BY a.id DESC LIMIT $2"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(open_only)
            .bind(limit.clamp(1, 200))
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(appeal_from).collect()
    }

    /// Check that `who` may decide this appeal (`MANAGE_CASES`, open, and not the staff member who made the ban
    /// unless they are the Owner and nobody else can review) and return it. The route calls this before any
    /// side effect such as lifting the ban. `another_staff_can_review`: some other staff member holds
    /// `MANAGE_CASES`.
    pub async fn check_appeal_review(
        &self,
        who: &Who<'_>,
        appeal_id: i64,
        another_staff_can_review: bool,
    ) -> ModResult<AppealView> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let Who::Staff(caller) = who else {
            return Err(ModError::Forbidden);
        };
        let mut conn = self.pool.acquire().await?;
        let appeal = appeal_by_id(&mut conn, appeal_id, false).await?;
        if appeal.state != "open" {
            return Err(conflict("This appeal is already decided"));
        }
        if !rules::appeal_reviewer_ok(
            caller.user_id,
            appeal.banned_by,
            caller.is_owner,
            another_staff_can_review,
        ) {
            return Err(ModError::Forbidden);
        }
        Ok(appeal)
    }

    /// Record the decision. The route has lifted the ban for an overturned appeal. Closes the case with the
    /// decision as its outcome, and closes the appellant's way back in: there is no second appeal.
    pub async fn decide_appeal(
        &self,
        who: &Who<'_>,
        appeal_id: i64,
        decision: AppealDecision,
        reason: Option<&str>,
        another_staff_can_review: bool,
    ) -> ModResult<AppealView> {
        let reason = rules::reason(reason)?;
        self.check_appeal_review(who, appeal_id, another_staff_can_review)
            .await?;
        let mut tx = self.pool.begin().await?;
        let appeal = appeal_by_id(&mut tx, appeal_id, true).await?;
        if appeal.state != "open" {
            return Err(conflict("This appeal is already decided"));
        }
        sqlx::query(
            "UPDATE moderation_appeals SET state = $2, decided_by = $3, decision_reason = $4, decided_at = now() \
             WHERE id = $1",
        )
        .bind(appeal_id)
        .bind(decision.as_str())
        .bind(who.user_id())
        .bind(&reason)
        .execute(&mut *tx)
        .await?;
        if let Some(case_id) = appeal.case_id {
            push_event(
                &mut tx,
                self.next_id(),
                case_id,
                Some(who.user_id()),
                "appeal_decided",
                json!({ "appeal_id": appeal_id.to_string(), "decision": decision.as_str(), "reason": reason }),
            )
            .await?;
            let outcome = format!("Appeal {}", decision.as_str());
            sqlx::query(
                "UPDATE moderation_cases SET state = 'closed', closed_at = COALESCE(closed_at, now()), \
                 outcome = $2, updated_at = now() WHERE id = $1 AND state <> 'closed'",
            )
            .bind(case_id)
            .bind(&outcome)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE moderation_reports SET state = 'closed', closed_at = now(), \
                 outcome_line = 'The moderators reviewed this again.' WHERE case_id = $1 AND state <> 'closed'",
            )
            .bind(case_id)
            .execute(&mut *tx)
            .await?;
        }
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.appeal.decide",
            ("appeal", appeal_id),
            &reason,
            json!({ "decision": decision.as_str(), "ban_id": appeal.ban_id.to_string() }),
        )
        .await?;
        let view = appeal_by_id(&mut tx, appeal_id, false).await?;
        tx.commit().await?;
        Ok(view)
    }
}
