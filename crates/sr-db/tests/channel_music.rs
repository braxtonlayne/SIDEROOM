//! Migration 1560 (MUS-JB-001): the voice channel music settings. Run with a scratch database:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test channel_music -- --include-ignored --test-threads=1`

use std::sync::Arc;

use sr_chat::repo::ChannelRepository;
use sr_chat::{ChatLimits, ChatService};
use sr_core::{ChannelId, Clock, SnowflakeGenerator, SystemClock, UserId};
use sr_db::chat_repo::PgChatRepo;
use sr_db::testing::TestDb;

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_voice_channel_plays_music_at_256_by_default_and_keeps_an_edit() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids = SnowflakeGenerator::new(clock.clone(), 21, 21).unwrap();
    let owner = ids.generate().get() as i64;
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) \
         VALUES ($1, $1, 'dj', 'dj@example.test', 'x')",
    )
    .bind(owner)
    .execute(&pool)
    .await
    .unwrap();
    let svc = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock.clone(),
        SnowflakeGenerator::new(clock.clone(), 22, 22).unwrap(),
        ChatLimits::default(),
    );
    let bp = svc
        .create_guild(UserId::new(owner as u64), "Radio")
        .await
        .unwrap();
    let channel = ids.generate().get() as i64;
    sqlx::query("INSERT INTO channels (id, guild_id, type, name) VALUES ($1, $2, 2, 'radio')")
        .bind(channel)
        .bind(bp.guild.id.get() as i64)
        .execute(&pool)
        .await
        .unwrap();

    let repo = PgChatRepo::new(pool.clone());
    let id = ChannelId::new(channel as u64);
    let mut c = repo.get_channel(id).await.unwrap().unwrap();
    assert!(c.music_allowed);
    assert_eq!(c.music_bitrate_kbps, 256);

    c.music_allowed = false;
    c.music_bitrate_kbps = 320;
    repo.update_channel(&c).await.unwrap();
    let back = repo.get_channel(id).await.unwrap().unwrap();
    assert!(!back.music_allowed);
    assert_eq!(back.music_bitrate_kbps, 320);

    // The CHECK holds the range 32..=320 whatever the caller does.
    for bad in [31_i16, 321] {
        let r = sqlx::query("UPDATE channels SET music_bitrate_kbps = $2 WHERE id = $1")
            .bind(channel)
            .bind(bad)
            .execute(&pool)
            .await;
        assert!(r.is_err(), "{bad} must be refused");
    }
}
