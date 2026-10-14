//! Ephemeral databases for tests (XC-TEST-001).
//!
//! `TestDb::new()` hands a test its own fully migrated database in about the time `CREATE DATABASE ... TEMPLATE`
//! takes (tens of milliseconds), so tests no longer share rows, need unique ids, or care about order. The
//! migrations run once per process into a *template* database named after the migration set (so editing a
//! migration yields a fresh template); each test then clones it. The clone is dropped with
//! `DROP DATABASE ... WITH (FORCE)` when the `TestDb` is dropped or `cleanup()`d.
//!
//! Needs `SR_TEST_DATABASE_URL` pointing at a role that may `CREATEDB` (the scratch Postgres used by the
//! `#[ignore]`d suites). The template stays on the server between runs and is reused; remove stale ones with
//! `DROP DATABASE sr_tpl_...` when you change Postgres versions.
//!
//! ```ignore
//! let db = sr_db::testing::TestDb::new().await?;
//! sqlx::query("select 1").execute(db.pool()).await?;
//! ```

use std::sync::atomic::{AtomicU32, Ordering};

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Connection, PgConnection, PgPool};

use crate::{DbError, DbResult};

/// Postgres advisory-lock key guarding template creation and cloning across test processes.
const LOCK_KEY: i64 = 0x5352_5445_5354; // "SRTEST"

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// One test's private, migrated database.
pub struct TestDb {
    pool: PgPool,
    name: String,
    admin: PgConnectOptions,
    options: PgConnectOptions,
    dropped: bool,
}

/// A query built from quoted identifiers (never user input), which sqlx 0.9 wants asserted safe.
fn dynamic(
    sql: String,
) -> sqlx::query::Query<'static, sqlx::Postgres, sqlx::postgres::PgArguments> {
    sqlx::query(sqlx::AssertSqlSafe(sql))
}

fn quote(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

/// Template name for the compiled-in migration set: count, last version and a checksum fold.
fn template_name() -> String {
    let m = sqlx::migrate!("./migrations");
    let mut fold: u64 = 0xcbf2_9ce4_8422_2325;
    for mig in m.migrations.iter() {
        for b in mig.checksum.iter() {
            fold = (fold ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    let last = m.migrations.last().map_or(0, |x| x.version);
    format!("sr_tpl_{}_{}_{:08x}", m.migrations.len(), last, fold as u32)
}

impl TestDb {
    /// Create a database from `SR_TEST_DATABASE_URL`.
    pub async fn new() -> DbResult<Self> {
        let url = std::env::var("SR_TEST_DATABASE_URL").map_err(|_| {
            DbError::Sqlx(sqlx::Error::Configuration(
                "SR_TEST_DATABASE_URL must be set for database tests".into(),
            ))
        })?;
        Self::from_url(&url).await
    }

    /// Create a database on the server `url` points at.
    pub async fn from_url(url: &str) -> DbResult<Self> {
        let base: PgConnectOptions = url.parse()?;
        // Connect to the maintenance database for CREATE/DROP DATABASE.
        let admin = base.clone().database("postgres");
        let tpl = template_name();
        let name = format!(
            "sr_t_{}_{}_{:04x}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
            rand_u16()
        );

        let mut conn = PgConnection::connect_with(&admin).await?;
        sqlx::query("select pg_advisory_lock($1)")
            .bind(LOCK_KEY)
            .execute(&mut conn)
            .await?;
        let result = Self::clone_template(&mut conn, &base, &tpl, &name).await;
        let _ = sqlx::query("select pg_advisory_unlock($1)")
            .bind(LOCK_KEY)
            .execute(&mut conn)
            .await;
        let _ = conn.close().await;
        result?;

        let options = base.database(&name);
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect_with(options.clone())
            .await?;
        Ok(Self {
            pool,
            name,
            admin,
            options,
            dropped: false,
        })
    }

    async fn clone_template(
        conn: &mut PgConnection,
        base: &PgConnectOptions,
        tpl: &str,
        name: &str,
    ) -> DbResult<()> {
        let exists: bool = sqlx::query_scalar(
            "select exists(select 1 from pg_database where datname = $1 and datistemplate)",
        )
        .bind(tpl)
        .fetch_one(&mut *conn)
        .await?;
        if !exists {
            // A half-built template from a crashed run is not marked as a template: rebuild it.
            dynamic(format!(
                "drop database if exists {} with (force)",
                quote(tpl)
            ))
            .execute(&mut *conn)
            .await?;
            dynamic(format!("create database {}", quote(tpl)))
                .execute(&mut *conn)
                .await?;
            let pool = PgPoolOptions::new()
                .max_connections(2)
                .connect_with(base.clone().database(tpl))
                .await?;
            crate::migrate(&pool).await?;
            pool.close().await;
            dynamic(format!(
                "alter database {} with is_template true",
                quote(tpl)
            ))
            .execute(&mut *conn)
            .await?;
        }
        dynamic(format!(
            "create database {} template {}",
            quote(name),
            quote(tpl)
        ))
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// The pool for this database.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// The database name (unique per test).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Connection options, for tests that open their own connections.
    pub fn connect_options(&self) -> PgConnectOptions {
        self.options.clone()
    }

    /// Drop the database now (also done on `Drop`, best effort).
    pub async fn cleanup(mut self) -> DbResult<()> {
        self.dropped = true;
        self.pool.close().await;
        drop_database(&self.admin, &self.name).await
    }
}

async fn drop_database(admin: &PgConnectOptions, name: &str) -> DbResult<()> {
    let mut conn = PgConnection::connect_with(admin).await?;
    dynamic(format!(
        "drop database if exists {} with (force)",
        quote(name)
    ))
    .execute(&mut conn)
    .await?;
    let _ = conn.close().await;
    Ok(())
}

impl Drop for TestDb {
    fn drop(&mut self) {
        if self.dropped {
            return;
        }
        let admin = self.admin.clone();
        let name = std::mem::take(&mut self.name);
        // A fresh runtime on its own thread: Drop may run inside a runtime that is shutting down.
        let _ = std::thread::spawn(move || {
            if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                let _ = rt.block_on(drop_database(&admin, &name));
            }
        })
        .join();
    }
}

fn rand_u16() -> u16 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    (n ^ (n >> 16)) as u16
}
