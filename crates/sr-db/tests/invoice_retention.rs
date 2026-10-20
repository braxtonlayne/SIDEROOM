//! DB-backed test of the invoice detach (N-08, D-325). Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test invoice_retention -- --ignored`

use sr_db::invoice_retention::detach_expired;
use sr_db::testing::TestDb;

/// 2026-06-15T00:00:00Z
const NOW: i64 = 1_781_481_600_000;
/// 2015-12-31T12:00:00Z and 2016-01-02T12:00:00Z
const END_2015: i64 = 1_451_563_200_000;
const EARLY_2016: i64 = 1_451_736_000_000;
const YEAR_2010: i64 = 1_262_304_000_000;

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn only_final_invoices_older_than_the_period_lose_their_member() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    sqlx::query("INSERT INTO persons (id) VALUES (1)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES (1, 1, 'u1', 'u1@example.test', 'x')")
        .execute(pool).await.unwrap();
    sqlx::query("INSERT INTO payment_wallets (id, network, descriptor, fingerprint) VALUES (8, 'regtest', 'wpkh(tpubD6NzVbkrYhZ4/0/*)', 'abcd1234')")
        .execute(pool).await.unwrap();
    // id, state, created_ms, credited_ms
    let rows: [(i64, &str, i64, Option<i64>); 5] = [
        (1, "confirmed", END_2015 - 1000, Some(END_2015)),
        (2, "confirmed", EARLY_2016 - 1000, Some(EARLY_2016)),
        (3, "open", YEAR_2010, None),
        (4, "expired", END_2015, None),
        (5, "underpaid", YEAR_2010, None),
    ];
    for (i, (id, state, created, credited)) in rows.iter().enumerate() {
        sqlx::query("INSERT INTO payment_invoices (id, wallet_id, user_id, purpose, usd_cents, sats, rate_cents_per_btc, rate_source, address, derivation_index, network, confirmations_required, state, created_ms, credited_ms, expires_ms, quote_expires_ms, next_check_ms) VALUES ($1, 8, 1, 'donate', 500, 5000, 10000000, 'test', $2, $3, 'regtest', 1, $4, $5, $6, 2, 3, 4)")
            .bind(id)
            .bind(format!("bcrt1q{id}"))
            .bind(i32::try_from(i).unwrap())
            .bind(state)
            .bind(created)
            .bind(credited)
            .execute(pool)
            .await
            .unwrap();
    }
    // Ten years: kept through the end of the year of payment plus ten, so 2015 goes and 2016 stays.
    let n = detach_expired(pool, 10, NOW).await.unwrap();
    assert_eq!(n, 2, "the 2015 payment and the 2015 expired invoice");
    let detached: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM payment_invoices WHERE user_id IS NULL ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(detached, [1, 4]);
    // Rows, amounts and addresses stay for the books; in-flight invoices keep their member.
    let kept: (i64, String) =
        sqlx::query_as("SELECT usd_cents, address FROM payment_invoices WHERE id = 1")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(kept, (500, "bcrt1q1".to_owned()));
    // A shorter period reaches the 2016 payment too; running it again changes nothing.
    assert_eq!(detach_expired(pool, 9, NOW).await.unwrap(), 1);
    assert_eq!(detach_expired(pool, 9, NOW).await.unwrap(), 0);
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM payment_invoices WHERE user_id = 1")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(left, 2, "open and underpaid invoices are never detached");
}
