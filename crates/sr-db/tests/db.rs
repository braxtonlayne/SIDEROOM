//! DB-backed tests. Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db -- --ignored`

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use sr_db::jobs::{self, NewJob};
use sr_db::{DbConfig, DbError, PgPool, lock};
use tokio::sync::watch;

async fn pool() -> PgPool {
    let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
    let pool = sr_db::connect(&DbConfig::new(url)).await.expect("connect");
    sr_db::migrate(&pool).await.expect("migrate");
    pool
}

fn uid() -> i64 {
    use std::sync::atomic::AtomicI64;
    static N: AtomicI64 = AtomicI64::new(0);
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + N.fetch_add(1, Ordering::Relaxed) % 1000
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn migrate_and_health() {
    let pool = pool().await;
    sr_db::migrate(&pool).await.expect("idempotent");
    sr_db::health_check(&pool, Duration::from_secs(2))
        .await
        .expect("healthy");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn job_lifecycle_backoff_and_dead_letter() {
    let pool = pool().await;
    let kind = format!("t-{}", uid());
    let kinds = vec![kind.clone()];
    let id = uid();
    let mut nj = NewJob::new(id, &kind, serde_json::json!({"a": 1}));
    nj.max_attempts = 2;
    jobs::enqueue(&pool, &nj).await.unwrap();

    let c = jobs::claim(&pool, &kinds, "w1", 5).await.unwrap();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].attempts, 1);
    // Already claimed: nobody else gets it.
    assert!(
        jobs::claim(&pool, &kinds, "w2", 5)
            .await
            .unwrap()
            .is_empty()
    );

    // First failure re-queues with backoff (not immediately claimable).
    assert_eq!(
        jobs::fail(&pool, id, "boom").await.unwrap().as_deref(),
        Some("queued")
    );
    assert!(
        jobs::claim(&pool, &kinds, "w1", 5)
            .await
            .unwrap()
            .is_empty()
    );
    sqlx::query("UPDATE jobs SET run_at = now() WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();

    let c = jobs::claim(&pool, &kinds, "w1", 5).await.unwrap();
    assert_eq!(c[0].attempts, 2);
    assert_eq!(
        jobs::fail(&pool, id, "boom2").await.unwrap().as_deref(),
        Some("dead")
    );
    assert!(
        jobs::list_dead(&pool, 100)
            .await
            .unwrap()
            .iter()
            .any(|j| j.id == id)
    );

    assert!(jobs::retry(&pool, id).await.unwrap());
    let c = jobs::claim(&pool, &kinds, "w1", 5).await.unwrap();
    assert_eq!(c[0].attempts, 1);
    assert!(jobs::complete(&pool, id).await.unwrap());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn concurrent_claims_never_double_claim() {
    let pool = pool().await;
    let kind = format!("t-{}", uid());
    for _ in 0..20 {
        jobs::enqueue(&pool, &NewJob::new(uid(), &kind, serde_json::json!({})))
            .await
            .unwrap();
    }
    let kinds = vec![kind];
    let mut tasks = Vec::new();
    for w in 0..5 {
        let (p, k) = (pool.clone(), kinds.clone());
        tasks.push(tokio::spawn(async move {
            jobs::claim(&p, &k, &format!("w{w}"), 10).await.unwrap()
        }));
    }
    let mut ids = Vec::new();
    for t in tasks {
        ids.extend(t.await.unwrap().into_iter().map(|j| j.id));
    }
    let total = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(total, 20);
    assert_eq!(ids.len(), 20);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn runner_processes_and_shuts_down() {
    let pool = pool().await;
    let kind = format!("t-{}", uid());
    for _ in 0..3 {
        jobs::enqueue(&pool, &NewJob::new(uid(), &kind, serde_json::json!({})))
            .await
            .unwrap();
    }
    let done = Arc::new(AtomicUsize::new(0));
    let d = done.clone();
    let runner = sr_db::runner::JobRunner::new(pool.clone(), "test")
        .poll_interval(Duration::from_millis(50))
        .register(kind, 2, move |_job| {
            let d = d.clone();
            Box::pin(async move {
                d.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        });
    let (tx, rx) = watch::channel(false);
    let handle = tokio::spawn(runner.run(rx));
    for _ in 0..100 {
        if done.load(Ordering::SeqCst) == 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(done.load(Ordering::SeqCst), 3);
    tx.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .expect("graceful shutdown")
        .unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn with_tx_commits_and_rolls_back() {
    let pool = pool().await;
    let key = format!("k-{}", uid());
    let k = key.clone();
    let r: Result<(), DbError> = sr_db::with_tx(&pool, |tx| {
        Box::pin(async move {
            sqlx::query("INSERT INTO platform_settings (key, value) VALUES ($1, '1')")
                .bind(k)
                .execute(&mut **tx)
                .await?;
            Err(DbError::Timeout)
        })
    })
    .await;
    assert!(r.is_err());
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM platform_settings WHERE key = $1")
        .bind(&key)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn session_lock_is_exclusive() {
    let pool = pool().await;
    let key = lock::lock_key(&format!("test.{}", uid()));
    let first = lock::SessionLock::try_acquire(&pool, key).await.unwrap();
    let first = first.expect("first acquires");
    assert!(
        lock::SessionLock::try_acquire(&pool, key)
            .await
            .unwrap()
            .is_none()
    );
    first.release().await.unwrap();
    assert!(
        lock::SessionLock::try_acquire(&pool, key)
            .await
            .unwrap()
            .is_some()
    );
}
