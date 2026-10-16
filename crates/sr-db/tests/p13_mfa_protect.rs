//! Postgres-backed tests for the Server Protection settings (G-95, D-224). Run with a scratch
//! Postgres: `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test p13_mfa_protect -- --ignored`

use std::sync::Arc;

use sr_chat::settings::repo::CommunityRepository;
use sr_chat::social::dm::{RequestReason, RequestState};
use sr_chat::social::repo::DmRepository;
use sr_chat::{ChatLimits, ChatService};
use sr_core::{ChannelId, Clock, GuildId, SnowflakeGenerator, SystemClock, UserId};
use sr_db::PgPool;
use sr_db::chat_repo::PgChatRepo;
use sr_db::settings_repo::PgSettingsRepo;
use sr_db::social_repo::PgSocialRepo;
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

async fn guild(pool: &PgPool, owner: UserId) -> GuildId {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let svc = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock,
        ids(5),
        ChatLimits::default(),
    );
    svc.create_guild(owner, "Protected").await.unwrap().guild.id
}

async fn join(pool: &PgPool, g: GuildId, u: UserId) {
    sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
        .bind(g.get() as i64)
        .bind(u.get() as i64)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn shared_protection_reads_every_shared_server() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(6);
    let a = account(&pool, &test_ids).await;
    let b = account(&pool, &test_ids).await;
    let repo = PgSocialRepo::new(pool.clone());
    let settings = PgSettingsRepo::new(pool.clone());

    // No shared server: nothing applies.
    let none = repo.shared_protection(a, b).await.unwrap();
    assert_eq!((none.shared, none.spam_filter_all), (0, false));

    // Two shared servers; only one filters: not "every" server.
    let (g1, g2) = (guild(&pool, a).await, guild(&pool, a).await);
    join(&pool, g1, b).await;
    join(&pool, g2, b).await;
    let mut c1 = settings.get_community(g1).await.unwrap();
    c1.dm_spam_filter = true;
    c1.warn_new_account_days = 14;
    settings.put_community(&c1).await.unwrap();
    let p = repo.shared_protection(a, b).await.unwrap();
    assert_eq!(p.shared, 2);
    assert!(!p.spam_filter_all && !p.explicit_media_any);
    assert_eq!(p.warn_new_account_days, 14);

    // The second server filters too and turns on the media filter and a larger threshold.
    let mut c2 = settings.get_community(g2).await.unwrap();
    c2.dm_spam_filter = true;
    c2.explicit_media_filter_dms = true;
    c2.warn_new_account_days = 30;
    settings.put_community(&c2).await.unwrap();
    let p = repo.shared_protection(a, b).await.unwrap();
    assert!(p.spam_filter_all && p.explicit_media_any);
    assert_eq!(p.warn_new_account_days, 30, "the largest threshold wins");
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn request_reason_round_trips_and_hide_suspicious_links_is_on_the_guild() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(6);
    let a = account(&pool, &test_ids).await;
    let b = account(&pool, &test_ids).await;
    let repo = PgSocialRepo::new(pool.clone());
    let ch = ChannelId::new(test_ids.generate().get());
    repo.insert_dm(ch, a, b).await.unwrap();
    assert_eq!(repo.get_request_reason(b, ch).await.unwrap(), None);
    repo.set_request_state(b, ch, RequestState::Pending)
        .await
        .unwrap();
    repo.set_request_reason(b, ch, Some(RequestReason::NewAccount))
        .await
        .unwrap();
    assert_eq!(
        repo.get_request_reason(b, ch).await.unwrap(),
        Some(RequestReason::NewAccount)
    );
    repo.set_request_reason(b, ch, None).await.unwrap();
    assert_eq!(repo.get_request_reason(b, ch).await.unwrap(), None);

    // Every member sees `hide_suspicious_links` on the guild.
    use sr_chat::repo::GuildRepository;
    let g = guild(&pool, a).await;
    let chat = PgChatRepo::new(pool.clone());
    assert!(
        !chat
            .get_guild(g)
            .await
            .unwrap()
            .unwrap()
            .hide_suspicious_links
    );
    let settings = PgSettingsRepo::new(pool.clone());
    let mut c = settings.get_community(g).await.unwrap();
    c.hide_suspicious_links = true;
    settings.put_community(&c).await.unwrap();
    assert!(
        chat.get_guild(g)
            .await
            .unwrap()
            .unwrap()
            .hide_suspicious_links
    );
    db.cleanup().await.unwrap();
}
