//! Postgres-backed tests for the phase-13 voice items. Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test p13_voice -- --ignored`

use std::sync::Arc;

use sr_chat::{ChatLimits, ChatService};
use sr_core::{Clock, GuildId, SnowflakeGenerator, SystemClock, UserId};
use sr_db::PgPool;
use sr_db::chat_repo::PgChatRepo;
use sr_db::testing::TestDb;

type Ids = SnowflakeGenerator<Arc<dyn Clock>>;

fn ids(worker: u8) -> Ids {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    SnowflakeGenerator::new(clock, worker, worker).expect("ids")
}

async fn account(pool: &PgPool, ids: &Ids) -> UserId {
    let person = ids.generate().get() as i64;
    let acct = ids.generate();
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(person)
        .execute(pool)
        .await
        .unwrap();
    let name = format!("u{}", acct.get());
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $2, $3, $4, 'x')",
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

async fn guild(pool: &PgPool, test_ids: &Ids) -> GuildId {
    let owner = account(pool, test_ids).await;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let svc = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock,
        ids(5),
        ChatLimits::default(),
    );
    svc.create_guild(owner, "Voice Club")
        .await
        .unwrap()
        .guild
        .id
}

const STATUS_BIT: u64 = 1 << 48;

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn migration_0860_gives_existing_servers_set_voice_channel_status() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(6);
    let g = guild(&pool, &test_ids).await;
    // New servers get it from the defaults; an old one never had it.
    sqlx::query("UPDATE roles SET permissions = permissions & ~$2 WHERE id = $1")
        .bind(g.get() as i64)
        .bind(STATUS_BIT as i64)
        .execute(&pool)
        .await
        .unwrap();
    let other = test_ids.generate().get() as i64;
    sqlx::query(
        "INSERT INTO roles (id, guild_id, name, position, permissions) VALUES ($1, $2, 'mods', 1, 8192)",
    )
    .bind(other)
    .bind(g.get() as i64)
    .execute(&pool)
    .await
    .unwrap();
    let bits = |id: i64| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>("SELECT permissions FROM roles WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap() as u64
        }
    };
    assert_eq!(bits(g.get() as i64).await & STATUS_BIT, 0);
    let sql = include_str!("../migrations/0860_voice_channel_status_permission.sql");
    sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    assert_eq!(bits(g.get() as i64).await & STATUS_BIT, STATUS_BIT);
    assert_eq!(bits(other).await, 8192, "other roles are untouched");
    // Idempotent.
    sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    assert_eq!(bits(g.get() as i64).await & STATUS_BIT, STATUS_BIT);
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn new_servers_start_with_set_voice_channel_status_for_everyone() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(7);
    let g = guild(&pool, &test_ids).await;
    let bits: i64 = sqlx::query_scalar("SELECT permissions FROM roles WHERE id = $1")
        .bind(g.get() as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(bits as u64 & STATUS_BIT, STATUS_BIT);
    db.cleanup().await.unwrap();
}
