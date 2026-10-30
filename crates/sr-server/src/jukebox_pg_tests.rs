//! The listening room cursor job against real Postgres (MUS-JB-003; run with `SR_TEST_DATABASE_URL`,
//! `--include-ignored`, `--test-threads=1`): the cursor across a loop and across a track with no duration, the
//! 15 second publication, the shelf rule, and that two ticks never both take a room.

use std::sync::Arc;

use sqlx::PgPool;
use sr_chat::{ChatLimits, ChatService};
use sr_core::{Clock, SnowflakeGenerator, SystemClock, UserId};
use sr_db::{chat_repo::PgChatRepo, testing::TestDb};
use sr_events::DomainEvent;

use crate::jukebox_jobs::tick;

struct Fx {
    _db: TestDb,
    pool: PgPool,
    guild: i64,
    channel: i64,
    playlist: i64,
}

async fn account(pool: &PgPool, id: i64, name: &str) {
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $1, $2, $3, 'x')",
    )
    .bind(id)
    .bind(name)
    .bind(format!("{name}@example.test"))
    .execute(pool)
    .await
    .unwrap();
}

async fn mount(pool: &PgPool, id: i64, owner_kind: &str, owner_id: i64) {
    sqlx::query(
        "INSERT INTO storage_mounts (id, owner_kind, owner_id, label, provider, endpoint, bucket, access_key_id, secret_key_enc) \
         VALUES ($1, $2, $3, 'm', 's3', 'https://s3.example.test', 'b', 'k', '\\x00')",
    )
    .bind(id)
    .bind(owner_kind)
    .bind(owner_id)
    .execute(pool)
    .await
    .unwrap();
}

/// A track on `mount` with `duration` (null for an unknown length), and its playlist item at `position`.
async fn item(fx: &Fx, n: i64, mount_id: i64, duration: Option<i32>, position: i32) -> i64 {
    sqlx::query(
        "INSERT INTO music_tracks (id, mount_id, mount_object_id, title, duration_ms) VALUES ($1, $2, $1, $3, $4)",
    )
    .bind(n)
    .bind(mount_id)
    .bind(format!("Song {n}"))
    .bind(duration)
    .execute(&fx.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO music_playlist_items (id, playlist_id, track_id, position) VALUES ($1, $2, $1, $3)",
    )
    .bind(n + 1000)
    .bind(fx.playlist)
    .bind(n)
    .bind(position)
    .execute(&fx.pool)
    .await
    .unwrap();
    n + 1000
}

async fn fx() -> Fx {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids = SnowflakeGenerator::new(clock.clone(), 14, 14).unwrap();
    let owner = ids.generate().get() as i64;
    account(&pool, owner, "owner").await;
    let svc = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock.clone(),
        SnowflakeGenerator::new(clock.clone(), 15, 15).unwrap(),
        ChatLimits::default(),
    );
    let bp = svc
        .create_guild(UserId::new(owner as u64), "Radio")
        .await
        .unwrap();
    let guild = bp.guild.id.get() as i64;
    let channel = ids.generate().get() as i64;
    sqlx::query("INSERT INTO channels (id, guild_id, type, name, position) VALUES ($1, $2, 2, 'radio', 9)")
        .bind(channel)
        .bind(guild)
        .execute(&pool)
        .await
        .unwrap();
    let playlist = ids.generate().get() as i64;
    sqlx::query("INSERT INTO music_playlists (id, owner_guild_id, name) VALUES ($1, $2, 'Loop')")
        .bind(playlist)
        .bind(guild)
        .execute(&pool)
        .await
        .unwrap();
    mount(&pool, 5001, "guild", guild).await;
    Fx {
        _db: db,
        pool,
        guild,
        channel,
        playlist,
    }
}

/// Put a room on the playlist with its cursor `ago_ms` in the past, on `item` at `position_ms`.
async fn room(fx: &Fx, item: Option<i64>, position_ms: i64, ago_ms: i64, published_ago_ms: Option<i64>) {
    sqlx::query(
        "INSERT INTO listening_rooms (channel_id, guild_id, playlist_id, item_id, position_ms, cursor_at, next_due_at, last_published_at) \
         VALUES ($1, $2, $3, $4, $5, now() - make_interval(secs => $6), now() - interval '1 second', \
                 CASE WHEN $7::float8 IS NULL THEN NULL ELSE now() - make_interval(secs => $7::float8) END)",
    )
    .bind(fx.channel)
    .bind(fx.guild)
    .bind(fx.playlist)
    .bind(item)
    .bind(position_ms)
    .bind(ago_ms as f64 / 1000.0)
    .bind(published_ago_ms.map(|m| m as f64 / 1000.0))
    .execute(&fx.pool)
    .await
    .unwrap();
}

async fn cursor(fx: &Fx) -> (Option<i64>, i64, i64) {
    sqlx::query_as::<_, (Option<i64>, i64, i64)>(
        "SELECT item_id, position_ms, seq FROM listening_rooms WHERE channel_id = $1",
    )
    .bind(fx.channel)
    .fetch_one(&fx.pool)
    .await
    .unwrap()
}

fn payload(ev: &DomainEvent) -> &serde_json::Value {
    match ev {
        DomainEvent::ListeningRoomUpdated { payload, .. } => payload,
        other => panic!("unexpected event {other:?}"),
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_cursor_crosses_a_track_boundary_and_a_loop() {
    let f = fx().await;
    let a = item(&f, 1, 5001, Some(60_000), 0).await;
    let b = item(&f, 2, 5001, Some(30_000), 1).await;
    // 50 s into A and 20 s have passed: 10 s left in A, so 10 s into B.
    room(&f, Some(a), 50_000, 20_000, Some(1_000)).await;
    let events = tick(&f.pool).await.unwrap();
    let (it, pos, seq) = cursor(&f).await;
    assert_eq!(it, Some(b));
    assert!((10_000..11_500).contains(&pos), "position was {pos}");
    assert_eq!(seq, 1);
    assert_eq!(events.len(), 1, "a track change is published at once");
    let p = payload(&events[0]);
    assert_eq!(p["now_playing"]["title"], "Song 2");
    assert_eq!(p["next"][0]["title"], "Song 1", "the lap wraps");

    // Another 40 s: B has 20 s left, then A again 20 s in.
    sqlx::query("UPDATE listening_rooms SET cursor_at = now() - interval '40 seconds', next_due_at = now()")
        .execute(&f.pool)
        .await
        .unwrap();
    let events = tick(&f.pool).await.unwrap();
    let (it, pos, seq) = cursor(&f).await;
    assert_eq!(it, Some(a));
    assert!((20_000..21_500).contains(&pos), "position was {pos}");
    assert_eq!(seq, 2);
    assert_eq!(events.len(), 1);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_track_without_a_duration_is_skipped_and_counted() {
    let f = fx().await;
    let a = item(&f, 1, 5001, Some(10_000), 0).await;
    item(&f, 2, 5001, None, 1).await;
    let c = item(&f, 3, 5001, Some(10_000), 2).await;
    room(&f, Some(a), 9_000, 2_000, Some(100)).await;
    let events = tick(&f.pool).await.unwrap();
    let (it, pos, _) = cursor(&f).await;
    assert_eq!(it, Some(c), "the unknown-length track is never scheduled");
    assert!((900..2_500).contains(&pos), "position was {pos}");
    assert_eq!(payload(&events[0])["skipped"], 1);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_quiet_room_publishes_every_fifteen_seconds_not_on_every_tick() {
    let f = fx().await;
    let a = item(&f, 1, 5001, Some(600_000), 0).await;
    room(&f, Some(a), 1_000, 1_000, Some(3_000)).await;
    assert!(tick(&f.pool).await.unwrap().is_empty(), "published 3 s ago, same track");
    let (_, _, seq) = cursor(&f).await;
    assert_eq!(seq, 0);
    sqlx::query("UPDATE listening_rooms SET last_published_at = now() - interval '16 seconds', next_due_at = now()")
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(tick(&f.pool).await.unwrap().len(), 1);
    // Not due again right away.
    assert!(tick(&f.pool).await.unwrap().is_empty());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn only_the_servers_own_shelf_and_the_instance_bucket_are_played() {
    let f = fx().await;
    mount(&f.pool, 5002, "user", 42).await;
    mount(&f.pool, 5003, "guild", f.guild + 1).await;
    mount(&f.pool, 5004, "platform", 0).await;
    let own = item(&f, 1, 5001, Some(10_000), 0).await;
    item(&f, 2, 5002, Some(10_000), 1).await;
    item(&f, 3, 5003, Some(10_000), 2).await;
    item(&f, 4, 5004, Some(10_000), 3).await;
    room(&f, Some(own), 0, 0, None).await;
    let events = tick(&f.pool).await.unwrap();
    let p = payload(&events[0]);
    let titles: Vec<_> = std::iter::once(p["now_playing"]["title"].clone())
        .chain(p["next"].as_array().unwrap().iter().map(|t| t["title"].clone()))
        .collect();
    assert_eq!(titles, vec!["Song 1", "Song 4", "Song 1"], "a member's or another server's bucket is not played");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_empty_playlist_is_announced_once_and_a_dj_stay_freezes_the_cursor() {
    let f = fx().await;
    room(&f, None, 0, 5_000, None).await;
    // Nothing was playing and nothing can: no event, the cursor stays empty.
    assert!(tick(&f.pool).await.unwrap().is_empty());
    assert_eq!(cursor(&f).await.0, None);

    // A DJ's stay: the job leaves the room alone.
    let dj = 7001;
    account(&f.pool, dj, "dj").await;
    sqlx::query("UPDATE listening_rooms SET mode = 'stream', stream_dj_id = $1, next_due_at = now() - interval '1 second'")
        .bind(dj)
        .execute(&f.pool)
        .await
        .unwrap();
    item(&f, 1, 5001, Some(10_000), 0).await;
    assert!(tick(&f.pool).await.unwrap().is_empty());
    assert_eq!(cursor(&f).await.0, None);
    // The stream mode needs a DJ (the table says so).
    assert!(
        sqlx::query("UPDATE listening_rooms SET stream_dj_id = NULL")
            .execute(&f.pool)
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_second_tick_inside_the_first_skips_the_locked_room() {
    let f = fx().await;
    let a = item(&f, 1, 5001, Some(600_000), 0).await;
    room(&f, Some(a), 0, 1_000, None).await;
    let mut tx = f.pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM listening_rooms WHERE channel_id = $1 FOR UPDATE")
        .bind(f.channel)
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    assert!(tick(&f.pool).await.unwrap().is_empty(), "the locked room is another node's");
    tx.rollback().await.unwrap();
    assert_eq!(tick(&f.pool).await.unwrap().len(), 1);
}
