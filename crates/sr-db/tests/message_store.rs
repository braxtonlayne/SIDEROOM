//! Postgres-backed message store tests (reactions, nonces, pins, attachments).
//! Run with a scratch database:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test message_store -- --ignored`

use std::sync::Arc;

use sr_chat::repo::{EmojiKey, MessageStore, Page};
use sr_chat::{ChatLimits, ChatService, SendMessage};
use sr_core::{Clock, SnowflakeGenerator, SystemClock, UserId};
use sr_db::chat_repo::{PgChatRepo, PgMessageStore};
use sr_db::{DbConfig, PgPool};

async fn pool() -> PgPool {
    let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
    let pool = sr_db::connect(&DbConfig::new(url)).await.expect("connect");
    sr_db::migrate(&pool).await.expect("migrate");
    pool
}

/// Insert a person + account directly (accounts are owned by the identity crates).
async fn account(pool: &PgPool, ids: &SnowflakeGenerator<Arc<dyn Clock>>) -> UserId {
    let person = ids.generate().get() as i64;
    let acct = ids.generate();
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(person)
        .execute(pool)
        .await
        .unwrap();
    let name = format!("m{}", acct.get());
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) \
         VALUES ($1, $2, $3, $4, 'x')",
    )
    .bind(acct.get() as i64)
    .bind(person)
    .bind(&name[..name.len().min(32)])
    .bind(format!("{name}@example.test"))
    .execute(pool)
    .await
    .unwrap();
    UserId::new(acct.get())
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn reactions_nonces_and_pins_round_trip() {
    let pool = pool().await;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids = SnowflakeGenerator::new(clock.clone(), 7, 7).expect("ids");
    let svc = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock.clone(),
        SnowflakeGenerator::new(clock.clone(), 8, 8).expect("ids"),
        ChatLimits::default(),
    );
    let store = PgMessageStore::new(pool.clone());
    let owner = account(&pool, &ids).await;
    let bp = svc.create_guild(owner, "Store Club").await.unwrap();
    let general = bp.channels[2].id;
    let m = svc
        .send_message(
            owner,
            general,
            SendMessage {
                content: "react to me".into(),
                nonce: Some("abc".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    store
        .put_nonce(general, owner, "abc", m.id, 0)
        .await
        .unwrap();
    let since = clock.now_unix_ms() - 60_000;
    assert_eq!(
        store
            .find_nonce(general, owner, "abc", since)
            .await
            .unwrap(),
        Some(m.id)
    );
    assert_eq!(store.nonce_of(m.id).await.unwrap().as_deref(), Some("abc"));

    let thumbs = EmojiKey::parse("\u{1F44D}").unwrap();
    assert!(
        store
            .add_reaction(general, m.id, owner, &thumbs, clock.now_unix_ms())
            .await
            .unwrap()
    );
    let rows = store.reactions_for(&[m.id], owner).await.unwrap();
    assert_eq!(rows[&m.id][0].count, 1);
    assert!(rows[&m.id][0].me);
    assert!(store.remove_reaction(m.id, owner, &thumbs).await.unwrap());

    let page = store
        .fetch(
            general,
            Page {
                before: None,
                after: None,
                limit: 10,
                ascending: false,
            },
        )
        .await
        .unwrap();
    assert!(page.iter().any(|x| x.id == m.id));
}
