//! Gateway test client utilities (XC-TEST-002): connect as N users against an in-process
//! [`Gateway`] (no sockets) and await dispatches.
//!
//! Enabled for this crate's own tests and, for other crates' tests, with the `test-support`
//! feature:
//!
//! ```toml
//! [dev-dependencies]
//! sr-gateway = { path = "../sr-gateway", features = ["test-support"] }
//! ```
//!
//! ```ignore
//! let world = TestWorld::new(GatewayConfig::default());
//! world.add_member(UserId::new(1), GuildId::new(1));
//! world.add_member(UserId::new(2), GuildId::new(1));
//! let mut users = world.connect_users(&[1, 2], intents::GUILDS | intents::GUILD_MESSAGES).await;
//! world.emit(event).await;
//! let d = users[1].await_dispatch("MESSAGE_CREATE").await;
//! ```

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use sr_core::{ChannelId, GuildId, UserId};
use sr_events::{DomainEvent, EventBus, InProcessBus};
use tokio::sync::mpsc;

use crate::{
    AuthError, AuthedUser, Authenticator, Compression, ConnectParams, Frame, Gateway,
    GatewayConfig, GuildDirectory, run_connection,
};

/// How long [`TestClient`] waits for a frame before failing the test.
pub const FRAME_TIMEOUT: Duration = Duration::from_secs(30);

/// Authenticator for tests. Tokens are `user:<id>` or `bot:<id>[:<allowed privileged bits>]`;
/// anything else is rejected. [`token_for`] builds them.
pub struct TestAuth;

/// Token for a human test user.
pub fn token_for(user: u64) -> String {
    format!("user:{user}")
}

#[async_trait]
impl Authenticator for TestAuth {
    async fn authenticate(&self, token: &str) -> Result<AuthedUser, AuthError> {
        let parts: Vec<&str> = token.split(':').collect();
        let id = parts.get(1).and_then(|s| s.parse::<u64>().ok());
        match (parts.first().copied(), id) {
            (Some(kind @ ("user" | "bot")), Some(id)) => Ok(AuthedUser {
                user_id: UserId::new(id),
                bot: kind == "bot",
                allowed_privileged_intents: parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0),
                user: json!({ "id": id.to_string() }),
            }),
            _ => Err(AuthError::Invalid),
        }
    }
}

/// In-memory membership and channel visibility.
#[derive(Default)]
pub struct TestDirectory {
    guilds: Mutex<HashMap<UserId, Vec<GuildId>>>,
    hidden: Mutex<HashSet<(UserId, ChannelId)>>,
    restricted: Mutex<HashSet<ChannelId>>,
    adults: Mutex<HashSet<UserId>>,
}

#[async_trait]
impl GuildDirectory for TestDirectory {
    async fn guilds_for_user(&self, user: UserId) -> Vec<GuildId> {
        lock(&self.guilds).get(&user).cloned().unwrap_or_default()
    }
    async fn can_view_channel(&self, user: UserId, _guild: GuildId, channel: ChannelId) -> bool {
        !lock(&self.hidden).contains(&(user, channel))
    }
    async fn channel_age_restricted(&self, channel: ChannelId) -> bool {
        lock(&self.restricted).contains(&channel)
    }
    async fn age_confirmed(&self, user: UserId) -> bool {
        lock(&self.adults).contains(&user)
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A running in-process gateway plus its bus and directory.
pub struct TestWorld {
    pub gw: Gateway,
    pub bus: Arc<InProcessBus>,
    pub dir: Arc<TestDirectory>,
}

impl TestWorld {
    /// Must be called inside a Tokio runtime (it starts the gateway's event pump).
    pub fn new(cfg: GatewayConfig) -> Self {
        let bus = Arc::new(InProcessBus::new());
        let dir = Arc::new(TestDirectory::default());
        let gw = Gateway::new(cfg, Arc::new(TestAuth), dir.clone(), bus.clone());
        gw.start();
        Self { gw, bus, dir }
    }

    /// Make `user` a member of `guild` (takes effect for sessions identified afterwards).
    pub fn add_member(&self, user: UserId, guild: GuildId) {
        let mut g = lock(&self.dir.guilds);
        let list = g.entry(user).or_default();
        if !list.contains(&guild) {
            list.push(guild);
        }
    }

    /// Deny `user` `VIEW_CHANNEL` on `channel`.
    pub fn hide_channel(&self, user: UserId, channel: ChannelId) {
        lock(&self.dir.hidden).insert((user, channel));
    }

    /// Mark `channel` as age-restricted (`nsfw`, or a thread of an `nsfw` channel).
    pub fn restrict_channel(&self, channel: ChannelId) {
        lock(&self.dir.restricted).insert(channel);
    }

    /// Record that `user` answered yes to the age gate.
    pub fn confirm_age(&self, user: UserId) {
        lock(&self.dir.adults).insert(user);
    }

    /// Publish a domain event on every topic it belongs to.
    pub async fn emit(&self, ev: DomainEvent) {
        for t in ev.topics() {
            // The in-process bus only fails when a topic is closed; tests treat that as a bug.
            self.bus
                .publish(t, ev.clone())
                .await
                .expect("publish test event");
        }
    }

    /// Connect one client without identifying.
    pub fn connect(&self) -> TestClient {
        TestClient::connect(&self.gw, Compression::None)
    }

    /// Connect and identify as each user id, in order. Each client has consumed HELLO, READY and one
    /// GUILD_CREATE per guild its user belongs to.
    pub async fn connect_users(&self, users: &[u64], intents: u64) -> Vec<TestClient> {
        let mut out = Vec::with_capacity(users.len());
        for &u in users {
            let guilds = lock(&self.dir.guilds)
                .get(&UserId::new(u))
                .map_or(0, Vec::len);
            let mut c = self.connect();
            c.identify(&token_for(u), intents, guilds).await;
            out.push(c);
        }
        out
    }
}

/// A fake WebSocket peer driving [`run_connection`] over channels.
pub struct TestClient {
    tx: mpsc::Sender<Frame>,
    rx: mpsc::Receiver<Frame>,
    zstd: bool,
}

impl TestClient {
    pub fn connect(gw: &Gateway, compression: Compression) -> Self {
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

    /// Send a JSON payload as a text frame.
    pub async fn send(&self, v: Value) {
        self.send_raw(Frame::Text(v.to_string())).await;
    }

    pub async fn send_raw(&self, f: Frame) {
        self.tx.send(f).await.expect("gateway connection closed");
    }

    pub async fn frame(&mut self) -> Frame {
        tokio::time::timeout(FRAME_TIMEOUT, self.rx.recv())
            .await
            .expect("timed out waiting for a frame")
            .expect("connection ended without a frame")
    }

    /// Next payload (decompressing if the stream is zstd). Panics on a close frame.
    pub async fn recv(&mut self) -> Value {
        match self.frame().await {
            Frame::Text(t) => serde_json::from_str(&t).expect("valid JSON frame"),
            Frame::Binary(b) => {
                assert!(self.zstd, "binary frame without compression");
                let mut dec = ruzstd::decoding::StreamingDecoder::new(&b[..]).expect("zstd frame");
                let mut s = String::new();
                dec.read_to_string(&mut s).expect("zstd payload");
                serde_json::from_str(&s).expect("valid JSON frame")
            }
            Frame::Close { code, .. } => panic!("unexpected close {code}"),
        }
    }

    /// Wait for the connection to close and return the close code.
    pub async fn recv_close(&mut self) -> u16 {
        loop {
            if let Frame::Close { code, .. } = self.frame().await {
                return code;
            }
        }
    }

    /// The next payload must be dispatch `name`.
    pub async fn expect_event(&mut self, name: &str) -> Value {
        let v = self.recv().await;
        assert_eq!(v["op"], 0, "{v}");
        assert_eq!(v["t"], name, "{v}");
        v
    }

    /// Skip payloads until dispatch `name` arrives; returns the whole payload (`d` is the body).
    pub async fn await_dispatch(&mut self, name: &str) -> Value {
        loop {
            let v = self.recv().await;
            if v["op"] == 0 && v["t"] == name {
                return v;
            }
        }
    }

    /// Like [`Self::await_dispatch`] but gives up after `within`, returning `None`.
    pub async fn try_dispatch(&mut self, name: &str, within: Duration) -> Option<Value> {
        tokio::time::timeout(within, self.await_dispatch(name))
            .await
            .ok()
    }

    /// Assert nothing arrives for a short while.
    pub async fn assert_silent(&mut self) {
        let r = tokio::time::timeout(Duration::from_millis(150), self.rx.recv()).await;
        assert!(r.is_err(), "expected no frame, got {r:?}");
    }

    /// Discard everything queued until the stream goes quiet.
    pub async fn drain(&mut self) {
        while tokio::time::timeout(Duration::from_millis(80), self.rx.recv())
            .await
            .is_ok()
        {}
    }

    /// HELLO, IDENTIFY, READY, then `guilds` GUILD_CREATEs. Returns READY.
    pub async fn identify(&mut self, token: &str, intents: u64, guilds: usize) -> Value {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intents;

    fn message(guild: u64, channel: u64, id: u64) -> DomainEvent {
        DomainEvent::MessageSent {
            guild: Some(GuildId::new(guild)),
            channel: ChannelId::new(channel),
            message_id: id,
            author: UserId::new(1),
            data: json!({
                "id": id.to_string(), "channel_id": channel.to_string(),
                "guild_id": guild.to_string(), "author": { "id": "1" },
                "content": "hi", "mentions": [],
            }),
        }
    }

    #[tokio::test]
    async fn n_users_receive_a_dispatch_and_hidden_channels_stay_silent() {
        let w = TestWorld::new(GatewayConfig::default());
        for u in 1..=3 {
            w.add_member(UserId::new(u), GuildId::new(1));
        }
        w.hide_channel(UserId::new(3), ChannelId::new(10));
        let mut users = w
            .connect_users(&[1, 2, 3], intents::GUILDS | intents::GUILD_MESSAGES)
            .await;
        w.emit(message(1, 10, 100)).await;
        for c in &mut users[..2] {
            let d = c.await_dispatch("MESSAGE_CREATE").await;
            assert_eq!(d["d"]["id"], "100");
        }
        assert!(
            users[2]
                .try_dispatch("MESSAGE_CREATE", Duration::from_millis(150))
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn bad_token_closes_with_authentication_failed() {
        let w = TestWorld::new(GatewayConfig::default());
        let mut c = w.connect();
        c.recv().await;
        c.send(json!({"op": 2, "d": {"token": "nope", "intents": 0}}))
            .await;
        assert_eq!(c.recv_close().await, crate::close::AUTHENTICATION_FAILED);
    }
}

#[cfg(test)]
mod age_gate_tests {
    use super::*;
    use crate::intents;

    fn event(name: &str, channel: u64) -> DomainEvent {
        let (g, c) = (GuildId::new(1), ChannelId::new(channel));
        match name {
            "MESSAGE_CREATE" => DomainEvent::MessageSent {
                guild: Some(g),
                channel: c,
                message_id: 5,
                author: UserId::new(9),
                data: json!({
                    "id": "5", "channel_id": channel.to_string(), "guild_id": "1",
                    "author": { "id": "9" }, "content": "x", "mentions": [],
                }),
            },
            "TYPING_START" => DomainEvent::TypingStarted {
                guild: Some(g),
                channel: c,
                user: UserId::new(9),
                timestamp: 1,
            },
            _ => unreachable!(),
        }
    }

    const INTENTS: u64 = intents::GUILDS | intents::GUILD_MESSAGES | intents::GUILD_MESSAGE_TYPING;

    #[tokio::test]
    async fn restricted_channels_are_silent_for_unconfirmed_sessions() {
        let w = TestWorld::new(GatewayConfig::default());
        for u in 1..=2 {
            w.add_member(UserId::new(u), GuildId::new(1));
        }
        w.restrict_channel(ChannelId::new(10));
        w.confirm_age(UserId::new(2));
        let mut users = w.connect_users(&[1, 2], INTENTS).await;
        for name in ["MESSAGE_CREATE", "TYPING_START"] {
            w.emit(event(name, 10)).await;
            assert!(
                users[0]
                    .try_dispatch(name, Duration::from_millis(150))
                    .await
                    .is_none()
            );
            users[1].await_dispatch(name).await;
        }
        // An unrestricted channel reaches both.
        w.emit(event("MESSAGE_CREATE", 11)).await;
        users[0].await_dispatch("MESSAGE_CREATE").await;
    }

    #[tokio::test]
    async fn confirming_mid_session_opens_the_channel_after_sr_user_update() {
        let w = TestWorld::new(GatewayConfig::default());
        w.add_member(UserId::new(1), GuildId::new(1));
        w.restrict_channel(ChannelId::new(10));
        let mut c = w.connect_users(&[1], INTENTS).await.remove(0);
        w.emit(event("MESSAGE_CREATE", 10)).await;
        assert!(
            c.try_dispatch("MESSAGE_CREATE", Duration::from_millis(150))
                .await
                .is_none()
        );
        // The answer is cached per session until the account update arrives.
        w.confirm_age(UserId::new(1));
        w.emit(event("MESSAGE_CREATE", 10)).await;
        assert!(
            c.try_dispatch("MESSAGE_CREATE", Duration::from_millis(150))
                .await
                .is_none()
        );
        w.emit(DomainEvent::UserScoped {
            users: vec![UserId::new(1)],
            name: "SR_USER_UPDATE".into(),
            payload: json!({ "age_confirmed": true }),
        })
        .await;
        c.await_dispatch("SR_USER_UPDATE").await;
        w.emit(event("MESSAGE_CREATE", 10)).await;
        c.await_dispatch("MESSAGE_CREATE").await;
    }
}

#[cfg(test)]
mod activity_tests {
    use super::*;
    use crate::intents;

    #[tokio::test]
    async fn connected_sessions_are_active_until_every_one_is_idle() {
        let w = TestWorld::new(GatewayConfig::default());
        let me = UserId::new(1);
        w.add_member(me, GuildId::new(1));
        assert!(!w.gw.has_active_session(me), "nobody connected");
        let mut a = w.connect_users(&[1], intents::GUILDS).await.remove(0);
        assert!(w.gw.has_active_session(me), "connected, no report: active");
        a.send(json!({"op": 3, "d": {"status": "online", "afk": true, "activities": []}}))
            .await;
        a.drain().await;
        assert!(!w.gw.has_active_session(me), "the only session is idle");
        let mut b = w.connect_users(&[1], intents::GUILDS).await.remove(0);
        assert!(w.gw.has_active_session(me), "a second, active session");
        b.send(json!({"op": 3, "d": {"status": "idle", "afk": false, "activities": []}}))
            .await;
        b.drain().await;
        assert!(!w.gw.has_active_session(me), "status idle counts as idle");
        a.send(json!({"op": 3, "d": {"status": "online", "afk": false, "activities": []}}))
            .await;
        a.drain().await;
        assert!(w.gw.has_active_session(me), "input clears afk");
    }
}
