//! The crash triage store behind `POST /crash-reports` and `GET /admin/crash-reports` (OPS-CRASH-001, wave 45-A).
//!
//! One row per distinct crash (see migration 2160): a repeat only raises the count and `last_seen`; the first card and
//! replay buffer are kept, because they are the reproduction an agent works from.

use crate::DbResult;
use serde_json::Value;
use sqlx::PgPool;

/// Most rows kept; the least recently seen beyond it are dropped when a new crash arrives.
pub const MAX_ROWS: i64 = 2000;

/// What [`PgCrashReports::record`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    pub id: i64,
    /// Reports with this fingerprint, this one included.
    pub count: i32,
    /// This fingerprint had not been seen before.
    pub is_new: bool,
    pub channel_id: Option<i64>,
    pub message_id: Option<i64>,
}

/// One stored crash. `card` and `replay` are only filled by [`PgCrashReports::get`].
#[derive(Debug, Clone, PartialEq)]
pub struct CrashRow {
    pub id: i64,
    pub fingerprint: String,
    pub commit_sha: String,
    pub kind: String,
    pub route: String,
    pub count: i32,
    pub first_seen_ms: i64,
    pub last_seen_ms: i64,
    pub card: Value,
    pub replay: Value,
}

type RowTuple = (
    i64,
    String,
    String,
    String,
    String,
    i32,
    i64,
    i64,
    Value,
    Value,
);

#[derive(Clone)]
pub struct PgCrashReports {
    pool: PgPool,
}

impl PgCrashReports {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Store one report: a new fingerprint inserts a row, a known one counts. The caller passes the card and replay it
    /// rebuilt from the allow-list, never the client's payload.
    pub async fn record(
        &self,
        fingerprint: &str,
        commit_sha: &str,
        kind: &str,
        route: &str,
        card: &Value,
        replay: &Value,
    ) -> DbResult<Recorded> {
        let (id, count, inserted, channel_id, message_id): (
            i64,
            i32,
            bool,
            Option<i64>,
            Option<i64>,
        ) = sqlx::query_as(
            "INSERT INTO crash_reports (fingerprint, commit_sha, kind, route, card, replay)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (fingerprint) DO UPDATE
                SET report_count = crash_reports.report_count + 1, last_seen = now()
             RETURNING id, report_count, (xmax = 0), channel_id, message_id",
        )
        .bind(fingerprint)
        .bind(commit_sha)
        .bind(kind)
        .bind(route)
        .bind(card)
        .bind(replay)
        .fetch_one(&self.pool)
        .await?;
        if inserted {
            sqlx::query(
                "DELETE FROM crash_reports WHERE id IN
                   (SELECT id FROM crash_reports ORDER BY last_seen DESC OFFSET $1)",
            )
            .bind(MAX_ROWS)
            .execute(&self.pool)
            .await?;
        }
        Ok(Recorded {
            id,
            count,
            is_new: inserted,
            channel_id,
            message_id,
        })
    }

    /// Remember where the card of crash `id` was posted.
    pub async fn set_message(&self, id: i64, channel: i64, message: i64) -> DbResult<()> {
        sqlx::query("UPDATE crash_reports SET channel_id = $2, message_id = $3 WHERE id = $1")
            .bind(id)
            .bind(channel)
            .bind(message)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Newest-seen crashes first, without the replay buffers' weight in mind (the card is small).
    pub async fn list(&self, limit: i64) -> DbResult<Vec<CrashRow>> {
        let rows: Vec<RowTuple> = sqlx::query_as(SELECT_ROWS_LIST)
            .bind(limit.clamp(1, 200))
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(row).collect())
    }

    pub async fn get(&self, id: i64) -> DbResult<Option<CrashRow>> {
        let r: Option<RowTuple> = sqlx::query_as(SELECT_ROW_ONE)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(r.map(row))
    }
}

const SELECT_ROWS_LIST: &str = "SELECT id, fingerprint, commit_sha, kind, route, report_count,
        (extract(epoch FROM first_seen) * 1000)::bigint, (extract(epoch FROM last_seen) * 1000)::bigint,
        card, '[]'::jsonb
   FROM crash_reports ORDER BY last_seen DESC, id DESC LIMIT $1";

const SELECT_ROW_ONE: &str = "SELECT id, fingerprint, commit_sha, kind, route, report_count,
        (extract(epoch FROM first_seen) * 1000)::bigint, (extract(epoch FROM last_seen) * 1000)::bigint,
        card, replay
   FROM crash_reports WHERE id = $1";

fn row(t: RowTuple) -> CrashRow {
    CrashRow {
        id: t.0,
        fingerprint: t.1,
        commit_sha: t.2,
        kind: t.3,
        route: t.4,
        count: t.5,
        first_seen_ms: t.6,
        last_seen_ms: t.7,
        card: t.8,
        replay: t.9,
    }
}
