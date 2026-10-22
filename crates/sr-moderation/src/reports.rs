//! Reports from members (spec section 3; D-209, D-214).
//!
//! The client sends ids and a note. The server resolves the target, snapshots the evidence, routes the case,
//! attaches a duplicate to the open case on the same target, and freezes a reported message with a system hold.
//! Reporting never hides or removes anything.

use serde_json::{Value, json};
use sqlx::{PgConnection, Row};

use crate::error::{ModError, ModResult, conflict, invalid};
use crate::holds::{HoldRequest, HoldScope, place_hold_in};
use crate::model::{
    CaseSeverity, MyReport, NewReport, ReportCategory, ReportReceipt, ReportState, Routing,
    TargetKind,
};
use crate::rules;
use crate::service::{Moderation, push_event};
use crate::snapshot::message_snapshot;

/// The system user, who is never reportable.
const SYSTEM_USER_ID: i64 = 1;

/// What a report points at, as the database sees it.
struct Resolved {
    guild_id: Option<i64>,
    channel_id: Option<i64>,
    subject_account_id: Option<i64>,
    subject_person_id: Option<i64>,
    /// A DM or group DM: its evidence is readable by platform staff only (D-214).
    private: bool,
    /// What the case shows next to the target (a channel name, a username, a title).
    label: String,
}

async fn is_guild_member(conn: &mut PgConnection, guild: i64, user: i64) -> ModResult<bool> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM guild_members WHERE guild_id = $1 AND user_id = $2)",
    )
    .bind(guild)
    .bind(user)
    .fetch_one(&mut *conn)
    .await?)
}

async fn is_recipient(conn: &mut PgConnection, channel: i64, user: i64) -> ModResult<bool> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM channel_recipients WHERE channel_id = $1 AND user_id = $2)",
    )
    .bind(channel)
    .bind(user)
    .fetch_one(&mut *conn)
    .await?)
}

async fn account_of(conn: &mut PgConnection, id: i64) -> ModResult<Option<(i64, String)>> {
    let row = sqlx::query("SELECT person_id, username::text AS username FROM accounts WHERE id = $1 AND state <> 'deleted'")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(|r| Ok((r.try_get("person_id")?, r.try_get("username")?)))
        .transpose()
}

/// Resolve the target and check that the reporter can see it (the route has already applied the channel
/// permissions; this is the database-level backstop).
async fn resolve(conn: &mut PgConnection, reporter: i64, req: &NewReport) -> ModResult<Resolved> {
    match req.target_kind {
        TargetKind::Message => {
            let channel = req
                .channel_id
                .ok_or_else(|| invalid("channel_id is required to report a message"))?;
            let row = sqlx::query(
                "SELECT m.author_id, m.visibility::text AS visibility, c.guild_id, c.name \
                 FROM messages m JOIN channels c ON c.id = m.channel_id \
                 WHERE m.id = $1 AND m.channel_id = $2",
            )
            .bind(req.target_id)
            .bind(channel)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or(ModError::NotFound("message"))?;
            if row.try_get::<String, _>("visibility")? != "visible" {
                return Err(ModError::NotFound("message"));
            }
            let guild_id: Option<i64> = row.try_get("guild_id")?;
            let seen = match guild_id {
                Some(g) => is_guild_member(conn, g, reporter).await?,
                None => is_recipient(conn, channel, reporter).await?,
            };
            if !seen {
                return Err(ModError::NotFound("message"));
            }
            let author: i64 = row.try_get("author_id")?;
            if author == reporter {
                return Err(invalid("You cannot report your own message"));
            }
            let person = account_of(conn, author).await?.map(|(p, _)| p);
            Ok(Resolved {
                guild_id,
                channel_id: Some(channel),
                subject_account_id: Some(author),
                subject_person_id: person,
                private: guild_id.is_none(),
                label: row
                    .try_get::<Option<String>, _>("name")?
                    .unwrap_or_default(),
            })
        }
        TargetKind::User => {
            if req.target_id == reporter {
                return Err(invalid("You cannot report yourself"));
            }
            if req.target_id == SYSTEM_USER_ID {
                return Err(ModError::NotFound("user"));
            }
            let (person, username) = account_of(conn, req.target_id)
                .await?
                .ok_or(ModError::NotFound("user"))?;
            // A server context only counts when both people belong to that server.
            let guild_id = match req.guild_id {
                Some(g)
                    if is_guild_member(conn, g, reporter).await?
                        && is_guild_member(conn, g, req.target_id).await? =>
                {
                    Some(g)
                }
                _ => None,
            };
            Ok(Resolved {
                guild_id,
                channel_id: None,
                subject_account_id: Some(req.target_id),
                subject_person_id: Some(person),
                private: false,
                label: username,
            })
        }
        TargetKind::Channel => {
            let row = sqlx::query("SELECT guild_id, name FROM channels WHERE id = $1")
                .bind(req.target_id)
                .fetch_optional(&mut *conn)
                .await?
                .ok_or(ModError::NotFound("channel"))?;
            let guild_id: Option<i64> = row.try_get("guild_id")?;
            let seen = match guild_id {
                Some(g) => is_guild_member(conn, g, reporter).await?,
                None => is_recipient(conn, req.target_id, reporter).await?,
            };
            if !seen {
                return Err(ModError::NotFound("channel"));
            }
            Ok(Resolved {
                guild_id,
                channel_id: Some(req.target_id),
                subject_account_id: None,
                subject_person_id: None,
                private: guild_id.is_none(),
                label: row
                    .try_get::<Option<String>, _>("name")?
                    .unwrap_or_default(),
            })
        }
        TargetKind::Guild => {
            let name: Option<String> = sqlx::query_scalar("SELECT name FROM guilds WHERE id = $1")
                .bind(req.target_id)
                .fetch_optional(&mut *conn)
                .await?;
            let name = name.ok_or(ModError::NotFound("server"))?;
            if !is_guild_member(conn, req.target_id, reporter).await? {
                return Err(ModError::NotFound("server"));
            }
            Ok(Resolved {
                guild_id: Some(req.target_id),
                channel_id: None,
                subject_account_id: None,
                subject_person_id: None,
                private: false,
                label: name,
            })
        }
        TargetKind::Library => {
            let title: Option<String> =
                sqlx::query_scalar("SELECT title FROM reading_series WHERE id = $1")
                    .bind(req.target_id)
                    .fetch_optional(&mut *conn)
                    .await?;
            Ok(item(title.ok_or(ModError::NotFound("series"))?))
        }
        TargetKind::Cinema => {
            let title: Option<String> =
                sqlx::query_scalar("SELECT title FROM video_titles WHERE id = $1")
                    .bind(req.target_id)
                    .fetch_optional(&mut *conn)
                    .await?;
            Ok(item(title.ok_or(ModError::NotFound("title"))?))
        }
        TargetKind::Person | TargetKind::Other => Err(invalid("That cannot be reported")),
    }
}

fn item(label: String) -> Resolved {
    Resolved {
        guild_id: None,
        channel_id: None,
        subject_account_id: None,
        subject_person_id: None,
        private: false,
        label,
    }
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == "23505")
}

impl Moderation {
    /// File a report as `reporter`. See the module docs for what happens. The route checks the
    /// `moderation.reports` flag and the reporter's channel permissions first.
    pub async fn file_report(&self, reporter: i64, req: &NewReport) -> ModResult<ReportReceipt> {
        let note = rules::report_note(&req.note)?;
        let limits = self.limits().await?;
        let mut tx = self.pool.begin().await?;

        let today: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM moderation_reports WHERE reporter_id = $1 AND created_at > now() - interval '1 day'",
        )
        .bind(reporter)
        .fetch_one(&mut *tx)
        .await?;
        if today >= limits.reports_per_day {
            return Err(ModError::RateLimited);
        }

        let target = resolve(&mut tx, reporter, req).await?;
        let already: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM moderation_reports WHERE reporter_id = $1 AND target_kind = $2 \
             AND target_id = $3 AND state <> 'closed')",
        )
        .bind(reporter)
        .bind(req.target_kind.as_str())
        .bind(req.target_id)
        .fetch_one(&mut *tx)
        .await?;
        if already {
            return Err(conflict("You already have an open report about this"));
        }

        let routing = rules::route(req.target_kind, target.guild_id, req.category);
        let severity = req.category.default_severity();

        // A duplicate attaches to the open case on the same target (spec 4.2).
        let open_case = sqlx::query(
            "SELECT id, routing, severity, state FROM moderation_cases \
             WHERE target_kind = $1 AND target_id = $2 AND state IN ('open', 'triage', 'investigating') \
             ORDER BY id DESC LIMIT 1 FOR UPDATE",
        )
        .bind(req.target_kind.as_str())
        .bind(req.target_id)
        .fetch_optional(&mut *tx)
        .await?;

        let (case_id, report_state, fresh) = match open_case {
            Some(row) => {
                let id: i64 = row.try_get("id")?;
                let case_routing = Routing::parse(&row.try_get::<String, _>("routing")?)
                    .unwrap_or(Routing::Platform);
                let case_severity: String = row.try_get("severity")?;
                let state: String = row.try_get("state")?;
                if routing == Routing::Platform && case_routing == Routing::Guild {
                    // A child-safety or illegal-content report on a case the server is working: it is now
                    // platform business (D-209). The system escalates, so the server's moderators are not
                    // told who or why.
                    sqlx::query(
                        "UPDATE moderation_cases SET routing = 'platform', escalated_at = now() WHERE id = $1",
                    )
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                    push_event(
                        &mut tx,
                        self.next_id(),
                        id,
                        None,
                        "escalated",
                        json!({ "by": "system", "category": req.category.as_str() }),
                    )
                    .await?;
                }
                if severity == CaseSeverity::Serious && case_severity != "serious" {
                    sqlx::query("UPDATE moderation_cases SET severity = 'serious' WHERE id = $1")
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                }
                let rs = if state == "open" {
                    ReportState::Received
                } else {
                    ReportState::UnderReview
                };
                (id, rs, false)
            }
            None => {
                let id = self.next_id();
                let summary = format!(
                    "{} report: {}",
                    req.target_kind.as_str(),
                    req.category.as_str()
                );
                sqlx::query(
                    "INSERT INTO moderation_cases (id, category, severity, routing, summary, target_kind, \
                     target_id, target_channel_id, guild_id, subject_person_id, subject_account_id, \
                     opened_by_kind, opened_by_id) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'report', NULL)",
                )
                .bind(id)
                .bind(req.category.as_str())
                .bind(severity.as_str())
                .bind(routing.as_str())
                .bind(&summary)
                .bind(req.target_kind.as_str())
                .bind(req.target_id)
                .bind(target.channel_id)
                .bind(target.guild_id)
                .bind(target.subject_person_id)
                .bind(target.subject_account_id)
                .execute(&mut *tx)
                .await?;
                (id, ReportState::Received, true)
            }
        };

        let report_id = self.next_id();
        sqlx::query(
            "INSERT INTO moderation_reports (id, case_id, reporter_id, target_kind, target_id, target_channel_id, \
             guild_id, category, note, moderators_may_see, state) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(report_id)
        .bind(case_id)
        .bind(reporter)
        .bind(req.target_kind.as_str())
        .bind(req.target_id)
        .bind(target.channel_id)
        .bind(target.guild_id)
        .bind(req.category.as_str())
        .bind(&note)
        .bind(req.moderators_may_see_name)
        .bind(report_state.as_str())
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                conflict("You already have an open report about this")
            } else {
                e.into()
            }
        })?;

        // Evidence is taken at report time and never changes (D-212, D-214).
        let (kind, payload): (&str, Value) = if req.target_kind == TargetKind::Message {
            let channel = target.channel_id.unwrap_or_default();
            (
                "message_snapshot",
                message_snapshot(&mut tx, channel, req.target_id).await?,
            )
        } else {
            (
                "target_summary",
                json!({
                    "kind": req.target_kind.as_str(),
                    "id": req.target_id.to_string(),
                    "label": target.label,
                    "guild_id": target.guild_id.map(|g| g.to_string()),
                }),
            )
        };
        sqlx::query(
            "INSERT INTO moderation_case_evidence (id, case_id, report_id, kind, private, payload) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(self.next_id())
        .bind(case_id)
        .bind(report_id)
        .bind(kind)
        .bind(target.private)
        .bind(payload)
        .execute(&mut *tx)
        .await?;

        // The event never carries the reporter: moderators read the timeline.
        push_event(
            &mut tx,
            self.next_id(),
            case_id,
            None,
            if fresh {
                "case_opened"
            } else {
                "report_attached"
            },
            json!({ "report_id": report_id.to_string(), "category": req.category.as_str() }),
        )
        .await?;

        // A reported message and its attachments are frozen for the case (spec 3.2). The system places this
        // hold; a message another hold already freezes keeps that one.
        if req.target_kind == TargetKind::Message {
            place_hold_in(
                &mut tx,
                &self.ids,
                &HoldRequest {
                    case_id,
                    scope: HoldScope::Message(req.target_id),
                    reason: "Reported by a member".to_owned(),
                    actor: None,
                },
            )
            .await?;
        }
        tx.commit().await?;
        Ok(ReportReceipt {
            id: report_id,
            state: report_state,
        })
    }

    /// The reporter's own reports with their status and outcome line, newest first. Nothing about the case or
    /// the other party (spec 3.3).
    pub async fn my_reports(&self, reporter: i64, limit: i64) -> ModResult<Vec<MyReport>> {
        let rows = sqlx::query(
            "SELECT id, target_kind, target_id, category, state, outcome_line, \
                    (extract(epoch FROM created_at) * 1000)::bigint AS created_ms \
             FROM moderation_reports WHERE reporter_id = $1 ORDER BY id DESC LIMIT $2",
        )
        .bind(reporter)
        .bind(limit.clamp(1, 100))
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                Ok(MyReport {
                    id: r.try_get("id")?,
                    target_kind: TargetKind::parse(&r.try_get::<String, _>("target_kind")?)
                        .unwrap_or(TargetKind::Other),
                    target_id: r.try_get("target_id")?,
                    category: ReportCategory::parse(&r.try_get::<String, _>("category")?)
                        .unwrap_or(ReportCategory::Other),
                    state: ReportState::parse(&r.try_get::<String, _>("state")?)
                        .unwrap_or(ReportState::Received),
                    outcome_line: r.try_get("outcome_line")?,
                    created_at_ms: r.try_get("created_ms")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .map_err(Into::into)
    }
}
