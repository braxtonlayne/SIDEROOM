//! `/readyz` probes for Postgres and object storage.

use async_trait::async_trait;
use sr_api::health::ReadinessCheck;
use sr_db::PgPool;
use sr_storage::{ObjectStorage, StorageError};
use std::{sync::Arc, time::Duration};

/// `SELECT 1` against the pool; also publishes pool gauges.
pub struct DbReady {
    pool: PgPool,
    timeout: Duration,
}

impl DbReady {
    pub fn new(pool: PgPool, timeout: Duration) -> Self {
        Self { pool, timeout }
    }
}

#[async_trait]
impl ReadinessCheck for DbReady {
    fn name(&self) -> &'static str {
        "database"
    }
    async fn check(&self) -> Result<(), String> {
        let h = sr_db::health_check(&self.pool, self.timeout)
            .await
            .map_err(|e| e.to_string())?;
        sr_api::observability::record_db_pool(h.pool_size, h.idle);
        Ok(())
    }
}

/// Heads a sentinel key: "not found" proves the bucket is reachable.
pub struct StorageReady {
    storage: Arc<dyn ObjectStorage>,
}

impl StorageReady {
    pub fn new(storage: Arc<dyn ObjectStorage>) -> Self {
        Self { storage }
    }
}

const SENTINEL: &str = ".sr-readyz";

#[async_trait]
impl ReadinessCheck for StorageReady {
    fn name(&self) -> &'static str {
        "storage"
    }
    async fn check(&self) -> Result<(), String> {
        match self.storage.head(SENTINEL).await {
            Ok(_) | Err(StorageError::NotFound(_)) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_storage::ObjectStoreStorage;

    #[tokio::test]
    async fn storage_ready_with_missing_sentinel() {
        let c = StorageReady::new(Arc::new(ObjectStoreStorage::in_memory()));
        assert!(c.check().await.is_ok());
        assert_eq!(c.name(), "storage");
    }
}
