//! Postgres advisory-lock helpers.

use sqlx::pool::PoolConnection;
use sqlx::{PgPool, Postgres, Transaction};

use crate::DbResult;

/// Registration cap check lock (docs/06 section 3).
pub const CAP_LOCK: i64 = lock_key("sr.cap");

/// Deterministic 64-bit key (FNV-1a) for a lock name.
pub const fn lock_key(name: &str) -> i64 {
    let bytes = name.as_bytes();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        i += 1;
    }
    hash as i64
}

/// Block until the transaction-scoped lock is held; released at commit/rollback.
pub async fn xact_lock(tx: &mut Transaction<'static, Postgres>, key: i64) -> DbResult<()> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(key)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Try to take the transaction-scoped lock without blocking.
pub async fn try_xact_lock(tx: &mut Transaction<'static, Postgres>, key: i64) -> DbResult<bool> {
    Ok(sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1)")
        .bind(key)
        .fetch_one(&mut **tx)
        .await?)
}

/// Session-scoped lock pinned to one pooled connection (e.g. singleton jobs).
/// Call [`SessionLock::release`]; if dropped without release the connection is
/// detached from the pool (and closed), which frees the lock server-side.
pub struct SessionLock {
    conn: Option<PoolConnection<Postgres>>,
    key: i64,
}

impl SessionLock {
    /// Returns `None` if another session holds the lock.
    pub async fn try_acquire(pool: &PgPool, key: i64) -> DbResult<Option<Self>> {
        let mut conn = pool.acquire().await?;
        let got: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
            .bind(key)
            .fetch_one(&mut *conn)
            .await?;
        Ok(got.then_some(Self {
            conn: Some(conn),
            key,
        }))
    }

    pub async fn release(mut self) -> DbResult<()> {
        if let Some(mut conn) = self.conn.take() {
            sqlx::query("SELECT pg_advisory_unlock($1)")
                .bind(self.key)
                .execute(&mut *conn)
                .await?;
        }
        Ok(())
    }
}

impl Drop for SessionLock {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            drop(conn.detach());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_stable_and_distinct() {
        assert_eq!(lock_key("a"), lock_key("a"));
        assert_ne!(lock_key("a"), lock_key("b"));
        assert_eq!(CAP_LOCK, lock_key("sr.cap"));
    }
}
