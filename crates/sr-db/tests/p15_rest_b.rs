//! Postgres-backed tests for phase-15 rest-b. Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test p15_rest_b -- --ignored`

use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use sr_auth::store::{AuthStore, DiscordLink};
use sr_db::PgPool;
use sr_db::auth_repo::{IdSource, PgAuthRepo};
use sr_db::auth_store::PgAuthStore;
use sr_db::testing::TestDb;

async fn account(pool: &PgPool, n: i64) -> i64 {
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(n)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) \
         VALUES ($1, $1, $2, $3, 'x')",
    )
    .bind(n)
    .bind(format!("user{n}"))
    .bind(format!("user{n}@example.test"))
    .execute(pool)
    .await
    .unwrap();
    n
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn discord_identity_links_unlinks_and_is_never_freed() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let next = Arc::new(AtomicI64::new(50_000));
    let ids: IdSource = Arc::new(move || next.fetch_add(1, Ordering::SeqCst));
    let store = PgAuthStore::new(
        pool.clone(),
        PgAuthRepo::new(pool.clone(), ids.clone(), [0u8; 32]),
        ids,
    );
    let a = account(&pool, 1).await;
    let b = account(&pool, 2).await;

    assert!(!store.discord_linked(a).await.unwrap());
    assert_eq!(
        store.link_discord(a, 100, "d-1").await.unwrap(),
        DiscordLink::Linked
    );
    assert!(store.discord_linked(a).await.unwrap());
    assert_eq!(store.find_by_discord("d-1").await.unwrap().unwrap().id, a);
    // One identity per account, and an identity belongs to one account.
    assert_eq!(
        store.link_discord(a, 101, "d-2").await.unwrap(),
        DiscordLink::AlreadyLinked
    );
    assert_eq!(
        store.link_discord(b, 102, "d-1").await.unwrap(),
        DiscordLink::TakenByOther
    );

    assert!(store.unlink_discord(a).await.unwrap());
    assert!(!store.unlink_discord(a).await.unwrap());
    assert!(!store.discord_linked(a).await.unwrap());
    assert!(store.find_by_discord("d-1").await.unwrap().is_none());
    // The record stays: nobody else can claim the identity, the owner can take it back.
    assert_eq!(
        store.link_discord(b, 103, "d-1").await.unwrap(),
        DiscordLink::TakenByOther
    );
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM external_identities WHERE provider_user_id = 'd-1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rows, 1);
    // After unlinking, a different identity may be linked.
    assert_eq!(
        store.link_discord(a, 104, "d-2").await.unwrap(),
        DiscordLink::Linked
    );
    assert!(store.unlink_discord(a).await.unwrap());
    assert_eq!(
        store.link_discord(a, 105, "d-1").await.unwrap(),
        DiscordLink::Linked
    );
    assert_eq!(store.find_by_discord("d-1").await.unwrap().unwrap().id, a);
    db.cleanup().await.unwrap();
}
