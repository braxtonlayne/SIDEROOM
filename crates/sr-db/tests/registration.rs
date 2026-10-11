//! DB-backed tests for invite redemption. Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test registration -- --ignored`
//! The concurrency test counts all active accounts, so run it without other
//! writers on that database.

use std::sync::atomic::{AtomicI64, Ordering};

use sr_auth::invite::InviteCode;
use sr_db::{
    DbConfig, PgPool,
    accounts::{self, NewAccount},
    registration::{NewRegistration, Rejection, register_with_invite},
};

fn uid() -> i64 {
    static N: AtomicI64 = AtomicI64::new(0);
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + N.fetch_add(1, Ordering::Relaxed) % 1000
}

async fn pool() -> PgPool {
    let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
    let pool = sr_db::connect(&DbConfig::new(url)).await.expect("connect");
    sr_db::migrate(&pool).await.expect("migrate");
    pool
}

/// An active issuer with `credits` credits.
async fn issuer(pool: &PgPool, credits: i32) -> i64 {
    let id = uid();
    let name = format!("i{id}");
    let ok = accounts::create_account(
        pool,
        &NewAccount {
            flags: 0,
            id,
            person_id: uid(),
            username: &name[..name.len().min(32)],
            display_name: None,
            email: &format!("{name}@example.test"),
            password_hash: "x",
            state: "active",
            email_verified: true,
            initial_credits: credits,
        },
        uid(),
    )
    .await
    .unwrap();
    assert!(ok);
    id
}

async fn invite(pool: &PgPool, issuer: i64) -> InviteCode {
    let code = InviteCode::generate().unwrap();
    sqlx::query("INSERT INTO platform_invites (id, code, issuer_account_id) VALUES ($1, $2, $3)")
        .bind(uid())
        .bind(code.canonical())
        .bind(issuer)
        .execute(pool)
        .await
        .unwrap();
    code
}

fn reg(code: &InviteCode, cap: i64, tag: &str) -> NewRegistration {
    let id = uid();
    NewRegistration {
        discord_id: None,
        identity_id: uid(),
        account_id: id,
        person_id: uid(),
        ledger_ids: [uid(), uid()],
        code: code.canonical().to_owned(),
        username: format!("{tag}{id}").chars().take(32).collect(),
        display_name: None,
        email: format!("{tag}{id}@example.test"),
        password_hash: "x".into(),
        account_cap: cap,
        initial_credits: 5,
    }
}

async fn balance(pool: &PgPool, account: i64) -> i64 {
    sqlx::query_scalar(
        "SELECT COALESCE(SUM(delta), 0)::BIGINT FROM invite_credits_ledger WHERE account_id = $1",
    )
    .bind(account)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn redeems_once_and_writes_the_ledger() {
    let pool = pool().await;
    let issuer = issuer(&pool, 5).await;
    let code = invite(&pool, issuer).await;
    let r = reg(&code, i64::MAX, "a");
    let ok = register_with_invite(&pool, &r).await.unwrap().unwrap();
    assert_eq!(ok.issuer_id, issuer);
    assert_eq!(balance(&pool, issuer).await, 4);
    assert_eq!(balance(&pool, r.account_id).await, 5);

    let again = register_with_invite(&pool, &reg(&code, i64::MAX, "b"))
        .await
        .unwrap();
    assert_eq!(again, Err(Rejection::InviteUnavailable));
    assert_eq!(balance(&pool, issuer).await, 4);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn rejections_write_nothing() {
    let pool = pool().await;
    let broke = issuer(&pool, 0).await;
    let code = invite(&pool, broke).await;
    assert_eq!(
        register_with_invite(&pool, &reg(&code, i64::MAX, "c"))
            .await
            .unwrap(),
        Err(Rejection::IssuerOutOfCredits)
    );

    let rich = issuer(&pool, 5).await;
    let code = invite(&pool, rich).await;
    assert_eq!(
        register_with_invite(&pool, &reg(&code, 0, "d"))
            .await
            .unwrap(),
        Err(Rejection::AtCap)
    );
    let mut dup = reg(&code, i64::MAX, "e");
    dup.username = format!("i{rich}").chars().take(32).collect();
    assert_eq!(
        register_with_invite(&pool, &dup).await.unwrap(),
        Err(Rejection::UsernameTaken)
    );
    // The invite survived all of that.
    assert!(
        register_with_invite(&pool, &reg(&code, i64::MAX, "f"))
            .await
            .unwrap()
            .is_ok()
    );
}

/// ENT-INV-010: 50 parallel redemptions of different codes at cap-1 produce
/// exactly one success.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn fifty_parallel_redemptions_at_cap_minus_one() {
    let pool = pool().await;
    let issuer = issuer(&pool, 500).await;
    let active: i64 = sqlx::query_scalar("SELECT count(*) FROM accounts WHERE state = 'active'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let cap = active + 1;
    let mut codes = vec![];
    for _ in 0..50 {
        codes.push(invite(&pool, issuer).await);
    }
    let mut tasks = vec![];
    for (i, code) in codes.iter().enumerate() {
        let pool = pool.clone();
        let r = reg(code, cap, &format!("p{i}x"));
        tasks.push(tokio::spawn(async move {
            register_with_invite(&pool, &r).await.unwrap()
        }));
    }
    let mut ok = 0;
    for t in tasks {
        match t.await.unwrap() {
            Ok(_) => ok += 1,
            Err(e) => assert_eq!(e, Rejection::AtCap),
        }
    }
    assert_eq!(ok, 1);
    assert_eq!(balance(&pool, issuer).await, 499);
}
