//! The beta bug report store behind `POST /bug-reports` and the staff routes (SEC-AUD-166 and 167, wave 64, D-650).
//!
//! One row per report (migration 2790). The row belongs to the reporter's account and may hold a JPEG of their
//! screen, so it is personal data: the picture is deleted when staff close the report or after
//! [`SCREENSHOT_DAYS`] days, the row after [`REPORT_DAYS`] days ([`PgBugReports::purge`], run daily), and erasing the
//! account deletes its reports (`account_deletion`). The caller passes only what the allow-list rebuilt.

use crate::DbResult;
use serde_json::Value;
use sqlx::PgPool;

/// A picture is kept at most this long.
pub const SCREENSHOT_DAYS: i32 = 30;
/// A report is kept at most this long.
pub const REPORT_DAYS: i32 = 90;

/// A person as a report names them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    pub id: i64,
    pub username: String,
}

/// One stored report. `description` and `bundle` are filled by [`PgBugReports::get`] only; the picture bytes are
/// never part of a row ([`PgBugReports::screenshot`] reads them).
#[derive(Debug, Clone, PartialEq)]
pub struct BugRow {
    pub id: i64,
    pub created_ms: i64,
    pub status: String,
    pub route: String,
    pub commit_sha: String,
    pub reporter: Person,
    pub has_screenshot: bool,
    pub description: Option<String>,
    pub bundle: Option<Value>,
    pub closed_ms: Option<i64>,
    pub closed_by: Option<Person>,
}

/// What a report is created from.
#[derive(Debug, Clone)]
pub struct NewReport<'a> {
    pub reporter_id: i64,
    pub commit_sha: &'a str,
    pub route: &'a str,
    pub description: &'a str,
    pub bundle: &'a Value,
    pub screenshot: Option<&'a [u8]>,
}

/// What a [`PgBugReports::purge`] pass removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Purged {
    pub pictures: u64,
    pub reports: u64,
}

type Tuple = (
    i64,
    i64,
    String,
    String,
    String,
    i64,
    String,
    bool,
    Option<String>,
    Option<Value>,
    Option<i64>,
    Option<i64>,
    Option<String>,
);

const SELECT: &str = "SELECT b.id, (extract(epoch FROM b.created_at) * 1000)::bigint, b.status, b.route, b.commit_sha,
        b.reporter_id, COALESCE(r.username, ''), b.screenshot IS NOT NULL,
        CASE WHEN $1 THEN b.description END, CASE WHEN $1 THEN b.bundle END,
        (extract(epoch FROM b.closed_at) * 1000)::bigint, b.closed_by, c.username
   FROM bug_reports b
   LEFT JOIN accounts r ON r.id = b.reporter_id
   LEFT JOIN accounts c ON c.id = b.closed_by";

fn row(t: Tuple) -> BugRow {
    BugRow {
        id: t.0,
        created_ms: t.1,
        status: t.2,
        route: t.3,
        commit_sha: t.4,
        reporter: Person {
            id: t.5,
            username: t.6,
        },
        has_screenshot: t.7,
        description: t.8,
        bundle: t.9,
        closed_ms: t.10,
        closed_by: t.11.map(|id| Person {
            id,
            username: t.12.unwrap_or_default(),
        }),
    }
}

#[derive(Clone)]
pub struct PgBugReports {
    pool: PgPool,
}

impl PgBugReports {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Store one report; returns its id.
    pub async fn insert(&self, new: &NewReport<'_>) -> DbResult<i64> {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO bug_reports (reporter_id, commit_sha, route, description, bundle, screenshot)
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        )
        .bind(new.reporter_id)
        .bind(new.commit_sha)
        .bind(new.route)
        .bind(new.description)
        .bind(new.bundle)
        .bind(new.screenshot)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    /// Newest first, without description or bundle, and the number of open reports in all.
    pub async fn list(&self, status: Option<&str>, limit: i64) -> DbResult<(Vec<BugRow>, i64)> {
        let sql = format!(
            "{SELECT} WHERE ($2::text IS NULL OR b.status = $2) ORDER BY b.created_at DESC, b.id DESC LIMIT $3"
        );
        let rows: Vec<Tuple> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(false)
            .bind(status)
            .bind(limit.clamp(1, 200))
            .fetch_all(&self.pool)
            .await?;
        let open: i64 =
            sqlx::query_scalar("SELECT count(*) FROM bug_reports WHERE status = 'open'")
                .fetch_one(&self.pool)
                .await?;
        Ok((rows.into_iter().map(row).collect(), open))
    }

    /// One report with its description and bundle.
    pub async fn get(&self, id: i64) -> DbResult<Option<BugRow>> {
        let sql = format!("{SELECT} WHERE b.id = $2");
        let r: Option<Tuple> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(true)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(r.map(row))
    }

    /// The picture's bytes; `None` when there is none (never sent, closed, or past its 30 days).
    pub async fn screenshot(&self, id: i64) -> DbResult<Option<Vec<u8>>> {
        let r: Option<Option<Vec<u8>>> =
            sqlx::query_scalar("SELECT screenshot FROM bug_reports WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(r.flatten())
    }

    /// Close the report and delete its picture. Idempotent: a second close keeps the first closer and time.
    /// `false` when there is no such report.
    pub async fn close(&self, id: i64, by: i64) -> DbResult<bool> {
        let r = sqlx::query(
            "UPDATE bug_reports SET status = 'closed', screenshot = NULL,
                    closed_at = COALESCE(closed_at, now()), closed_by = COALESCE(closed_by, $2)
              WHERE id = $1",
        )
        .bind(id)
        .bind(by)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected() > 0)
    }

    /// The daily retention pass: pictures older than `screenshot_days` are deleted, reports older than `report_days`
    /// are deleted. Both measured from `created_at`.
    pub async fn purge(&self, screenshot_days: i32, report_days: i32) -> DbResult<Purged> {
        let pictures = sqlx::query(
            "UPDATE bug_reports SET screenshot = NULL
              WHERE screenshot IS NOT NULL AND created_at < now() - make_interval(days => $1)",
        )
        .bind(screenshot_days)
        .execute(&self.pool)
        .await?
        .rows_affected();
        let reports = sqlx::query(
            "DELETE FROM bug_reports WHERE created_at < now() - make_interval(days => $1)",
        )
        .bind(report_days)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(Purged { pictures, reports })
    }
}
