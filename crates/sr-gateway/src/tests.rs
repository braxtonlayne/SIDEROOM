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
    lists: Mutex<HashMap<(GuildId, ChannelId), member_list::MemberListSource>>,
    offline: Mutex<Vec<UserId>>,
    restricted: Mutex<HashSet<ChannelId>>,
    safety: Mutex<HashMap<UserId, (sr_chat::social::safety::SensitiveMedia, HashSet<UserId>)>>,
    /// Audience sizes of the `dispatch_audience` calls, in order.
    audience_calls: Mutex<Vec<usize>>,
}

#[async_trait]
impl GuildDirectory for FakeDir {
    async fn age_confirmed(&self, _user: UserId) -> bool {
        true
    }
    async fn channel_age_restricted(&self, channel: ChannelId) -> bool {
        self.restricted.lock().unwrap().contains(&channel)
    }
    async fn sensitive_view(
        &self,
        user: UserId,
    ) -> Option<(sr_chat::social::safety::SensitiveMedia, HashSet<UserId>)> {
        self.safety.lock().unwrap().get(&user).cloned()
    }
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
    async fn dispatch_audience(
        &self,
        g: GuildId,
        channel: Option<ChannelId>,
        needed: Option<Permissions>,
        users: &[UserId],
    ) -> HashSet<UserId> {
        self.audience_calls.lock().unwrap().push(users.len());
        let mut out = HashSet::new();
        for &u in users {
            let view = channel.is_none_or(|ch| !self.hidden.lock().unwrap().contains(&(u, ch)));
            let perm = match needed {
                Some(p) => self.has_guild_permission(u, g, p).await,
                None => true,
            };
            if view && perm {
                out.insert(u);
            }
        }
        out
    }
    async fn member_list(
        &self,
        guild: GuildId,
        channel: ChannelId,
    ) -> Option<member_list::MemberListSource> {
        self.lists.lock().unwrap().get(&(guild, channel)).cloned()
    }
    async fn user_offline(&self, user: UserId) {
        self.offline.lock().unwrap().push(user);
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
        self.identify_with(token, intents, guilds, json!({})).await
    }

    /// Like `identify` with IDENTIFY `properties` (os, browser, device).
    async fn identify_with(
        &mut self,
        token: &str,
        intents: u64,
        guilds: usize,
        properties: Value,
    ) -> Value {
        let hello = self.recv().await;
        assert_eq!(hello["op"], 10);
        self.send(
            json!({"op": 2, "d": {"token": token, "intents": intents, "properties": properties}}),
        )
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

/// P24-GW-1: Chrome's intensive throttling wakes a hidden tab's chained timers once a minute, so the default
/// deadline (interval + grace) must clear 60 s with room for latency; tightening it should be deliberate.
#[test]
fn default_heartbeat_deadline_survives_a_once_a_minute_tab() {
    let c = GatewayConfig::default();
    assert!(c.heartbeat_interval + c.heartbeat_grace >= Duration::from_secs(70));
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

/// RDY-PERF-002: a guild dispatch asks the directory once for its whole audience, not once per recipient.
#[tokio::test]
async fn fan_out_asks_the_directory_once_per_dispatch() {
    let e = env();
    let mut a = connect(&e);
    a.identify("bot:1", GUILDS_MSGS, 1).await;
    let mut b = connect(&e);
    b.identify("bot:2", GUILDS_MSGS, 1).await;
    e.dir.audience_calls.lock().unwrap().clear();
    emit(&e.bus, msg(G1, OPEN, 200)).await;
    a.expect_event("MESSAGE_CREATE").await;
    b.expect_event("MESSAGE_CREATE").await;
    emit(&e.bus, msg(G1, SECRET, 201)).await;
    a.expect_event("MESSAGE_CREATE").await;
    b.silent().await;
    assert_eq!(*e.dir.audience_calls.lock().unwrap(), vec![2, 2]);
}

/// N-41, D-338: a restored message is a `MESSAGE_CREATE` with the full object and `restored: true`; a plain
/// send has no such field.
#[tokio::test]
async fn a_restored_message_is_marked_on_its_message_create() {
    let e = env();
    let mut a = connect(&e);
    a.identify("bot:1", GUILDS_MSGS, 1).await;
    emit(&e.bus, msg(G1, OPEN, 300)).await;
    let sent = a.expect_event("MESSAGE_CREATE").await;
    assert_eq!(sent["d"]["id"], "300");
    assert!(sent["d"].get("restored").is_none(), "{sent}");

    let DomainEvent::MessageSent { data, .. } = msg(G1, OPEN, 301) else {
        unreachable!()
    };
    emit(
        &e.bus,
        DomainEvent::MessageRestored {
            guild: Some(G1),
            channel: OPEN,
            message_id: 301,
            data,
        },
    )
    .await;
    let back = a.expect_event("MESSAGE_CREATE").await;
    assert_eq!(back["d"]["id"], "301");
    assert_eq!(back["d"]["content"], "hello");
    assert_eq!(back["d"]["restored"], true);
}

#[tokio::test]
async fn sensitive_media_is_filtered_per_session() {
    use sr_chat::social::safety::{MediaFilter, SensitiveMedia};
    let e = env();
    e.dir.guilds.lock().unwrap().insert(u(4), vec![G1]);
    e.dir.restricted.lock().unwrap().insert(OPEN);
    let setting = |servers| SensitiveMedia {
        friends: MediaFilter::Show,
        others: MediaFilter::Show,
        servers,
    };
    e.dir
        .safety
        .lock()
        .unwrap()
        .insert(u(2), (setting(MediaFilter::Blur), HashSet::new()));
    e.dir
        .safety
        .lock()
        .unwrap()
        .insert(u(4), (setting(MediaFilter::Block), HashSet::new()));
    // The author has the strictest setting and must still see their own file.
    e.dir
        .safety
        .lock()
        .unwrap()
        .insert(u(1), (setting(MediaFilter::Block), HashSet::new()));
    let mut author = connect(&e);
    author
        .identify("bot:1:32768", GUILDS_MSGS | intents::MESSAGE_CONTENT, 1)
        .await;
    let mut blurred = connect(&e);
    blurred
        .identify("bot:2:32768", GUILDS_MSGS | intents::MESSAGE_CONTENT, 1)
        .await;
    let mut blocked = connect(&e);
    blocked
        .identify("bot:4:32768", GUILDS_MSGS | intents::MESSAGE_CONTENT, 1)
        .await;

    let with_file = |channel: ChannelId, id: u64| DomainEvent::MessageSent {
        guild: Some(G1),
        channel,
        message_id: id,
        author: u(1),
        data: json!({
            "id": id.to_string(), "channel_id": channel.to_string(),
            "guild_id": G1.to_string(), "author": { "id": "1" },
            "content": "look", "mentions": [],
            "attachments": [{"id": "9", "filename": "cat.png", "content_type": "image/png", "url": "/cdn/x"}],
        }),
    };
    emit(&e.bus, with_file(OPEN, 200)).await;
    let a = author.expect_event("MESSAGE_CREATE").await;
    assert_eq!(a["d"]["attachments"][0]["filename"], "cat.png");
    assert_eq!(a["d"]["attachments"][0]["url"], "/cdn/x");
    let b = blurred.expect_event("MESSAGE_CREATE").await;
    assert_eq!(b["d"]["attachments"][0]["filename"], "SPOILER_cat.png");
    assert_eq!(b["d"]["attachments"][0]["url"], "/cdn/x");
    let k = blocked.expect_event("MESSAGE_CREATE").await;
    assert_eq!(k["d"]["attachments"][0]["url"], "");
    assert_eq!(k["d"]["attachments"][0]["sr_filtered"], true);

    // An ordinary channel is not a signal: the same message arrives untouched.
    emit(&e.bus, with_file(ChannelId::new(12), 202)).await;
    let plain = blocked.expect_event("MESSAGE_CREATE").await;
    assert_eq!(plain["d"]["attachments"][0]["url"], "/cdn/x");
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

#[tokio::test]
async fn platform_broadcast_reaches_every_session() {
    let e = env();
    let mut a = connect(&e);
    a.identify("bot:1", intents::GUILDS, 1).await;
    let mut b = connect(&e);
    b.identify("bot:2", 0, 1).await;
    emit(
        &e.bus,
        DomainEvent::PlatformBroadcast {
            name: "SR_FEATURE_UPDATE".into(),
            payload: json!({ "scope": "platform", "flags": { "moderation.reports": true } }),
        },
    )
    .await;
    let d = a.expect_event("SR_FEATURE_UPDATE").await;
    assert_eq!(d["d"]["scope"], "platform");
    assert_eq!(d["d"]["flags"]["moderation.reports"], true);
    let d = b.expect_event("SR_FEATURE_UPDATE").await;
    assert_eq!(d["d"]["flags"]["moderation.reports"], true);
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
async fn guild_appearance_update_is_dispatched_to_members() {
    let e = env();
    let mut member = connect(&e);
    member.identify("bot:1", intents::GUILDS, 1).await;
    emit(
        &e.bus,
        DomainEvent::GuildAppearanceUpdated {
            guild: G1,
            payload: json!({ "guild_id": "1" }),
        },
    )
    .await;
    let d = member.expect_event("SR_GUILD_APPEARANCE_UPDATE").await;
    assert_eq!(d["d"]["guild_id"], "1");
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

// ---- lazy member list (op 102, LNG-GLD-010) and the offline hook -----------------------

fn list_member(id: u64, name: &str, hoist: Option<(&str, i32)>) -> member_list::ListMember {
    member_list::ListMember {
        user: u(id),
        member: json!({ "user": { "id": id.to_string() }, "nick": name, "roles": [] }),
        hoist: hoist.map(|(r, p)| (r.to_owned(), p)),
        sort_key: name.to_owned(),
    }
}

/// Eight members of `G1` as the directory sees them for `OPEN`.
fn set_list(e: &Env, members: Vec<member_list::ListMember>) {
    e.dir.lists.lock().unwrap().insert(
        (G1, OPEN),
        member_list::MemberListSource {
            members,
            everyone: true,
        },
    );
}

fn eight() -> Vec<member_list::ListMember> {
    (1..=8)
        .map(|n| list_member(n, &format!("m{n}"), None))
        .collect()
}

/// A client's slice of the list, kept by applying dispatches the way a real client does.
#[derive(Default)]
struct Slice(Vec<Value>);

impl Slice {
    fn apply(&mut self, update: &Value) {
        for op in update["d"]["ops"].as_array().unwrap() {
            match op["op"].as_str().unwrap() {
                "SYNC" => self.0.clone_from(op["items"].as_array().unwrap()),
                "UPDATE" => self.0[op["index"].as_u64().unwrap() as usize] = op["item"].clone(),
                "INSERT" => self
                    .0
                    .insert(op["index"].as_u64().unwrap() as usize, op["item"].clone()),
                "DELETE" => {
                    self.0.remove(op["index"].as_u64().unwrap() as usize);
                }
                other => panic!("unknown op {other}"),
            }
        }
    }

    /// `g:<id>` for headers and `m:<id>` for members.
    fn keys(&self) -> Vec<String> {
        self.0
            .iter()
            .map(|i| match i.get("group") {
                Some(g) => format!("g:{}", g["id"].as_str().unwrap()),
                None => format!("m:{}", i["member"]["user"]["id"].as_str().unwrap()),
            })
            .collect()
    }
}

fn ask(guild: u64, ranges: Value) -> Value {
    json!({"op": 102, "d": {"guild_id": guild.to_string(), "channels": {OPEN.to_string(): ranges}}})
}

#[tokio::test]
async fn lazy_member_list_syncs_ranges_then_follows_presence() {
    let e = env();
    set_list(&e, eight());
    let mut a = connect(&e);
    a.identify("user:1", 0, 1).await;
    a.drain().await;
    a.send(ask(1, json!([[0, 9]]))).await;
    let first = a.expect_event("GUILD_MEMBER_LIST_UPDATE").await;
    assert_eq!(first["d"]["guild_id"], "1");
    assert_eq!(first["d"]["id"], "everyone");
    assert_eq!(first["d"]["member_count"], 8);
    assert_eq!(first["d"]["online_count"], 1);
    assert_eq!(first["d"]["ops"][0]["op"], "SYNC");
    assert_eq!(first["d"]["ops"][0]["range"], json!([0, 9]));
    assert_eq!(
        first["d"]["groups"],
        json!([{"id": "online", "count": 1}, {"id": "offline", "count": 7}])
    );
    let mut mine = Slice::default();
    mine.apply(&first);
    assert_eq!(
        mine.keys(),
        [
            "g:online",
            "m:1",
            "g:offline",
            "m:2",
            "m:3",
            "m:4",
            "m:5",
            "m:6",
            "m:7",
            "m:8"
        ]
    );
    assert_eq!(
        first["d"]["ops"][0]["items"][1]["member"]["presence"]["status"],
        "online"
    );

    // Another member comes online: the ops move them up and update the counts.
    let mut b = connect(&e);
    b.identify("user:2", 0, 1).await;
    let upd = a.expect_event("PRESENCE_UPDATE").await;
    assert_eq!(upd["d"]["user"]["id"], "2");
    let next = a.expect_event("GUILD_MEMBER_LIST_UPDATE").await;
    assert_ne!(
        next["d"]["ops"][0]["op"], "SYNC",
        "small changes are ops, not a resync"
    );
    mine.apply(&next);
    assert_eq!(
        mine.keys(),
        [
            "g:online",
            "m:1",
            "m:2",
            "g:offline",
            "m:3",
            "m:4",
            "m:5",
            "m:6",
            "m:7",
            "m:8"
        ]
    );
    assert_eq!(mine.0[0]["group"]["count"], 2);
    assert_eq!(mine.0[3]["group"]["count"], 6);
    assert_eq!(next["d"]["online_count"], 2);

    // ... and goes offline again.
    drop(b);
    a.expect_event("PRESENCE_UPDATE").await;
    let back = a.expect_event("GUILD_MEMBER_LIST_UPDATE").await;
    mine.apply(&back);
    assert_eq!(
        mine.keys(),
        [
            "g:online",
            "m:1",
            "g:offline",
            "m:2",
            "m:3",
            "m:4",
            "m:5",
            "m:6",
            "m:7",
            "m:8"
        ]
    );
}

#[tokio::test]
async fn lazy_member_list_follows_membership_and_role_events() {
    let e = env();
    set_list(&e, eight());
    let mut a = connect(&e);
    a.identify("user:1", 0, 1).await;
    a.drain().await;
    a.send(ask(1, json!([[0, 3], [4, 9]]))).await;
    let first = a.expect_event("GUILD_MEMBER_LIST_UPDATE").await;
    assert_eq!(
        first["d"]["ops"].as_array().unwrap().len(),
        2,
        "one SYNC per range"
    );
    assert_eq!(first["d"]["ops"][0]["range"], json!([0, 3]));
    assert_eq!(first["d"]["ops"][1]["range"], json!([4, 9]));
    let head = Slice(first["d"]["ops"][0]["items"].as_array().unwrap().clone());
    assert_eq!(head.keys(), ["g:online", "m:1", "g:offline", "m:2"]);

    // A member leaves: the directory no longer lists them.
    let mut rest = eight();
    rest.retain(|m| m.user != u(3));
    set_list(&e, rest);
    emit(
        &e.bus,
        DomainEvent::MemberLeft {
            guild: G1,
            user: u(3),
        },
    )
    .await;
    a.expect_event("GUILD_MEMBER_REMOVE").await;
    let upd = a.expect_event("GUILD_MEMBER_LIST_UPDATE").await;
    assert_eq!(upd["d"]["member_count"], 7);
    let ops = upd["d"]["ops"].as_array().unwrap();
    assert!(ops.iter().any(|o| o["op"] == "DELETE"), "{upd}");

    // A role is created with hoisted members: the moderator group appears on top.
    let mut hoisted = eight();
    hoisted.retain(|m| m.user != u(3));
    hoisted[0] = list_member(1, "m1", Some(("mods", 5)));
    set_list(&e, hoisted);
    emit(
        &e.bus,
        DomainEvent::RoleUpdated {
            guild: G1,
            role: sr_core::RoleId::new(9),
            payload: json!({"guild_id": "1", "role": {"id": "9"}}),
        },
    )
    .await;
    a.expect_event("GUILD_ROLE_UPDATE").await;
    let upd = a.expect_event("GUILD_MEMBER_LIST_UPDATE").await;
    assert_eq!(upd["d"]["groups"][0], json!({"id": "mods", "count": 1}));
}

#[tokio::test]
async fn lazy_member_list_requests_are_checked() {
    let e = env();
    set_list(&e, eight());
    // u2 cannot see SECRET: its list is refused (OPEN stays available).
    e.dir
        .lists
        .lock()
        .unwrap()
        .insert((G1, SECRET), member_list::MemberListSource::default());
    let mut b = connect(&e);
    b.identify("user:2", 0, 1).await;
    b.drain().await;
    b.send(json!({"op": 102, "d": {"guild_id": "1", "channels": {SECRET.to_string(): [[0, 9]]}}}))
        .await;
    b.silent().await;
    // u3 is in another guild.
    let mut c = connect(&e);
    c.identify("user:3", 0, 1).await;
    c.drain().await;
    c.send(ask(1, json!([[0, 9]]))).await;
    c.silent().await;
    // Malformed ranges, unknown channels and a directory without a list are ignored.
    b.send(ask(1, json!([[9, 0], "x"]))).await;
    b.silent().await;
    b.send(json!({"op": 102, "d": {"guild_id": "1", "channels": {"999": [[0, 9]]}}}))
        .await;
    b.silent().await;
    b.send(json!({"op": 102, "d": {"guild_id": "nope"}})).await;
    b.silent().await;
    // A good request works, and an empty range list unsubscribes.
    b.send(ask(1, json!([[0, 9]]))).await;
    b.expect_event("GUILD_MEMBER_LIST_UPDATE").await;
    b.send(ask(1, json!([]))).await;
    b.silent().await;
    let mut a = connect(&e);
    a.identify("user:1", 0, 1).await;
    settle().await;
    b.drain().await;
    emit(
        &e.bus,
        DomainEvent::MemberLeft {
            guild: G1,
            user: u(8),
        },
    )
    .await;
    b.expect_event("GUILD_MEMBER_REMOVE").await;
    b.silent().await;
}

#[tokio::test]
async fn bots_need_the_members_intent_for_the_lazy_list() {
    let e = env();
    set_list(&e, eight());
    let mut bot = connect(&e);
    bot.identify("bot:1", intents::GUILDS, 1).await;
    bot.drain().await;
    bot.send(ask(1, json!([[0, 9]]))).await;
    bot.silent().await;
}

#[tokio::test]
async fn going_offline_tells_the_directory_once_per_transition() {
    let e = env();
    let mut a = connect(&e);
    a.identify("user:1", 0, 1).await;
    let mut a2 = connect(&e);
    a2.identify("user:1", 0, 1).await;
    settle().await;
    drop(a);
    settle().await;
    assert!(
        e.dir.offline.lock().unwrap().is_empty(),
        "another socket is still attached"
    );
    drop(a2);
    settle().await;
    assert_eq!(*e.dir.offline.lock().unwrap(), vec![u(1)]);
}

// ---- LNG-PRF-013/014/015: per-session presence, idle, activities, client_status --------

#[tokio::test]
async fn idle_is_per_session_and_every_session_must_be_idle() {
    let e = env();
    let mut watcher = connect(&e);
    watcher.identify("user:2", 0, 1).await;
    let mut web = connect(&e);
    web.identify_with("user:1", 0, 1, json!({"os": "Linux", "browser": "Firefox"}))
        .await;
    let mut phone = connect(&e);
    phone
        .identify_with(
            "user:1",
            0,
            1,
            json!({"os": "Android", "browser": "SideRoom"}),
        )
        .await;
    settle().await;
    watcher.drain().await;

    // The browser goes AFK after 10 minutes; the phone is still in use.
    web.send(
        json!({"op": 3, "d": {"status": "online", "afk": true, "since": 1000, "activities": []}}),
    )
    .await;
    let p = watcher.expect_event("PRESENCE_UPDATE").await;
    assert_eq!(
        p["d"]["status"], "online",
        "one active session keeps the user online"
    );
    assert_eq!(
        p["d"]["client_status"],
        json!({"web": "idle", "mobile": "online"})
    );
    watcher.drain().await;

    // Now the phone idles too: the user shows idle.
    phone
        .send(json!({"op": 3, "d": {"status": "online", "afk": true, "activities": []}}))
        .await;
    let p = watcher.expect_event("PRESENCE_UPDATE").await;
    assert_eq!(p["d"]["status"], "idle");
    assert_eq!(e.gw.presence(u(1)).unwrap().status, "idle");
    watcher.drain().await;

    // Activity on the phone brings the user back online.
    phone
        .send(json!({"op": 3, "d": {"status": "online", "afk": false, "activities": []}}))
        .await;
    let p = watcher.expect_event("PRESENCE_UPDATE").await;
    assert_eq!(p["d"]["status"], "online");
    assert_eq!(p["d"]["client_status"]["mobile"], "online");
    // Closing the active phone leaves the idle browser: idle again.
    watcher.drain().await;
    drop(phone);
    let p = watcher.expect_event("PRESENCE_UPDATE").await;
    assert_eq!(p["d"]["status"], "idle");
    assert_eq!(p["d"]["client_status"], json!({"web": "idle"}));
}

#[tokio::test]
async fn activities_are_sanitised_and_cleared_when_the_session_leaves() {
    let e = env();
    let mut watcher = connect(&e);
    watcher.identify("user:2", 0, 1).await;
    let mut a = connect(&e);
    a.identify("user:1", 0, 1).await;
    settle().await;
    watcher.drain().await;

    a.send(json!({"op": 3, "d": {"status": "online", "activities": [
        {"type": 6, "name": "Some Manga", "state": "Chapter 4", "secret": "dropped"},
        {"type": 99, "name": "bad"},
        {"type": 0, "name": ""}
    ]}}))
    .await;
    let p = watcher.expect_event("PRESENCE_UPDATE").await;
    assert_eq!(
        p["d"]["activities"],
        json!([{"type": 6, "name": "Some Manga", "state": "Chapter 4"}])
    );
    watcher.drain().await;
    drop(a);
    let p = watcher.expect_event("PRESENCE_UPDATE").await;
    assert_eq!(p["d"]["status"], "offline");
    assert_eq!(e.gw.presence(u(1)).unwrap().activities, Vec::<Value>::new());
}

#[tokio::test]
async fn dnd_from_one_session_applies_to_all_and_invisible_hides_the_platforms() {
    let e = env();
    let mut watcher = connect(&e);
    watcher.identify("user:2", 0, 1).await;
    let mut a = connect(&e);
    a.identify("user:1", 0, 1).await;
    let mut b = connect(&e);
    b.identify("user:1", 0, 1).await;
    settle().await;
    watcher.drain().await;

    a.send(json!({"op": 3, "d": {"status": "dnd", "activities": []}}))
        .await;
    assert_eq!(
        watcher.expect_event("PRESENCE_UPDATE").await["d"]["status"],
        "dnd"
    );
    watcher.drain().await;
    a.send(json!({"op": 3, "d": {"status": "invisible", "activities": []}}))
        .await;
    let p = watcher.expect_event("PRESENCE_UPDATE").await;
    assert_eq!(p["d"]["status"], "offline");
    assert_eq!(
        p["d"]["client_status"],
        json!({}),
        "invisible shows no platform"
    );
}

// ---------------------------------------------------------------- sharding (DEV-BOT-002)

/// `(guild_id >> 22) % shard_count` picks the shard: these two guilds land on shards 0 and 1
/// of 2 (and both on shard 0 of 1).
const GA: GuildId = GuildId::new(5);
const GB: GuildId = GuildId::new((1 << 22) | 5);

fn sharded_env() -> Env {
    let e = env();
    e.dir.guilds.lock().unwrap().insert(u(6), vec![GA, GB]);
    e
}

async fn shard_client(e: &Env, shard: Value) -> Client {
    let mut c = connect(e);
    c.recv().await;
    c.send(json!({"op": 2, "d": {"token": "bot:6", "intents": GUILDS_MSGS, "shard": shard}}))
        .await;
    c
}

#[tokio::test]
async fn invalid_shards_are_refused_and_the_default_is_accepted() {
    let e = sharded_env();
    let mut c = shard_client(&e, json!([0, 1])).await;
    c.expect_event("READY").await;
    for shard in [
        json!([0, 0]),
        json!([2, 2]),
        json!([5, 2]),
        json!([0]),
        json!([0, 1025]),
        json!([0, 1, 2]),
    ] {
        let mut c = shard_client(&e, shard.clone()).await;
        assert_eq!(c.recv_close().await, close::INVALID_SHARD, "{shard}");
    }
    // Bots may use any count up to the limit, users never shard and are not asked to.
    let mut c = shard_client(&e, json!([1023, 1024])).await;
    c.expect_event("READY").await;
    let mut c = connect(&e);
    c.recv().await;
    c.send(json!({"op": 2, "d": {"token": "user:1", "shard": [3, 9]}}))
        .await;
    c.expect_event("READY").await;
}

#[tokio::test]
async fn a_shard_only_gets_the_guilds_it_owns() {
    let e = sharded_env();
    let mut s0 = shard_client(&e, json!([0, 2])).await;
    let ready = s0.expect_event("READY").await;
    assert_eq!(ready["d"]["shard"], json!([0, 2]));
    assert_eq!(ready["d"]["guilds"].as_array().unwrap().len(), 1);
    assert_eq!(ready["d"]["guilds"][0]["id"], json!(GA));
    let gc = s0.expect_event("GUILD_CREATE").await;
    assert_eq!(gc["d"]["id"], json!(GA));

    let mut s1 = shard_client(&e, json!([1, 2])).await;
    let ready = s1.expect_event("READY").await;
    assert_eq!(ready["d"]["guilds"][0]["id"], json!(GB));
    assert_eq!(s1.expect_event("GUILD_CREATE").await["d"]["id"], json!(GB));

    // one shard of one gets both
    let mut all = shard_client(&e, json!([0, 1])).await;
    assert_eq!(
        all.expect_event("READY").await["d"]["guilds"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn guild_events_and_joins_reach_only_the_owning_shard() {
    let e = sharded_env();
    let mut s0 = shard_client(&e, json!([0, 2])).await;
    s0.expect_event("READY").await;
    s0.expect_event("GUILD_CREATE").await;
    let mut s1 = shard_client(&e, json!([1, 2])).await;
    s1.expect_event("READY").await;
    s1.expect_event("GUILD_CREATE").await;

    emit(&e.bus, msg(GB, OPEN, 900)).await;
    let m = s1.expect_event("MESSAGE_CREATE").await;
    assert_eq!(m["d"]["id"], "900");
    settle().await;
    s0.silent().await;

    emit(&e.bus, msg(GA, OPEN, 901)).await;
    assert_eq!(s0.expect_event("MESSAGE_CREATE").await["d"]["id"], "901");
    settle().await;
    s1.silent().await;

    // the bot joins another guild owned by shard 1: only that shard sees GUILD_CREATE
    let gc = GuildId::new((3 << 22) | 9);
    e.dir
        .guilds
        .lock()
        .unwrap()
        .get_mut(&u(6))
        .unwrap()
        .push(gc);
    emit(
        &e.bus,
        DomainEvent::MemberJoined {
            guild: gc,
            user: u(6),
            payload: json!({}),
        },
    )
    .await;
    assert_eq!(s1.expect_event("GUILD_CREATE").await["d"]["id"], json!(gc));
    settle().await;
    s0.silent().await;
}

#[tokio::test]
async fn user_scoped_events_follow_the_guild_or_go_to_shard_zero() {
    let e = sharded_env();
    let mut s0 = shard_client(&e, json!([0, 2])).await;
    s0.expect_event("READY").await;
    s0.expect_event("GUILD_CREATE").await;
    let mut s1 = shard_client(&e, json!([1, 2])).await;
    s1.expect_event("READY").await;
    s1.expect_event("GUILD_CREATE").await;
    let scoped = |payload: Value| DomainEvent::UserScoped {
        users: vec![u(6)],
        name: "INTERACTION_CREATE".to_owned(),
        payload,
    };
    emit(&e.bus, scoped(json!({"guild_id": GB.to_string(), "n": 1}))).await;
    assert_eq!(s1.expect_event("INTERACTION_CREATE").await["d"]["n"], 1);
    emit(&e.bus, scoped(json!({"n": 2}))).await;
    assert_eq!(s0.expect_event("INTERACTION_CREATE").await["d"]["n"], 2);
    settle().await;
    s0.silent().await;
    s1.silent().await;
}

/// PAR-FU-002: the likely-spam flag is the recipient's view; the author's session gets the message without it.
#[tokio::test]
async fn the_author_never_receives_the_likely_spam_flag() {
    let e = env();
    let mut author = connect(&e);
    author.identify("bot:1", GUILDS_MSGS, 1).await;
    let mut reader = connect(&e);
    reader.identify("bot:2", GUILDS_MSGS, 1).await;
    let flagged = 1u64 << 29;
    let mut ev = msg(G1, OPEN, 300);
    if let DomainEvent::MessageSent { data, .. } = &mut ev {
        data["flags"] = json!(flagged | 4);
    }
    emit(&e.bus, ev).await;
    // Author is user 1: the bit is gone, the other flags stay.
    assert_eq!(author.expect_event("MESSAGE_CREATE").await["d"]["flags"], 4);
    assert_eq!(
        reader.expect_event("MESSAGE_CREATE").await["d"]["flags"],
        flagged | 4
    );
}
