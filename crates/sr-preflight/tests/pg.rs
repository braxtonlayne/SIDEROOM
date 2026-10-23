//! The database checks against real Postgres (run with `SR_TEST_DATABASE_URL`, `--include-ignored`,
//! `--test-threads=1`).

use sr_db::testing::TestDb;
use sr_preflight::{
    Check, Status, database_checks,
    probes::{PgDb, RealHost},
};

fn by(rows: &[Check], id: &str) -> Check {
    rows.iter().find(|c| c.id == id).unwrap().clone()
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_database_checks_against_a_migrated_database() {
    let db = TestDb::new().await.unwrap();
    let probe = PgDb(db.pool().clone());

    let rows = database_checks(&probe, &RealHost).await;
    assert_eq!(by(&rows, "database").status, Status::Pass);
    assert_eq!(by(&rows, "migrations").status, Status::Pass);
    assert_eq!(
        by(&rows, "clock").status,
        Status::Pass,
        "{:?}",
        by(&rows, "clock")
    );
    // The VAPID row is created by sr-server on first start, so a fresh database warns.
    assert_eq!(by(&rows, "vapid").status, Status::Warn);

    sqlx::query("INSERT INTO push_vapid_key (singleton, secret) VALUES (TRUE, '\x01')")
        .execute(db.pool())
        .await
        .unwrap();
    let rows = database_checks(&probe, &RealHost).await;
    assert_eq!(by(&rows, "vapid").status, Status::Pass);

    // A migration that was never applied is pending and fails the check.
    sqlx::query(
        "DELETE FROM _sqlx_migrations WHERE version = (SELECT max(version) FROM _sqlx_migrations)",
    )
    .execute(db.pool())
    .await
    .unwrap();
    let rows = database_checks(&probe, &RealHost).await;
    let m = by(&rows, "migrations");
    assert_eq!(m.status, Status::Fail);
    assert!(m.detail.contains("1 pending"), "{}", m.detail);

    // A closed pool is a failed database row, not a panic.
    db.pool().close().await;
    let rows = database_checks(&probe, &RealHost).await;
    assert_eq!(by(&rows, "database").status, Status::Fail);
    db.cleanup().await.unwrap();
}
