use std::time::{Duration, Instant};

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use crate::{DbError, DbResult};

/// Pool settings.
#[derive(Debug, Clone)]
pub struct DbConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout: Duration,
}

impl DbConfig {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            max_connections: 20,
            min_connections: 1,
            acquire_timeout: Duration::from_secs(5),
        }
    }
}

/// Open a connection pool.
pub async fn connect(cfg: &DbConfig) -> DbResult<PgPool> {
    Ok(PgPoolOptions::new()
        .max_connections(cfg.max_connections)
        .min_connections(cfg.min_connections)
        .acquire_timeout(cfg.acquire_timeout)
        .connect(&cfg.url)
        .await?)
}

/// Apply all pending forward-only migrations from `crates/sr-db/migrations/`.
pub async fn migrate(pool: &PgPool) -> DbResult<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

/// Where the database stands against the migrations compiled into this binary (`sr-cli upgrade`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationStatus {
    /// Migrations this binary knows.
    pub known: usize,
    /// Versions applied successfully.
    pub applied: Vec<i64>,
    /// Known migrations that are not applied yet: `(version, description)`, oldest first.
    pub pending: Vec<(i64, String)>,
    /// Applied versions this binary does not know: the database is newer than the program (a downgrade).
    pub unknown: Vec<i64>,
    /// Versions recorded as failed (an interrupted migration that needs a human).
    pub failed: Vec<i64>,
}

/// Compare `_sqlx_migrations` with the embedded migrations. A database that was never migrated has
/// everything pending.
pub async fn migration_status(pool: &PgPool) -> DbResult<MigrationStatus> {
    let migrator = sqlx::migrate!("./migrations");
    let table: Option<String> = sqlx::query_scalar("SELECT to_regclass('_sqlx_migrations')::text")
        .fetch_one(pool)
        .await?;
    let rows: Vec<(i64, bool)> = if table.is_some() {
        sqlx::query_as("SELECT version, success FROM _sqlx_migrations ORDER BY version")
            .fetch_all(pool)
            .await?
    } else {
        Vec::new()
    };
    let applied: Vec<i64> = rows.iter().filter(|(_, ok)| *ok).map(|(v, _)| *v).collect();
    let failed: Vec<i64> = rows
        .iter()
        .filter(|(_, ok)| !*ok)
        .map(|(v, _)| *v)
        .collect();
    let known: Vec<(i64, String)> = migrator
        .iter()
        .filter(|m| !m.migration_type.is_down_migration())
        .map(|m| (m.version, m.description.to_string()))
        .collect();
    let pending = known
        .iter()
        .filter(|(v, _)| !applied.contains(v) && !failed.contains(v))
        .cloned()
        .collect();
    let unknown = rows
        .iter()
        .map(|(v, _)| *v)
        .filter(|v| !known.iter().any(|(k, _)| k == v))
        .collect();
    Ok(MigrationStatus {
        known: known.len(),
        applied,
        pending,
        unknown,
        failed,
    })
}

/// Result of a health probe.
#[derive(Debug, Clone, Copy)]
pub struct Health {
    pub latency: Duration,
    pub pool_size: u32,
    pub idle: usize,
}

/// Run `SELECT 1` with a timeout.
pub async fn health_check(pool: &PgPool, timeout: Duration) -> DbResult<Health> {
    let start = Instant::now();
    let probe = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(pool);
    tokio::time::timeout(timeout, probe)
        .await
        .map_err(|_| DbError::Timeout)??;
    Ok(Health {
        latency: start.elapsed(),
        pool_size: pool.size(),
        idle: pool.num_idle(),
    })
}
