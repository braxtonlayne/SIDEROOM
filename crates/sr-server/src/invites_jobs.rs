//! The `invites.prune` job (OPS-INV-001): housekeeping for invite codes and the Admin > Invites view.
//!
//! One leased singleton (id 1890) that runs every six hours. A pass, in one transaction,
//!
//! 1. revokes every unused (`open`) invite code older than `invites.unused_code_days` (default 30). Operator
//!    emergency codes are outside the credit economy and are left alone. A revoked code returns nothing: credits
//!    are only spent when a code is redeemed;
//! 2. marks, in `invite_inactive_roots`, every account that redeemed an invite more than that many days ago and
//!    never activated (email never verified, never seen online), with the size of the subtree below it. A mark is
//!    lifted when its root activates. The mark is display data for Admin > Invites;
//! 3. writes what it did to `invite_prune_runs`.
//!
//! It NEVER deletes an `invite_graph` row. `invite_graph` is a view over redeemed `platform_invites`, and a pass
//! only ever touches `open` ones; the pass counts the view before and after and rolls back if the numbers differ.
//! docs/09 keeps the graph forever for the suspicion review and the Person model, so "pruning" means codes and
//! display, never edges.

use std::time::Duration;

use serde_json::json;
use sqlx::PgPool;
use sr_db::DbResult;
use sr_db::jobs::{self, Job};
use sr_db::runner::{HandlerError, JobRunner};

/// Kind of the prune job.
pub const PRUNE_KIND: &str = "invites.prune";
/// Reserved singleton id (below any Snowflake; unique among `*_JOB_ID`; 1890 is the first of the invites range).
pub const PRUNE_JOB_ID: i64 = 1890;
/// How often the job runs.
pub const INTERVAL: Duration = Duration::from_secs(6 * 3600);
/// `invites.unused_code_days` when staff set nothing.
pub const DEFAULT_UNUSED_CODE_DAYS: i64 = 30;
/// Longest chain of invites the subtree count follows (the graph is a forest; this only bounds a bad row).
const MAX_DEPTH: i32 = 64;
/// Passes older than this are dropped from `invite_prune_runs`.
const KEEP_RUNS_DAYS: i32 = 180;

/// What a pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub codes_revoked: i64,
    pub roots_marked: i64,
    pub roots_cleared: i64,
    /// Redeemed edges of `invite_graph`, before the pass and after it (always equal).
    pub graph_rows_before: i64,
    pub graph_rows_after: i64,
}

/// Why a pass failed.
#[derive(Debug)]
pub enum PruneError {
    Db(sqlx::Error),
    /// The graph changed during the pass; nothing was kept.
    GraphChanged { before: i64, after: i64 },
}

impl std::fmt::Display for PruneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(e) => write!(f, "{e}"),
            Self::GraphChanged { before, after } => {
                write!(f, "invite_graph went from {before} to {after} rows")
            }
        }
    }
}

impl From<sqlx::Error> for PruneError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e)
    }
}

async fn unused_code_days(conn: &mut sqlx::PgConnection) -> Result<i32, sqlx::Error> {
    let v: Option<i64> = sqlx::query_scalar(
        "SELECT floor((value #>> '{}')::numeric)::bigint FROM platform_settings \
         WHERE key = 'invites.unused_code_days' AND jsonb_typeof(value) = 'number'",
    )
    .fetch_optional(&mut *conn)
    .await?;
    let days = v.unwrap_or(DEFAULT_UNUSED_CODE_DAYS).clamp(1, 3_650);
    Ok(i32::try_from(days).unwrap_or(30))
}

async fn graph_rows(conn: &mut sqlx::PgConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT count(*) FROM invite_graph")
        .fetch_one(&mut *conn)
        .await
}

/// The roots that qualify now, with their subtree sizes. `$1` is the age in days, `$2` the depth bound.
const ROOTS_CTE: &str = "WITH RECURSIVE roots AS ( \
        SELECT a.id FROM accounts a \
        WHERE a.invited_by_invite_id IS NOT NULL AND a.state = 'active' AND NOT a.is_bot \
          AND a.email_verified_at IS NULL AND a.last_seen_at IS NULL \
          AND a.created_at < now() - make_interval(days => $1) \
     ), tree(root_id, account_id, depth) AS ( \
        SELECT id, id, 0 FROM roots \
        UNION ALL \
        SELECT t.root_id, pi.redeemed_by_account_id, t.depth + 1 \
        FROM tree t \
        JOIN platform_invites pi ON pi.issuer_account_id = t.account_id AND pi.state = 'redeemed' \
        WHERE t.depth < $2 \
     ), sizes AS ( \
        SELECT root_id, count(*)::int AS subtree_size FROM tree GROUP BY root_id \
     )";

/// One pass. Everything happens in one transaction; a failure keeps nothing.
pub async fn run_once(pool: &PgPool) -> Result<Outcome, PruneError> {
    let mut tx = pool.begin().await?;
    let days = unused_code_days(&mut tx).await?;
    let before = graph_rows(&mut tx).await?;

    // 1. Unused codes past their age. Only `open` rows are touched, so no redeemed edge can change.
    let revoked = sqlx::query(
        "UPDATE platform_invites SET state = 'revoked', revoked_at = now(), updated_at = now() \
         WHERE state = 'open' AND NOT emergency AND created_at < now() - make_interval(days => $1)",
    )
    .bind(days)
    .execute(&mut *tx)
    .await?
    .rows_affected();

    // 2. The display mark: lift the ones that no longer qualify, then add or refresh the ones that do.
    let cleared = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{ROOTS_CTE} DELETE FROM invite_inactive_roots m \
         WHERE NOT EXISTS (SELECT 1 FROM sizes s WHERE s.root_id = m.root_account_id)"
    )))
    .bind(days)
    .bind(MAX_DEPTH)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    let marked: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "{ROOTS_CTE}, up AS ( \
            INSERT INTO invite_inactive_roots (root_account_id, subtree_size) \
            SELECT root_id, subtree_size FROM sizes \
            ON CONFLICT (root_account_id) DO UPDATE SET subtree_size = EXCLUDED.subtree_size \
            RETURNING (xmax = 0) AS inserted \
         ) SELECT count(*) FILTER (WHERE inserted) FROM up"
    )))
    .bind(days)
    .bind(MAX_DEPTH)
    .fetch_one(&mut *tx)
    .await?;

    // 3. The graph must be exactly as it was.
    let after = graph_rows(&mut tx).await?;
    if after != before {
        tx.rollback().await?;
        return Err(PruneError::GraphChanged { before, after });
    }
    let out = Outcome {
        codes_revoked: i64::try_from(revoked).unwrap_or(i64::MAX),
        roots_marked: marked,
        roots_cleared: i64::try_from(cleared).unwrap_or(i64::MAX),
        graph_rows_before: before,
        graph_rows_after: after,
    };
    sqlx::query(
        "INSERT INTO invite_prune_runs (codes_revoked, roots_marked, roots_cleared, graph_rows) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(i32::try_from(out.codes_revoked).unwrap_or(i32::MAX))
    .bind(i32::try_from(out.roots_marked).unwrap_or(i32::MAX))
    .bind(i32::try_from(out.roots_cleared).unwrap_or(i32::MAX))
    .bind(after)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM invite_prune_runs WHERE ran_at < now() - make_interval(days => $1)")
        .bind(KEEP_RUNS_DAYS)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(out)
}

/// Register the handler on the runner.
pub fn register(runner: JobRunner, pool: &PgPool) -> JobRunner {
    let pool = pool.clone();
    runner.register(PRUNE_KIND, 1, move |job: Job| {
        let pool = pool.clone();
        Box::pin(async move {
            match run_once(&pool).await {
                Ok(o) => tracing::info!(
                    codes_revoked = o.codes_revoked,
                    roots_marked = o.roots_marked,
                    roots_cleared = o.roots_cleared,
                    graph_rows = o.graph_rows_after,
                    "invites.prune"
                ),
                // The next pass retries; a failure must not stop the singleton.
                Err(e) => tracing::warn!(error = %e, "invites.prune failed"),
            }
            jobs::reschedule_running(&pool, job.id, INTERVAL)
                .await
                .map_err(HandlerError::from)?;
            Ok(())
        })
    })
}

/// Create or re-arm the singleton. Call only when [`register`] is on the runner.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(
        pool,
        PRUNE_JOB_ID,
        PRUNE_KIND,
        &json!({}),
        Duration::from_secs(60),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_job_id_is_in_the_invites_migration_range() {
        assert_eq!(PRUNE_JOB_ID, 1890);
        assert_eq!(PRUNE_KIND, "invites.prune");
        assert_eq!(INTERVAL, Duration::from_secs(21_600));
    }
}
