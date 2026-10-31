//! Postgres tests of the bridge model. Run with a scratch Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-bridge --test db -- --ignored --test-threads=1`

#[path = "support/mod.rs"]
mod support;
use support::*;

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_fresh_server_has_no_bridge_rows() {
    let db = TestDb::new().await.unwrap();
    let owner = account(db.pool()).await;
    let (g, _c) = guild(db.pool(), owner).await;
    for table in [
        "bridges",
        "bridge_channel_maps",
        "bridge_message_map",
        "bridge_optouts",
        "bridge_outbox",
        "bridge_teardown_residue",
    ] {
        let n: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(n, 0, "{table} starts empty");
    }
    assert!(
        sr_bridge::model::get_bridge(db.pool(), g)
            .await
            .unwrap()
            .is_none()
    );
    let _ = db.cleanup().await;
}
