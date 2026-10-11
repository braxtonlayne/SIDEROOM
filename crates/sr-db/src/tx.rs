use std::future::Future;
use std::pin::Pin;

use sqlx::{PgPool, Postgres, Transaction};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Run `f` inside a transaction. Commits on `Ok`, rolls back on `Err` (and on drop).
///
/// ```ignore
/// with_tx(&pool, |tx| Box::pin(async move {
///     sqlx::query("UPDATE ...").execute(&mut **tx).await?;
///     Ok::<_, sr_db::DbError>(())
/// })).await?;
/// ```
pub async fn with_tx<T, E, F>(pool: &PgPool, f: F) -> Result<T, E>
where
    E: From<sqlx::Error>,
    F: for<'c> FnOnce(&'c mut Transaction<'static, Postgres>) -> BoxFuture<'c, Result<T, E>>,
{
    let mut tx = pool.begin().await?;
    match f(&mut tx).await {
        Ok(v) => {
            tx.commit().await?;
            Ok(v)
        }
        Err(e) => {
            if let Err(rb) = tx.rollback().await {
                tracing::warn!(error = %rb, "transaction rollback failed");
            }
            Err(e)
        }
    }
}
