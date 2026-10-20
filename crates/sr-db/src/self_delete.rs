//! A member's bulk self-delete of their own messages (N-05, D-324, doc 09 section 1).
//!
//! The rows move to `user_deleted` with the 30-day purge window of D-121, exactly as the moderator
//! bulk delete does (`hide_user_messages`): pins of the messages go first, attachments follow their
//! messages in the purge, and a row under a moderation hold keeps `purge_after` unset so it is never
//! purged while held (D-122). The run is a job (`user.messages_self_delete`); this module holds the
//! bookkeeping, the count shown before confirming, and the hiding itself. One run per account at a
//! time (unique index), one member run per 24 hours; a staff erasure run skips the 24-hour limit.

use serde_json::json;
use sqlx::{PgPool, Row};

use crate::{
    DbResult,
    jobs::{self, NewJob},
};

/// Job kind of the run.
pub const JOB_KIND: &str = "user.messages_self_delete";
/// Hours between two member runs.
pub const COOLDOWN_HOURS: i64 = 24;
/// Days a hidden message waits before it is purged (D-121).
pub const PURGE_DAYS: i64 = 30;
/// Channels handled per statement.
const CHANNEL_CHUNK: usize = 50;

/// Who asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    Member,
    Staff,
}

impl Via {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Member => "member",
            Self::Staff => "staff",
        }
    }
}

/// What a run covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scope {
    /// Messages in servers (including servers the member has left).
    pub servers: bool,
    /// Messages in direct messages and group DMs.
    pub dms: bool,
    /// Only messages older than this (Unix ms).
    pub before_ms: Option<i64>,
}

/// One run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub id: i64,
    pub account_id: i64,
    /// `queued`, `running`, `done` or `failed`.
    pub state: String,
    pub scope: Scope,
    pub via: String,
    pub requested_ms: i64,
    pub finished_ms: Option<i64>,
    pub hidden_count: Option<i64>,
}

/// Why a request was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// A run is queued or running.
    InProgress(Run),
    /// The last member run was less than [`COOLDOWN_HOURS`] ago.
    TooSoon { retry_after_secs: i64 },
}

/// What the hide did in one channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hidden {
    pub guild_id: Option<i64>,
    pub channel_id: i64,
    pub message_ids: Vec<i64>,
}

const COLS: &str = "id, account_id, state, servers, dms, via, \
    (EXTRACT(EPOCH FROM before_at) * 1000)::BIGINT AS before_ms, \
    (EXTRACT(EPOCH FROM requested_at) * 1000)::BIGINT AS requested_ms, \
    (EXTRACT(EPOCH FROM finished_at) * 1000)::BIGINT AS finished_ms, hidden_count";

fn query(sql: String) -> sqlx::query::Query<'static, sqlx::Postgres, sqlx::postgres::PgArguments> {
    sqlx::query(sqlx::AssertSqlSafe(sql))
}

fn run(r: &sqlx::postgres::PgRow) -> DbResult<Run> {
    Ok(Run {
        id: r.try_get("id")?,
        account_id: r.try_get("account_id")?,
        state: r.try_get("state")?,
        scope: Scope {
            servers: r.try_get("servers")?,
            dms: r.try_get("dms")?,
            before_ms: r.try_get("before_ms")?,
        },
        via: r.try_get("via")?,
        requested_ms: r.try_get("requested_ms")?,
        finished_ms: r.try_get("finished_ms")?,
        hidden_count: r.try_get("hidden_count")?,
    })
}

/// The `WHERE` fragment selecting the member's visible messages inside `scope`; binds `$1` (account),
/// `$2` (servers), `$3` (dms) and `$4` (cutoff ms, NULL for none).
const SCOPE_SQL: &str = "m.author_id = $1 AND m.visibility = 'visible' \
    AND ((c.guild_id IS NOT NULL AND $2) OR (c.guild_id IS NULL AND $3)) \
    AND ($4::bigint IS NULL OR m.id < sr_snowflake_floor(to_timestamp($4::double precision / 1000.0)))";

/// How many messages a run with this scope would hide now (the number the confirm dialog states).
pub async fn count_visible(pool: &PgPool, account: i64, scope: Scope) -> DbResult<i64> {
    Ok(query(format!(
        "SELECT count(*) FROM messages m JOIN channels c ON c.id = m.channel_id WHERE {SCOPE_SQL}"
    ))
    .bind(account)
    .bind(scope.servers)
    .bind(scope.dms)
    .bind(scope.before_ms)
    .fetch_one(pool)
    .await?
    .try_get(0)?)
}

/// The newest run of `account`, if any.
pub async fn latest(pool: &PgPool, account: i64) -> DbResult<Option<Run>> {
    let row = query(format!(
        "SELECT {COLS} FROM message_self_deletes WHERE account_id = $1 \
         ORDER BY requested_at DESC, id DESC LIMIT 1"
    ))
    .bind(account)
    .fetch_optional(pool)
    .await?;
    row.as_ref().map(run).transpose()
}

/// Record a run and queue its job in one transaction.
pub async fn request(
    pool: &PgPool,
    id: i64,
    job_id: i64,
    account: i64,
    scope: Scope,
    via: Via,
) -> DbResult<Result<Run, Refusal>> {
    let mut tx = pool.begin().await?;
    // Serialise one account's requests.
    sqlx::query("SELECT id FROM accounts WHERE id = $1 FOR UPDATE")
        .bind(account)
        .fetch_optional(&mut *tx)
        .await?;
    let open = query(format!(
        "SELECT {COLS} FROM message_self_deletes WHERE account_id = $1 AND state IN ('queued', 'running')"
    ))
    .bind(account)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(r) = open {
        return Ok(Err(Refusal::InProgress(run(&r)?)));
    }
    if via == Via::Member {
        let wait: Option<f64> = sqlx::query_scalar(
            "SELECT EXTRACT(EPOCH FROM (requested_at + make_interval(hours => $2) - now()))::float8 \
             FROM message_self_deletes WHERE account_id = $1 AND via = 'member' AND state <> 'failed' \
             ORDER BY requested_at DESC LIMIT 1",
        )
        .bind(account)
        .bind(i32::try_from(COOLDOWN_HOURS).unwrap_or(24))
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(w) = wait.filter(|w| *w > 0.0) {
            return Ok(Err(Refusal::TooSoon {
                retry_after_secs: w.ceil() as i64,
            }));
        }
    }
    sqlx::query(
        "INSERT INTO message_self_deletes (id, account_id, servers, dms, before_at, via) \
         VALUES ($1, $2, $3, $4, CASE WHEN $5::bigint IS NULL THEN NULL \
                                 ELSE to_timestamp($5::double precision / 1000.0) END, $6)",
    )
    .bind(id)
    .bind(account)
    .bind(scope.servers)
    .bind(scope.dms)
    .bind(scope.before_ms)
    .bind(via.as_str())
    .execute(&mut *tx)
    .await?;
    jobs::enqueue(
        &mut *tx,
        &NewJob::new(
            job_id,
            JOB_KIND,
            json!({ "run_id": id.to_string(), "account_id": account.to_string() }),
        ),
    )
    .await?;
    let row = query(format!(
        "SELECT {COLS} FROM message_self_deletes WHERE id = $1"
    ))
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Ok(run(&row)?))
}

/// Claim a queued run. `None` when it is gone or already finished.
pub async fn begin(pool: &PgPool, id: i64) -> DbResult<Option<Run>> {
    let row = query(format!(
        "UPDATE message_self_deletes SET state = 'running' WHERE id = $1 \
         AND state IN ('queued', 'running') RETURNING {COLS}"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    row.as_ref().map(run).transpose()
}

/// The run is done: `hidden` rows were hidden.
pub async fn finish(pool: &PgPool, id: i64, hidden: i64) -> DbResult<()> {
    sqlx::query(
        "UPDATE message_self_deletes SET state = 'done', finished_at = now(), hidden_count = $2 WHERE id = $1",
    )
    .bind(id)
    .bind(hidden)
    .execute(pool)
    .await?;
    Ok(())
}

/// The run gave up (a failed run does not count against the 24-hour limit).
pub async fn fail(pool: &PgPool, id: i64, error: &str) -> DbResult<()> {
    sqlx::query(
        "UPDATE message_self_deletes SET state = 'failed', finished_at = now(), error = $2 WHERE id = $1",
    )
    .bind(id)
    .bind(error.chars().take(500).collect::<String>())
    .execute(pool)
    .await?;
    Ok(())
}

/// Hide the member's visible messages inside `scope` as `user_deleted`, with the purge date
/// `now + PURGE_DAYS` (unset for rows under a hold). Returns the hidden ids per channel, for the
/// live `MESSAGE_DELETE_BULK` dispatch.
pub async fn hide(pool: &PgPool, account: i64, scope: Scope, now_ms: i64) -> DbResult<Vec<Hidden>> {
    let channels: Vec<(i64, Option<i64>)> = query(format!(
        "SELECT DISTINCT m.channel_id, c.guild_id FROM messages m \
         JOIN channels c ON c.id = m.channel_id WHERE {SCOPE_SQL} ORDER BY m.channel_id"
    ))
    .bind(account)
    .bind(scope.servers)
    .bind(scope.dms)
    .bind(scope.before_ms)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|r| Ok((r.try_get("channel_id")?, r.try_get("guild_id")?)))
    .collect::<DbResult<_>>()?;
    let purge_ms = now_ms + PURGE_DAYS * 86_400_000;
    let mut out = Vec::new();
    for chunk in channels.chunks(CHANNEL_CHUNK) {
        let ids: Vec<i64> = chunk.iter().map(|(c, _)| *c).collect();
        let mut tx = pool.begin().await?;
        query(format!(
            "DELETE FROM pins WHERE message_id IN ( \
                SELECT m.id FROM messages m JOIN channels c ON c.id = m.channel_id \
                WHERE {SCOPE_SQL} AND m.channel_id = ANY($5))"
        ))
        .bind(account)
        .bind(scope.servers)
        .bind(scope.dms)
        .bind(scope.before_ms)
        .bind(&ids)
        .execute(&mut *tx)
        .await?;
        let rows = query(format!(
            "UPDATE messages m SET visibility = 'user_deleted', pinned = FALSE, \
             deleted_at = to_timestamp($6::double precision / 1000.0), \
             purge_after = CASE WHEN m.hold_id IS NULL \
                 THEN to_timestamp($7::double precision / 1000.0) END \
             FROM channels c WHERE c.id = m.channel_id AND {SCOPE_SQL} AND m.channel_id = ANY($5) \
             RETURNING m.id, m.channel_id"
        ))
        .bind(account)
        .bind(scope.servers)
        .bind(scope.dms)
        .bind(scope.before_ms)
        .bind(&ids)
        .bind(now_ms)
        .bind(purge_ms)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        for (channel, guild) in chunk {
            let message_ids: Vec<i64> = rows
                .iter()
                .filter_map(|r| {
                    (r.try_get::<i64, _>("channel_id").ok()? == *channel)
                        .then(|| r.try_get::<i64, _>("id").ok())
                        .flatten()
                })
                .collect();
            if !message_ids.is_empty() {
                out.push(Hidden {
                    guild_id: *guild,
                    channel_id: *channel,
                    message_ids,
                });
            }
        }
    }
    Ok(out)
}
