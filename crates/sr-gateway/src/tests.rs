//! Protocol tests driven by an in-process fake client (no sockets).

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use sr_core::{ChannelId, GuildId, UserId};
use sr_events::{DomainEvent, EventBus, InProcessBus};
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
    DomainEvent::MessageCreated {
        guild: Some(guild),
        channel,
        message_id: id,
        author: u(1),
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
            ConnectParams { compression },
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

    e.gw.typing_start(u(1), G1, SECRET).await;
    let t = a.expect_event("TYPING_START").await;
    assert_eq!(t["d"]["user_id"], "1");
    assert_eq!(t["d"]["channel_id"], "11");
    b.silent().await;
    assert_eq!(e.gw.typing_users(SECRET), vec![u(1)]);
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(e.gw.typing_users(SECRET).is_empty());
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
