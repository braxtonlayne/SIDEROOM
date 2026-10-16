//! Postgres-backed test for the leased scheduled-events clock (G-92). Run with a scratch
//! Postgres: `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test p13_events --
//! --ignored --test-threads=1`

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use sr_db::jobs;
use sr_db::runner::{HandlerError, JobRunner};
use sr_db::testing::TestDb;
use tokio::sync::watch;

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn two_runners_apply_one_event_tick_and_the_next_is_thirty_seconds_ahead() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();

    // Every node seeds at startup; only the first inserts, later calls leave the row alone.
    assert!(jobs::seed_scheduled_events_tick(&pool).await.unwrap());
    assert!(!jobs::seed_scheduled_events_tick(&pool).await.unwrap());
    assert!(!jobs::seed_scheduled_events_tick(&pool).await.unwrap());

    // Two runners (two nodes) with the same handler: it re-enqueues the job ahead first and
    // then "advances events" (counts).
    let applied = Arc::new(AtomicUsize::new(0));
    let runner = |worker: &str| {
        let (pool, applied) = (pool.clone(), applied.clone());
        JobRunner::new(pool.clone(), worker)
            .poll_interval(Duration::from_millis(20))
            .register(jobs::SCHEDULED_EVENTS_TICK_KIND, 1, move |job| {
                let (pool, applied) = (pool.clone(), applied.clone());
                Box::pin(async move {
                    jobs::reschedule_running(&pool, job.id, jobs::SCHEDULED_EVENTS_TICK_INTERVAL)
                        .await?;
                    applied.fetch_add(1, Ordering::SeqCst);
                    Ok::<(), HandlerError>(())
                })
            })
    };
    let (stop_tx, stop_rx) = watch::channel(false);
    let a = tokio::spawn(runner("node-a").run(stop_rx.clone()));
    let b = tokio::spawn(runner("node-b").run(stop_rx));
    tokio::time::sleep(Duration::from_millis(800)).await;
    stop_tx.send(true).unwrap();
    a.await.unwrap();
    b.await.unwrap();
    assert_eq!(
        applied.load(Ordering::SeqCst),
        1,
        "one tick applied across two runners"
    );

    // The row is queued again, about 30 s ahead, and a claim now finds nothing due.
    let (state, ahead): (String, f64) = sqlx::query_as(
        "SELECT state::text, EXTRACT(EPOCH FROM (run_at - now()))::float8 FROM jobs WHERE id = $1",
    )
    .bind(jobs::SCHEDULED_EVENTS_TICK_JOB_ID)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, "queued");
    assert!((25.0..=31.0).contains(&ahead), "run_at is {ahead} s ahead");
    let kinds = vec![jobs::SCHEDULED_EVENTS_TICK_KIND.to_owned()];
    assert!(
        jobs::claim(&pool, &kinds, "node-c", 1)
            .await
            .unwrap()
            .is_empty()
    );
    db.cleanup().await.unwrap();
}
