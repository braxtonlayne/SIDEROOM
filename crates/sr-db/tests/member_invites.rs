//! DB-backed tests of the member invite rules (G-57). Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test member_invites -- --ignored`

use sr_auth::store::InviteBlock;
use sr_db::{PgPool, member_invites, testing::TestDb};

const NOW: i64 = 2_000_000_000;
const WEEK: i64 = 7 * 86_400;

async fn account(pool: &PgPool, id: i64, verified: bool, age_days: i64) {
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash, email_verified_at, created_at) \
         VALUES ($1, $1, $2, $3, 'x', CASE WHEN $4 THEN now() END, to_timestamp($5))",
    )
    .bind(id)
    .bind(format!("u{id}"))
    .bind(format!("u{id}@example.test"))
    .bind(verified)
    .bind((NOW - age_days * 86_400) as f64)
    .execute(pool)
    .await
    .unwrap();
}

async fn credits(pool: &PgPool, account: i64, n: i32) {
    sqlx::query(
        "INSERT INTO invite_credits_ledger (id, account_id, delta, reason) VALUES ($1, $2, $3, 'initial_grant')",
    )
    .bind(account * 10)
    .bind(account)
    .bind(n)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn creating_and_revoking_follow_the_invite_rules() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    let _ = WEEK;
    account(pool, 1, true, 30).await;
    credits(pool, 1, 2).await;

    // Two credits: two open codes, then the third is refused (D-026).
    let o = member_invites::overview(pool, 1, NOW).await.unwrap();
    assert_eq!((o.balance, o.open.len(), o.blocked), (2, 0, None));
    assert_eq!(
        member_invites::create(pool, 1, 101, "AAAAAAAAAA", "for Sam", NOW)
            .await
            .unwrap(),
        Ok(())
    );
    assert_eq!(
        member_invites::create(pool, 1, 102, "BBBBBBBBBB", "", NOW)
            .await
            .unwrap(),
        Ok(())
    );
    assert_eq!(
        member_invites::create(pool, 1, 103, "CCCCCCCCCC", "", NOW)
            .await
            .unwrap(),
        Err(InviteBlock::NoCredits)
    );
    let o = member_invites::overview(pool, 1, NOW).await.unwrap();
    assert_eq!(o.open.len(), 2);
    assert_eq!(o.blocked, Some(InviteBlock::NoCredits));
    assert!(o.open.iter().any(|i| i.note == "for Sam"));

    // Revoking frees the credit again; only the issuer can revoke, and only an open code.
    assert!(!member_invites::revoke(pool, 2, "AAAAAAAAAA").await.unwrap());
    assert!(member_invites::revoke(pool, 1, "AAAAAAAAAA").await.unwrap());
    assert!(!member_invites::revoke(pool, 1, "AAAAAAAAAA").await.unwrap());
    assert_eq!(
        member_invites::create(pool, 1, 104, "DDDDDDDDDD", "", NOW)
            .await
            .unwrap(),
        Ok(())
    );

    // A redeemed code moves to the redeemed list with who joined.
    account(pool, 5, true, 1).await;
    sqlx::query(
        "UPDATE platform_invites SET state = 'redeemed', redeemed_by_account_id = 5, redeemed_at = now() \
         WHERE code = 'BBBBBBBBBB'",
    )
    .execute(pool)
    .await
    .unwrap();
    let o = member_invites::overview(pool, 1, NOW).await.unwrap();
    assert_eq!(o.redeemed.len(), 1);
    assert_eq!(o.redeemed[0].redeemed_by.as_ref().unwrap().1, "u5");
    assert!(!member_invites::revoke(pool, 1, "BBBBBBBBBB").await.unwrap());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn unverified_new_and_frozen_accounts_cannot_create() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool();
    account(pool, 1, false, 30).await;
    account(pool, 2, true, 2).await;
    account(pool, 3, true, 30).await;
    for a in [1, 2, 3] {
        credits(pool, a, 5).await;
    }
    let make = |a: i64| async move {
        member_invites::create(pool, a, a * 100, &format!("{a:0>10}"), "", NOW)
            .await
            .unwrap()
    };
    assert_eq!(make(1).await, Err(InviteBlock::Unverified));
    assert_eq!(make(2).await, Err(InviteBlock::TooNew { days: 7 }));
    // The freeze threshold (default 2) on the Person blocks creation, reported as frozen.
    sqlx::query("UPDATE persons SET suspicion_level = 2 WHERE id = 3")
        .execute(pool)
        .await
        .unwrap();
    assert_eq!(make(3).await, Err(InviteBlock::Frozen));
    sqlx::query("UPDATE persons SET suspicion_level = 0 WHERE id = 3")
        .execute(pool)
        .await
        .unwrap();
    // The account cap (D-014) blocks creation too.
    sqlx::query(
        "INSERT INTO platform_settings (key, value) VALUES ('accounts.cap', '1'::jsonb) \
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
    )
    .execute(pool)
    .await
    .unwrap();
    assert_eq!(make(3).await, Err(InviteBlock::AtCap));
}
