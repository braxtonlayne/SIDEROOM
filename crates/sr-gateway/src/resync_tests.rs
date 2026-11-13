//! D-646 (SEC-AUD-169): the resync that follows event-bus lag, as seen from a client, and its queue.

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use sr_core::{ChannelId, GuildId, UserId};
use sr_events::{DomainEvent, EventBus, InProcessBus, Topic};
use tokio::time::Instant;

use crate::resync::{Key, ResyncQueue};
use crate::test_support::{TestClient, TestWorld};
use crate::{Frame, GatewayConfig, close, intents};

const G: GuildId = GuildId::new(1);
const G2: GuildId = GuildId::new(2);

fn u(n: u64) -> UserId {
    UserId::new(n)
}

/// A world whose topics hold four events, so a dozen published before a pump runs make it lag.
fn small_bus_world(cfg: GatewayConfig) -> TestWorld {
    TestWorld::with_bus(cfg, Arc::new(InProcessBus::with_capacity(4)))
}

async fn flood_user(w: &TestWorld, user: u64) {
    for n in 0..12 {
        w.bus
            .publish(
                Topic::User(u(user)),
                DomainEvent::GuildDeleted {
                    guild: GuildId::new(500 + n),
                },
            )
            .await
            .unwrap();
    }
}

async fn flood_guild(w: &TestWorld, guild: GuildId) {
    for n in 0..12 {
        let ev = DomainEvent::MessageSent {
            guild: Some(guild),
            channel: ChannelId::new(10),
            message_id: 100 + n,
            author: u(1),
            data: json!({
                "id": (100 + n).to_string(), "channel_id": "10", "guild_id": guild.to_string(),
                "author": { "id": "1" }, "content": "x", "mentions": [],
            }),
        };
        w.bus.publish(Topic::Guild(guild), ev).await.unwrap();
    }
}

/// D-646: a user topic that lagged sends each session a fresh READY under the same session id, then each guild.
#[tokio::test]
async fn a_lagged_user_topic_sends_a_fresh_ready_and_guilds_with_the_same_session_id() {
    let w = small_bus_world(GatewayConfig::default());
    w.add_member(u(7), G);
    w.add_member(u(7), G2);
    let mut c = w.connect();
    let first = c.identify("user:7:0:a", intents::GUILDS, 2).await;
    let sid = first["d"]["session_id"].as_str().unwrap().to_owned();
    c.drain().await;
    flood_user(&w, 7).await;
    let ready = c.await_dispatch("READY").await;
    assert_eq!(ready["d"]["session_id"], json!(sid));
    assert!(ready["s"].as_u64() > first["s"].as_u64(), "{ready}");
    assert_eq!(
        ready["d"]["guilds"],
        json!([{"id": "1", "unavailable": true}, {"id": "2", "unavailable": true}])
    );
    assert_eq!(c.expect_event("GUILD_CREATE").await["d"]["id"], "1");
    assert_eq!(c.expect_event("GUILD_CREATE").await["d"]["id"], "2");
}

/// D-646: a revoked session closes with 4004 and is not sent a snapshot first (the re-check is queued on its socket
/// before the resync is).
#[tokio::test]
async fn a_resync_is_not_sent_to_a_revoked_session() {
    let w = small_bus_world(GatewayConfig::default());
    w.add_member(u(7), G);
    let mut gone = w.connect();
    gone.identify("user:7:0:a", intents::GUILDS, 1).await;
    gone.drain().await;
    w.auth.revoke("user:7:0:a");
    flood_user(&w, 7).await;
    match gone.frame().await {
        Frame::Close { code, .. } => assert_eq!(code, close::AUTHENTICATION_FAILED),
        other => panic!("a revoked session was sent {other:?} before its close"),
    }
}

/// D-646: the node tells at most `resync_sessions_per_second` sessions per second (burst: one second's worth).
#[tokio::test(start_paused = true)]
async fn resync_stays_under_the_session_rate() {
    let w = small_bus_world(GatewayConfig {
        resync_sessions_per_second: 4,
        ..GatewayConfig::default()
    });
    let users: Vec<u64> = (101..113).collect();
    for n in &users {
        w.add_member(u(*n), G);
    }
    let mut clients: Vec<TestClient> = w.connect_users(&users, intents::GUILDS).await;
    for c in &mut clients {
        c.drain().await;
    }
    let started = Instant::now();
    flood_guild(&w, G).await;
    for c in &mut clients {
        c.await_dispatch("SR_RESYNC").await;
    }
    let took = started.elapsed();
    // 12 sessions at 4 per second with a burst of 4: the last one is told two seconds in.
    assert!(took >= Duration::from_secs(2), "{took:?}");
    assert!(took < Duration::from_secs(3), "{took:?}");
}

fn queue_with(max: usize, floor: Duration) -> ResyncQueue {
    ResyncQueue::new(&GatewayConfig {
        resync_queue_max: max,
        resync_floor: floor,
        ..GatewayConfig::default()
    })
}

/// D-646: no more than `resync_queue_max` keys wait; the oldest is dropped.
#[tokio::test]
async fn the_queue_is_bounded() {
    let q = queue_with(8, Duration::from_secs(30));
    for n in 1..=13 {
        q.request(Key::Guild(GuildId::new(n)));
    }
    let waiting = q.waiting();
    assert_eq!(waiting.len(), 8);
    assert_eq!(waiting.first(), Some(&Key::Guild(GuildId::new(6))));
    assert_eq!(waiting.last(), Some(&Key::Guild(GuildId::new(13))));
    // A key that is already waiting only sets its flag: nothing is dropped for it.
    q.request(Key::Guild(GuildId::new(13)));
    assert_eq!(q.waiting().len(), 8);
}

/// D-646: the floor counts from the end of a round, also for a lag that arrived while it ran.
#[tokio::test(start_paused = true)]
async fn the_floor_counts_from_the_end_of_the_round() {
    use crate::resync::Take;
    let floor = Duration::from_secs(30);
    let q = queue_with(8, floor);
    let key = Key::Guild(G);
    q.request(key);
    let Take::Run(k) = q.take(Instant::now()) else {
        panic!("the first round is due at once");
    };
    assert_eq!(k, key);
    // A lag while the round runs.
    tokio::time::advance(Duration::from_secs(50)).await;
    q.request(key);
    q.finish(key, Instant::now());
    assert!(matches!(q.take(Instant::now()), Take::Wait(Some(_))));
    tokio::time::advance(floor - Duration::from_secs(1)).await;
    assert!(matches!(q.take(Instant::now()), Take::Wait(Some(_))));
    tokio::time::advance(Duration::from_secs(1)).await;
    assert!(matches!(q.take(Instant::now()), Take::Run(_)));
    assert!(matches!(q.take(Instant::now()), Take::Wait(None)));
}
