//! Postgres-backed media column tests (guild icon/banner/splash, user avatar/banner hashes). Run with a scratch database:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test media_columns -- --ignored`

use std::sync::Arc;

use sr_chat::admin_rules::GuildPatch;
use sr_chat::{ChatLimits, ChatService};
use sr_core::{Clock, SnowflakeGenerator, SystemClock, UserId};
use sr_db::chat_repo::PgChatRepo;
use sr_db::{DbConfig, PgPool};

async fn pool() -> PgPool {
    let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
    let pool = sr_db::connect(&DbConfig::new(url)).await.expect("connect");
    sr_db::migrate(&pool).await.expect("migrate");
    pool
}

type Ids = SnowflakeGenerator<Arc<dyn Clock>>;

fn service(pool: &PgPool) -> (ChatService<PgChatRepo>, Ids) {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let svc_ids = SnowflakeGenerator::new(clock.clone(), 5, 5).expect("ids");
    let test_ids = SnowflakeGenerator::new(clock.clone(), 6, 6).expect("ids");
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
async fn guild_images_round_trip() {
    let pool = pool().await;
    let (svc, ids) = service(&pool);
    let owner = account(&pool, &ids).await;
    let bp = svc.create_guild(owner, "Media Test").await.unwrap();
    let gid = bp.guild.id;
    assert_eq!(bp.guild.icon_key, None);

    let h = |c: char| c.to_string().repeat(32);
    let g = svc
        .edit_guild(
            owner,
            gid,
            GuildPatch {
                icon_key: Some(Some(h('a'))),
                banner_key: Some(Some(h('b'))),
                splash_key: Some(Some(h('c'))),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(g.icon_key.as_deref(), Some(h('a').as_str()));
    // Read back through a fresh query, not the returned value.
    let (read, _) = svc.get_guild(owner, gid).await.unwrap();
    assert_eq!(read.icon_key.as_deref(), Some(h('a').as_str()));
    assert_eq!(read.banner_key.as_deref(), Some(h('b').as_str()));
    assert_eq!(read.splash_key.as_deref(), Some(h('c').as_str()));
    assert_eq!(
        svc.list_my_guilds(owner).await.unwrap()[0].0.splash_key,
        read.splash_key
    );

    // An edit that leaves the images alone keeps them; None clears one.
    svc.edit_guild(
        owner,
        gid,
        GuildPatch {
            name: Some("Renamed".into()),
            banner_key: Some(None),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let (read, _) = svc.get_guild(owner, gid).await.unwrap();
    assert_eq!(read.icon_key.as_deref(), Some(h('a').as_str()));
    assert_eq!(read.banner_key, None);
    assert_eq!(read.splash_key.as_deref(), Some(h('c').as_str()));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn user_lookup_returns_avatar_and_banner() {
    use sr_chat::repo::UserLookup;
    let pool = pool().await;
    let (_, ids) = service(&pool);
    let user = account(&pool, &ids).await;
    sqlx::query("UPDATE accounts SET avatar_key = $2, banner_key = $3 WHERE id = $1")
        .bind(user.get() as i64)
        .bind("0".repeat(32))
        .bind("1".repeat(32))
        .execute(&pool)
        .await
        .unwrap();
    let found = PgChatRepo::new(pool.clone()).users(&[user]).await.unwrap();
    assert_eq!(
        found[0].avatar_key.as_deref(),
        Some("0".repeat(32).as_str())
    );
    assert_eq!(
        found[0].banner_key.as_deref(),
        Some("1".repeat(32).as_str())
    );
}
