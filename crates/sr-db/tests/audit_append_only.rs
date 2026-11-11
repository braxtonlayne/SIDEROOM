//! SEC-AUD-088 (D-606): the append-only audit tables refuse TRUNCATE, and `application_audit` and `payment_audit`
//! refuse DELETE too. The D-563 retention purge of `audit_log_entries` stays the one sanctioned delete (its own
//! test is `settings_repo.rs`, `an_audit_purge_that_removed_rows_is_itself_audited`).
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test audit_append_only -- --ignored`

use sr_db::{DbConfig, PgPool};

async fn pool() -> PgPool {
    let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
    let pool = sr_db::connect(&DbConfig::new(url)).await.expect("connect");
    sr_db::migrate(&pool).await.expect("migrate");
    pool
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn every_append_only_audit_table_refuses_truncate() {
    let pool = pool().await;
    for t in [
        "platform_audit_log",
        "audit_log_entries",
        "staff_access_log",
        "invite_credits_ledger",
        "moderation_case_events",
        "moderation_case_evidence",
        "moderation_case_notes",
        "takedown_events",
        "application_audit",
        "payment_audit",
    ] {
        let e = sqlx::query(sqlx::AssertSqlSafe(format!("TRUNCATE {t}")))
            .execute(&pool)
            .await
            .unwrap_err();
        assert!(e.to_string().contains("append-only"), "{t}: {e}");
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn application_and_payment_audit_refuse_update_and_delete() {
    let pool = pool().await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO application_audit (id, application_id, actor_id, action) VALUES (7590001, 1, 1, 'test')",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    // The savepoint keeps the transaction usable after each refused statement.
    for (i, sql) in [
        "DELETE FROM application_audit WHERE id = 7590001",
        "UPDATE application_audit SET action = 'x' WHERE id = 7590001",
    ]
    .into_iter()
    .enumerate()
    {
        sqlx::query(sqlx::AssertSqlSafe(format!("SAVEPOINT s{i}")))
            .execute(&mut *tx)
            .await
            .unwrap();
        let e = sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert!(e.to_string().contains("append-only"), "{sql}: {e}");
        sqlx::query(sqlx::AssertSqlSafe(format!("ROLLBACK TO SAVEPOINT s{i}")))
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    // payment_audit needs an invoice; the foreign key is switched off for the insert only (the test database
    // user is a superuser), then back on so the trigger is live for the DELETE.
    sqlx::query("SET LOCAL session_replication_role = replica")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO payment_audit (invoice_id, at_ms, kind) VALUES (-1, 0, 'test')")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SET LOCAL session_replication_role = origin")
        .execute(&mut *tx)
        .await
        .unwrap();
    let e = sqlx::query("DELETE FROM payment_audit WHERE invoice_id = -1")
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert!(e.to_string().contains("append-only"), "{e}");
    tx.rollback().await.unwrap();
}
