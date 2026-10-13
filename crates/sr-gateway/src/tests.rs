//! Protocol tests driven by an in-process fake client (no sockets).

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use sr_core::{ChannelId, GuildId, Permissions, UserId};
use sr_events::{DomainEvent, EventBus, InProcessBus, Topic};
use tokio::sync::mpsc;

use crate::*;

const G1: GuildId = GuildId::new(1);
const G2: GuildId = GuildId::new(2);
const OPEN: ChannelId = ChannelId::new(10);
const SECRET: ChannelId = ChannelId::new(11);

fn u(n: u64) -> UserId {
    UserId::new(n)
}

#[derive(Default)]
struct FakeDir {
    guilds: Mutex<HashMap<UserId, Vec<GuildId>>>,
    hidden: Mutex<HashSet<(UserId, ChannelId)>>,
    staff: Mutex<Vec<UserId>>,
    perms: Mutex<HashMap<(UserId, GuildId), Permissions>>,
    friends: Mutex<HashMap<UserId, Vec<UserId>>>,
    sync: Mutex<HashMap<(UserId, GuildId), Value>>,
}

#[async_trait]
impl GuildDirectory for FakeDir {
    async fn guilds_for_user(&self, user: UserId) -> Vec<GuildId> {
        self.guilds
            .lock()
            .unwrap()
            .get(&user)
            .cloned()
            .unwrap_or_default()
    }
    async fn thread_list_sync(&self, user: UserId, g: GuildId) -> Option<Value> {
        self.sync.lock().unwrap().get(&(user, g)).cloned()
    }
    async fn staff_users(&self) -> Vec<UserId> {
        self.staff.lock().unwrap().clone()
    }
    async fn friends_of(&self, user: UserId) -> Vec<UserId> {
        self.friends
            .lock()
            .unwrap()
            .get(&user)
            .cloned()
            .unwrap_or_default()
    }
    async fn has_guild_permission(&self, user: UserId, g: GuildId, needed: Permissions) -> bool {
        self.perms
            .lock()
            .unwrap()
            .get(&(user, g))
            .is_some_and(|p| p.contains(needed))
    }
    async fn can_view_channel(&self, user: UserId, _g: GuildId, ch: ChannelId) -> bool {
        !self.hidden.lock().unwrap().contains(&(user, ch))
    }
}

/// Tokens: `user:<id>`, `bot:<id>[:<allowed privileged bits>]`; anything else fails.
struct FakeAuth;

#[async_trait]
impl Authenticator for FakeAuth {
    async fn authenticate(&self, token: &str) -> Result<AuthedUser, AuthError> {
        let parts: Vec<&str> = token.split(':').collect();
        let id = parts.get(1).and_then(|s| s.parse::<u64>().ok());
        match (parts.first().copied(), id) {
            (Some(kind @ ("user" | "bot")), Some(id)) => Ok(AuthedUser {
                user_id: u(id),
                bot: kind == "bot",
                allowed_privileged_intents: parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0),
                user: json!({ "id": id.to_string() }),
            }),
            _ => Err(AuthError::Invalid),
        }
    }

    async fn authenticate_session_cookie(&self, cookie: &str) -> Result<AuthedUser, AuthError> {
        self.authenticate(&format!("user:{cookie}")).await
    }
}

struct Env {
    gw: Gateway,
    bus: Arc<InProcessBus>,
    dir: Arc<FakeDir>,
}

fn env_with(cfg: GatewayConfig) -> Env {
    let bus = Arc::new(InProcessBus::new());
    let dir = Arc::new(FakeDir::default());
    {
        let mut g = dir.guilds.lock().unwrap();
        g.insert(u(1), vec![G1]);
        g.insert(u(2), vec![G1]);
        g.insert(u(3), vec![G2]);
        g.insert(u(5), vec![]);
        dir.hidden.lock().unwrap().insert((u(2), SECRET));
    }
    let gw = Gateway::new(cfg, Arc::new(FakeAuth), dir.clone(), bus.clone());
    gw.start();
    Env { gw, bus, dir }
}

fn env() -> Env {
    env_with(GatewayConfig::default())
}

async fn emit(bus: &InProcessBus, ev: DomainEvent) {
    for t in ev.topics() {
        bus.publish(t, ev.clone()).await.unwrap();
    }
}

fn msg(guild: GuildId, channel: ChannelId, id: u64) -> DomainEvent {
    DomainEvent::MessageSent {
        guild: Some(guild),
        channel,
        message_id: id,
        author: u(1),
        data: json!({
            "id": id.to_string(), "channel_id": channel.to_string(),
            "guild_id": guild.to_string(), "author": { "id": "1" },
            "content": "hello", "mentions": [],
        }),
    }
}

async fn settle() {
    tokio::time::sleep(Duration::from_millis(60)).await;
}

const GUILDS_MSGS: u64 = intents::GUILDS | intents::GUILD_MESSAGES;

struct Client {
    tx: mpsc::Sender<Frame>,
    rx: mpsc::Receiver<Frame>,
    zstd: bool,
}

impl Client {
    fn connect(gw: &Gateway, compression: Compression) -> Self {
        let (tx, in_rx) = mpsc::channel(64);
        let (out_tx, rx) = mpsc::channel(256);
        tokio::spawn(run_connection(
            gw.clone(),
            ConnectParams {
                compression,
                ..Default::default()
            },
            in_rx,
            out_tx,
        ));
        Self {
            tx,
            rx,
            zstd: compression == Compression::ZstdStream,
        }
    }

    async fn send(&self, v: Value) {
        self.tx.send(Frame::Text(v.to_string())).await.unwrap();
    }

    async fn frame(&mut self) -> Frame {
        tokio::time::timeout(Duration::from_secs(30), self.rx.recv())
            .await
            .expect("timed out waiting for a frame")
            .expect("connection ended without a frame")
    }

    async fn recv(&mut self) -> Value {
        match self.frame().await {
            Frame::Text(t) => serde_json::from_str(&t).unwrap(),
            Frame::Binary(b) => {
                assert!(self.zstd, "binary frame without compression");
                let mut dec = ruzstd::decoding::StreamingDecoder::new(&b[..]).unwrap();
                let mut s = String::new();
                dec.read_to_string(&mut s).unwrap();
                serde_json::from_str(&s).unwrap()
            }
            Frame::Close { code, .. } => panic!("unexpected close {code}"),
        }
    }

    async fn recv_close(&mut self) -> u16 {
        loop {
            if let Frame::Close { code, .. } = self.frame().await {
                return code;
            }
        }
    }

    async fn expect_event(&mut self, name: &str) -> Value {
        let v = self.recv().await;
        assert_eq!(v["op"], 0, "{v}");
        assert_eq!(v["t"], name, "{v}");
        v
    }

    async fn silent(&mut self) {
        let r = tokio::time::timeout(Duration::from_millis(150), self.rx.recv()).await;
        assert!(r.is_err(), "expected no frame, got {r:?}");
    }

    async fn drain(&mut self) {
        while tokio::time::timeout(Duration::from_millis(80), self.rx.recv())
            .await
            .is_ok()
        {}
    }

    /// HELLO, IDENTIFY, READY, one GUILD_CREATE per guild. Returns READY.
    async fn identify(&mut self, token: &str, intents: u64, guilds: usize) -> Value {
        let hello = self.recv().await;
        assert_eq!(hello["op"], 10);
        self.send(json!({"op": 2, "d": {"token": token, "intents": intents, "properties": {}}}))
            .await;
        let ready = self.expect_event("READY").await;
        for _ in 0..guilds {
            self.expect_event("GUILD_CREATE").await;
        }
        ready
    }
}

fn connect(e: &Env) -> Client {
    Client::connect(&e.gw, Compression::None)
}

#[tokio::test]
async fn hello_identify_ready() {
    let e = env();
    let mut c = connect(&e);
    let hello = c.recv().await;
    assert_eq!(hello["op"], 10);
    assert_eq!(hello["d"]["heartbeat_interval"], 41250);
    c.send(json!({"op": 2, "d": {"token": "bot:1", "intents": GUILDS_MSGS}}))
        .await;
    let ready = c.expect_event("READY").await;
    assert_eq!(ready["s"], 1);
    assert_eq!(ready["d"]["user"]["id"], "1");
    assert_eq!(
        ready["d"]["guilds"],
        json!([{"id": "1", "unavailable": true}])
    );
    assert!(ready["d"]["session_id"].as_str().unwrap().len() >= 16);
    let gc = c.expect_event("GUILD_CREATE").await;
    assert_eq!(gc["s"], 2);
    assert_eq!(gc["d"]["id"], "1");
}

#[tokio::test(start_paused = true)]
async fn heartbeat_ack_and_timeout() {
    let e = env_with(GatewayConfig {
        heartbeat_interval: Duration::from_secs(1),
        heartbeat_grace: Duration::from_secs(1),
        ..GatewayConfig::default()
    });
    let mut c = connect(&e);
    assert_eq!(c.recv().await["d"]["heartbeat_interval"], 1000);
    // 1.5s apart: would time out at 2s without the heartbeats resetting it.
    for _ in 0..2 {
        tokio::time::advance(Duration::from_millis(1500)).await;
        c.send(json!({"op": 1, "d": null})).await;
        assert_eq!(c.recv().await["op"], 11);
    }
    assert_eq!(c.recv_close().await, close::SESSION_TIMED_OUT);
}

#[tokio::test(start_paused = true)]
async fn seen_is_marked_on_identify_and_at_most_every_ten_minutes() {
    let e = env_with(GatewayConfig {
        heartbeat_interval: Duration::from_secs(3600),
        ..GatewayConfig::default()
    });
    let mut c = connect(&e);
    c.identify("bot:1", GUILDS_MSGS, 1).await;
    assert_eq!(e.gw.drain_seen(), vec![u(1)]);
    assert!(e.gw.drain_seen().is_empty(), "drained once");
    // Heartbeats inside the window do not mark again.
    tokio::time::advance(Duration::from_secs(300)).await;
    c.send(json!({"op": 1, "d": null})).await;
    assert_eq!(c.recv().await["op"], 11);
    assert!(e.gw.drain_seen().is_empty());
    // Past ten minutes the next heartbeat marks the user once more.
    tokio::time::advance(Duration::from_secs(301)).await;
    c.send(json!({"op": 1, "d": null})).await;
    assert_eq!(c.recv().await["op"], 11);
    assert_eq!(e.gw.drain_seen(), vec![u(1)]);
}

#[tokio::test]
async fn protocol_errors_close_with_discord_codes() {
    let e = env();

    let mut c = connect(&e);
    c.recv().await;
    c.send(json!({"op": 3, "d": {"status": "online"}})).await;
    assert_eq!(c.recv_close().await, close::NOT_AUTHENTICATED);

    let mut c = connect(&e);
    c.recv().await;
    c.send(json!({"op": 2, "d": {"token": "nope", "intents": 0}}))
        .await;
    assert_eq!(c.recv_close().await, close::AUTHENTICATION_FAILED);

    let mut c = connect(&e);
    c.recv().await;
    c.tx.send(Frame::Text("not json".into())).await.unwrap();
    assert_eq!(c.recv_close().await, close::DECODE_ERROR);

    let mut c = connect(&e);
    c.recv().await;
    c.send(json!({"op": 77, "d": null})).await;
    assert_eq!(c.recv_close().await, close::UNKNOWN_OPCODE);

    let mut c = connect(&e);
    c.identify("bot:1", GUILDS_MSGS, 1).await;
    c.send(json!({"op": 2, "d": {"token": "bot:1", "intents": 0}}))
        .await;
    assert_eq!(c.recv_close().await, close::ALREADY_AUTHENTICATED);
}

#[tokio::test]
async fn privileged_and_invalid_intents() {
    let e = env();
    let mut c = connect(&e);
    c.recv().await;
    c.send(json!({"op": 2, "d": {"token": "bot:1:0", "intents": intents::GUILD_MEMBERS}}))
        .await;
    assert_eq!(c.recv_close().await, close::DISALLOWED_INTENTS);

    let mut c = connect(&e);
    c.recv().await;
    c.send(json!({"op": 2, "d": {"token": "bot:1", "intents": 1u64 << 40}}))
        .await;
    assert_eq!(c.recv_close().await, close::INVALID_INTENTS);

    let mut c = connect(&e);
    c.identify(
        &format!("bot:1:{}", intents::GUILD_MEMBERS),
        intents::GUILDS | intents::GUILD_MEMBERS,
        1,
    )
    .await;
}

#[tokio::test]
async fn fan_out_is_permission_filtered_per_recipient() {
    let e = env();
    let mut a = connect(&e);
    a.identify("bot:1", GUILDS_MSGS, 1).await;
    let mut b = connect(&e);
    b.identify("bot:2", GUILDS_MSGS, 1).await;
    let mut other_guild = connect(&e);
    other_guild.identify("bot:3", GUILDS_MSGS, 1).await;

    // User 2 cannot view SECRET.
    emit(&e.bus, msg(G1, SECRET, 100)).await;
    let m = a.expect_event("MESSAGE_CREATE").await;
    assert_eq!(m["d"]["id"], "100");
    assert_eq!(m["d"]["channel_id"], "11");
    b.silent().await;
    other_guild.silent().await;

    emit(&e.bus, msg(G1, OPEN, 101)).await;
    assert_eq!(a.expect_event("MESSAGE_CREATE").await["d"]["id"], "101");
    assert_eq!(b.expect_event("MESSAGE_CREATE").await["d"]["id"], "101");
    other_guild.silent().await;

    // Permissions are checked at delivery time.
    e.dir.hidden.lock().unwrap().insert((u(1), OPEN));
    emit(&e.bus, msg(G1, OPEN, 102)).await;
    assert_eq!(b.expect_event("MESSAGE_CREATE").await["d"]["id"], "102");
    a.silent().await;
}

#[tokio::test]
async fn intents_filter_dispatches() {
    let e = env();
    let mut c = connect(&e);
    c.identify("bot:1", intents::GUILDS, 1).await;
    emit(&e.bus, msg(G1, OPEN, 1)).await;
    c.silent().await;
}

#[tokio::test]
async fn membership_changes_update_subscriptions() {
    let e = env();
    let mut c = connect(&e);
    c.identify("bot:5", GUILDS_MSGS, 0).await;
    emit(&e.bus, msg(G1, OPEN, 1)).await;
    c.silent().await;

    e.dir.guilds.lock().unwrap().insert(u(5), vec![G1]);
    emit(
        &e.bus,
        DomainEvent::MemberJoined {
            guild: G1,
            user: u(5),
            payload: json!({ "guild_id": "1", "user": { "id": "5" } }),
        },
    )
    .await;
    c.expect_event("GUILD_CREATE").await;
    settle().await;
    emit(&e.bus, msg(G1, OPEN, 2)).await;
    assert_eq!(c.expect_event("MESSAGE_CREATE").await["d"]["id"], "2");

    emit(
        &e.bus,
        DomainEvent::MemberLeft {
            guild: G1,
            user: u(5),
        },
    )
    .await;
    c.expect_event("GUILD_DELETE").await;
    settle().await;
    emit(&e.bus, msg(G1, OPEN, 3)).await;
    c.silent().await;
}

#[tokio::test]
async fn guild_deleted_notifies_and_unsubscribes() {
    let e = env();
    let mut c = connect(&e);
    c.identify("bot:1", GUILDS_MSGS, 1).await;
    emit(&e.bus, DomainEvent::GuildDeleted { guild: G1 }).await;
    assert_eq!(c.expect_event("GUILD_DELETE").await["d"]["id"], "1");
    settle().await;
    emit(&e.bus, msg(G1, OPEN, 1)).await;
    c.silent().await;
}

#[tokio::test]
async fn resume_replays_missed_events() {
    let e = env();
    let mut c = connect(&e);
    let ready = c.identify("bot:1", GUILDS_MSGS, 1).await;
    let sid = ready["d"]["session_id"].as_str().unwrap().to_owned();
    emit(&e.bus, msg(G1, OPEN, 1)).await;
    let seen = c.expect_event("MESSAGE_CREATE").await["s"]
        .as_u64()
        .unwrap();
    drop(c);
    settle().await;

    emit(&e.bus, msg(G1, OPEN, 2)).await;
    emit(&e.bus, msg(G1, OPEN, 3)).await;
    settle().await;

    let mut c2 = connect(&e);
    c2.recv().await;
    c2.send(json!({"op": 6, "d": {"token": "bot:1", "session_id": sid, "seq": seen}}))
        .await;
    let a = c2.expect_event("MESSAGE_CREATE").await;
    let b = c2.expect_event("MESSAGE_CREATE").await;
    assert_eq!(a["d"]["id"], "2");
    assert_eq!(b["d"]["id"], "3");
    assert_eq!(a["s"], seen + 1);
    let r = c2.expect_event("RESUMED").await;
    assert_eq!(r["s"], seen + 3);

    // Live again afterwards.
    emit(&e.bus, msg(G1, OPEN, 4)).await;
    assert_eq!(c2.expect_event("MESSAGE_CREATE").await["s"], seen + 4);
}

#[tokio::test]
async fn resume_rejected_when_unknown_wrong_user_or_gap() {
    let e = env_with(GatewayConfig {
        resume_buffer: 2,
        ..GatewayConfig::default()
    });
    let mut c = connect(&e);
    let ready = c.identify("bot:1", GUILDS_MSGS, 1).await;
    let sid = ready["d"]["session_id"].as_str().unwrap().to_owned();
    drop(c);
    settle().await;

    // Unknown session.
    let mut x = connect(&e);
    x.recv().await;
    x.send(json!({"op": 6, "d": {"token": "bot:1", "session_id": "zz", "seq": 1}}))
        .await;
    assert_eq!(x.recv().await, json!({"op": 9, "d": false}));

    // Different user's token.
    x.send(json!({"op": 6, "d": {"token": "bot:2", "session_id": sid, "seq": 2}}))
        .await;
    assert_eq!(x.recv().await["op"], 9);

    // Sequence ahead of the server.
    x.send(json!({"op": 6, "d": {"token": "bot:1", "session_id": sid, "seq": 99}}))
        .await;
    assert_eq!(x.recv().await["op"], 9);

    // Buffer (2 events) overflowed while away: the gap cannot be replayed.
    for i in 0..5 {
        emit(&e.bus, msg(G1, OPEN, i)).await;
    }
    settle().await;
    x.send(json!({"op": 6, "d": {"token": "bot:1", "session_id": sid, "seq": 2}}))
        .await;
    assert_eq!(x.recv().await, json!({"op": 9, "d": false}));

    // The client may then fall back to a fresh IDENTIFY on the same socket.
    x.send(json!({"op": 2, "d": {"token": "bot:1", "intents": GUILDS_MSGS}}))
        .await;
    x.expect_event("READY").await;
}

#[tokio::test(start_paused = true)]
async fn resume_window_expires_and_sweep_cleans_up() {
    let e = env();
    let mut c = connect(&e);
    let ready = c.identify("user:1", 0, 1).await;
    let sid = ready["d"]["session_id"].as_str().unwrap().to_owned();
    assert_eq!(e.gw.presence(u(1)).unwrap().status, "online");
    drop(c);
    settle().await;
    tokio::time::advance(Duration::from_secs(121)).await;
    e.gw.sweep().await;
    assert_eq!(e.gw.session_count(), 0);
    assert!(e.gw.presence(u(1)).is_none());

    let mut c2 = connect(&e);
    c2.recv().await;
    c2.send(json!({"op": 6, "d": {"token": "user:1", "session_id": sid, "seq": 1}}))
        .await;
    assert_eq!(c2.recv().await, json!({"op": 9, "d": false}));
}

#[tokio::test]
async fn request_reconnect_sends_op7_then_closes() {
    let e = env();
    let mut c = connect(&e);
    let ready = c.identify("bot:1", GUILDS_MSGS, 1).await;
    let sid = ready["d"]["session_id"].as_str().unwrap().to_owned();
    e.gw.request_reconnect();
    assert_eq!(c.recv().await["op"], 7);
    c.recv_close().await;
    // Still resumable.
    settle().await;
    let mut c2 = connect(&e);
    c2.recv().await;
    c2.send(json!({"op": 6, "d": {"token": "bot:1", "session_id": sid, "seq": 2}}))
        .await;
    c2.expect_event("RESUMED").await;
}

#[tokio::test]
async fn zstd_compression_round_trips() {
    let e = env();
    let mut c = Client::connect(&e.gw, Compression::ZstdStream);
    let hello = c.recv().await;
    assert_eq!(hello["op"], 10);
    c.send(json!({"op": 2, "d": {"token": "bot:1", "intents": GUILDS_MSGS}}))
        .await;
    assert_eq!(c.expect_event("READY").await["d"]["user"]["id"], "1");
}

#[tokio::test]
async fn presence_updates_fan_out_and_invisible_shows_offline() {
    let e = env();
    let mut a = connect(&e);
    a.identify("user:1", 0, 1).await;
    let mut b = connect(&e);
    b.identify("user:2", 0, 1).await;
    // Drop the "online" presences emitted while connecting.
    settle().await;
    a.drain().await;
    b.drain().await;

    a.send(json!({"op": 3, "d": {"status": "invisible", "activities": []}}))
        .await;
    let p = b.expect_event("PRESENCE_UPDATE").await;
    assert_eq!(p["d"]["user"]["id"], "1");
    assert_eq!(p["d"]["status"], "offline");
    assert_eq!(e.gw.presence(u(1)).unwrap().status, "invisible");
}

#[tokio::test]
async fn typing_is_filtered_and_expires() {
    let e = env_with(GatewayConfig {
        typing_ttl: Duration::from_millis(200),
        ..GatewayConfig::default()
    });
    let all = GUILDS_MSGS | intents::GUILD_MESSAGE_TYPING;
    let mut a = connect(&e);
    a.identify("bot:1", all, 1).await;
    let mut b = connect(&e);
    b.identify("bot:2", all, 1).await;

    emit(
        &e.bus,
        DomainEvent::TypingStarted {
            guild: Some(G1),
            channel: SECRET,
            user: u(1),
            timestamp: 0,
        },
    )
    .await;
    let t = a.expect_event("TYPING_START").await;
    assert_eq!(t["d"]["user_id"], "1");
    assert_eq!(t["d"]["channel_id"], "11");
    b.silent().await;
    assert_eq!(e.gw.typing_users(SECRET), vec![u(1)]);
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(e.gw.typing_users(SECRET).is_empty());
}

#[tokio::test]
async fn rest_message_family_is_dispatched_with_payloads() {
    let e = env();
    let all = GUILDS_MSGS | intents::GUILD_MESSAGE_REACTIONS | intents::GUILD_MESSAGE_TYPING;
    let mut a = connect(&e);
    a.identify("bot:1", all, 1).await;
    let mut only_guilds = connect(&e);
    only_guilds.identify("bot:2", intents::GUILDS, 1).await;

    emit(
        &e.bus,
        DomainEvent::MessageSent {
            guild: Some(G1),
            channel: OPEN,
            message_id: 5,
            author: u(1),
            data: json!({"id": "5", "content": "hi", "channel_id": "10"}),
        },
    )
    .await;
    assert_eq!(a.expect_event("MESSAGE_CREATE").await["d"]["content"], "hi");

    emit(
        &e.bus,
        DomainEvent::MessageDeleted {
            guild: Some(G1),
            channel: OPEN,
            message_id: 5,
        },
    )
    .await;
    assert_eq!(a.expect_event("MESSAGE_DELETE").await["d"]["id"], "5");

    emit(
        &e.bus,
        DomainEvent::ReactionAdded {
            guild: Some(G1),
            channel: OPEN,
            message_id: 5,
            user: u(2),
            emoji: json!({"id": null, "name": "x"}),
        },
    )
    .await;
    let r = a.expect_event("MESSAGE_REACTION_ADD").await;
    assert_eq!(r["d"]["user_id"], "2");
    assert_eq!(r["d"]["message_id"], "5");

    emit(
        &e.bus,
        DomainEvent::ChannelPinsUpdated {
            guild: Some(G1),
            channel: OPEN,
            last_pin_ms: Some(1_800_000_000_000),
        },
    )
    .await;
    let p = a.expect_event("CHANNEL_PINS_UPDATE").await;
    assert_eq!(
        p["d"]["last_pin_timestamp"],
        "2027-01-15T08:00:00.000000+00:00"
    );

    emit(
        &e.bus,
        DomainEvent::TypingStarted {
            guild: Some(G1),
            channel: OPEN,
            user: u(2),
            timestamp: 99,
        },
    )
    .await;
    assert_eq!(a.expect_event("TYPING_START").await["d"]["timestamp"], 99);
    // The GUILDS intent only covers pins updates from this family.
    only_guilds.expect_event("CHANNEL_PINS_UPDATE").await;
    only_guilds.silent().await;
}

#[tokio::test]
async fn guild_admin_events_dispatch_their_payload_objects() {
    use sr_core::RoleId;
    let e = env();
    let mut c = connect(&e);
    c.identify("bot:1", intents::GUILDS | intents::GUILD_MODERATION, 1)
        .await;
    emit(
        &e.bus,
        DomainEvent::RoleCreated {
            guild: G1,
            role: RoleId::new(50),
            payload: json!({ "guild_id": "1", "role": { "id": "50", "name": "mods" } }),
        },
    )
    .await;
    let m = c.expect_event("GUILD_ROLE_CREATE").await;
    assert_eq!(m["d"]["role"]["name"], "mods");

    emit(
        &e.bus,
        DomainEvent::ChannelUpdated {
            guild: G1,
            channel: OPEN,
            payload: json!({ "id": "10", "name": "renamed", "guild_id": "1" }),
        },
    )
    .await;
    assert_eq!(
        c.expect_event("CHANNEL_UPDATE").await["d"]["name"],
        "renamed"
    );

    emit(
        &e.bus,
        DomainEvent::MemberBanned {
            guild: G1,
            user: u(9),
            payload: json!({ "guild_id": "1", "user": { "id": "9" } }),
        },
    )
    .await;
    assert_eq!(
        c.expect_event("GUILD_BAN_ADD").await["d"]["user"]["id"],
        "9"
    );

    // Hidden channels do not leak updates; member updates need GUILD_MEMBERS.
    emit(
        &e.bus,
        DomainEvent::MemberUpdated {
            guild: G1,
            user: u(9),
            payload: json!({ "guild_id": "1" }),
        },
    )
    .await;
    c.silent().await;
    let mut hidden = connect(&e);
    hidden.identify("bot:2", intents::GUILDS, 1).await;
    emit(
        &e.bus,
        DomainEvent::ChannelUpdated {
            guild: G1,
            channel: SECRET,
            payload: json!({ "id": "11" }),
        },
    )
    .await;
    hidden.silent().await;
}

#[tokio::test]
async fn dm_events_reach_only_the_recipient_user_topic() {
    let e = env();
    let mut a = connect(&e);
    a.identify("user:1", 0, 1).await;
    let mut b = connect(&e);
    b.identify("user:2", 0, 1).await;
    settle().await;
    a.drain().await;
    b.drain().await;
    let dm = DomainEvent::MessageSent {
        guild: None,
        channel: OPEN,
        message_id: 9,
        author: u(2),
        data: json!({ "id": "9", "channel_id": "10", "author": { "id": "2" }, "content": "hi" }),
    };
    e.bus.publish(Topic::User(u(1)), dm).await.unwrap();
    assert_eq!(a.expect_event("MESSAGE_CREATE").await["d"]["id"], "9");
    b.silent().await;
}

#[tokio::test]
async fn user_scoped_events_are_dispatched_by_name_to_the_addressed_user_only() {
    let e = env();
    let mut a = connect(&e);
    a.identify("user:1", 0, 1).await;
    let mut b = connect(&e);
    b.identify("user:2", 0, 1).await;
    settle().await;
    a.drain().await;
    b.drain().await;
    emit(
        &e.bus,
        DomainEvent::UserScoped {
            users: vec![u(1)],
            name: "RELATIONSHIP_ADD".into(),
            payload: json!({ "id": "2", "type": 1 }),
        },
    )
    .await;
    let r = a.expect_event("RELATIONSHIP_ADD").await;
    assert_eq!(r["d"]["id"], "2");
    b.silent().await;
}

#[tokio::test]
async fn closing_the_last_socket_goes_offline_immediately() {
    let e = env();
    let mut a = connect(&e);
    a.identify("user:1", 0, 1).await;
    let mut b = connect(&e);
    b.identify("user:2", 0, 1).await;
    settle().await;
    a.drain().await;
    b.drain().await;
    drop(a);
    let p = b.expect_event("PRESENCE_UPDATE").await;
    assert_eq!(p["d"]["user"]["id"], "1");
    assert_eq!(p["d"]["status"], "offline");
    // Still resumable, but no longer online.
    assert_eq!(e.gw.session_count(), 2);
    assert_eq!(e.gw.presence(u(1)).unwrap().status, "offline");
}

#[tokio::test]
async fn bots_without_message_content_see_stripped_messages() {
    let e = env();
    let mut c = connect(&e);
    c.identify("bot:1", GUILDS_MSGS, 1).await;
    let base = json!({
        "id": "5", "channel_id": "10", "guild_id": "1", "author": { "id": "2" },
        "content": "secret", "embeds": [{ "title": "x" }], "attachments": [{ "id": "9" }],
        "mentions": [],
    });
    emit(
        &e.bus,
        DomainEvent::MessageSent {
            guild: Some(G1),
            channel: OPEN,
            message_id: 5,
            author: u(2),
            data: base.clone(),
        },
    )
    .await;
    let m = c.expect_event("MESSAGE_CREATE").await;
    assert_eq!(m["d"]["content"], "");
    assert_eq!(m["d"]["embeds"], json!([]));
    assert_eq!(m["d"]["attachments"], json!([]));

    let mut mentioned = base;
    mentioned["id"] = json!("6");
    mentioned["mentions"] = json!([{ "id": "1" }]);
    emit(
        &e.bus,
        DomainEvent::MessageSent {
            guild: Some(G1),
            channel: OPEN,
            message_id: 6,
            author: u(2),
            data: mentioned,
        },
    )
    .await;
    assert_eq!(
        c.expect_event("MESSAGE_CREATE").await["d"]["content"],
        "secret"
    );
}

#[tokio::test]
async fn more_than_120_ops_per_minute_close_4008() {
    let e = env();
    let mut c = connect(&e);
    c.identify("user:1", 0, 1).await;
    for _ in 0..121 {
        c.send(json!({"op": 1, "d": null})).await;
    }
    assert_eq!(c.recv_close().await, 4008);
}

fn connect_with(gw: &Gateway, params: ConnectParams) -> Client {
    let (tx, in_rx) = mpsc::channel(64);
    let (out_tx, rx) = mpsc::channel(256);
    tokio::spawn(run_connection(gw.clone(), params, in_rx, out_tx));
    Client {
        tx,
        rx,
        zstd: false,
    }
}

#[tokio::test]
async fn cookie_auth_requires_an_allowed_origin() {
    let e = env_with(GatewayConfig {
        allowed_origins: vec!["https://sr.test".into()],
        ..GatewayConfig::default()
    });
    let mut ok = connect_with(
        &e.gw,
        ConnectParams {
            session_cookie: Some("1".into()),
            origin: Some("https://sr.test".into()),
            ..Default::default()
        },
    );
    ok.recv().await;
    ok.send(json!({"op": 2, "d": {"token": "", "intents": 0}}))
        .await;
    assert_eq!(ok.expect_event("READY").await["d"]["user"]["id"], "1");

    let mut bad = connect_with(
        &e.gw,
        ConnectParams {
            session_cookie: Some("1".into()),
            origin: Some("https://evil.test".into()),
            ..Default::default()
        },
    );
    bad.recv().await;
    bad.send(json!({"op": 2, "d": {"token": "", "intents": 0}}))
        .await;
    assert_eq!(bad.recv_close().await, 4004);
}

struct Echo;

#[async_trait]
impl OpHandler for Echo {
    fn ops(&self) -> &[u8] {
        &[100]
    }
    async fn handle(&self, session: &SessionHandle, op: u8, d: Value) {
        session.deliver(
            "ECHO",
            json!({ "op": op, "d": d, "user": session.user().to_string() }),
        );
    }
}

#[tokio::test]
async fn registered_op_handlers_receive_their_ops_only() {
    let e = env_with(GatewayConfig {
        handlers: vec![Arc::new(Echo) as Arc<dyn OpHandler>],
        ..GatewayConfig::default()
    });
    let mut c = connect(&e);
    c.identify("user:1", 0, 1).await;
    c.drain().await;
    c.send(json!({"op": 100, "d": {"x": 1}})).await;
    let v = c.expect_event("ECHO").await;
    assert_eq!(v["d"]["d"]["x"], 1);
    assert_eq!(v["d"]["user"], "1");
    c.send(json!({"op": 101, "d": null})).await;
    c.silent().await;
}

#[tokio::test]
async fn channel_delete_reaches_only_the_producer_computed_viewers() {
    let e = env();
    let mut a = connect(&e);
    a.identify("bot:1", intents::GUILDS, 1).await;
    let mut b = connect(&e);
    b.identify("bot:2", intents::GUILDS, 1).await;
    emit(
        &e.bus,
        DomainEvent::ChannelDeleted {
            guild: G1,
            channel: SECRET,
            payload: json!({ "id": "11", "name": "secret" }),
            viewers: vec![u(1)],
        },
    )
    .await;
    assert_eq!(a.expect_event("CHANNEL_DELETE").await["d"]["id"], "11");
    b.silent().await;
}

#[tokio::test]
async fn staff_alert_is_dispatched_to_staff_only() {
    let e = env();
    e.dir.staff.lock().unwrap().push(u(1));
    let mut staff = connect(&e);
    staff.identify("bot:1", intents::GUILDS, 1).await;
    let mut other = connect(&e);
    other.identify("bot:2", intents::GUILDS, 1).await;
    emit(
        &e.bus,
        DomainEvent::StaffAlert {
            kind: sr_events::StaffAlertKind::BannedPersonAttempt,
            detail: "registration refused".into(),
        },
    )
    .await;
    let d = staff.expect_event("SR_STAFF_ALERT").await;
    assert_eq!(d["d"]["kind"], "banned_person_attempt");
    other.silent().await;
}

// ---- settings dispatches (invites, expressions, webhooks, automod, audit log) -------

/// User 1 is a moderator of G1; user 2 is a plain member.
fn grant_moderator(e: &Env) {
    e.dir.perms.lock().unwrap().insert(
        (u(1), G1),
        Permissions::MANAGE_GUILD | Permissions::MANAGE_CHANNELS | Permissions::VIEW_AUDIT_LOG,
    );
}

const ALL_SETTINGS_INTENTS: u64 = intents::GUILDS
    | intents::GUILD_MODERATION
    | intents::GUILD_EXPRESSIONS
    | intents::GUILD_WEBHOOKS
    | intents::GUILD_INVITES
    | intents::AUTO_MODERATION_CONFIGURATION
    | intents::AUTO_MODERATION_EXECUTION;

#[tokio::test]
async fn invite_create_and_delete_dispatch() {
    let e = env();
    grant_moderator(&e);
    let mut admin = connect(&e);
    admin.identify("bot:1", intents::GUILD_INVITES, 1).await;
    let mut member = connect(&e);
    member.identify("bot:2", intents::GUILD_INVITES, 1).await;
    let mut no_intent = connect(&e);
    no_intent.identify("bot:1", intents::GUILDS, 1).await;

    emit(
        &e.bus,
        DomainEvent::InviteCreated {
            guild: G1,
            channel: OPEN,
            payload: json!({ "code": "abc", "channel_id": "10", "guild_id": "1" }),
        },
    )
    .await;
    let d = admin.expect_event("INVITE_CREATE").await;
    assert_eq!(d["d"]["code"], "abc");
    member.silent().await; // lacks MANAGE_CHANNELS
    no_intent.silent().await;

    emit(
        &e.bus,
        DomainEvent::InviteDeleted {
            guild: G1,
            channel: OPEN,
            payload: json!({ "code": "abc", "channel_id": "10", "guild_id": "1" }),
        },
    )
    .await;
    assert_eq!(
        admin.expect_event("INVITE_DELETE").await["d"]["code"],
        "abc"
    );
    member.silent().await;
}

#[tokio::test]
async fn invite_for_a_hidden_channel_is_not_delivered() {
    let e = env();
    // Both can manage channels, but user 2 cannot view SECRET.
    for uid in [1, 2] {
        e.dir
            .perms
            .lock()
            .unwrap()
            .insert((u(uid), G1), Permissions::MANAGE_CHANNELS);
    }
    let mut a = connect(&e);
    a.identify("bot:1", intents::GUILD_INVITES, 1).await;
    let mut b = connect(&e);
    b.identify("bot:2", intents::GUILD_INVITES, 1).await;
    emit(
        &e.bus,
        DomainEvent::InviteCreated {
            guild: G1,
            channel: SECRET,
            payload: json!({ "code": "zzz" }),
        },
    )
    .await;
    a.expect_event("INVITE_CREATE").await;
    b.silent().await;
}

#[tokio::test]
async fn expressions_updates_dispatch_with_the_expressions_intent() {
    let e = env();
    let mut c = connect(&e);
    c.identify("bot:2", intents::GUILD_EXPRESSIONS, 1).await;
    let mut other = connect(&e);
    other.identify("bot:1", intents::GUILDS, 1).await;
    emit(
        &e.bus,
        DomainEvent::EmojisUpdated {
            guild: G1,
            payload: json!({ "guild_id": "1", "emojis": [{ "id": "5", "name": "pog" }] }),
        },
    )
    .await;
    let d = c.expect_event("GUILD_EMOJIS_UPDATE").await;
    assert_eq!(d["d"]["emojis"][0]["name"], "pog");
    emit(
        &e.bus,
        DomainEvent::StickersUpdated {
            guild: G1,
            payload: json!({ "guild_id": "1", "stickers": [] }),
        },
    )
    .await;
    assert_eq!(
        c.expect_event("GUILD_STICKERS_UPDATE").await["d"]["guild_id"],
        "1"
    );
    other.silent().await;
}

#[tokio::test]
async fn webhooks_update_carries_guild_and_channel() {
    let e = env();
    let mut a = connect(&e);
    a.identify("bot:1", intents::GUILD_WEBHOOKS, 1).await;
    let mut b = connect(&e);
    b.identify("bot:2", intents::GUILD_WEBHOOKS, 1).await;
    let mut no_intent = connect(&e);
    no_intent.identify("bot:1", intents::GUILDS, 1).await;
    emit(
        &e.bus,
        DomainEvent::WebhooksUpdated {
            guild: G1,
            channel: SECRET,
        },
    )
    .await;
    let d = a.expect_event("WEBHOOKS_UPDATE").await;
    assert_eq!(d["d"], json!({ "guild_id": "1", "channel_id": "11" }));
    b.silent().await; // cannot view the channel
    no_intent.silent().await;
}

#[tokio::test]
async fn automod_dispatches_go_to_guild_managers_with_the_right_intent() {
    let e = env();
    grant_moderator(&e);
    let mut config = connect(&e);
    config
        .identify("bot:1", intents::AUTO_MODERATION_CONFIGURATION, 1)
        .await;
    let mut exec = connect(&e);
    exec.identify("bot:1", intents::AUTO_MODERATION_EXECUTION, 1)
        .await;
    let mut member = connect(&e);
    member
        .identify(
            "bot:2",
            intents::AUTO_MODERATION_CONFIGURATION | intents::AUTO_MODERATION_EXECUTION,
            1,
        )
        .await;
    let rule = json!({ "id": "7", "guild_id": "1", "name": "no links" });

    emit(
        &e.bus,
        DomainEvent::AutoModRuleCreated {
            guild: G1,
            payload: rule.clone(),
        },
    )
    .await;
    assert_eq!(
        config.expect_event("AUTO_MODERATION_RULE_CREATE").await["d"]["id"],
        "7"
    );
    emit(
        &e.bus,
        DomainEvent::AutoModRuleUpdated {
            guild: G1,
            payload: rule.clone(),
        },
    )
    .await;
    config.expect_event("AUTO_MODERATION_RULE_UPDATE").await;
    emit(
        &e.bus,
        DomainEvent::AutoModRuleDeleted {
            guild: G1,
            payload: rule,
        },
    )
    .await;
    config.expect_event("AUTO_MODERATION_RULE_DELETE").await;
    emit(
        &e.bus,
        DomainEvent::AutoModActionExecuted {
            guild: G1,
            payload: json!({ "guild_id": "1", "rule_id": "7", "user_id": "2" }),
        },
    )
    .await;
    let d = exec.expect_event("AUTO_MODERATION_ACTION_EXECUTION").await;
    assert_eq!(d["d"]["rule_id"], "7");
    // The configuration socket has no execution intent and vice versa.
    config.silent().await;
    exec.silent().await;
    // A plain member never sees any of them, intent or not.
    member.silent().await;
}

#[tokio::test]
async fn audit_log_entries_go_only_to_members_with_view_audit_log() {
    let e = env();
    grant_moderator(&e);
    let mut moderator = connect(&e);
    moderator
        .identify("bot:1", intents::GUILD_MODERATION, 1)
        .await;
    let mut member = connect(&e);
    member.identify("bot:2", intents::GUILD_MODERATION, 1).await;
    let mut no_intent = connect(&e);
    no_intent.identify("bot:1", intents::GUILDS, 1).await;
    emit(
        &e.bus,
        DomainEvent::AuditLogEntryCreated {
            guild: G1,
            payload: json!({ "id": "99", "guild_id": "1", "action_type": 1 }),
        },
    )
    .await;
    let d = moderator.expect_event("GUILD_AUDIT_LOG_ENTRY_CREATE").await;
    assert_eq!(d["d"]["id"], "99");
    member.silent().await;
    no_intent.silent().await;
}

#[tokio::test]
async fn settings_dispatches_use_discord_intent_bits() {
    assert_eq!(
        intents::required_for("INVITE_CREATE"),
        Some(intents::GUILD_INVITES)
    );
    assert_eq!(intents::required_for("GUILD_EMOJIS_UPDATE"), Some(1 << 3));
    assert_eq!(
        intents::required_for("WEBHOOKS_UPDATE"),
        Some(intents::GUILD_WEBHOOKS)
    );
    assert_eq!(
        intents::required_for("AUTO_MODERATION_RULE_UPDATE"),
        Some(1 << 20)
    );
    assert_eq!(
        intents::required_for("AUTO_MODERATION_ACTION_EXECUTION"),
        Some(1 << 21)
    );
    assert_eq!(
        intents::required_for("GUILD_AUDIT_LOG_ENTRY_CREATE"),
        Some(intents::GUILD_MODERATION)
    );
    // All of them are accepted at IDENTIFY.
    let e = env();
    let mut c = connect(&e);
    c.identify("bot:1", ALL_SETTINGS_INTENTS, 1).await;
}

fn befriend(e: &Env, a: u64, b: u64) {
    let mut f = e.dir.friends.lock().unwrap();
    f.entry(u(a)).or_default().push(u(b));
    f.entry(u(b)).or_default().push(u(a));
}

fn presence_changed(user: u64, status: &str, custom: Option<Value>) -> DomainEvent {
    DomainEvent::PresenceChanged {
        user: u(user),
        status: status.into(),
        custom_status: custom,
    }
}

#[tokio::test]
async fn rest_presence_change_reaches_guild_members_and_shows_invisible_as_offline() {
    let e = env();
    let mut a = connect(&e);
    a.identify("user:1", 0, 1).await;
    let mut b = connect(&e);
    b.identify("user:2", 0, 1).await;
    let mut other_guild = connect(&e);
    other_guild.identify("user:3", 0, 1).await;
    settle().await;
    a.drain().await;
    b.drain().await;
    other_guild.drain().await;

    emit(&e.bus, presence_changed(1, "dnd", None)).await;
    let p = b.expect_event("PRESENCE_UPDATE").await;
    assert_eq!(p["d"]["user"]["id"], "1");
    assert_eq!(p["d"]["guild_id"], "1");
    assert_eq!(p["d"]["status"], "dnd");
    // The user's own session hears it too; a stranger in another guild does not.
    assert_eq!(
        a.expect_event("PRESENCE_UPDATE").await["d"]["status"],
        "dnd"
    );
    other_guild.silent().await;

    emit(&e.bus, presence_changed(1, "invisible", None)).await;
    assert_eq!(
        b.expect_event("PRESENCE_UPDATE").await["d"]["status"],
        "offline"
    );
    assert_eq!(e.gw.presence(u(1)).unwrap().status, "invisible");
}

#[tokio::test]
async fn custom_status_travels_as_an_activity() {
    let e = env();
    let mut a = connect(&e);
    a.identify("user:1", 0, 1).await;
    let mut b = connect(&e);
    b.identify("user:2", 0, 1).await;
    settle().await;
    b.drain().await;

    let custom = json!({ "type": 4, "name": "Custom Status", "state": "brb" });
    emit(&e.bus, presence_changed(1, "online", Some(custom.clone()))).await;
    let p = b.expect_event("PRESENCE_UPDATE").await;
    assert_eq!(p["d"]["activities"][0], custom);
}

#[tokio::test]
async fn presence_reaches_friends_outside_shared_guilds_exactly_once() {
    let e = env();
    // 5 is in no guild and friends with 1; 2 shares G1 with 1 and is a friend too.
    befriend(&e, 1, 5);
    befriend(&e, 1, 2);
    let mut a = connect(&e);
    a.identify("user:1", 0, 1).await;
    let mut friend = connect(&e);
    friend.identify("user:5", 0, 0).await;
    let mut both = connect(&e);
    both.identify("user:2", 0, 1).await;
    settle().await;
    a.drain().await;
    friend.drain().await;
    both.drain().await;

    emit(&e.bus, presence_changed(1, "idle", None)).await;
    let p = friend.expect_event("PRESENCE_UPDATE").await;
    assert_eq!(p["d"]["user"]["id"], "1");
    assert_eq!(p["d"]["status"], "idle");
    assert!(
        p["d"].get("guild_id").is_none(),
        "friend dispatch is guildless"
    );
    friend.silent().await;
    // The friend who also shares the guild gets the guild dispatch, once.
    assert_eq!(
        both.expect_event("PRESENCE_UPDATE").await["d"]["guild_id"],
        "1"
    );
    both.silent().await;

    // Invisible looks offline to friends too; going offline reaches them as well.
    emit(&e.bus, presence_changed(1, "invisible", None)).await;
    assert_eq!(
        friend.expect_event("PRESENCE_UPDATE").await["d"]["status"],
        "offline"
    );
    friend.drain().await;
    drop(a);
    assert_eq!(
        friend.expect_event("PRESENCE_UPDATE").await["d"]["status"],
        "offline"
    );
}

#[tokio::test]
async fn bots_need_guild_presences_for_presence_updates() {
    let e = env();
    e.dir.guilds.lock().unwrap().insert(u(7), vec![G1]);
    e.dir.guilds.lock().unwrap().insert(u(8), vec![G1]);
    let mut a = connect(&e);
    a.identify("user:1", 0, 1).await;
    let mut without = connect(&e);
    without.identify("bot:7", GUILDS_MSGS, 1).await;
    let mut with = connect(&e);
    with.identify("bot:8:256", intents::GUILDS | intents::GUILD_PRESENCES, 1)
        .await;
    settle().await;
    a.drain().await;
    without.drain().await;
    with.drain().await;

    emit(&e.bus, presence_changed(1, "dnd", None)).await;
    assert_eq!(
        with.expect_event("PRESENCE_UPDATE").await["d"]["status"],
        "dnd"
    );
    without.silent().await;
}

#[tokio::test]
async fn presence_merges_all_sessions_of_a_user() {
    let e = env();
    let mut watcher = connect(&e);
    watcher.identify("user:2", 0, 1).await;
    let mut first = connect(&e);
    first.identify("user:1", 0, 1).await;
    let mut second = connect(&e);
    second.identify("user:1", 0, 1).await;
    settle().await;
    watcher.drain().await;

    // A chosen dnd applies to every session.
    emit(&e.bus, presence_changed(1, "dnd", None)).await;
    assert_eq!(
        watcher.expect_event("PRESENCE_UPDATE").await["d"]["status"],
        "dnd"
    );
    assert_eq!(e.gw.presence(u(1)).unwrap().status, "dnd");
    // One session closing keeps the user present; the last one makes them offline.
    drop(first);
    settle().await;
    assert_eq!(e.gw.presence(u(1)).unwrap().status, "dnd");
    watcher.drain().await;
    drop(second);
    assert_eq!(
        watcher.expect_event("PRESENCE_UPDATE").await["d"]["status"],
        "offline"
    );
    assert_eq!(e.gw.presence(u(1)).unwrap().status, "offline");
}

#[tokio::test]
async fn user_update_reaches_guild_members_but_not_the_changed_user_twice() {
    let e = env();
    let mut a = connect(&e);
    a.identify("user:1", 0, 1).await;
    let mut b = connect(&e);
    b.identify("user:2", 0, 1).await;
    let mut stranger = connect(&e);
    stranger.identify("user:3", 0, 1).await;
    settle().await;
    a.drain().await;
    b.drain().await;
    stranger.drain().await;

    emit(
        &e.bus,
        DomainEvent::UserUpdated {
            guild: G1,
            user: u(1),
            payload: json!({ "user": { "id": "1" }, "bio": "hi" }),
        },
    )
    .await;
    assert_eq!(b.expect_event("USER_UPDATE").await["d"]["bio"], "hi");
    // The changed user gets the self-scoped copy instead; strangers get nothing.
    a.silent().await;
    stranger.silent().await;
}

// ---- threads (LNG-THR-003) ----------------------------------------------------

const THREAD: ChannelId = ChannelId::new(12);

fn thread_payload() -> Value {
    json!({ "id": "12", "guild_id": "1", "parent_id": "10", "type": 11, "name": "t" })
}

#[tokio::test]
async fn thread_events_reach_only_members_who_can_view_the_thread() {
    let e = env();
    // User 2 cannot see the (private) thread.
    e.dir.hidden.lock().unwrap().insert((u(2), THREAD));
    let mut a = connect(&e);
    a.identify("bot:1", intents::GUILDS, 1).await;
    let mut b = connect(&e);
    b.identify("bot:2", intents::GUILDS, 1).await;
    settle().await;
    emit(
        &e.bus,
        DomainEvent::ThreadCreated {
            guild: G1,
            thread: THREAD,
            payload: thread_payload(),
        },
    )
    .await;
    emit(
        &e.bus,
        DomainEvent::ThreadUpdated {
            guild: G1,
            thread: THREAD,
            payload: thread_payload(),
        },
    )
    .await;
    let created = a.expect_event("THREAD_CREATE").await;
    assert_eq!(created["d"]["id"], "12");
    a.expect_event("THREAD_UPDATE").await;
    b.silent().await;
}

#[tokio::test]
async fn thread_delete_goes_to_the_precomputed_audience() {
    let e = env();
    let mut a = connect(&e);
    a.identify("bot:1", intents::GUILDS, 1).await;
    let mut b = connect(&e);
    b.identify("bot:2", intents::GUILDS, 1).await;
    settle().await;
    emit(
        &e.bus,
        DomainEvent::ThreadDeleted {
            guild: G1,
            thread: THREAD,
            payload: json!({ "id": "12", "guild_id": "1", "parent_id": "10", "type": 11 }),
            viewers: vec![u(1)],
        },
    )
    .await;
    assert_eq!(
        a.expect_event("THREAD_DELETE").await["d"]["parent_id"],
        "10"
    );
    b.silent().await;
}

#[tokio::test]
async fn thread_members_update_needs_the_members_intent() {
    let e = env();
    let mut plain = connect(&e);
    plain.identify("bot:1", intents::GUILDS, 1).await;
    let mut privileged = connect(&e);
    privileged
        .identify(
            &format!("bot:2:{}", intents::GUILD_MEMBERS),
            intents::GUILDS | intents::GUILD_MEMBERS,
            1,
        )
        .await;
    settle().await;
    emit(
        &e.bus,
        DomainEvent::ThreadMembersUpdated {
            guild: G1,
            thread: THREAD,
            payload: json!({ "id": "12", "guild_id": "1", "member_count": 2,
                             "added_members": [{ "user_id": "7" }] }),
            also: vec![],
        },
    )
    .await;
    let v = privileged.expect_event("THREAD_MEMBERS_UPDATE").await;
    assert_eq!(v["d"]["member_count"], 2);
    plain.silent().await;
}

#[tokio::test]
async fn removed_members_still_hear_about_their_removal() {
    let e = env();
    // User 2 lost access with the removal, but is named in `also`.
    e.dir.hidden.lock().unwrap().insert((u(2), THREAD));
    let mut gone = connect(&e);
    gone.identify(
        &format!("bot:2:{}", intents::GUILD_MEMBERS),
        intents::GUILDS | intents::GUILD_MEMBERS,
        1,
    )
    .await;
    settle().await;
    emit(
        &e.bus,
        DomainEvent::ThreadMembersUpdated {
            guild: G1,
            thread: THREAD,
            payload: json!({ "id": "12", "guild_id": "1", "member_count": 1,
                             "removed_member_ids": ["2"] }),
            also: vec![u(2)],
        },
    )
    .await;
    let v = gone.expect_event("THREAD_MEMBERS_UPDATE").await;
    assert_eq!(v["d"]["removed_member_ids"], json!(["2"]));
    gone.silent().await;
}

#[tokio::test]
async fn thread_member_update_is_a_self_event() {
    let e = env();
    let mut a = connect(&e);
    a.identify("bot:1", intents::GUILDS, 1).await;
    let mut b = connect(&e);
    b.identify("bot:2", intents::GUILDS, 1).await;
    settle().await;
    emit(
        &e.bus,
        DomainEvent::ThreadMemberUpdated {
            user: u(1),
            payload: json!({ "id": "12", "user_id": "1", "guild_id": "1", "flags": 0 }),
        },
    )
    .await;
    assert_eq!(
        a.expect_event("THREAD_MEMBER_UPDATE").await["d"]["user_id"],
        "1"
    );
    b.silent().await;
}

#[tokio::test]
async fn thread_list_sync_follows_guild_create() {
    let e = env();
    e.dir.sync.lock().unwrap().insert(
        (u(1), G1),
        json!({ "guild_id": "1", "threads": [thread_payload()], "members": [] }),
    );
    let mut c = connect(&e);
    let hello = c.recv().await;
    assert_eq!(hello["op"], 10);
    c.send(json!({"op": 2, "d": {"token": "bot:1", "intents": intents::GUILDS, "properties": {}}}))
        .await;
    c.expect_event("READY").await;
    c.expect_event("GUILD_CREATE").await;
    let sync = c.expect_event("THREAD_LIST_SYNC").await;
    assert_eq!(sync["d"]["threads"][0]["id"], "12");
    // A session without the GUILDS intent gets neither.
    let mut d = connect(&e);
    d.recv().await;
    d.send(json!({"op": 2, "d": {"token": "bot:1", "intents": intents::GUILD_MESSAGES, "properties": {}}}))
        .await;
    d.expect_event("READY").await;
    d.expect_event("GUILD_CREATE").await;
    d.silent().await;
}
