//! Postgres tests for the voice jukebox (MUS-JB-002, migration 1570): the queue, votes, listeners and the
//! session lock round trip through `PgJukebox` and the rules.
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test jukebox_repo -- --include-ignored --test-threads=1`

use sr_chat::jukebox::model::{ItemState, NewTrack};
use sr_chat::jukebox::service::{Actor, JukeboxService};
use sr_core::Clock;
use sr_db::jukebox_repo::PgJukebox;
use sr_db::testing::TestDb;
use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

const GUILD: i64 = 50;
const CHANNEL: i64 = 60;
const USERS: [i64; 3] = [1, 2, 3];

struct Clk(AtomicI64);
impl Clock for Clk {
    fn now_unix_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

async fn world() -> (TestDb, JukeboxService, Arc<Clk>) {
    let db = TestDb::new().await.expect("db");
    let pool = db.pool();
    for id in USERS {
        sqlx::query("INSERT INTO persons (id) VALUES ($1)")
            .bind(id + 100_000)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $2, $3, $4, 'x')",
        )
        .bind(id)
        .bind(id + 100_000)
        .bind(format!("u{id}"))
        .bind(format!("u{id}@example.test"))
        .execute(pool)
        .await
        .unwrap();
    }
    sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Room', 1)")
        .bind(GUILD)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO channels (id, guild_id, type, name) VALUES ($1, $2, 2, 'music')")
        .bind(CHANNEL)
        .bind(GUILD)
        .execute(pool)
        .await
        .unwrap();
    let clock = Arc::new(Clk(AtomicI64::new(10_000_000)));
    let n = Arc::new(AtomicI64::new(1000));
    let svc = JukeboxService::new(
        Arc::new(PgJukebox::new(pool.clone())),
        clock.clone(),
        Arc::new(move || n.fetch_add(1, Ordering::SeqCst)),
    );
    (db, svc, clock)
}

fn t(n: i64, mount: i64, len: i64) -> NewTrack {
    NewTrack {
        track_id: n,
        mount_id: mount,
        key: format!("m{mount}/{n}.flac"),
        title: format!("Song {n}"),
        artist: "Band".into(),
        duration_ms: Some(len),
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_queue_round_trips_and_plays_in_order() {
    let (_db, svc, clock) = world().await;
    assert!(svc.state(CHANNEL, 50).await.unwrap().is_none());
    let d = svc
        .enqueue(CHANNEL, GUILD, 1, &[t(1, 7, 5_000), t(2, 8, 5_000)], 50)
        .await
        .unwrap();
    assert_eq!(d.snapshot.doc.items.len(), 2);
    assert_eq!(d.snapshot.doc.current().unwrap().mount_id, 7);
    // The stored mount id and key are what the presign is made from at play time.
    assert_eq!(d.snapshot.doc.current().unwrap().key, "m7/1.flac");
    clock.0.store(10_007_000, Ordering::SeqCst);
    let (snap, changed) = svc.state(CHANNEL, 50).await.unwrap().unwrap();
    assert!(changed);
    assert_eq!(snap.doc.current().unwrap().mount_id, 8);
    assert_eq!(snap.position_ms, 2_000);
    let (_, again) = svc.state(CHANNEL, 50).await.unwrap().unwrap();
    assert!(!again, "settling is stored: a second read changes nothing");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn votes_listeners_and_the_dj_persist_and_the_threshold_skips_once() {
    let (_db, svc, _clock) = world().await;
    svc.enqueue(CHANNEL, GUILD, 1, &[t(1, 7, 600_000), t(2, 7, 1_000)], 50)
        .await
        .unwrap();
    svc.listen(CHANNEL, GUILD, 2, 50).await.unwrap();
    svc.listen(CHANNEL, GUILD, 3, 50).await.unwrap();
    // Three live listeners at 50%: two votes.
    let v = svc.vote_skip(CHANNEL, GUILD, 2, 50).await.unwrap();
    assert_eq!((v.value.votes, v.value.needed, v.value.skipped), (1, 2, false));
    let mine = Actor { user: 1, manage: false };
    svc.set_dj(CHANNEL, GUILD, mine, Some(1), 50).await.unwrap();
    let v = svc.vote_skip(CHANNEL, GUILD, 3, 50).await.unwrap();
    assert!(v.value.skipped);
    let (snap, _) = svc.state(CHANNEL, 50).await.unwrap().unwrap();
    assert_eq!(snap.doc.current().unwrap().track_id, Some(2));
    assert!(snap.doc.votes.is_empty());
    assert_eq!(snap.doc.session.dj, Some(1));
    let skipped = snap
        .doc
        .items
        .iter()
        .filter(|i| i.state == ItemState::Skipped)
        .count();
    assert_eq!(skipped, 1);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn two_racing_ends_advance_the_queue_once() {
    let (_db, svc, clock) = world().await;
    let d = svc
        .enqueue(CHANNEL, GUILD, 1, &[t(1, 7, 1_000), t(2, 7, 100_000), t(3, 7, 100_000)], 50)
        .await
        .unwrap();
    let first = d.snapshot.doc.session.current.unwrap();
    clock.0.store(10_002_000, Ordering::SeqCst);
    let a = svc.ended(CHANNEL, GUILD, first, 50);
    let b = svc.ended(CHANNEL, GUILD, first, 50);
    let (ra, rb) = tokio::join!(a, b);
    ra.unwrap();
    rb.unwrap();
    let (snap, _) = svc.state(CHANNEL, 50).await.unwrap().unwrap();
    assert_eq!(snap.doc.current().unwrap().track_id, Some(2), "not 3: the lock serialised them");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_channel_that_does_not_exist_gets_no_session() {
    let (_db, svc, _clock) = world().await;
    let e = svc.enqueue(9999, GUILD, 1, &[t(1, 7, 1_000)], 50).await;
    assert!(e.is_err());
    assert!(svc.state(9999, 50).await.unwrap().is_none());
}
