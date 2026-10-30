//! Postgres side of the scrapbook vault (PRF-SCR-001, migration 1690): [`PgScrapbook`] implements
//! `sr_storage::scrapbook::ScrapbookStore`.
//!
//! Deleting entries and queueing the removals of their copies is one statement ([`delete_in`]), so a crash
//! cannot leave a copy in a bucket that nothing remembers. The same function serves the message-delete
//! cascade, the channel-delete cascade and the erasure of a member (`scrapbook_legal::erase_for`).

use async_trait::async_trait;
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, PgPool, Row};
use sr_storage::scrapbook::{
    DeleteScope, Deleted, Entry, EntryWithObjects, Facets, Filter, Object, ObjectKind, Removal,
    RemovalState, ScrapbookStore, StoreError, StoreResult, Target, Via,
};
use std::collections::HashMap;

fn se(e: sqlx::Error) -> StoreError {
    StoreError(e.to_string())
}

/// Postgres-backed [`ScrapbookStore`].
#[derive(Clone)]
pub struct PgScrapbook {
    pool: PgPool,
}

impl PgScrapbook {
    /// Wrap a pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const MS: &str = "round(extract(epoch FROM {c}) * 1000)::bigint";

fn ms(col: &str) -> String {
    MS.replace("{c}", col)
}

fn entry_cols() -> String {
    format!(
        "id, guild_id, channel_id, message_id, author_id, author_name, content, \
         {} AS sent_ms, archived_by, via, tags, mount_id, skipped, {} AS created_ms",
        ms("sent_at"),
        ms("created_at")
    )
}

fn entry_from(r: &PgRow) -> Result<Entry, sqlx::Error> {
    let via: String = r.try_get("via")?;
    Ok(Entry {
        id: r.try_get("id")?,
        guild_id: r.try_get("guild_id")?,
        channel_id: r.try_get("channel_id")?,
        message_id: r.try_get("message_id")?,
        author_id: r.try_get("author_id")?,
        author_name: r.try_get("author_name")?,
        content: r.try_get("content")?,
        sent_at_ms: r.try_get("sent_ms")?,
        archived_by: r.try_get("archived_by")?,
        via: Via::parse(&via).unwrap_or(Via::Manual),
        tags: r.try_get("tags")?,
        mount_id: r.try_get("mount_id")?,
        skipped: r.try_get("skipped")?,
        created_at_ms: r.try_get("created_ms")?,
    })
}

fn object_from(r: &PgRow) -> Result<Object, sqlx::Error> {
    let kind: String = r.try_get("kind")?;
    Ok(Object {
        id: r.try_get("id")?,
        entry_id: r.try_get("entry_id")?,
        position: r.try_get("position")?,
        mount_id: r.try_get("mount_id")?,
        key: r.try_get("key")?,
        filename: r.try_get("filename")?,
        content_type: r.try_get("content_type")?,
        size: r.try_get("size_bytes")?,
        kind: ObjectKind::parse(&kind).unwrap_or(ObjectKind::File),
    })
}

fn removal_from(r: &PgRow) -> Result<Removal, sqlx::Error> {
    let state: String = r.try_get("state")?;
    Ok(Removal {
        id: r.try_get("id")?,
        guild_id: r.try_get("guild_id")?,
        mount_id: r.try_get("mount_id")?,
        key: r.try_get("key")?,
        message_id: r.try_get("message_id")?,
        state: RemovalState::parse(&state),
        attempts: r.try_get("attempts")?,
        last_error: r.try_get("last_error")?,
        created_at_ms: r.try_get("created_ms")?,
        next_attempt_ms: r.try_get("next_ms")?,
    })
}

fn removal_cols() -> String {
    format!(
        "id, guild_id, mount_id, key, message_id, state, attempts, last_error, {} AS created_ms, \
         {} AS next_ms",
        ms("created_at"),
        ms("next_attempt_at")
    )
}

/// Attach the copies to a list of entries.
async fn with_objects(
    conn: &mut PgConnection,
    entries: Vec<Entry>,
) -> Result<Vec<EntryWithObjects>, sqlx::Error> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<i64> = entries.iter().map(|e| e.id).collect();
    let rows = sqlx::query(
        "SELECT id, entry_id, position, mount_id, key, filename, content_type, size_bytes, kind \
         FROM scrapbook_objects WHERE entry_id = ANY($1) ORDER BY entry_id, position",
    )
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;
    let mut by_entry: HashMap<i64, Vec<Object>> = HashMap::new();
    for r in &rows {
        let o = object_from(r)?;
        by_entry.entry(o.entry_id).or_default().push(o);
    }
    Ok(entries
        .into_iter()
        .map(|e| {
            let objects = by_entry.remove(&e.id).unwrap_or_default();
            EntryWithObjects { entry: e, objects }
        })
        .collect())
}

/// Delete the entries `scope` names and queue one removal per copy, in one statement. Used inside the erasure
/// transaction as well as by [`PgScrapbook`].
pub async fn delete_in(conn: &mut PgConnection, scope: DeleteScope) -> Result<Deleted, sqlx::Error> {
    let (cond, a, b) = match scope {
        DeleteScope::Entry { guild_id, id } => ("guild_id = $1 AND id = $2", guild_id, id),
        DeleteScope::Message(m) => ("message_id = $1 AND $2::bigint IS NOT NULL", m, 0),
        DeleteScope::Channel(c) => ("channel_id = $1 AND $2::bigint IS NOT NULL", c, 0),
        DeleteScope::Author(u) => ("author_id = $1 AND $2::bigint IS NOT NULL", u, 0),
    };
    // `cond` is one of four literals above.
    let sql = format!(
        "WITH gone AS ( \
            DELETE FROM scrapbook_entries WHERE {cond} RETURNING id, guild_id, message_id \
         ), q AS ( \
            INSERT INTO scrapbook_removals (id, guild_id, mount_id, key, message_id) \
            SELECT o.id, g.guild_id, o.mount_id, o.key, g.message_id \
            FROM scrapbook_objects o JOIN gone g ON g.id = o.entry_id \
            ON CONFLICT (mount_id, key) DO NOTHING RETURNING id \
         ) \
         SELECT (SELECT count(*) FROM gone)::bigint AS entries, \
                COALESCE((SELECT array_agg(id) FROM q), '{{}}'::bigint[]) AS queued"
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(a)
        .bind(b)
        .fetch_one(&mut *conn)
        .await?;
    let entries: i64 = row.try_get("entries")?;
    Ok(Deleted {
        entries: u64::try_from(entries).unwrap_or(0),
        queued: row.try_get("queued")?,
    })
}

#[async_trait]
impl ScrapbookStore for PgScrapbook {
    async fn target(&self, guild: i64) -> StoreResult<Option<Target>> {
        let row = sqlx::query(
            "SELECT guild_id, mount_id, granted_by FROM scrapbook_targets WHERE guild_id = $1",
        )
        .bind(guild)
        .fetch_optional(&self.pool)
        .await
        .map_err(se)?;
        row.map(|r| {
            Ok(Target {
                guild_id: r.try_get("guild_id")?,
                mount_id: r.try_get("mount_id")?,
                granted_by: r.try_get("granted_by")?,
            })
        })
        .transpose()
        .map_err(se)
    }

    async fn set_target(&self, t: &Target) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO scrapbook_targets (guild_id, mount_id, granted_by) VALUES ($1, $2, $3) \
             ON CONFLICT (guild_id) DO UPDATE SET mount_id = $2, granted_by = $3, granted_at = now()",
        )
        .bind(t.guild_id)
        .bind(t.mount_id)
        .bind(t.granted_by)
        .execute(&self.pool)
        .await
        .map_err(se)?;
        Ok(())
    }

    async fn clear_target(&self, guild: i64) -> StoreResult<bool> {
        let r = sqlx::query("DELETE FROM scrapbook_targets WHERE guild_id = $1")
            .bind(guild)
            .execute(&self.pool)
            .await
            .map_err(se)?;
        Ok(r.rows_affected() > 0)
    }

    async fn insert(&self, e: &Entry, objects: &[Object]) -> StoreResult<bool> {
        let mut tx = self.pool.begin().await.map_err(se)?;
        let inserted = sqlx::query(
            "INSERT INTO scrapbook_entries (id, guild_id, channel_id, message_id, author_id, author_name, \
               content, sent_at, archived_by, via, tags, mount_id, skipped, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, to_timestamp($8::double precision / 1000.0), $9, $10, \
               $11, $12, $13, to_timestamp($14::double precision / 1000.0)) \
             ON CONFLICT (guild_id, message_id) DO NOTHING",
        )
        .bind(e.id)
        .bind(e.guild_id)
        .bind(e.channel_id)
        .bind(e.message_id)
        .bind(e.author_id)
        .bind(&e.author_name)
        .bind(&e.content)
        .bind(e.sent_at_ms)
        .bind(e.archived_by)
        .bind(e.via.as_str())
        .bind(&e.tags)
        .bind(e.mount_id)
        .bind(e.skipped)
        .bind(e.created_at_ms)
        .execute(&mut *tx)
        .await
        .map_err(se)?;
        if inserted.rows_affected() == 0 {
            return Ok(false);
        }
        for o in objects {
            sqlx::query(
                "INSERT INTO scrapbook_objects (id, entry_id, position, mount_id, key, filename, \
                   content_type, size_bytes, kind) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            )
            .bind(o.id)
            .bind(o.entry_id)
            .bind(o.position)
            .bind(o.mount_id)
            .bind(&o.key)
            .bind(&o.filename)
            .bind(&o.content_type)
            .bind(o.size)
            .bind(o.kind.as_str())
            .execute(&mut *tx)
            .await
            .map_err(se)?;
            // The same key may still be waiting in the removal list from an earlier life of this message
            // (deleted, restored, archived again): the copy is wanted now, so the removal must not run.
            sqlx::query("DELETE FROM scrapbook_removals WHERE mount_id = $1 AND key = $2")
                .bind(o.mount_id)
                .bind(&o.key)
                .execute(&mut *tx)
                .await
                .map_err(se)?;
        }
        tx.commit().await.map_err(se)?;
        Ok(true)
    }

    async fn by_message(&self, guild: i64, message: i64) -> StoreResult<Option<EntryWithObjects>> {
        let sql = format!(
            "SELECT {} FROM scrapbook_entries WHERE guild_id = $1 AND message_id = $2",
            entry_cols()
        );
        let mut conn = self.pool.acquire().await.map_err(se)?;
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(guild)
            .bind(message)
            .fetch_optional(&mut *conn)
            .await
            .map_err(se)?;
        let Some(row) = row else { return Ok(None) };
        let e = entry_from(&row).map_err(se)?;
        Ok(with_objects(&mut conn, vec![e])
            .await
            .map_err(se)?
            .into_iter()
            .next())
    }

    async fn get(&self, guild: i64, id: i64) -> StoreResult<Option<EntryWithObjects>> {
        let sql = format!(
            "SELECT {} FROM scrapbook_entries WHERE guild_id = $1 AND id = $2",
            entry_cols()
        );
        let mut conn = self.pool.acquire().await.map_err(se)?;
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(guild)
            .bind(id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(se)?;
        let Some(row) = row else { return Ok(None) };
        let e = entry_from(&row).map_err(se)?;
        Ok(with_objects(&mut conn, vec![e])
            .await
            .map_err(se)?
            .into_iter()
            .next())
    }

    async fn list(
        &self,
        guild: i64,
        f: &Filter,
        before: Option<i64>,
        limit: i64,
    ) -> StoreResult<Vec<EntryWithObjects>> {
        let sql = format!(
            "SELECT {} FROM scrapbook_entries \
             WHERE guild_id = $1 AND channel_id = ANY($2) \
               AND ($3::bigint IS NULL OR id < $3) \
               AND ($4::bigint IS NULL OR channel_id = $4) \
               AND ($5::bigint IS NULL OR author_id = $5) \
               AND ($6::text IS NULL OR $6 = ANY(tags)) \
               AND ($7::bigint IS NULL OR sent_at >= to_timestamp($7::double precision / 1000.0)) \
               AND ($8::bigint IS NULL OR sent_at < to_timestamp($8::double precision / 1000.0)) \
             ORDER BY id DESC LIMIT $9",
            entry_cols()
        );
        let mut conn = self.pool.acquire().await.map_err(se)?;
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(guild)
            .bind(&f.viewable)
            .bind(before)
            .bind(f.channel_id)
            .bind(f.author_id)
            .bind(&f.tag)
            .bind(f.from_ms)
            .bind(f.to_ms)
            .bind(limit.clamp(1, 200))
            .fetch_all(&mut *conn)
            .await
            .map_err(se)?;
        let entries = rows
            .iter()
            .map(entry_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(se)?;
        with_objects(&mut conn, entries).await.map_err(se)
    }

    async fn facets(&self, guild: i64, viewable: &[i64]) -> StoreResult<Facets> {
        let tags = sqlx::query(
            "SELECT t AS tag, count(*)::bigint AS n FROM scrapbook_entries, unnest(tags) AS t \
             WHERE guild_id = $1 AND channel_id = ANY($2) GROUP BY t ORDER BY n DESC, t LIMIT 100",
        )
        .bind(guild)
        .bind(viewable)
        .fetch_all(&self.pool)
        .await
        .map_err(se)?;
        let authors = sqlx::query(
            "SELECT author_id, max(author_name) AS name, count(*)::bigint AS n FROM scrapbook_entries \
             WHERE guild_id = $1 AND channel_id = ANY($2) GROUP BY author_id ORDER BY n DESC LIMIT 100",
        )
        .bind(guild)
        .bind(viewable)
        .fetch_all(&self.pool)
        .await
        .map_err(se)?;
        let channels = sqlx::query(
            "SELECT channel_id, count(*)::bigint AS n FROM scrapbook_entries \
             WHERE guild_id = $1 AND channel_id = ANY($2) GROUP BY channel_id ORDER BY n DESC",
        )
        .bind(guild)
        .bind(viewable)
        .fetch_all(&self.pool)
        .await
        .map_err(se)?;
        let g = |e: sqlx::Error| se(e);
        Ok(Facets {
            tags: tags
                .iter()
                .map(|r| Ok((r.try_get("tag")?, r.try_get("n")?)))
                .collect::<Result<_, sqlx::Error>>()
                .map_err(g)?,
            authors: authors
                .iter()
                .map(|r| Ok((r.try_get("author_id")?, r.try_get("name")?, r.try_get("n")?)))
                .collect::<Result<_, sqlx::Error>>()
                .map_err(g)?,
            channels: channels
                .iter()
                .map(|r| Ok((r.try_get("channel_id")?, r.try_get("n")?)))
                .collect::<Result<_, sqlx::Error>>()
                .map_err(g)?,
        })
    }

    async fn set_tags(&self, guild: i64, id: i64, tags: &[String]) -> StoreResult<bool> {
        let r = sqlx::query("UPDATE scrapbook_entries SET tags = $3 WHERE guild_id = $1 AND id = $2")
            .bind(guild)
            .bind(id)
            .bind(tags)
            .execute(&self.pool)
            .await
            .map_err(se)?;
        Ok(r.rows_affected() > 0)
    }

    async fn delete(&self, scope: DeleteScope) -> StoreResult<Deleted> {
        let mut tx = self.pool.begin().await.map_err(se)?;
        let out = delete_in(&mut tx, scope).await.map_err(se)?;
        tx.commit().await.map_err(se)?;
        Ok(out)
    }

    async fn due_removals(&self, now_ms: i64, limit: i64) -> StoreResult<Vec<Removal>> {
        let sql = format!(
            "SELECT {} FROM scrapbook_removals \
             WHERE next_attempt_at <= to_timestamp($1::double precision / 1000.0) \
             ORDER BY next_attempt_at, id LIMIT $2",
            removal_cols()
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(now_ms)
            .bind(limit.clamp(1, 500))
            .fetch_all(&self.pool)
            .await
            .map_err(se)?;
        rows.iter()
            .map(removal_from)
            .collect::<Result<_, _>>()
            .map_err(se)
    }

    async fn removal(&self, id: i64) -> StoreResult<Option<Removal>> {
        let sql = format!(
            "SELECT {} FROM scrapbook_removals WHERE id = $1",
            removal_cols()
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(se)?;
        row.as_ref().map(removal_from).transpose().map_err(se)
    }

    async fn removal_done(&self, id: i64) -> StoreResult<()> {
        sqlx::query("DELETE FROM scrapbook_removals WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(se)?;
        Ok(())
    }

    async fn removal_failed(&self, id: i64, error: &str, next_ms: i64) -> StoreResult<()> {
        sqlx::query(
            "UPDATE scrapbook_removals SET state = 'failed', attempts = attempts + 1, last_error = $2, \
               next_attempt_at = to_timestamp($3::double precision / 1000.0) WHERE id = $1",
        )
        .bind(id)
        .bind(error.chars().take(500).collect::<String>())
        .bind(next_ms)
        .execute(&self.pool)
        .await
        .map_err(se)?;
        Ok(())
    }

    async fn failed_removals(&self, guild: i64) -> StoreResult<Vec<Removal>> {
        let sql = format!(
            "SELECT {} FROM scrapbook_removals WHERE guild_id = $1 AND state = 'failed' \
             ORDER BY created_at, id LIMIT 200",
            removal_cols()
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(guild)
            .fetch_all(&self.pool)
            .await
            .map_err(se)?;
        rows.iter()
            .map(removal_from)
            .collect::<Result<_, _>>()
            .map_err(se)
    }

    async fn retry_now(&self, guild: i64, id: i64, now_ms: i64) -> StoreResult<bool> {
        let r = sqlx::query(
            "UPDATE scrapbook_removals SET next_attempt_at = to_timestamp($3::double precision / 1000.0) \
             WHERE guild_id = $1 AND id = $2",
        )
        .bind(guild)
        .bind(id)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(se)?;
        Ok(r.rows_affected() > 0)
    }

    async fn dismiss(&self, guild: i64, id: i64) -> StoreResult<bool> {
        let r = sqlx::query("DELETE FROM scrapbook_removals WHERE guild_id = $1 AND id = $2")
            .bind(guild)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(se)?;
        Ok(r.rows_affected() > 0)
    }

    async fn queue_orphan(
        &self,
        guild: i64,
        mount_id: i64,
        key: &str,
        message: i64,
        error: &str,
    ) -> StoreResult<()> {
        // The id only has to be unique among removals; orphans have no object row, so mint one from the
        // key and mount (stable, so a repeat of the same failure is the same row).
        let id = orphan_id(mount_id, key);
        sqlx::query(
            "INSERT INTO scrapbook_removals (id, guild_id, mount_id, key, message_id, state, attempts, last_error) \
             VALUES ($1, $2, $3, $4, $5, 'failed', 1, $6) ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(guild)
        .bind(mount_id)
        .bind(key)
        .bind(message)
        .bind(error.chars().take(500).collect::<String>())
        .execute(&self.pool)
        .await
        .map_err(se)?;
        Ok(())
    }
}

/// A negative id derived from the mount and key (object ids are positive snowflakes, so these never clash).
fn orphan_id(mount_id: i64, key: &str) -> i64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ (mount_id as u64);
    for b in key.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    -((h >> 2) as i64) - 1
}
