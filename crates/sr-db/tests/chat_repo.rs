//! Postgres-backed chat tests. Run with a scratch database:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test chat_repo -- --ignored`

use std::sync::Arc;

use sr_chat::domain::{ChannelType, Visibility};
use sr_chat::error::ChatError;
use sr_chat::message_rules::EditMessageInput;
use sr_chat::{ChatLimits, ChatService, SendMessage};
use sr_core::{Clock, SnowflakeGenerator, SystemClock, UserId};
use sr_db::chat_repo::PgChatRepo;
use sr_db::{DbConfig, PgPool};

async fn pool() -> PgPool {
    let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
    let pool = sr_db::connect(&DbConfig::new(url)).await.expect("connect");
    sr_db::migrate(&pool).await.expect("migrate");
    pool
}

fn service(pool: &PgPool) -> (ChatService<PgChatRepo>, SnowflakeGenerator<Arc<dyn Clock>>) {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    // Distinct worker/process ids keep test-created ids apart from the service's.
    let svc_ids = SnowflakeGenerator::new(clock.clone(), 3, 3).expect("ids");
    let test_ids = SnowflakeGenerator::new(clock.clone(), 4, 4).expect("ids");
    (
        ChatService::new(
            PgChatRepo::new(pool.clone()),
            clock,
            svc_ids,
            ChatLimits::default(),
        ),
        test_ids,
    )
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
    let name = format!("u{}", acct.get());
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
async fn message_partitions_exist_for_the_current_month() {
    let pool = pool().await;
    let name: String = sqlx::query_scalar("SELECT sr_ensure_message_partition(CURRENT_DATE)")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(name.starts_with("messages_"));
    // Idempotent.
    let again: String = sqlx::query_scalar("SELECT sr_ensure_message_partition(CURRENT_DATE)")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(name, again);
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_inherits WHERE inhparent = 'messages'::regclass",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(n >= 13);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn guild_message_lifecycle() {
    let pool = pool().await;
    let (svc, ids) = service(&pool);
    let owner = account(&pool, &ids).await;
    let bp = svc.create_guild(owner, "Integration Club").await.unwrap();
    let general = bp.channels[2].id;

    let extra = svc
        .create_channel(
            owner,
            bp.guild.id,
            sr_chat::channel_rules::NewChannel::new(ChannelType::Text, "Side Room"),
        )
        .await
        .unwrap();
    assert_eq!(extra.name.as_deref(), Some("side-room"));

    let m = svc
        .send_message(
            owner,
            general,
            SendMessage {
                content: "hello".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let plan = svc
        .edit_message(
            owner,
            general,
            m.id,
            EditMessageInput {
                content: Some("hello world".into()),
                flags: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(plan.history_previous_content.as_deref(), Some("hello"));
    let edits: i64 = sqlx::query_scalar("SELECT count(*) FROM message_edits WHERE message_id = $1")
        .bind(m.id.get() as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(edits, 1);

    svc.pin_message(owner, general, m.id).await.unwrap();
    svc.unpin_message(owner, general, m.id).await.unwrap();

    let del = svc.delete_message(owner, general, m.id).await.unwrap();
    assert_eq!(del.visibility, Visibility::UserDeleted);
    let (vis, purge): (String, bool) = sqlx::query_as(
        "SELECT visibility::text, purge_after IS NOT NULL FROM messages WHERE id = $1",
    )
    .bind(m.id.get() as i64)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((vis.as_str(), purge), ("user_deleted", true));
    assert!(matches!(
        svc.delete_message(owner, general, m.id).await,
        Err(ChatError::UnknownMessage)
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn non_member_is_refused() {
    let pool = pool().await;
    let (svc, ids) = service(&pool);
    let owner = account(&pool, &ids).await;
    let outsider = account(&pool, &ids).await;
    let bp = svc.create_guild(owner, "Private Club").await.unwrap();
    let err = svc
        .send_message(
            outsider,
            bp.channels[2].id,
            SendMessage {
                content: "hi".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ChatError::MissingAccess));
}
