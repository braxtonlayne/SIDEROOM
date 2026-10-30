//! Postgres tests for tool rooms (UTL-COL-001, migration 1660): the version rule under concurrency, the
//! channel binding, the cap, the 7-day life and the sweep.
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test tool_rooms_repo -- --include-ignored --test-threads=1`

use serde_json::{Value, json};
use sr_chat::tool_rooms::{
    LIFE_MS, MAX_ROOMS_PER_CHANNEL, Patch, RoomError, ToolRoom, ToolRoomStore, expires_at,
};
use sr_db::testing::TestDb;
use sr_db::tool_rooms_repo::PgToolRooms;

const T0: i64 = 1_800_000_000_000;

async fn world() -> (TestDb, PgToolRooms) {
    let db = TestDb::new().await.expect("db");
    for ch in [11, 12] {
        sqlx::query("INSERT INTO channels (id, type) VALUES ($1, 1)")
            .bind(ch)
            .execute(db.pool())
            .await
            .unwrap();
    }
    let store = PgToolRooms::new(db.pool().clone());
    (db, store)
}

fn room(id: u64, channel: u64) -> ToolRoom {
    ToolRoom {
        id,
        channel_id: channel,
        guild_id: None,
        kind: "tierlist".into(),
        title: "Pizza".into(),
        state: json!({"t.s": {"name": "S"}}).as_object().unwrap().clone(),
        version: 1,
        created_by: None,
        created_ms: T0,
        updated_ms: T0,
        expires_ms: expires_at(T0),
    }
}

fn patch(base: i64, set: Value) -> Patch {
    Patch {
        base_version: base,
        set: set.as_object().unwrap().clone(),
        title: None,
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn version_rule_returns_the_current_state_on_a_stale_patch() {
    let (_db, s) = world().await;
    let r = s.create(room(1, 11), T0).await.unwrap();
    assert_eq!(r.version, 1);
    let r2 = s
        .patch(11, 1, &patch(1, json!({"i.a": {"t": "x"}})), T0 + 1)
        .await
        .unwrap();
    assert_eq!(r2.version, 2);
    assert!(r2.state.contains_key("i.a") && r2.state.contains_key("t.s"));
    // A second writer still on version 1 is refused with the current room back.
    match s.patch(11, 1, &patch(1, json!({"i.b": 1})), T0 + 2).await {
        Err(RoomError::Stale(cur)) => {
            assert_eq!(cur.version, 2);
            assert!(cur.state.contains_key("i.a"));
        }
        other => panic!("{other:?}"),
    }
    // Removing a key with null.
    let r3 = s
        .patch(11, 1, &patch(2, json!({"i.a": null})), T0 + 3)
        .await
        .unwrap();
    assert_eq!(r3.version, 3);
    assert!(!r3.state.contains_key("i.a"));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn concurrent_writers_on_one_version_have_exactly_one_winner() {
    let (_db, s) = world().await;
    s.create(room(1, 11), T0).await.unwrap();
    let mut tasks = Vec::new();
    for n in 0..8 {
        let s = s.clone();
        tasks.push(tokio::spawn(async move {
            s.patch(11, 1, &patch(1, json!({ format!("k{n}"): n })), T0 + 5)
                .await
        }));
    }
    let mut won = 0;
    let mut stale = 0;
    for t in tasks {
        match t.await.unwrap() {
            Ok(_) => won += 1,
            Err(RoomError::Stale(_)) => stale += 1,
            Err(e) => panic!("{e:?}"),
        }
    }
    assert_eq!((won, stale), (1, 7));
    assert_eq!(s.get(11, 1, T0 + 6).await.unwrap().unwrap().version, 2);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_room_belongs_to_its_channel() {
    let (_db, s) = world().await;
    s.create(room(1, 11), T0).await.unwrap();
    // Another channel cannot read, edit or delete it.
    assert!(s.get(12, 1, T0).await.unwrap().is_none());
    assert!(matches!(
        s.patch(12, 1, &patch(1, json!({"a": 1})), T0).await,
        Err(RoomError::NotFound)
    ));
    assert!(!s.delete(12, 1).await.unwrap());
    assert_eq!(s.list(11, T0).await.unwrap().len(), 1);
    assert!(s.list(12, T0).await.unwrap().is_empty());
    assert!(s.delete(11, 1).await.unwrap());
    assert!(s.get(11, 1, T0).await.unwrap().is_none());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_channel_cap_holds() {
    let (_db, s) = world().await;
    for i in 0..MAX_ROOMS_PER_CHANNEL as u64 {
        s.create(room(i + 1, 11), T0).await.unwrap();
    }
    assert!(matches!(
        s.create(room(900, 11), T0).await,
        Err(RoomError::TooMany)
    ));
    // Another channel is unaffected, and an expired room frees a slot.
    s.create(room(901, 12), T0).await.unwrap();
    s.create(room(902, 11), T0 + LIFE_MS).await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn life_is_seven_days_and_the_sweep_removes_expired_rooms() {
    let (db, s) = world().await;
    s.create(room(1, 11), T0).await.unwrap();
    // Alive just before day 7, gone at day 7 for reads and edits, even before the sweep ran.
    let before = T0 + LIFE_MS - 1;
    assert!(s.get(11, 1, before).await.unwrap().is_some());
    let at = T0 + LIFE_MS;
    assert!(s.get(11, 1, at).await.unwrap().is_none());
    assert!(matches!(
        s.patch(11, 1, &patch(1, json!({"a": 1})), at).await,
        Err(RoomError::NotFound)
    ));
    assert!(s.list(11, at).await.unwrap().iter().all(|r| r.id != 1));
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM tool_rooms WHERE id = 1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(rows, 1, "the row stays until the sweep");
    let swept = s.sweep(at).await.unwrap();
    assert!(swept >= 1);
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM tool_rooms WHERE id = 1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(rows, 0);
    assert_eq!(s.sweep(at).await.unwrap(), 0);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn deleting_the_channel_removes_its_rooms() {
    let (db, s) = world().await;
    s.create(room(1, 11), T0).await.unwrap();
    sqlx::query("DELETE FROM channels WHERE id = 11")
        .execute(db.pool())
        .await
        .unwrap();
    assert!(s.get(11, 1, T0).await.unwrap().is_none());
}
