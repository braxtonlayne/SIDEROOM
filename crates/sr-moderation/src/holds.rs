//! Holds (spec 4.3; D-122, D-212, D-213). `moderation_holds` is the one hold implementation: a hold row names its
//! case and scope, and the rows it freezes carry its id in `hold_id` (messages, attachments, submissions).
//! The retention jobs never delete a row with a non-null `hold_id`.
//!
//! This is the API the takedown step (and any later step) calls:
//!
//! * [`Moderation::place_hold`] / [`Moderation::release_hold`] for platform staff (`MANAGE_CASES`), in their own
//!   transaction, with the access-log row.
//! * [`place_hold_in`] / [`release_hold_in`] for a caller that composes the hold into a transaction of its own
//!   (a takedown that also unpublishes an item). They do not check permissions: the caller already did, and says
//!   who acted (`None` = the system, as when a report freezes the reported message).

use serde_json::json;
use sqlx::{PgConnection, Row};
use sr_admin::AccessKind;
use sr_core::PlatformPermissions as P;

use crate::error::{ModError, ModResult, conflict};
use crate::model::{HoldView, Limits, Who};
use crate::rules;
use crate::service::{IdSource, Moderation, log_staff, push_event, require_staff};

/// What a hold freezes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldScope {
    /// A message, its attachments and its edit history.
    Message(i64),
    /// One attachment (the message around it is not frozen).
    Attachment(i64),
    /// A library or cinema submission.
    Submission(i64),
    /// A library series and all its chapters (a takedown; the rows are not deleted while held).
    LibrarySeries(i64),
    /// One library chapter and its pages.
    LibraryChapter(i64),
    /// A cinema title and all its episodes.
    CinemaTitle(i64),
    /// One cinema episode.
    CinemaEpisode(i64),
    /// Every message of an account inside a window (Unix ms, `since` inclusive, `until` exclusive), with their
    /// attachments.
    UserMessages {
        account_id: i64,
        since_ms: i64,
        until_ms: i64,
    },
}

impl HoldScope {
    fn parts(self) -> (&'static str, i64) {
        match self {
            HoldScope::Message(id) => ("message", id),
            HoldScope::Attachment(id) => ("attachment", id),
            HoldScope::Submission(id) => ("submission", id),
            HoldScope::LibrarySeries(id) => ("library_series", id),
            HoldScope::LibraryChapter(id) => ("library_chapter", id),
            HoldScope::CinemaTitle(id) => ("cinema_title", id),
            HoldScope::CinemaEpisode(id) => ("cinema_episode", id),
            HoldScope::UserMessages { account_id, .. } => ("user_messages", account_id),
        }
    }
}

/// A request to freeze something for a case.
#[derive(Debug, Clone)]
pub struct HoldRequest {
    pub case_id: i64,
    pub scope: HoldScope,
    pub reason: String,
    /// The staff member, or `None` for the system.
    pub actor: Option<i64>,
}

/// What a placed hold froze.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldOutcome {
    /// `None` when everything in scope was already under a hold, so no new hold was made.
    pub hold_id: Option<i64>,
    pub messages: i64,
    pub attachments: i64,
    pub submissions: i64,
    /// Library and cinema rows frozen (series, chapters, titles, episodes).
    pub catalog_rows: i64,
    /// Rows in scope that an earlier hold already freezes (they keep that hold).
    pub already_held: i64,
}

/// Freeze `req.scope` for `req.case_id` on this transaction. Does not check permissions and writes no access
/// log (see the module docs); it does write the case event.
pub async fn place_hold_in(
    conn: &mut PgConnection,
    ids: &IdSource,
    req: &HoldRequest,
) -> ModResult<HoldOutcome> {
    let reason = rules::required_text("reason", &req.reason, rules::MAX_REASON_CHARS)?;
    let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM moderation_cases WHERE id = $1")
        .bind(req.case_id)
        .fetch_optional(&mut *conn)
        .await?;
    if exists.is_none() {
        return Err(ModError::NotFound("case"));
    }
    let hold_id = ids();
    let (scope_kind, scope_id) = req.scope.parts();
    sqlx::query(
        "INSERT INTO moderation_holds (id, reason, created_by, case_id, case_ref, scope_kind, scope_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(hold_id)
    .bind(&reason)
    .bind(req.actor)
    .bind(req.case_id)
    .bind(req.case_id.to_string())
    .bind(scope_kind)
    .bind(scope_id)
    .execute(&mut *conn)
    .await?;

    let mut out = HoldOutcome {
        hold_id: Some(hold_id),
        messages: 0,
        attachments: 0,
        submissions: 0,
        catalog_rows: 0,
        already_held: 0,
    };
    match req.scope {
        HoldScope::Message(id) => {
            let n =
                sqlx::query("UPDATE messages SET hold_id = $1 WHERE id = $2 AND hold_id IS NULL")
                    .bind(hold_id)
                    .bind(id)
                    .execute(&mut *conn)
                    .await?
                    .rows_affected();
            out.messages = i64::try_from(n).unwrap_or(0);
            if out.messages == 0 {
                let there: Option<i64> =
                    sqlx::query_scalar("SELECT id FROM messages WHERE id = $1")
                        .bind(id)
                        .fetch_optional(&mut *conn)
                        .await?;
                if there.is_none() {
                    return Err(ModError::NotFound("message"));
                }
                out.already_held += 1;
            }
            let a = sqlx::query(
                "UPDATE attachments SET hold_id = $1 WHERE message_id = $2 AND hold_id IS NULL",
            )
            .bind(hold_id)
            .bind(id)
            .execute(&mut *conn)
            .await?
            .rows_affected();
            out.attachments = i64::try_from(a).unwrap_or(0);
        }
        HoldScope::Attachment(id) => {
            let n = sqlx::query(
                "UPDATE attachments SET hold_id = $1 WHERE id = $2 AND hold_id IS NULL",
            )
            .bind(hold_id)
            .bind(id)
            .execute(&mut *conn)
            .await?
            .rows_affected();
            out.attachments = i64::try_from(n).unwrap_or(0);
            if out.attachments == 0 {
                let there: Option<i64> =
                    sqlx::query_scalar("SELECT id FROM attachments WHERE id = $1")
                        .bind(id)
                        .fetch_optional(&mut *conn)
                        .await?;
                if there.is_none() {
                    return Err(ModError::NotFound("attachment"));
                }
                out.already_held += 1;
            }
        }
        HoldScope::Submission(id) => {
            let n = sqlx::query(
                "UPDATE submissions SET hold_id = $1 WHERE id = $2 AND hold_id IS NULL",
            )
            .bind(hold_id)
            .bind(id)
            .execute(&mut *conn)
            .await?
            .rows_affected();
            out.submissions = i64::try_from(n).unwrap_or(0);
            if out.submissions == 0 {
                let there: Option<i64> =
                    sqlx::query_scalar("SELECT id FROM submissions WHERE id = $1")
                        .bind(id)
                        .fetch_optional(&mut *conn)
                        .await?;
                if there.is_none() {
                    return Err(ModError::NotFound("submission"));
                }
                out.already_held += 1;
            }
        }
        HoldScope::LibrarySeries(id) => {
            let n = freeze(conn, "reading_series", "id", id, hold_id).await?;
            if n == 0 {
                catalog_missing_or_held(conn, "reading_series", id, "series", &mut out).await?;
            }
            out.catalog_rows += n;
            out.catalog_rows += freeze(conn, "reading_chapters", "series_id", id, hold_id).await?;
        }
        HoldScope::LibraryChapter(id) => {
            let n = freeze(conn, "reading_chapters", "id", id, hold_id).await?;
            if n == 0 {
                catalog_missing_or_held(conn, "reading_chapters", id, "chapter", &mut out).await?;
            }
            out.catalog_rows += n;
        }
        HoldScope::CinemaTitle(id) => {
            let n = freeze(conn, "video_titles", "id", id, hold_id).await?;
            if n == 0 {
                catalog_missing_or_held(conn, "video_titles", id, "title", &mut out).await?;
            }
            out.catalog_rows += n;
            out.catalog_rows += freeze(conn, "video_episodes", "title_id", id, hold_id).await?;
        }
        HoldScope::CinemaEpisode(id) => {
            let n = freeze(conn, "video_episodes", "id", id, hold_id).await?;
            if n == 0 {
                catalog_missing_or_held(conn, "video_episodes", id, "episode", &mut out).await?;
            }
            out.catalog_rows += n;
        }
        HoldScope::UserMessages {
            account_id,
            since_ms,
            until_ms,
        } => {
            if until_ms <= since_ms {
                return Err(ModError::Invalid(
                    "the window must end after it starts".into(),
                ));
            }
            // The id bound prunes message partitions; created_at keeps the window exact.
            let n = sqlx::query(
                "UPDATE messages SET hold_id = $1 WHERE author_id = $2 AND hold_id IS NULL \
                 AND id >= sr_snowflake_floor(to_timestamp($3::double precision / 1000.0)) \
                 AND created_at >= to_timestamp($3::double precision / 1000.0) \
                 AND created_at < to_timestamp($4::double precision / 1000.0)",
            )
            .bind(hold_id)
            .bind(account_id)
            .bind(since_ms)
            .bind(until_ms)
            .execute(&mut *conn)
            .await?
            .rows_affected();
            out.messages = i64::try_from(n).unwrap_or(0);
            let a = sqlx::query(
                "UPDATE attachments a SET hold_id = $1 FROM messages m \
                 WHERE a.message_id = m.id AND m.author_id = $2 AND a.hold_id IS NULL \
                 AND m.id >= sr_snowflake_floor(to_timestamp($3::double precision / 1000.0)) \
                 AND m.created_at >= to_timestamp($3::double precision / 1000.0) \
                 AND m.created_at < to_timestamp($4::double precision / 1000.0)",
            )
            .bind(hold_id)
            .bind(account_id)
            .bind(since_ms)
            .bind(until_ms)
            .execute(&mut *conn)
            .await?
            .rows_affected();
            out.attachments = i64::try_from(a).unwrap_or(0);
        }
    }

    if out.messages + out.attachments + out.submissions + out.catalog_rows == 0 {
        // Nothing new to freeze: do not leave an empty hold behind.
        sqlx::query("DELETE FROM moderation_holds WHERE id = $1")
            .bind(hold_id)
            .execute(&mut *conn)
            .await?;
        out.hold_id = None;
        return Ok(out);
    }
    push_event(
        conn,
        ids(),
        req.case_id,
        req.actor,
        "hold_placed",
        json!({
            "hold_id": hold_id.to_string(),
            "scope_kind": scope_kind,
            "scope_id": scope_id.to_string(),
            "reason": reason,
            "messages": out.messages,
            "attachments": out.attachments,
            "submissions": out.submissions,
            "catalog_rows": out.catalog_rows,
        }),
    )
    .await?;
    Ok(out)
}

/// The catalogue tables a hold can freeze (library and cinema rows).
const CATALOG_TABLES: [&str; 4] = [
    "reading_series",
    "reading_chapters",
    "video_titles",
    "video_episodes",
];

/// Set `hold_id` on the rows of `table` whose `col` is `value` and that no hold freezes yet. Both names are
/// literals of this module, never input.
async fn freeze(
    conn: &mut PgConnection,
    table: &'static str,
    col: &'static str,
    value: i64,
    hold_id: i64,
) -> ModResult<i64> {
    let sql = format!("UPDATE {table} SET hold_id = $1 WHERE {col} = $2 AND hold_id IS NULL");
    let n = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(hold_id)
        .bind(value)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    Ok(i64::try_from(n).unwrap_or(0))
}

/// Nothing was frozen by id: either the row is not there (`NotFound`) or an earlier hold has it.
async fn catalog_missing_or_held(
    conn: &mut PgConnection,
    table: &'static str,
    id: i64,
    what: &'static str,
    out: &mut HoldOutcome,
) -> ModResult<()> {
    let sql = format!("SELECT id FROM {table} WHERE id = $1");
    let there: Option<i64> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    if there.is_none() {
        return Err(ModError::NotFound(what));
    }
    out.already_held += 1;
    Ok(())
}

/// What releasing a hold did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseOutcome {
    pub messages: i64,
    pub attachments: i64,
    pub submissions: i64,
    pub catalog_rows: i64,
    /// Released messages that were already deleted from user view: the next purge run takes them.
    pub purgeable_messages: i64,
}

/// Release a hold on this transaction: clears `hold_id` on every row it froze and stamps the hold. A deleted
/// message that never got a `purge_after` (a delete while held clears it, D-122) gets one now, from its
/// deletion time and the retention window, so it is purgeable at the next job run. No permission check, no
/// access log (see the module docs); it writes the case event.
pub async fn release_hold_in(
    conn: &mut PgConnection,
    ids: &IdSource,
    hold_id: i64,
    actor: Option<i64>,
    reason: &str,
    limits: &Limits,
) -> ModResult<ReleaseOutcome> {
    let reason = rules::required_text("reason", reason, rules::MAX_REASON_CHARS)?;
    let row = sqlx::query(
        "SELECT case_id, released_at IS NOT NULL AS released FROM moderation_holds WHERE id = $1 FOR UPDATE",
    )
    .bind(hold_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(ModError::NotFound("hold"))?;
    if row.try_get::<bool, _>("released")? {
        return Err(conflict("The hold is already released"));
    }
    let case_id: Option<i64> = row.try_get("case_id")?;

    let purgeable = sqlx::query(
        "UPDATE messages SET hold_id = NULL, \
         purge_after = CASE WHEN visibility <> 'visible' AND purge_after IS NULL \
             THEN deleted_at + make_interval(days => $2) ELSE purge_after END \
         WHERE hold_id = $1 RETURNING (visibility <> 'visible') AS deleted",
    )
    .bind(hold_id)
    .bind(i32::try_from(limits.deleted_message_days).unwrap_or(30))
    .fetch_all(&mut *conn)
    .await?;
    let messages = i64::try_from(purgeable.len()).unwrap_or(0);
    let purgeable_messages = i64::try_from(
        purgeable
            .iter()
            .filter(|r| r.try_get::<bool, _>("deleted").unwrap_or(false))
            .count(),
    )
    .unwrap_or(0);
    let attachments = sqlx::query("UPDATE attachments SET hold_id = NULL WHERE hold_id = $1")
        .bind(hold_id)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    let submissions = sqlx::query("UPDATE submissions SET hold_id = NULL WHERE hold_id = $1")
        .bind(hold_id)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    let mut catalog_rows = 0_i64;
    for table in CATALOG_TABLES {
        let sql = format!("UPDATE {table} SET hold_id = NULL WHERE hold_id = $1");
        let n = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(hold_id)
            .execute(&mut *conn)
            .await?
            .rows_affected();
        catalog_rows += i64::try_from(n).unwrap_or(0);
    }
    sqlx::query(
        "UPDATE moderation_holds SET released_at = now(), released_by = $2, release_reason = $3 WHERE id = $1",
    )
    .bind(hold_id)
    .bind(actor)
    .bind(&reason)
    .execute(&mut *conn)
    .await?;
    let out = ReleaseOutcome {
        messages,
        attachments: i64::try_from(attachments).unwrap_or(0),
        submissions: i64::try_from(submissions).unwrap_or(0),
        catalog_rows,
        purgeable_messages,
    };
    if let Some(case_id) = case_id {
        push_event(
            conn,
            ids(),
            case_id,
            actor,
            "hold_released",
            json!({
                "hold_id": hold_id.to_string(),
                "reason": reason,
                "messages": out.messages,
                "attachments": out.attachments,
                "submissions": out.submissions,
                "catalog_rows": out.catalog_rows,
                "purgeable_messages": out.purgeable_messages,
            }),
        )
        .await?;
    }
    if out.purgeable_messages > 0 {
        tracing::info!(
            hold = hold_id,
            purgeable = out.purgeable_messages,
            "hold released; deleted messages become purgeable at the next job run"
        );
    }
    Ok(out)
}

const HOLD_SELECT: &str = "SELECT h.id, h.case_id, h.scope_kind, h.scope_id, h.reason, h.created_by, \
    (extract(epoch FROM h.created_at) * 1000)::bigint AS created_ms, \
    (extract(epoch FROM h.released_at) * 1000)::bigint AS released_ms, h.release_reason, \
    (SELECT count(*) FROM messages m WHERE m.hold_id = h.id) AS messages, \
    (SELECT count(*) FROM attachments a WHERE a.hold_id = h.id) AS attachments, \
    (SELECT count(*) FROM submissions s WHERE s.hold_id = h.id) AS submissions, \
    ((SELECT count(*) FROM reading_series x WHERE x.hold_id = h.id) \
     + (SELECT count(*) FROM reading_chapters x WHERE x.hold_id = h.id) \
     + (SELECT count(*) FROM video_titles x WHERE x.hold_id = h.id) \
     + (SELECT count(*) FROM video_episodes x WHERE x.hold_id = h.id)) AS catalog_rows, \
    (h.released_at IS NULL AND coalesce(h.reviewed_at, h.created_at) < now() - make_interval(days => $1)) AS review_due \
    FROM moderation_holds h";

fn hold_from(r: &sqlx::postgres::PgRow) -> Result<HoldView, sqlx::Error> {
    Ok(HoldView {
        id: r.try_get("id")?,
        case_id: r.try_get("case_id")?,
        scope_kind: r.try_get("scope_kind")?,
        scope_id: r.try_get("scope_id")?,
        reason: r.try_get("reason")?,
        created_by: r.try_get("created_by")?,
        created_at_ms: r.try_get("created_ms")?,
        released_at_ms: r.try_get("released_ms")?,
        release_reason: r.try_get("release_reason")?,
        messages: r.try_get("messages")?,
        attachments: r.try_get("attachments")?,
        submissions: r.try_get("submissions")?,
        catalog_rows: r.try_get("catalog_rows")?,
        review_due: r.try_get("review_due")?,
    })
}

pub(crate) async fn holds_of_case(
    conn: &mut PgConnection,
    case_id: i64,
    review_days: i64,
) -> ModResult<Vec<HoldView>> {
    let sql = format!("{HOLD_SELECT} WHERE h.case_id = $2 ORDER BY h.id");
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(i32::try_from(review_days).unwrap_or(90))
        .bind(case_id)
        .fetch_all(&mut *conn)
        .await?;
    rows.iter()
        .map(hold_from)
        .collect::<Result<_, _>>()
        .map_err(Into::into)
}

impl Moderation {
    /// Place a hold as platform staff (D-213): `MANAGE_CASES`, a reason, a case. Writes the case event and the
    /// access-log row in one transaction. `Conflict` when everything in scope was already held.
    pub async fn place_hold(
        &self,
        who: &Who<'_>,
        case_id: i64,
        scope: HoldScope,
        reason: Option<&str>,
    ) -> ModResult<HoldOutcome> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let reason = rules::reason(reason)?;
        let mut tx = self.pool.begin().await?;
        let out = place_hold_in(
            &mut tx,
            &self.ids,
            &HoldRequest {
                case_id,
                scope,
                reason: reason.clone(),
                actor: Some(who.user_id()),
            },
        )
        .await?;
        let Some(hold_id) = out.hold_id else {
            return Err(conflict("That content is already under a hold"));
        };
        let (kind, sid) = scope.parts();
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.hold.place",
            ("case", case_id),
            &reason,
            json!({ "hold_id": hold_id.to_string(), "scope_kind": kind, "scope_id": sid.to_string(),
                    "messages": out.messages, "attachments": out.attachments, "submissions": out.submissions }),
        )
        .await?;
        tx.commit().await?;
        Ok(out)
    }

    /// Release a hold as platform staff (`MANAGE_CASES`, a reason). Both directions are logged.
    pub async fn release_hold(
        &self,
        who: &Who<'_>,
        hold_id: i64,
        reason: Option<&str>,
    ) -> ModResult<ReleaseOutcome> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let reason = rules::reason(reason)?;
        let limits = self.limits().await?;
        let mut tx = self.pool.begin().await?;
        let out = release_hold_in(
            &mut tx,
            &self.ids,
            hold_id,
            Some(who.user_id()),
            &reason,
            &limits,
        )
        .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.hold.release",
            ("hold", hold_id),
            &reason,
            json!({ "messages": out.messages, "attachments": out.attachments, "submissions": out.submissions,
                    "purgeable_messages": out.purgeable_messages }),
        )
        .await?;
        tx.commit().await?;
        Ok(out)
    }

    /// Every hold, newest first, with the size of what it freezes and the review flag (spec 9, item 4).
    /// Platform staff with `MANAGE_CASES` only.
    pub async fn list_holds(
        &self,
        who: &Who<'_>,
        active_only: bool,
        limit: i64,
    ) -> ModResult<Vec<HoldView>> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let limits = self.limits().await?;
        let sql = format!(
            "{HOLD_SELECT} WHERE ($2 = FALSE OR h.released_at IS NULL) ORDER BY h.id DESC LIMIT $3"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i32::try_from(limits.hold_review_days).unwrap_or(90))
            .bind(active_only)
            .bind(limit.clamp(1, 200))
            .fetch_all(&self.pool)
            .await?;
        rows.iter()
            .map(hold_from)
            .collect::<Result<_, _>>()
            .map_err(Into::into)
    }

    /// Mark a hold as looked at, so it leaves the review list until the next window (D-212). A decision to
    /// keep it, not a release.
    pub async fn mark_hold_reviewed(&self, who: &Who<'_>, hold_id: i64) -> ModResult<()> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let n = sqlx::query(
            "UPDATE moderation_holds SET reviewed_at = now() WHERE id = $1 AND released_at IS NULL",
        )
        .bind(hold_id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if n == 0 {
            return Err(ModError::NotFound("hold"));
        }
        Ok(())
    }
}
