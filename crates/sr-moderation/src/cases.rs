//! Cases (spec section 4): the unit staff work. Listing and reading follow the routing rules of D-209
//! ([`crate::rules::access`]); every change writes a case event, and a platform staff member's change also
//! writes the hash-chained access log in the same transaction (D-125).

use serde_json::{Value, json};
use sqlx::{PgConnection, Postgres, QueryBuilder, Row, postgres::PgRow};
use sr_admin::AccessKind;
use sr_core::PlatformPermissions as P;

use crate::error::{ModError, ModResult, conflict, invalid};
use crate::holds::{holds_of_case, release_system_holds_in, replace_system_hold_in};
use crate::model::{
    Access, AppealView, CaseDetail, CaseEvent, CaseEvidence, CaseFilter, CaseNote, CasePage,
    CaseReport, CaseSeverity, CaseState, CaseSummary, NewCase, ReportCategory, ReporterView,
    Routing, TargetKind, Who,
};
use crate::rules::{self, CaseHead};
use crate::service::{Moderation, log_staff, push_event, require_staff};

/// The columns of a case row (aliased `c`) that [`summary_from`] and [`head_from`] read.
const CASE_COLS: &str = "c.id, c.state, c.category, c.severity, c.routing, \
    (c.escalated_at IS NOT NULL) AS escalated, (c.escalated_by IS NOT NULL) AS escalated_by_guild, \
    c.summary, c.target_kind, c.target_id, c.guild_id, c.assignee_id, \
    c.subject_person_id, c.subject_account_id, c.linked_case_id, c.outcome, \
    (SELECT count(*) FROM moderation_reports r WHERE r.case_id = c.id) AS report_count, \
    (SELECT count(*) FROM moderation_holds h WHERE h.case_id = c.id AND h.released_at IS NULL) AS active_holds, \
    (extract(epoch FROM c.opened_at) * 1000)::bigint AS opened_ms, \
    (extract(epoch FROM c.updated_at) * 1000)::bigint AS updated_ms, \
    (extract(epoch FROM c.closed_at) * 1000)::bigint AS closed_ms";

fn summary_from(r: &PgRow) -> ModResult<CaseSummary> {
    Ok(CaseSummary {
        id: r.try_get("id")?,
        state: CaseState::parse(&r.try_get::<String, _>("state")?).unwrap_or(CaseState::Open),
        category: ReportCategory::parse(&r.try_get::<String, _>("category")?)
            .unwrap_or(ReportCategory::Other),
        severity: CaseSeverity::parse(&r.try_get::<String, _>("severity")?)
            .unwrap_or(CaseSeverity::Standard),
        routing: Routing::parse(&r.try_get::<String, _>("routing")?).unwrap_or(Routing::Platform),
        escalated: r.try_get("escalated")?,
        summary: r.try_get("summary")?,
        target_kind: TargetKind::parse(&r.try_get::<String, _>("target_kind")?)
            .unwrap_or(TargetKind::Other),
        target_id: r.try_get("target_id")?,
        guild_id: r.try_get("guild_id")?,
        assignee_id: r.try_get("assignee_id")?,
        report_count: r.try_get("report_count")?,
        active_holds: r.try_get("active_holds")?,
        opened_at_ms: r.try_get("opened_ms")?,
        updated_at_ms: r.try_get("updated_ms")?,
        closed_at_ms: r.try_get("closed_ms")?,
    })
}

fn head_of(s: &CaseSummary) -> CaseHead {
    CaseHead {
        routing: s.routing,
        guild_id: s.guild_id,
        target_kind: s.target_kind,
    }
}

/// A case as loaded for an action: the summary, the columns the detail adds, and what `who` may do.
struct Loaded {
    summary: CaseSummary,
    subject_person_id: Option<i64>,
    subject_account_id: Option<i64>,
    linked_case_id: Option<i64>,
    outcome: String,
    access: Access,
}

async fn load(
    conn: &mut PgConnection,
    who: &Who<'_>,
    id: i64,
    need: Access,
    lock: bool,
) -> ModResult<Loaded> {
    let sql = format!(
        "SELECT {CASE_COLS} FROM moderation_cases c WHERE c.id = $1{}",
        if lock { " FOR UPDATE OF c" } else { "" }
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(ModError::NotFound("case"))?;
    let summary = summary_from(&row)?;
    let access = rules::access(&head_of(&summary), who);
    if access == Access::None {
        // Not told apart from a case that does not exist.
        return Err(ModError::NotFound("case"));
    }
    if access < need {
        return Err(match (who, need) {
            (Who::Staff(_), Access::Manage) => ModError::MissingPermission("MANAGE_CASES"),
            _ => ModError::Forbidden,
        });
    }
    Ok(Loaded {
        subject_person_id: row.try_get("subject_person_id")?,
        subject_account_id: row.try_get("subject_account_id")?,
        linked_case_id: row.try_get("linked_case_id")?,
        outcome: row.try_get("outcome")?,
        summary,
        access,
    })
}

/// The move a state change asks for.
#[derive(Debug, Clone)]
pub struct Transition {
    pub to: CaseState,
    /// What was decided. Required when closing; kept as the case outcome.
    pub outcome: Option<String>,
    /// The line the reporters read when the case closes. Never names the other party.
    pub reporter_line: Option<String>,
}

/// The actions a server moderator or staff member can record on a case as having been taken elsewhere
/// (the guild's own moderation tools). Recording does not perform them.
pub const RECORDABLE_ACTIONS: &[&str] = &[
    "delete_message",
    "restore_message",
    "remove_attachment",
    "timeout",
    "kick",
    "guild_ban",
    "mark_nsfw",
    "other",
];

impl Moderation {
    // ---- reading --------------------------------------------------------------------------------------------

    /// The routing facts of a case, for choosing who is acting ([`rules::pick_who`]). Reveals nothing but
    /// where the case belongs; the real checks happen in each call.
    pub async fn case_head(&self, id: i64) -> ModResult<CaseHead> {
        let row = sqlx::query(
            "SELECT routing, guild_id, target_kind FROM moderation_cases WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(ModError::NotFound("case"))?;
        Ok(CaseHead {
            routing: Routing::parse(&row.try_get::<String, _>("routing")?)
                .unwrap_or(Routing::Platform),
            guild_id: row.try_get("guild_id")?,
            target_kind: TargetKind::parse(&row.try_get::<String, _>("target_kind")?)
                .unwrap_or(TargetKind::Other),
        })
    }

    /// The Person and account a case is about, for platform staff who act on them (a ban from the case).
    pub async fn case_subject(
        &self,
        who: &Who<'_>,
        id: i64,
    ) -> ModResult<(Option<i64>, Option<i64>, ReportCategory, CaseSeverity)> {
        let mut conn = self.pool.acquire().await?;
        let c = load(&mut conn, who, id, Access::Manage, false).await?;
        Ok((
            c.subject_person_id,
            c.subject_account_id,
            c.summary.category,
            c.summary.severity,
        ))
    }

    /// The queue (spec 9.1). Platform staff see what is routed to platform staff or reviewers; server moderators
    /// see their server's cases.
    pub async fn list_cases(&self, who: &Who<'_>, f: &CaseFilter) -> ModResult<CasePage> {
        let mut q: QueryBuilder<Postgres> =
            QueryBuilder::new(format!("SELECT {CASE_COLS} FROM moderation_cases c WHERE "));
        match who {
            Who::GuildMod { guild_id, .. } => {
                q.push("c.guild_id = ").push_bind(*guild_id);
                q.push(" AND (c.routing = 'guild' OR c.escalated_by IS NOT NULL)");
            }
            Who::Staff(c) => {
                if c.permissions.has(P::MANAGE_CASES) {
                    q.push("c.routing IN ('platform', 'reviewers')");
                } else {
                    let mut kinds: Vec<String> = Vec::new();
                    if c.permissions.has(P::DELETE_CONTENT) || c.permissions.has(P::REVIEW_READING)
                    {
                        kinds.push("library".to_owned());
                    }
                    if c.permissions.has(P::DELETE_CONTENT) || c.permissions.has(P::REVIEW_MEDIA) {
                        kinds.push("cinema".to_owned());
                    }
                    if kinds.is_empty() {
                        return Err(ModError::MissingPermission("MANAGE_CASES"));
                    }
                    q.push("c.routing = 'reviewers' AND c.target_kind = ANY(")
                        .push_bind(kinds)
                        .push(")");
                }
            }
        }
        if let Some(s) = f.state {
            q.push(" AND c.state = ").push_bind(s.as_str());
        }
        if f.open_only {
            q.push(" AND c.state IN ('open', 'triage', 'investigating')");
        }
        if let Some(c) = f.category {
            q.push(" AND c.category = ").push_bind(c.as_str());
        }
        if let Some(s) = f.severity {
            q.push(" AND c.severity = ").push_bind(s.as_str());
        }
        if let Some(a) = f.assignee {
            q.push(" AND c.assignee_id = ").push_bind(a);
        }
        if let (Some(g), Who::Staff(_)) = (f.guild_id, who) {
            q.push(" AND c.guild_id = ").push_bind(g);
        }
        if let Some(cur) = f.cursor {
            q.push(if f.oldest_first {
                " AND c.id > "
            } else {
                " AND c.id < "
            })
            .push_bind(cur);
        }
        q.push(if f.oldest_first {
            " ORDER BY c.id ASC LIMIT "
        } else {
            " ORDER BY c.id DESC LIMIT "
        });
        let limit = f.limit.clamp(1, 100);
        q.push_bind(limit + 1);
        let rows = q.build().fetch_all(&self.pool).await?;
        let mut items = rows
            .iter()
            .map(summary_from)
            .collect::<ModResult<Vec<_>>>()?;
        let next_cursor = if i64::try_from(items.len()).unwrap_or(0) > limit {
            items.truncate(usize::try_from(limit).unwrap_or(100));
            items.last().map(|c| c.id.to_string())
        } else {
            None
        };
        Ok(CasePage { items, next_cursor })
    }

    /// One case with its reports, evidence, notes, timeline, holds and appeal, as far as `who` may see it.
    ///
    /// Reading a reporter's name as platform staff and opening a DM snapshot are written to the access log
    /// before anything is returned (D-125, D-209, D-214).
    pub async fn get_case(&self, who: &Who<'_>, id: i64) -> ModResult<CaseDetail> {
        let limits = self.limits().await?;
        let mut tx = self.pool.begin().await?;
        let loaded = load(&mut tx, who, id, Access::Read, false).await?;

        let report_rows = sqlx::query(
            "SELECT r.id, r.category, r.note, r.state, r.moderators_may_see, r.reporter_id, \
                    a.username::text AS username, \
                    (extract(epoch FROM r.created_at) * 1000)::bigint AS created_ms \
             FROM moderation_reports r LEFT JOIN accounts a ON a.id = r.reporter_id \
             WHERE r.case_id = $1 ORDER BY r.id",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        let mut reports = Vec::with_capacity(report_rows.len());
        let mut reporter_shown = false;
        for r in &report_rows {
            let may: bool = r.try_get("moderators_may_see")?;
            let visible = rules::reporter_visible(who, may);
            reporter_shown |= visible && matches!(who, Who::Staff(_));
            reports.push(CaseReport {
                id: r.try_get("id")?,
                category: ReportCategory::parse(&r.try_get::<String, _>("category")?)
                    .unwrap_or(ReportCategory::Other),
                note: r.try_get("note")?,
                state: crate::model::ReportState::parse(&r.try_get::<String, _>("state")?)
                    .unwrap_or(crate::model::ReportState::Received),
                moderators_may_see_name: may,
                reporter: if visible {
                    Some(ReporterView {
                        account_id: r.try_get("reporter_id")?,
                        username: r
                            .try_get::<Option<String>, _>("username")?
                            .unwrap_or_default(),
                    })
                } else {
                    None
                },
                created_at_ms: r.try_get("created_ms")?,
            });
        }

        let evidence_rows = sqlx::query(
            "SELECT id, report_id, kind, private, payload, \
                    (extract(epoch FROM created_at) * 1000)::bigint AS created_ms \
             FROM moderation_case_evidence WHERE case_id = $1 ORDER BY id",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        let mut evidence = Vec::new();
        let mut private_opened = false;
        for r in &evidence_rows {
            let private: bool = r.try_get("private")?;
            if private && !matches!(who, Who::Staff(_)) {
                continue;
            }
            private_opened |= private;
            evidence.push(CaseEvidence {
                id: r.try_get("id")?,
                report_id: r.try_get("report_id")?,
                kind: r.try_get("kind")?,
                payload: r.try_get::<Value, _>("payload")?,
                created_at_ms: r.try_get("created_ms")?,
            });
        }

        // Write the reads down before the content leaves (D-125).
        if reporter_shown {
            log_staff(
                &mut tx,
                who,
                AccessKind::Read,
                "moderation.reporter.read",
                ("case", id),
                "",
                json!({ "reports": reports.len() }),
            )
            .await?;
        }
        if private_opened {
            log_staff(
                &mut tx,
                who,
                AccessKind::PrivateRead,
                "moderation.evidence.read",
                ("case", id),
                &format!("case {id}"),
                json!({ "snapshot": true }),
            )
            .await?;
        }

        let notes = sqlx::query(
            "SELECT id, author_id, body, (extract(epoch FROM created_at) * 1000)::bigint AS created_ms \
             FROM moderation_case_notes WHERE case_id = $1 ORDER BY id",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?
        .iter()
        .map(|r| {
            Ok(CaseNote {
                id: r.try_get("id")?,
                author_id: r.try_get("author_id")?,
                body: r.try_get("body")?,
                created_at_ms: r.try_get("created_ms")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;

        let events = sqlx::query(
            "SELECT id, actor_id, kind, detail, (extract(epoch FROM created_at) * 1000)::bigint AS created_ms \
             FROM moderation_case_events WHERE case_id = $1 ORDER BY id",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?
        .iter()
        .map(|r| {
            Ok(CaseEvent {
                id: r.try_get("id")?,
                actor_id: r.try_get("actor_id")?,
                kind: r.try_get("kind")?,
                detail: r.try_get("detail")?,
                created_at_ms: r.try_get("created_ms")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;

        let holds = holds_of_case(&mut tx, id, limits.hold_review_days).await?;
        let appeal = appeal_of_case(&mut tx, id).await?;
        tx.commit().await?;
        Ok(CaseDetail {
            case: loaded.summary,
            subject_person_id: loaded.subject_person_id,
            subject_account_id: loaded.subject_account_id,
            linked_case_id: loaded.linked_case_id,
            outcome: loaded.outcome,
            reports,
            evidence,
            notes,
            events,
            holds,
            appeal,
            access: loaded.access.as_str().to_owned(),
        })
    }

    // ---- opening ----------------------------------------------------------------------------------------------

    /// Open a case without a report (platform staff, `MANAGE_CASES`): about an account, a Person or anything
    /// that has an id. Takedowns and appeals open cases the same way.
    pub async fn open_case(&self, who: &Who<'_>, c: &NewCase) -> ModResult<CaseSummary> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let summary = c.summary.trim();
        if summary.chars().count() > 200 {
            return Err(invalid("summary must be 200 characters or fewer"));
        }
        let mut tx = self.pool.begin().await?;
        let (person, account) = match c.subject_account_id {
            Some(a) => {
                let p: Option<i64> =
                    sqlx::query_scalar("SELECT person_id FROM accounts WHERE id = $1")
                        .bind(a)
                        .fetch_optional(&mut *tx)
                        .await?;
                (Some(p.ok_or(ModError::NotFound("account"))?), Some(a))
            }
            None => (None, None),
        };
        let id = self.next_id();
        sqlx::query(
            "INSERT INTO moderation_cases (id, category, severity, routing, summary, target_kind, target_id, \
             subject_person_id, subject_account_id, opened_by_kind, opened_by_id) \
             VALUES ($1, $2, $3, 'platform', $4, $5, $6, $7, $8, 'staff', $9)",
        )
        .bind(id)
        .bind(c.category.as_str())
        .bind(c.category.default_severity().as_str())
        .bind(summary)
        .bind(c.target_kind.as_str())
        .bind(c.target_id)
        .bind(person)
        .bind(account)
        .bind(who.user_id())
        .execute(&mut *tx)
        .await?;
        push_event(
            &mut tx,
            self.next_id(),
            id,
            Some(who.user_id()),
            "case_opened",
            json!({ "by": "staff", "category": c.category.as_str() }),
        )
        .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.case.open",
            ("case", id),
            "",
            json!({ "category": c.category.as_str(), "target_kind": c.target_kind.as_str() }),
        )
        .await?;
        let s = load(&mut tx, who, id, Access::Read, false).await?.summary;
        tx.commit().await?;
        Ok(s)
    }

    // ---- working a case ---------------------------------------------------------------------------------------

    /// Move a case along its lifecycle (spec 4.2). Closing needs the outcome and closes the reports with the
    /// line the reporters read.
    pub async fn set_state(
        &self,
        who: &Who<'_>,
        id: i64,
        t: &Transition,
    ) -> ModResult<CaseSummary> {
        let outcome = match (t.to, t.outcome.as_deref()) {
            (CaseState::Closed, o) => Some(
                rules::reason(o)
                    .map_err(|_| invalid("closing a case needs the outcome in a few words"))?,
            ),
            (_, Some(o)) if !o.trim().is_empty() => Some(rules::reason(Some(o))?),
            _ => None,
        };
        let line = match t.reporter_line.as_deref().map(str::trim) {
            Some(l) if !l.is_empty() => {
                if l.chars().count() > 200 {
                    return Err(invalid(
                        "the line for the reporter must be 200 characters or fewer",
                    ));
                }
                l.to_owned()
            }
            _ => "The moderators reviewed your report.".to_owned(),
        };
        let limits = self.limits().await?;
        let mut tx = self.pool.begin().await?;
        let cur = load(&mut tx, who, id, Access::Work, true).await?;
        let from = cur.summary.state;
        if !rules::can_transition(from, t.to) {
            return Err(conflict(format!(
                "A case that is {} cannot move to {}",
                from.as_str(),
                t.to.as_str()
            )));
        }
        sqlx::query(
            "UPDATE moderation_cases SET state = $2, updated_at = now(), \
             first_action_at = COALESCE(first_action_at, CASE WHEN state = 'open' THEN now() END), \
             closed_at = CASE WHEN $2 = 'closed' THEN now() END, \
             outcome = COALESCE($3, outcome) WHERE id = $1",
        )
        .bind(id)
        .bind(t.to.as_str())
        .bind(outcome.as_deref())
        .execute(&mut *tx)
        .await?;
        match t.to {
            CaseState::Triage | CaseState::Investigating => {
                sqlx::query(
                    "UPDATE moderation_reports SET state = 'under_review' WHERE case_id = $1 AND state = 'received'",
                )
                .bind(id)
                .execute(&mut *tx)
                .await?;
            }
            CaseState::Closed => {
                sqlx::query(
                    "UPDATE moderation_reports SET state = 'closed', closed_at = now(), outcome_line = $2 \
                     WHERE case_id = $1 AND state <> 'closed'",
                )
                .bind(id)
                .bind(&line)
                .execute(&mut *tx)
                .await?;
            }
            _ => {}
        }
        // D-328: dismissal releases the report hold (the evidence freeze), staff holds stay; moving a dismissed
        // case back to work puts the report hold back.
        let mut released = 0;
        if t.to == CaseState::Dismissed {
            let why = match outcome.as_deref() {
                Some(o) => format!("case dismissed: {o}"),
                None => "case dismissed".to_owned(),
            };
            released =
                release_system_holds_in(&mut tx, &self.ids, id, Some(who.user_id()), &why, &limits)
                    .await?;
        } else if from == CaseState::Dismissed {
            replace_system_hold_in(&mut tx, &self.ids, id).await?;
        }
        let holds: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM moderation_holds WHERE case_id = $1 AND released_at IS NULL",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        push_event(
            &mut tx,
            self.next_id(),
            id,
            Some(who.user_id()),
            "state_changed",
            json!({ "from": from.as_str(), "to": t.to.as_str(), "outcome": outcome,
                    "holds_in_force": holds, "holds_released": released }),
        )
        .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.case.state",
            ("case", id),
            outcome.as_deref().unwrap_or(""),
            json!({ "from": from.as_str(), "to": t.to.as_str() }),
        )
        .await?;
        let s = load(&mut tx, who, id, Access::Read, false).await?.summary;
        tx.commit().await?;
        Ok(s)
    }

    /// Reopen a closed case inside `moderation.case_reopen_days`; later, a new case links back to the old one
    /// (spec 4.2). Returns the case to work on.
    pub async fn reopen_case(
        &self,
        who: &Who<'_>,
        id: i64,
        reason: Option<&str>,
    ) -> ModResult<CaseSummary> {
        let reason = rules::reason(reason)?;
        let limits = self.limits().await?;
        let mut tx = self.pool.begin().await?;
        let cur = load(&mut tx, who, id, Access::Work, true).await?;
        if cur.summary.state != CaseState::Closed {
            return Err(conflict("Only a closed case can be reopened"));
        }
        let within: bool = sqlx::query_scalar(
            "SELECT closed_at > now() - make_interval(days => $2) FROM moderation_cases WHERE id = $1",
        )
        .bind(id)
        .bind(i32::try_from(limits.case_reopen_days).unwrap_or(30))
        .fetch_one(&mut *tx)
        .await?;
        let result_id = if within {
            sqlx::query(
                "UPDATE moderation_cases SET state = 'investigating', closed_at = NULL, updated_at = now() WHERE id = $1",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
            replace_system_hold_in(&mut tx, &self.ids, id).await?;
            push_event(
                &mut tx,
                self.next_id(),
                id,
                Some(who.user_id()),
                "reopened",
                json!({ "reason": reason }),
            )
            .await?;
            id
        } else {
            let new_id = self.next_id();
            sqlx::query(
                "INSERT INTO moderation_cases (id, category, severity, routing, summary, target_kind, target_id, \
                 target_channel_id, guild_id, subject_person_id, subject_account_id, opened_by_kind, opened_by_id, \
                 linked_case_id, escalated_at, escalated_by) \
                 SELECT $2, category, severity, routing, summary, target_kind, target_id, target_channel_id, \
                        guild_id, subject_person_id, subject_account_id, 'staff', $3, id, escalated_at, escalated_by \
                 FROM moderation_cases WHERE id = $1",
            )
            .bind(id)
            .bind(new_id)
            .bind(who.user_id())
            .execute(&mut *tx)
            .await?;
            push_event(
                &mut tx,
                self.next_id(),
                new_id,
                Some(who.user_id()),
                "case_opened",
                json!({ "by": "staff", "linked_case_id": id.to_string(), "reason": reason }),
            )
            .await?;
            push_event(
                &mut tx,
                self.next_id(),
                id,
                Some(who.user_id()),
                "continued_in",
                json!({ "case_id": new_id.to_string() }),
            )
            .await?;
            new_id
        };
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.case.reopen",
            ("case", result_id),
            &reason,
            json!({ "from_case": id.to_string(), "continued": !within }),
        )
        .await?;
        let s = load(&mut tx, who, result_id, Access::Read, false)
            .await?
            .summary;
        tx.commit().await?;
        Ok(s)
    }

    /// Add an internal note (append-only).
    pub async fn add_note(&self, who: &Who<'_>, id: i64, body: &str) -> ModResult<CaseNote> {
        let body = rules::required_text("note", body, rules::MAX_CASE_NOTE_CHARS)?;
        let mut tx = self.pool.begin().await?;
        load(&mut tx, who, id, Access::Work, false).await?;
        let note_id = self.next_id();
        sqlx::query(
            "INSERT INTO moderation_case_notes (id, case_id, author_id, body) VALUES ($1, $2, $3, $4)",
        )
        .bind(note_id)
        .bind(id)
        .bind(who.user_id())
        .bind(&body)
        .execute(&mut *tx)
        .await?;
        push_event(
            &mut tx,
            self.next_id(),
            id,
            Some(who.user_id()),
            "note_added",
            json!({ "note_id": note_id.to_string() }),
        )
        .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.case.note",
            ("case", id),
            "",
            json!({ "note_id": note_id.to_string() }),
        )
        .await?;
        let row = sqlx::query(
            "SELECT id, author_id, body, (extract(epoch FROM created_at) * 1000)::bigint AS created_ms \
             FROM moderation_case_notes WHERE id = $1",
        )
        .bind(note_id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(CaseNote {
            id: row.try_get("id")?,
            author_id: row.try_get("author_id")?,
            body: row.try_get("body")?,
            created_at_ms: row.try_get("created_ms")?,
        })
    }

    /// Claim a case for the caller, or release it (the assignee or somebody with `MANAGE_CASES` may release).
    pub async fn assign(&self, who: &Who<'_>, id: i64, claim: bool) -> ModResult<CaseSummary> {
        let mut tx = self.pool.begin().await?;
        let cur = load(&mut tx, who, id, Access::Work, true).await?;
        let me = who.user_id();
        let to = if claim {
            if cur.summary.assignee_id.is_some_and(|a| a != me) && cur.access < Access::Manage {
                return Err(conflict("Somebody else has this case"));
            }
            Some(me)
        } else {
            if cur.summary.assignee_id.is_some_and(|a| a != me) && cur.access < Access::Manage {
                return Err(ModError::Forbidden);
            }
            None
        };
        sqlx::query(
            "UPDATE moderation_cases SET assignee_id = $2, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(to)
        .execute(&mut *tx)
        .await?;
        push_event(
            &mut tx,
            self.next_id(),
            id,
            Some(me),
            if claim { "claimed" } else { "released" },
            json!({ "previous": cur.summary.assignee_id.map(|a| a.to_string()) }),
        )
        .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            if claim {
                "moderation.case.claim"
            } else {
                "moderation.case.release"
            },
            ("case", id),
            "",
            json!({}),
        )
        .await?;
        let s = load(&mut tx, who, id, Access::Read, false).await?.summary;
        tx.commit().await?;
        Ok(s)
    }

    /// Hand a server's case to platform staff (spec 3.4). Server moderators only; afterwards they see a list row
    /// and no detail. The case, its reports and its holds stay as they are.
    pub async fn escalate(
        &self,
        who: &Who<'_>,
        id: i64,
        reason: Option<&str>,
    ) -> ModResult<CaseSummary> {
        let Who::GuildMod { user_id, .. } = who else {
            return Err(invalid(
                "Only a server's moderators escalate; platform staff already see it",
            ));
        };
        let reason = rules::reason(reason)?;
        let mut tx = self.pool.begin().await?;
        let cur = load(&mut tx, who, id, Access::Work, true).await?;
        if cur.summary.routing != Routing::Guild {
            return Err(conflict("This case is already with platform staff"));
        }
        sqlx::query(
            "UPDATE moderation_cases SET routing = 'platform', escalated_at = now(), escalated_by = $2, \
             assignee_id = NULL, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
        push_event(
            &mut tx,
            self.next_id(),
            id,
            Some(*user_id),
            "escalated",
            json!({ "by": "guild_moderator", "reason": reason }),
        )
        .await?;
        // The server's moderators lose access with the routing, so read the row as it is now.
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {CASE_COLS} FROM moderation_cases c WHERE c.id = $1"
        )))
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        let s = summary_from(&row)?;
        tx.commit().await?;
        Ok(s)
    }

    /// Warn the person a case is about (`MANAGE_CASES`): a case event and a system DM. The DM never names the
    /// reporter. `delivered` says whether the DM went out.
    pub async fn warn(&self, who: &Who<'_>, id: i64, text: &str) -> ModResult<bool> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let text = rules::required_text("warning", text, 1000)?;
        let mut tx = self.pool.begin().await?;
        let cur = load(&mut tx, who, id, Access::Manage, false).await?;
        let Some(account) = cur.subject_account_id else {
            return Err(conflict("This case has no account to warn"));
        };
        push_event(
            &mut tx,
            self.next_id(),
            id,
            Some(who.user_id()),
            "warned",
            json!({ "account_id": account.to_string(), "text": text }),
        )
        .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.case.warn",
            ("case", id),
            &text,
            json!({ "account_id": account.to_string() }),
        )
        .await?;
        tx.commit().await?;
        let dm = format!(
            "A moderator sent you a warning ({}):\n\n{}\n\nIf you think this is a mistake, reply to the staff or appeal from your account.",
            cur.summary.category.as_str().replace('_', " "),
            text
        );
        Ok(self.dm_subject(account, &dm).await)
    }

    /// Record that an action was taken elsewhere (the server's own tools: deleting the message, a timeout, a kick,
    /// a server ban) and note it on the timeline. `action` is one of [`RECORDABLE_ACTIONS`]. The case moves to
    /// `actioned` when it is still being worked.
    pub async fn record_action(
        &self,
        who: &Who<'_>,
        id: i64,
        action: &str,
        note: &str,
    ) -> ModResult<CaseSummary> {
        if !RECORDABLE_ACTIONS.contains(&action) {
            return Err(invalid("unknown action"));
        }
        let note = note.trim();
        if note.chars().count() > 500 {
            return Err(invalid("note must be 500 characters or fewer"));
        }
        let mut tx = self.pool.begin().await?;
        let cur = load(&mut tx, who, id, Access::Work, true).await?;
        push_event(
            &mut tx,
            self.next_id(),
            id,
            Some(who.user_id()),
            "action_recorded",
            json!({ "action": action, "note": note }),
        )
        .await?;
        if rules::can_transition(cur.summary.state, CaseState::Actioned) {
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
                json!({ "from": cur.summary.state.as_str(), "to": "actioned" }),
            )
            .await?;
        }
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.case.action",
            ("case", id),
            note,
            json!({ "action": action }),
        )
        .await?;
        let s = load(&mut tx, who, id, Access::Read, false).await?.summary;
        tx.commit().await?;
        Ok(s)
    }

    /// Note on the case that the Person was platform-banned from it. The route bans through the admin service
    /// first (`PLATFORM_BAN`, its own log row); this stamps `reason_category` on the open ban (spec 8), writes the
    /// timeline entry, moves the case to `actioned`, and tells the person when the ban is not serious.
    pub async fn record_platform_ban(
        &self,
        who: &Who<'_>,
        id: i64,
        reason_category: ReportCategory,
        note: &str,
    ) -> ModResult<i64> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let mut tx = self.pool.begin().await?;
        let cur = load(&mut tx, who, id, Access::Manage, true).await?;
        let Some(person) = cur.subject_person_id else {
            return Err(conflict("This case has no person to ban"));
        };
        let ban = sqlx::query(
            "UPDATE person_bans SET reason_category = $2::ban_reason_category \
             WHERE person_id = $1 AND lifted_at IS NULL RETURNING id, (category = 'serious') AS serious",
        )
        .bind(person)
        .bind(reason_category.as_str())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| conflict("The person is not banned"))?;
        let ban_id: i64 = ban.try_get("id")?;
        let serious: bool = ban.try_get("serious")?;
        push_event(
            &mut tx,
            self.next_id(),
            id,
            Some(who.user_id()),
            "platform_ban",
            json!({ "ban_id": ban_id.to_string(), "person_id": person.to_string(),
                    "reason_category": reason_category.as_str(), "serious": serious, "note": note }),
        )
        .await?;
        if rules::can_transition(cur.summary.state, CaseState::Actioned) {
            sqlx::query(
                "UPDATE moderation_cases SET state = 'actioned', updated_at = now(), \
                 first_action_at = COALESCE(first_action_at, now()) WHERE id = $1",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.case.ban",
            ("case", id),
            note,
            json!({ "ban_id": ban_id.to_string(), "reason_category": reason_category.as_str() }),
        )
        .await?;
        let account = cur.subject_account_id;
        tx.commit().await?;
        if !serious && let Some(a) = account {
            let _ = self
                .dm_subject(
                    a,
                    "Your account was banned for breaking the rules. You can appeal from the sign-in screen; a different staff member will read it.",
                )
                .await;
        }
        Ok(ban_id)
    }
}

/// The appeal of a case, if there is one.
pub(crate) async fn appeal_of_case(
    conn: &mut PgConnection,
    case_id: i64,
) -> ModResult<Option<AppealView>> {
    let row = sqlx::query(crate::appeals::APPEAL_SELECT_BY_CASE)
        .bind(case_id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(|r| crate::appeals::appeal_from(&r)).transpose()
}
