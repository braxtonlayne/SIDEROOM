//! Postgres tests for PAR-FU-003 (D-375, D-376): migration 1360 and the batched emoji lookup of the
//! external-use gate. Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test p26_expressions -- --include-ignored --test-threads=1`

use std::sync::Arc;

use sr_chat::rich_repo::RichStore;
use sr_chat::{ChatLimits, ChatService};
use sr_core::{Clock, GuildId, SnowflakeGenerator, SystemClock, UserId};
use sr_db::PgPool;
use sr_db::chat_repo::PgChatRepo;
use sr_db::rich_repo::PgRichStore;
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
    let name = format!("x{}", acct.get());
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

async fn guild(pool: &PgPool, test_ids: &Ids, name: &str) -> GuildId {
    let owner = account(pool, test_ids).await;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let svc = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock,
        ids(5),
        ChatLimits::default(),
    );
    svc.create_guild(owner, name).await.unwrap().guild.id
}

const BIT: u64 = 1 << 60;

async fn bits(pool: &PgPool, id: i64) -> u64 {
    sqlx::query_scalar::<_, i64>("SELECT permissions FROM roles WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap() as u64
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn new_servers_get_the_bit_and_migration_1360_backfills_old_ones() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(6);
    // A server created after the change carries the bit in @everyone.
    let fresh = guild(&pool, &test_ids, "Fresh").await;
    assert_eq!(bits(&pool, fresh.get() as i64).await & BIT, BIT);

    // An old server: @everyone predates the bit; another role never had it.
    let old = guild(&pool, &test_ids, "Old").await;
    sqlx::query("UPDATE roles SET permissions = permissions & ~$2 WHERE id = $1")
        .bind(old.get() as i64)
        .bind(BIT as i64)
        .execute(&pool)
        .await
        .unwrap();
    let other = test_ids.generate().get() as i64;
    sqlx::query(
        "INSERT INTO roles (id, guild_id, name, position, permissions) VALUES ($1, $2, 'mods', 1, 8192)",
    )
    .bind(other)
    .bind(old.get() as i64)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(bits(&pool, old.get() as i64).await & BIT, 0);

    let migration = include_str!("../migrations/1360_use_expressions_externally.sql");
    sqlx::raw_sql(migration).execute(&pool).await.unwrap();
    assert_eq!(bits(&pool, old.get() as i64).await & BIT, BIT);
    assert_eq!(bits(&pool, other).await, 8192, "other roles are untouched");
    // Idempotent in effect, and it keeps the other bits of @everyone.
    sqlx::raw_sql(migration).execute(&pool).await.unwrap();
    assert_eq!(bits(&pool, old.get() as i64).await & BIT, BIT);
    assert_ne!(bits(&pool, old.get() as i64).await & (1 << 11), 0);
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn guild_emojis_answers_a_batch_in_one_lookup() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(7);
    let g1 = guild(&pool, &test_ids, "One").await;
    let g2 = guild(&pool, &test_ids, "Two").await;
    let creator = account(&pool, &test_ids).await;
    let (open, restricted, off, app) = (
        test_ids.generate().get(),
        test_ids.generate().get(),
        test_ids.generate().get(),
        test_ids.generate().get(),
    );
    for (id, g, name, available) in [
        (open, g1, "open", true),
        (restricted, g2, "restricted", true),
        (off, g2, "off", false),
    ] {
        sqlx::query(
            "INSERT INTO emojis (id, guild_id, name, available, creator_id, media_key) \
             VALUES ($1, $2, $3, $4, $5, 'emoji/x.png')",
        )
        .bind(id as i64)
        .bind(g.get() as i64)
        .bind(name)
        .bind(available)
        .bind(creator.get() as i64)
        .execute(&pool)
        .await
        .unwrap();
    }
    // Role restriction on one emoji (two roles).
    let mut roles = Vec::new();
    for n in 0..2 {
        let r = test_ids.generate().get() as i64;
        sqlx::query(
            "INSERT INTO roles (id, guild_id, name, position, permissions) VALUES ($1, $2, $3, 1, 0)",
        )
        .bind(r)
        .bind(g2.get() as i64)
        .bind(format!("vip{n}"))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO emoji_roles (emoji_id, role_id) VALUES ($1, $2)")
            .bind(restricted as i64)
            .bind(r)
            .execute(&pool)
            .await
            .unwrap();
        roles.push(r as u64);
    }
    // An application emoji is not a server emoji.
    let owner = account(&pool, &test_ids).await;
    let bot = account(&pool, &test_ids).await;
    let application = test_ids.generate().get() as i64;
    sqlx::query(
        "INSERT INTO applications (id, owner_id, bot_user_id, name, client_secret_hash) \
         VALUES ($1, $2, $3, 'Helper', decode(repeat('ab', 32), 'hex'))",
    )
    .bind(application)
    .bind(owner.get() as i64)
    .bind(bot.get() as i64)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO application_emojis (id, application_id, name, media_key) \
         VALUES ($1, $2, 'wave', 'emoji/app/x.png')",
    )
    .bind(app as i64)
    .bind(application)
    .execute(&pool)
    .await
    .unwrap();

    let store = PgRichStore::new(pool.clone());
    let found = store
        .guild_emojis(&[open, restricted, off, app, 424_242])
        .await
        .unwrap();
    assert_eq!(found.len(), 3, "application and unknown ids are omitted");
    assert_eq!(found[&open].guild_id, g1);
    assert!(found[&open].available && found[&open].role_ids.is_empty());
    assert_eq!(found[&restricted].guild_id, g2);
    let mut got: Vec<u64> = found[&restricted]
        .role_ids
        .iter()
        .map(|r| r.get())
        .collect();
    got.sort_unstable();
    roles.sort_unstable();
    assert_eq!(got, roles);
    assert!(!found[&off].available);
    assert!(store.guild_emojis(&[]).await.unwrap().is_empty());
    db.cleanup().await.unwrap();
}
