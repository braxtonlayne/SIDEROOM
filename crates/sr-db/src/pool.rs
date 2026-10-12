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
