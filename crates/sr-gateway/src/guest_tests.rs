//! ARC-GST-001: the guest socket enforces its one topic at the relay (no sockets: the state machine is driven
//! through channels, like `run_connection`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sr_core::UserId;
use sr_voice::lobby::LobbyHub;
use tokio::sync::mpsc;

use crate::guest::{
    CLOSE_BAD_FRAME, CLOSE_FORBIDDEN, CLOSE_FULL, CLOSE_GONE, CLOSE_TOO_LARGE, CLOSE_UNAUTHORIZED,
    GuestAuthority, GuestConfig, GuestGateway, MAX_FRAME, Seating, run_guest,
};
use crate::{AuthError, AuthedUser, Authenticator, ConnectParams, Frame};

fn lobby_token(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}

fn digest(t: &str) -> [u8; 32] {
    Sha256::digest(t.as_bytes()).into()
}

#[derive(Default)]
struct World {
    /// guest token -> (lobby token, callsign)
    guests: Mutex<HashMap<String, (String, String)>>,
    /// (user, lobby token) allowed to sit
    members: Mutex<Vec<(u64, String)>>,
}

struct Fake(Arc<World>);

#[async_trait]
impl GuestAuthority for Fake {
    async fn guest_seating(&self, t: &str) -> Result<Option<Seating>, String> {
        Ok(self.0.guests.lock().unwrap().get(t).map(|(l, c)| Seating {
            lobby: digest(l),
            member: false,
            callsign: c.clone(),
        }))
    }
    async fn member_seating(&self, u: UserId, l: &str) -> Result<Option<Seating>, String> {
        let ok = self
            .0
            .members
            .lock()
            .unwrap()
            .iter()
            .any(|(m, t)| *m == u.get() && t == l);
        Ok(ok.then(|| Seating {
            lobby: digest(l),
            member: true,
            callsign: "Host".into(),
        }))
    }
}

struct Cookies;

#[async_trait]
impl Authenticator for Cookies {
    async fn authenticate(&self, _t: &str) -> Result<AuthedUser, AuthError> {
        Err(AuthError::Invalid)
    }
    async fn authenticate_session_cookie(&self, c: &str) -> Result<AuthedUser, AuthError> {
        let id: u64 = c
            .strip_prefix("user")
            .and_then(|s| s.parse().ok())
            .ok_or(AuthError::Invalid)?;
        Ok(AuthedUser {
            user_id: UserId::new(id),
            bot: false,
            allowed_privileged_intents: 0,
            user: json!({"id": id.to_string()}),
        })
    }
}

struct Client {
    tx: mpsc::Sender<Frame>,
    rx: mpsc::Receiver<Frame>,
}

impl Client {
    fn connect(gg: &GuestGateway, params: ConnectParams) -> Client {
        let (c_tx, in_rx) = mpsc::channel(64);
        let (out_tx, c_rx) = mpsc::channel(64);
        tokio::spawn(run_guest(gg.clone(), params, in_rx, out_tx));
        Client { tx: c_tx, rx: c_rx }
    }
    async fn send(&self, v: Value) {
        self.tx.send(Frame::Text(v.to_string())).await.unwrap();
    }
    async fn next(&mut self) -> Frame {
        tokio::time::timeout(Duration::from_secs(5), self.rx.recv())
            .await
            .expect("a frame in time")
            .expect("the socket stayed open")
    }
    async fn json(&mut self) -> Value {
        match self.next().await {
            Frame::Text(t) => serde_json::from_str(&t).unwrap(),
            other => panic!("expected text, got {other:?}"),
        }
    }
    async fn closed_with(&mut self) -> u16 {
        loop {
            match tokio::time::timeout(Duration::from_secs(5), self.rx.recv()).await {
                Ok(Some(Frame::Close { code, .. })) => return code,
                Ok(Some(_)) => continue,
                other => panic!("expected a close, got {other:?}"),
            }
        }
    }
    async fn silent(&mut self) {
        assert!(
            tokio::time::timeout(Duration::from_millis(150), self.rx.recv())
                .await
                .is_err(),
            "nothing should arrive"
        );
    }
}

fn setup(recheck_ms: u64, seats: usize) -> (GuestGateway, Arc<World>) {
    let world = Arc::new(World::default());
    let gg = GuestGateway::new(
        Arc::new(Cookies),
        Arc::new(Fake(world.clone())),
        LobbyHub::new(256, seats),
        GuestConfig {
            hello_timeout: Duration::from_millis(300),
            recheck: Duration::from_millis(recheck_ms),
            allowed_origins: vec!["https://chat.example".into()],
            max_connections: 100,
        },
    );
    (gg, world)
}

fn add_guest(w: &World, token: &str, lobby: &str, callsign: &str) {
    w.guests
        .lock()
        .unwrap()
        .insert(token.into(), (lobby.into(), callsign.into()));
}

fn member_params() -> ConnectParams {
    ConnectParams {
        session_cookie: Some("user7".into()),
        origin: Some("https://chat.example".into()),
        ..ConnectParams::default()
    }
}

async fn guest_in(gg: &GuestGateway, token: &str) -> (Client, Value) {
    let mut c = Client::connect(gg, ConnectParams::default());
    c.send(json!({"op": "hello", "guest_token": token})).await;
    let ready = c.json().await;
    assert_eq!(ready["op"], "ready", "{ready}");
    (c, ready)
}

#[tokio::test]
async fn a_guest_and_the_host_exchange_opaque_frames_on_their_one_topic() {
    let (gg, w) = setup(5000, 8);
    let lobby = lobby_token(1);
    add_guest(&w, "srg_a", &lobby, "Amber Otter 42");
    w.members.lock().unwrap().push((7, lobby.clone()));

    let mut host = Client::connect(&gg, member_params());
    host.send(json!({"op": "hello", "lobby_token": lobby}))
        .await;
    let ready = host.json().await;
    assert_eq!(ready["role"], "member");
    assert_eq!(ready["scope"], json!({"data": true, "media": false}));
    assert_eq!(ready["key_id"].as_str().unwrap().len(), 32);

    let (mut guest, gready) = guest_in(&gg, "srg_a").await;
    assert_eq!(gready["role"], "guest");
    assert_eq!(gready["peers"].as_array().unwrap().len(), 1);
    let joined = host.json().await;
    assert_eq!(joined["op"], "peer");
    assert_eq!(joined["peer"]["callsign"], "Amber Otter 42");

    let topic = format!("game:{lobby}");
    guest
        .send(json!({"op": "publish", "topic": topic, "data": "aGVsbG8="}))
        .await;
    let d = host.json().await;
    assert_eq!(d["op"], "data");
    assert_eq!(d["data"], "aGVsbG8=");
    host.send(json!({"op": "publish", "topic": topic, "data": "d29ybGQ="}))
        .await;
    assert_eq!(guest.json().await["data"], "d29ybGQ=");
    guest.send(json!({"op": "subscribe", "topic": topic})).await;
    assert_eq!(guest.json().await["op"], "subscribed");
}

#[tokio::test]
async fn a_second_subscription_is_dropped_before_anything_reaches_the_lobby() {
    let (gg, w) = setup(5000, 8);
    let mine = lobby_token(0xab);
    let other = lobby_token(2);
    add_guest(&w, "srg_a", &mine, "A");
    add_guest(&w, "srg_b", &other, "B");
    let (mut victim, _) = guest_in(&gg, "srg_b").await;

    for forged in [
        // another lobby, a channel, a guild, the call, a user, an unshaped game topic
        json!({"op": "subscribe", "topic": format!("game:{other}")}),
        json!({"op": "subscribe", "topic": "channel:1234"}),
        json!({"op": "subscribe", "topic": "guild:1234"}),
        json!({"op": "subscribe", "topic": "voice:1234"}),
        json!({"op": "subscribe", "topic": "user:1234"}),
        json!({"op": "subscribe", "topic": "game:*"}),
        json!({"op": "subscribe", "topic": format!("game:{}", mine.to_uppercase())}),
        json!({"op": "subscribe"}),
        // publishing into another lobby
        json!({"op": "publish", "topic": format!("game:{other}"), "data": "eA=="}),
        // gateway ops: identify, resume, voice state, anything the main gateway speaks
        json!({"op": "identify", "token": "x"}),
        json!({"op": "unsubscribe", "topic": format!("game:{mine}")}),
        json!({"op": 2, "d": {}}),
    ] {
        let (mut g, _) = guest_in(&gg, "srg_a").await;
        g.send(forged.clone()).await;
        let code = g.closed_with().await;
        assert!(
            code == CLOSE_FORBIDDEN || code == CLOSE_BAD_FRAME,
            "{forged}: {code}"
        );
    }
    // The forged frames reached nobody in the other lobby.
    victim.silent().await;
    assert_eq!(gg.hub().seats(lobby_id(&other)), 1);
}

fn lobby_id(token: &str) -> sr_voice::lobby::LobbyId {
    sr_voice::lobby::LobbyId(digest(token))
}

#[tokio::test]
async fn binary_frames_and_frames_before_hello_are_dropped() {
    let (gg, w) = setup(5000, 8);
    add_guest(&w, "srg_a", &lobby_token(1), "A");
    let mut c = Client::connect(&gg, ConnectParams::default());
    c.send(json!({"op": "publish", "topic": format!("game:{}", lobby_token(1)), "data": "eA=="}))
        .await;
    assert_eq!(c.closed_with().await, CLOSE_FORBIDDEN);

    let (mut g, _) = guest_in(&gg, "srg_a").await;
    g.tx.send(Frame::Binary(vec![1, 2, 3])).await.unwrap();
    assert_eq!(g.closed_with().await, CLOSE_FORBIDDEN);

    let (mut g, _) = guest_in(&gg, "srg_a").await;
    g.tx.send(Frame::Text("not json".into())).await.unwrap();
    assert_eq!(g.closed_with().await, CLOSE_BAD_FRAME);

    let (mut g, _) = guest_in(&gg, "srg_a").await;
    g.tx.send(Frame::Text("x".repeat(MAX_FRAME + 1)))
        .await
        .unwrap();
    assert_eq!(g.closed_with().await, CLOSE_TOO_LARGE);
}

#[tokio::test]
async fn bad_credentials_are_refused_and_a_silent_socket_times_out() {
    let (gg, w) = setup(5000, 8);
    add_guest(&w, "srg_a", &lobby_token(1), "A");
    for hello in [
        json!({"op": "hello", "guest_token": "srg_unknown"}),
        json!({"op": "hello"}),
        // a member seat without a participant row
        json!({"op": "hello", "lobby_token": lobby_token(1)}),
    ] {
        let mut c = Client::connect(&gg, member_params());
        c.send(hello).await;
        assert_eq!(c.closed_with().await, CLOSE_UNAUTHORIZED);
    }
    let mut silent = Client::connect(&gg, ConnectParams::default());
    assert_eq!(silent.closed_with().await, CLOSE_UNAUTHORIZED);
    assert_eq!(gg.hub().lobbies(), 0);
}

#[tokio::test]
async fn a_member_seat_needs_the_cookie_an_allowed_origin_and_a_participant_row() {
    let (gg, w) = setup(5000, 8);
    let lobby = lobby_token(1);
    w.members.lock().unwrap().push((7, lobby.clone()));
    for p in [
        ConnectParams {
            origin: None,
            ..member_params()
        },
        ConnectParams {
            origin: Some("https://evil.example".into()),
            ..member_params()
        },
        ConnectParams {
            session_cookie: None,
            ..member_params()
        },
        ConnectParams {
            session_cookie: Some("user8".into()),
            ..member_params()
        },
    ] {
        let mut c = Client::connect(&gg, p);
        c.send(json!({"op": "hello", "lobby_token": lobby})).await;
        assert_eq!(c.closed_with().await, CLOSE_UNAUTHORIZED);
    }
    let mut ok = Client::connect(&gg, member_params());
    ok.send(json!({"op": "hello", "lobby_token": lobby})).await;
    assert_eq!(ok.json().await["op"], "ready");
}

#[tokio::test]
async fn a_revoked_session_is_disconnected_and_told_why() {
    let (gg, w) = setup(40, 8);
    add_guest(&w, "srg_a", &lobby_token(1), "A");
    let (mut g, _) = guest_in(&gg, "srg_a").await;
    w.guests.lock().unwrap().remove("srg_a");
    let closed = g.json().await;
    assert_eq!(closed, json!({"op": "closed", "reason": "revoked"}));
    assert_eq!(g.closed_with().await, CLOSE_GONE);
    // And the seat is gone from the relay.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(gg.hub().lobbies(), 0);
    assert_eq!(gg.open_sockets(), 0);
}

#[tokio::test]
async fn closing_a_lobby_in_process_disconnects_every_seat() {
    let (gg, w) = setup(5000, 8);
    add_guest(&w, "srg_a", &lobby_token(1), "A");
    let (mut g, _) = guest_in(&gg, "srg_a").await;
    gg.hub().close(
        lobby_id(&lobby_token(1)),
        sr_voice::lobby::CloseReason::Ended,
    );
    assert_eq!(g.json().await, json!({"op": "closed", "reason": "ended"}));
    assert_eq!(g.closed_with().await, CLOSE_GONE);
}

#[tokio::test]
async fn a_full_lobby_refuses_the_next_seat_and_an_over_budget_sender_only_loses_frames() {
    let (gg, w) = setup(5000, 2);
    for t in ["srg_a", "srg_b", "srg_c"] {
        add_guest(&w, t, &lobby_token(1), t);
    }
    let (mut a, _) = guest_in(&gg, "srg_a").await;
    let (mut b, _) = guest_in(&gg, "srg_b").await;
    assert_eq!(a.json().await["op"], "peer");
    let mut c = Client::connect(&gg, ConnectParams::default());
    c.send(json!({"op": "hello", "guest_token": "srg_c"})).await;
    assert_eq!(c.closed_with().await, CLOSE_FULL);

    // 256 kbit/s holds one second (32 KB, the largest frame is 16 KB + 64): flood and stay connected.
    let topic = format!("game:{}", lobby_token(1));
    let blob = "A".repeat(20_000);
    for _ in 0..12 {
        b.send(json!({"op": "publish", "topic": topic, "data": blob}))
            .await;
    }
    b.send(json!({"op": "ping"})).await;
    let mut pong = false;
    for _ in 0..20 {
        if b.json().await["op"] == "pong" {
            pong = true;
            break;
        }
    }
    assert!(pong, "the flooding seat is not disconnected");
    assert!(gg.hub().stats().dropped_budget > 0);
}

/// The MLS voice stream is reachable only with a join token signed for a member of the channel: a guest token or a
/// lobby token is not one, and the lobby sub-room has no way to mint one (it holds no signer).
#[test]
fn a_guest_or_lobby_token_is_not_a_voice_join_token() {
    let signer = sr_voice_proto::token::TokenSigner::new(b"voice-secret".to_vec());
    for t in [
        format!("srg_{}", "0f".repeat(32)),
        lobby_token(7),
        format!("game:{}", lobby_token(7)),
        String::new(),
    ] {
        assert!(signer.verify(&t, 0).is_err(), "{t}");
    }
}
