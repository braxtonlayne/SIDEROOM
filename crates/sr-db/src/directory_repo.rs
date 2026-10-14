//! Postgres [`DirectoryRepository`]: Server Directory listings (LNG-GLD-013).
//! Tests: `tests/directory_repo.rs` (needs `SR_TEST_DATABASE_URL`).

use async_trait::async_trait;
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use sr_chat::repo::RepoResult;
use sr_chat::settings::directory::{DirectoryEntry, DirectoryRepository, DirectoryStatus};
use sr_core::{GuildId, UserId};

use crate::chat_repo::{db, i, u};

#[derive(Clone)]
pub struct PgDirectoryRepo {
    pool: PgPool,
}

impl PgDirectoryRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const COLS: &str = "guild_id, status, category, description, featured, requested_by, \
    (extract(epoch FROM requested_at) * 1000)::bigint AS requested_ms, decided_by, \
    (extract(epoch FROM decided_at) * 1000)::bigint AS decided_ms, decision_note";

fn entry_from(r: &PgRow) -> RepoResult<DirectoryEntry> {
    let status: String = r.try_get("status").map_err(db)?;
    Ok(DirectoryEntry {
        guild_id: GuildId::new(u(r.try_get("guild_id").map_err(db)?)),
        status: DirectoryStatus::parse(&status)
            .ok_or_else(|| sr_chat::RepoError(format!("unknown directory status {status}")))?,
        category: r.try_get("category").map_err(db)?,
        description: r.try_get("description").map_err(db)?,
        featured: r.try_get("featured").map_err(db)?,
        requested_by: UserId::new(u(r.try_get("requested_by").map_err(db)?)),
        requested_at_ms: r.try_get("requested_ms").map_err(db)?,
        decided_by: r
            .try_get::<Option<i64>, _>("decided_by")
            .map_err(db)?
            .map(|v| UserId::new(u(v))),
        decided_at_ms: r.try_get("decided_ms").map_err(db)?,
        decision_note: r.try_get("decision_note").map_err(db)?,
    })
}

#[async_trait]
impl DirectoryRepository for PgDirectoryRepo {
    async fn get_listing(&self, guild: GuildId) -> RepoResult<Option<DirectoryEntry>> {
        let sql = format!("SELECT {COLS} FROM guild_directory WHERE guild_id = $1");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(guild.get()))
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(entry_from)
            .transpose()
    }

    async fn put_listing(&self, e: &DirectoryEntry) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO guild_directory (guild_id, status, category, description, featured, \
             requested_by, requested_at, decided_by, decided_at, decision_note) \
             VALUES ($1, $2, $3, $4, $5, $6, to_timestamp($7::double precision / 1000.0), $8, \
                     to_timestamp($9::double precision / 1000.0), $10) \
             ON CONFLICT (guild_id) DO UPDATE SET status = EXCLUDED.status, \
               category = EXCLUDED.category, description = EXCLUDED.description, \
               featured = EXCLUDED.featured, requested_by = EXCLUDED.requested_by, \
               requested_at = EXCLUDED.requested_at, decided_by = EXCLUDED.decided_by, \
               decided_at = EXCLUDED.decided_at, decision_note = EXCLUDED.decision_note, \
               updated_at = now()",
        )
        .bind(i(e.guild_id.get()))
        .bind(e.status.as_str())
        .bind(&e.category)
        .bind(&e.description)
        .bind(e.featured)
        .bind(i(e.requested_by.get()))
        .bind(e.requested_at_ms)
        .bind(e.decided_by.map(|d| i(d.get())))
        .bind(e.decided_at_ms)
        .bind(e.decision_note.as_deref())
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn delete_listing(&self, guild: GuildId) -> RepoResult<bool> {
        let r = sqlx::query("DELETE FROM guild_directory WHERE guild_id = $1")
            .bind(i(guild.get()))
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(r.rows_affected() > 0)
    }

    async fn list_listings(
        &self,
        status: Option<DirectoryStatus>,
        category: Option<&str>,
    ) -> RepoResult<Vec<DirectoryEntry>> {
        let sql = format!(
            "SELECT {COLS} FROM guild_directory \
             WHERE ($1::text IS NULL OR status = $1) AND ($2::text IS NULL OR category = $2) \
             ORDER BY requested_at, guild_id"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(status.map(DirectoryStatus::as_str))
            .bind(category)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter().map(entry_from).collect()
    }
}
