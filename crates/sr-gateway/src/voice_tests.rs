//! Voice protocol tests: op 4, dispatches, join checks, moderation, cleanup. Driven by an
//! in-process fake client and a fake directory (no sockets, no database).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde_json::{Value, json};
use sr_core::{ChannelId, GuildId, Permissions, UserId};
use sr_events::{DomainEvent, EventBus, InProcessBus};
use sr_voice_proto::{TokenSigner, TurnConfig, VoiceCommand, VoiceRelay};
use tokio::sync::mpsc;

use crate::*;

const G: GuildId = GuildId::new(1);
const OTHER_GUILD: GuildId = GuildId::new(2);
const LOBBY: ChannelId = ChannelId::new(20);
const ROOM_B: ChannelId = ChannelId::new(21);
const SMALL: ChannelId = ChannelId::new(22);
const TEXT: ChannelId = ChannelId::new(23);
const STAGE: ChannelId = ChannelId::new(24);
const STAFF_VOICE: ChannelId = ChannelId::new(25);
const FOREIGN: ChannelId = ChannelId::new(26);
const ADULT: ChannelId = ChannelId::new(27);
const AFK: ChannelId = ChannelId::new(28);
/// A group DM of MEMBER, FRIEND and MOD.
const DM: ChannelId = ChannelId::new(300);

const MEMBER: u64 = 1;
const FRIEND: u64 = 2;
const MUTED_OUT: u64 = 3;
const MOD: u64 = 4;
const OUTSIDER: u64 = 5;

fn u(n: u64) -> UserId {
    UserId::new(n)
}
fn member_perms() -> Permissions {
    Permissions::VIEW_CHANNEL
        | Permissions::CONNECT
        | Permissions::SPEAK
        | Permissions::USE_VAD
        | Permissions::STREAM
        | Permissions::REQUEST_TO_SPEAK
}

fn mod_perms() -> Permissions {
    member_perms()
        | Permissions::MUTE_MEMBERS
        | Permissions::DEAFEN_MEMBERS
        | Permissions::MOVE_MEMBERS
        | Permissions::MANAGE_CHANNELS
}

struct Dir {
    perms: Mutex<HashMap<(UserId, ChannelId), Permissions>>,
    /// Users who answered yes to "Are you 18 or older?".
    adults: Mutex<Vec<UserId>>,
    afk: Mutex<VoiceGuildInfo>,
    limits: Mutex<HashMap<UserId, VoiceLimits>>,
    default_limits: Mutex<VoiceLimits>,
}

impl Dir {
    fn new() -> Self {
        let mut p = HashMap::new();
        for ch in [LOBBY, ROOM_B, SMALL, TEXT, STAGE, STAFF_VOICE, ADULT, AFK] {
            p.insert((u(MEMBER), ch), member_perms());
            p.insert((u(FRIEND), ch), member_perms());
            p.insert((u(MOD), ch), mod_perms());
            // No CONNECT anywhere: sees channels, cannot join.
            p.insert((u(MUTED_OUT), ch), Permissions::VIEW_CHANNEL);
        }
        // A crowd of ordinary members for the sender-limit tests.
        for n in 10..=30 {
            p.insert((u(n), LOBBY), member_perms());
        }
        // The staff room is invisible to FRIEND.
        p.insert((u(FRIEND), STAFF_VOICE), Permissions::empty());
        Self {
            perms: Mutex::new(p),
            adults: Mutex::new(Vec::new()),
            afk: Mutex::new(VoiceGuildInfo::default()),
            limits: Mutex::new(HashMap::new()),
            default_limits: Mutex::new(VoiceLimits::free()),
        }
    }
}

#[async_trait]
impl GuildDirectory for Dir {
    async fn guilds_for_user(&self, user: UserId) -> Vec<GuildId> {
        match user.get() {
            MEMBER | FRIEND | MUTED_OUT | MOD | 10..=30 => vec![G],
            OUTSIDER => vec![OTHER_GUILD],
            _ => vec![],
        }
    }

    async fn has_guild_permission(
        &self,
        user: UserId,
        guild: GuildId,
        _needed: Permissions,
    ) -> bool {
        guild == G && matches!(user.get(), MEMBER | FRIEND | MUTED_OUT | MOD | 10..=30)
    }

    async fn can_view_channel(&self, user: UserId, guild: GuildId, ch: ChannelId) -> bool {
        guild == G
            && self
                .perms
                .lock()
                .unwrap()
                .get(&(user, ch))
                .is_some_and(|p| p.contains(Permissions::VIEW_CHANNEL))
    }

    async fn voice_channel(&self, guild: GuildId, ch: ChannelId) -> Option<VoiceChannelInfo> {
        if guild != G {
            return None;
        }
        let (kind, limit) = match ch {
            LOBBY | ROOM_B | STAFF_VOICE | ADULT | AFK => (VoiceChannelKind::Voice, 0),
            SMALL => (VoiceChannelKind::Voice, 1),
            TEXT => (VoiceChannelKind::Other, 0),
            STAGE => (VoiceChannelKind::Stage, 0),
            _ => return None,
        };
        Some(VoiceChannelInfo {
            kind,
            user_limit: limit,
            bitrate: Some(64_000),
            age_restricted: ch == ADULT,
        })
    }

    async fn channel_permissions(
        &self,
        user: UserId,
        guild: GuildId,
        ch: ChannelId,
    ) -> Option<Permissions> {
        if guild != G || !matches!(user.get(), MEMBER | FRIEND | MUTED_OUT | MOD | 10..=30) {
            return None;
        }
        Some(
            self.perms
                .lock()
                .unwrap()
                .get(&(user, ch))
                .copied()
                .unwrap_or_else(Permissions::empty),
        )
    }

    async fn channel_guild(&self, ch: ChannelId) -> Option<GuildId> {
        matches!(
            ch,
            LOBBY | ROOM_B | SMALL | TEXT | STAGE | STAFF_VOICE | FOREIGN | ADULT | AFK
        )
        .then_some(G)
    }

    async fn guild_payload(&self, _user: UserId, guild: GuildId) -> Value {
        json!({
            "id": guild,
            "channels": [
                {"id": LOBBY.to_string(), "name": "lobby"},
                {"id": STAGE.to_string(), "name": "stage"},
            ],
        })
    }

    async fn voice_guild(&self, guild: GuildId) -> VoiceGuildInfo {
        if guild == G {
            *self.afk.lock().unwrap()
        } else {
            VoiceGuildInfo::default()
        }
    }

    async fn soundboard_sounds(&self, guild: GuildId) -> Vec<Value> {
        if guild == G {
            vec![json!({"name": "ding", "sound_id": "9", "volume": 1.0, "guild_id": "1"})]
        } else {
            vec![]
        }
    }

    async fn age_confirmed(&self, user: UserId) -> bool {
        self.adults.lock().unwrap().contains(&user)
    }

    async fn voice_limits(&self, user: UserId, _guild: Option<GuildId>) -> VoiceLimits {
        self.limits
            .lock()
            .unwrap()
            .get(&user)
            .cloned()
            .unwrap_or_else(|| self.default_limits.lock().unwrap().clone())
    }

    async fn dm_call_members(&self, ch: ChannelId, user: UserId) -> Option<Vec<UserId>> {
        let members = vec![u(MEMBER), u(FRIEND), u(MOD)];
        (ch == DM && members.contains(&user)).then_some(members)
    }

    async fn member_payload(&self, _guild: GuildId, user: UserId) -> Option<Value> {
        Some(json!({ "user": { "id": user.to_string() }, "nick": null }))
    }
}

struct Auth;

#[async_trait]
impl Authenticator for Auth {
    async fn authenticate(&self, token: &str) -> Result<AuthedUser, AuthError> {
        let id: u64 = token
            .strip_prefix("user:")
            .and_then(|s| s.parse().ok())
            .ok_or(AuthError::Invalid)?;
        Ok(AuthedUser {
            user_id: u(id),
            bot: false,
            allowed_privileged_intents: 0,
            user: json!({ "id": id.to_string() }),
        })
    }
}

#[derive(Default)]
struct Recorder(Mutex<Vec<VoiceCommand>>);

#[async_trait]
impl VoiceRelay for Recorder {
    async fn apply(&self, c: VoiceCommand) {
        self.0.lock().unwrap().push(c);
    }
}

impl Recorder {
    fn take(&self) -> Vec<VoiceCommand> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

#[derive(Default)]
struct CallLog(Mutex<Vec<CallSummary>>);

#[async_trait]
impl CallHooks for CallLog {
    async fn call_ended(&self, summary: CallSummary) {
        self.0.lock().unwrap().push(summary);
    }
}

const SECRET: &[u8] = b"gateway-voice-secret";

struct Env {
    gw: Gateway,
    bus: Arc<InProcessBus>,
    dir: Arc<Dir>,
    relay: Arc<Recorder>,
    calls: Arc<CallLog>,
}

fn env_with_voice(enabled: bool) -> Env {
    env_full(enabled, true)
}

fn env_full(enabled: bool, e2ee: bool) -> Env {
    let relay = Arc::new(Recorder::default());
    let calls = Arc::new(CallLog::default());
    let cfg = GatewayConfig {
        voice: enabled.then(|| VoiceConfig {
            signer: TokenSigner::new(SECRET.to_vec()),
            endpoints: vec!["wss://sfu.example/voice/ws".into()],
            turn: TurnConfig {
                urls: vec![
                    "stun:turn.example:3478".into(),
                    "turn:turn.example:3478?transport=udp".into(),
                ],
                secret: "turn-secret".into(),
                ttl_secs: 3600,
            },
            relay: Some(relay.clone()),
            session_grace: Duration::from_secs(20),
            calls: Some(calls.clone()),
            e2ee,
        }),
        ..GatewayConfig::default()
    };
    let bus = Arc::new(InProcessBus::new());
    let dir = Arc::new(Dir::new());
    let gw = Gateway::new(cfg, Arc::new(Auth), dir.clone(), bus.clone());
    gw.start();
    Env {
        gw,
        bus,
        dir,
        relay,
        calls,
    }
}

fn env() -> Env {
    env_with_voice(true)
}

struct Client {
    tx: mpsc::Sender<Frame>,
    rx: mpsc::Receiver<Frame>,
}

impl Client {
    fn connect(gw: &Gateway) -> Self {
        let (tx, in_rx) = mpsc::channel(64);
        let (out_tx, rx) = mpsc::channel(256);
        tokio::spawn(run_connection(
            gw.clone(),
            ConnectParams::default(),
            in_rx,
            out_tx,
        ));
        Self { tx, rx }
    }

    async fn send(&self, v: Value) {
        self.tx.send(Frame::Text(v.to_string())).await.unwrap();
    }

    async fn recv(&mut self) -> Value {
        let f = tokio::time::timeout(Duration::from_secs(30), self.rx.recv())
            .await
            .expect("timed out")
            .expect("ended");
        match f {
            Frame::Text(t) => serde_json::from_str(&t).unwrap(),
            other => panic!("unexpected frame {other:?}"),
        }
    }

    /// The next dispatch named `name`, skipping others.
    async fn event(&mut self, name: &str) -> Value {
        loop {
            let v = self.recv().await;
            if v["t"] == name {
                return v["d"].clone();
            }
        }
    }

    /// All dispatches that arrive within a short window.
    async fn drain(&mut self) -> Vec<Value> {
        let mut out = Vec::new();
        while let Ok(Some(Frame::Text(t))) =
            tokio::time::timeout(Duration::from_millis(120), self.rx.recv()).await
        {
            out.push(serde_json::from_str::<Value>(&t).unwrap());
        }
        out
    }

    async fn identify(&mut self, user: u64) -> Value {
        assert_eq!(self.recv().await["op"], 10);
        self.send(json!({"op": 2, "d": {"token": format!("user:{user}"), "intents": 0}}))
            .await;
        self.event("READY").await
    }

    async fn voice(&self, guild: u64, channel: Option<u64>, self_mute: bool, self_deaf: bool) {
        self.send(json!({"op": 4, "d": {
            "guild_id": guild.to_string(),
            "channel_id": channel.map(|c| c.to_string()),
            "self_mute": self_mute, "self_deaf": self_deaf,
        }}))
        .await;
    }
}

async fn client(e: &Env, user: u64) -> Client {
    let mut c = Client::connect(&e.gw);
    c.identify(user).await;
    c.drain().await;
    c
}

fn names(events: &[Value]) -> Vec<String> {
    events
        .iter()
        .filter_map(|v| v["t"].as_str().map(str::to_owned))
        .collect()
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[tokio::test]
async fn joining_dispatches_state_to_the_guild_and_server_update_to_the_joiner() {
    let e = env();
    let mut a = client(&e, MEMBER).await;
    let mut b = client(&e, FRIEND).await;
    let mut outsider = client(&e, OUTSIDER).await;

    a.voice(1, Some(LOBBY.get()), false, false).await;

    let st = a.event("VOICE_STATE_UPDATE").await;
    assert_eq!(st["channel_id"], "20");
    assert_eq!(st["user_id"], "1");
    assert_eq!(st["guild_id"], "1");
    assert_eq!(st["mute"], false);
    assert_eq!(st["self_mute"], false);
    assert_eq!(st["member"]["user"]["id"], "1");

    let srv = a.event("VOICE_SERVER_UPDATE").await;
    assert_eq!(srv["guild_id"], "1");
    assert_eq!(srv["channel_id"], "20");
    assert_eq!(srv["endpoint"], "wss://sfu.example/voice/ws");
    // The token is a real join token for this user, channel and session.
    let claims = TokenSigner::new(SECRET.to_vec())
        .verify(srv["token"].as_str().unwrap(), unix_now())
        .unwrap();
    assert_eq!(claims.user_id, u(MEMBER));
    assert_eq!(claims.channel_id, LOBBY);
    assert_eq!(claims.guild_id, Some(G));
    assert!(claims.speak && !claims.deaf && !claims.ptt_required);
    assert_eq!(claims.session_id, st["session_id"].as_str().unwrap());
    assert!(claims.exp <= unix_now() + 60);
    // ICE servers: STUN plain, TURN with ephemeral credentials.
    let ice = srv["ice_servers"].as_array().unwrap();
    assert_eq!(ice.len(), 2);
    assert!(ice[0].get("credential").is_none());
    assert!(ice[1]["username"].as_str().unwrap().ends_with(":1"));
    assert!(ice[1]["credential"].as_str().unwrap().len() >= 20);

    // Another member sees the join, but no VOICE_SERVER_UPDATE.
    let seen = b.drain().await;
    assert_eq!(names(&seen), ["VOICE_STATE_UPDATE"]);
    // Someone in another guild hears nothing.
    assert!(outsider.drain().await.is_empty());
    assert_eq!(e.gw.voice_occupants(LOBBY).len(), 1);
}

#[tokio::test]
async fn states_in_channels_a_member_cannot_see_are_not_revealed() {
    let e = env();
    let mut a = client(&e, MEMBER).await;
    let mut hidden_from = client(&e, FRIEND).await;
    let mut seer = client(&e, MOD).await;
    // The staff room is visible to MEMBER and MOD only; give MOD view.
    e.dir
        .perms
        .lock()
        .unwrap()
        .insert((u(MOD), STAFF_VOICE), mod_perms());

    a.voice(1, Some(STAFF_VOICE.get()), false, false).await;
    let st = a.event("VOICE_STATE_UPDATE").await;
    assert_eq!(st["channel_id"], "25");
    // (Presence updates from the other test clients may arrive; only voice events matter.)
    assert!(
        !names(&hidden_from.drain().await).contains(&"VOICE_STATE_UPDATE".to_owned()),
        "FRIEND cannot see that channel"
    );
    assert!(names(&seer.drain().await).contains(&"VOICE_STATE_UPDATE".to_owned()));

    // GUILD_CREATE for a later session: only channels the viewer can see.
    let mut late_friend = Client::connect(&e.gw);
    assert_eq!(late_friend.recv().await["op"], 10);
    late_friend
        .send(json!({"op": 2, "d": {"token": "user:2", "intents": 0}}))
        .await;
    let gc = late_friend.event("GUILD_CREATE").await;
    assert_eq!(gc["voice_states"], json!([]));

    let mut late_mod = Client::connect(&e.gw);
    assert_eq!(late_mod.recv().await["op"], 10);
    late_mod
        .send(json!({"op": 2, "d": {"token": "user:4", "intents": 0}}))
        .await;
    let gc = late_mod.event("GUILD_CREATE").await;
    let vs = gc["voice_states"].as_array().unwrap();
    assert_eq!(vs.len(), 1);
    assert_eq!(vs[0]["user_id"], "1");
    assert_eq!(vs[0]["channel_id"], "25");
    assert!(
        vs[0].get("guild_id").is_none(),
        "GUILD_CREATE entries omit guild_id"
    );
}

#[tokio::test]
async fn refused_joins_say_why_and_change_nothing() {
    let e = env();
    let mut a = client(&e, MEMBER).await;
    let mut no_connect = client(&e, MUTED_OUT).await;
    let mut outsider = client(&e, OUTSIDER).await;

    async fn expect_error(c: &mut Client, code: &str) {
        let err = c.event("SR_VOICE_ERROR").await;
        assert_eq!(err["code"], code, "{err}");
        assert!(!err["message"].as_str().unwrap().is_empty());
    }

    no_connect.voice(1, Some(LOBBY.get()), false, false).await;
    expect_error(&mut no_connect, "missing_permission").await;
    a.voice(1, Some(TEXT.get()), false, false).await;
    expect_error(&mut a, "not_voice_channel").await;
    a.voice(1, Some(ADULT.get()), false, false).await;
    expect_error(&mut a, "age_confirmation_required").await;
    a.voice(1, Some(999), false, false).await;
    expect_error(&mut a, "unknown_channel").await;
    // Not in the guild at all.
    outsider.voice(1, Some(LOBBY.get()), false, false).await;
    expect_error(&mut outsider, "not_member").await;
    // A foreign channel id in the right guild.
    a.voice(1, Some(FOREIGN.get()), false, false).await;
    expect_error(&mut a, "unknown_channel").await;
    // Malformed: no channel_id key.
    a.send(json!({"op": 4, "d": {"guild_id": "1"}})).await;
    expect_error(&mut a, "bad_request").await;
    // No guild means a DM call, and a server channel is not a DM of yours.
    a.send(json!({"op": 4, "d": {"channel_id": "20"}})).await;
    expect_error(&mut a, "not_member").await;
    a.send(json!({"op": 4, "d": "nope"})).await;
    expect_error(&mut a, "bad_request").await;

    assert!(e.gw.voice_state(u(MEMBER)).is_none());
    assert!(e.gw.voice_state(u(MUTED_OUT)).is_none());
    assert!(e.relay.take().is_empty());
}

#[tokio::test]
async fn voice_is_refused_when_no_sfu_is_configured() {
    let e = env_with_voice(false);
    let mut a = client(&e, MEMBER).await;
    a.voice(1, Some(LOBBY.get()), false, false).await;
    let err = a.event("SR_VOICE_ERROR").await;
    assert_eq!(err["code"], "voice_unavailable");
    assert!(e.gw.voice_state(u(MEMBER)).is_none());
}

#[tokio::test]
async fn user_limit_is_enforced_with_a_move_members_bypass() {
    let e = env();
    let mut a = client(&e, MEMBER).await;
    let mut b = client(&e, FRIEND).await;
    let mut m = client(&e, MOD).await;

    a.voice(1, Some(SMALL.get()), false, false).await;
    a.event("VOICE_SERVER_UPDATE").await;
    b.voice(1, Some(SMALL.get()), false, false).await;
    assert_eq!(b.event("SR_VOICE_ERROR").await["code"], "channel_full");
    assert!(e.gw.voice_state(u(FRIEND)).is_none());

    m.voice(1, Some(SMALL.get()), false, false).await;
    m.event("VOICE_SERVER_UPDATE").await;
    assert_eq!(e.gw.voice_occupants(SMALL).len(), 2);

    // Re-sending the same channel never counts against the limit.
    a.voice(1, Some(SMALL.get()), true, false).await;
    a.drain().await;
    assert_eq!(e.gw.voice_occupants(SMALL).len(), 2);
}

#[tokio::test]
async fn moving_between_channels_is_one_update_and_a_new_token() {
    let e = env();
    let mut a = client(&e, MEMBER).await;
    let mut b = client(&e, FRIEND).await;
    a.voice(1, Some(LOBBY.get()), false, false).await;
    a.event("VOICE_SERVER_UPDATE").await;
    b.drain().await;
    e.relay.take();

    a.voice(1, Some(ROOM_B.get()), false, false).await;
    let st = a.event("VOICE_STATE_UPDATE").await;
    assert_eq!(st["channel_id"], "21");
    let srv = a.event("VOICE_SERVER_UPDATE").await;
    assert_eq!(srv["channel_id"], "21");
    assert_eq!(names(&b.drain().await), ["VOICE_STATE_UPDATE"]);
    // The SFU is told to drop the user from the old room.
    assert_eq!(
        e.relay.take(),
        vec![VoiceCommand::Disconnect {
            channel: LOBBY,
            user: u(MEMBER)
        }]
    );
    assert!(e.gw.voice_occupants(LOBBY).is_empty());
    assert_eq!(e.gw.voice_occupants(ROOM_B).len(), 1);
}

#[tokio::test]
async fn leaving_clears_the_state_and_tells_the_sfu() {
    let e = env();
    let mut a = client(&e, MEMBER).await;
    let mut b = client(&e, FRIEND).await;
    a.voice(1, Some(LOBBY.get()), false, false).await;
    a.event("VOICE_SERVER_UPDATE").await;
    b.drain().await;
    e.relay.take();

    a.voice(1, None, false, false).await;
    let st = a.event("VOICE_STATE_UPDATE").await;
    assert!(st["channel_id"].is_null());
    assert_eq!(st["user_id"], "1");
    let seen = b.drain().await;
    assert_eq!(names(&seen), ["VOICE_STATE_UPDATE"]);
    assert!(seen[0]["d"]["channel_id"].is_null());
    assert!(e.gw.voice_state(u(MEMBER)).is_none());
    assert_eq!(
        e.relay.take(),
        vec![VoiceCommand::Disconnect {
            channel: LOBBY,
            user: u(MEMBER)
        }]
    );
    // Leaving when not connected is a no-op.
    a.voice(1, None, false, false).await;
    assert!(a.drain().await.is_empty());
}

#[tokio::test]
async fn self_mute_and_deafen_update_in_place_without_a_new_token() {
    let e = env();
    let mut a = client(&e, MEMBER).await;
    let mut b = client(&e, FRIEND).await;
    a.voice(1, Some(LOBBY.get()), false, false).await;
    a.event("VOICE_SERVER_UPDATE").await;
    b.drain().await;
    e.relay.take();

    a.voice(1, Some(LOBBY.get()), true, false).await;
    let seen = a.drain().await;
    assert_eq!(
        names(&seen),
        ["VOICE_STATE_UPDATE"],
        "no second VOICE_SERVER_UPDATE"
    );
    assert_eq!(seen[0]["d"]["self_mute"], true);
    assert_eq!(b.drain().await[0]["d"]["self_mute"], true);
    // A self-muted client cannot leak audio: the SFU stops forwarding it.
    assert_eq!(
        e.relay.take(),
        vec![VoiceCommand::SetFlags {
            channel: LOBBY,
            user: u(MEMBER),
            speak: false,
            deaf: false,
            ptt_required: false,
            priority: false,
            camera: false,
            screen: false,
        }]
    );

    a.voice(1, Some(LOBBY.get()), true, true).await;
    a.drain().await;
    let cmds = e.relay.take();
    assert!(matches!(
        cmds[..],
        [VoiceCommand::SetFlags {
            deaf: true,
            speak: false,
            ..
        }]
    ));
}

#[tokio::test]
async fn a_second_session_takes_over_the_voice_state() {
    let e = env();
    let mut tab1 = client(&e, MEMBER).await;
    let mut tab2 = client(&e, MEMBER).await;
    tab1.voice(1, Some(LOBBY.get()), false, false).await;
    tab1.event("VOICE_SERVER_UPDATE").await;
    tab2.drain().await;

    tab2.voice(1, Some(LOBBY.get()), false, false).await;
    // The new session gets its own token and owns the state.
    let srv = tab2.event("VOICE_SERVER_UPDATE").await;
    assert_eq!(srv["channel_id"], "20");
    assert_eq!(
        e.gw.voice_state(u(MEMBER)).unwrap().session_id,
        TokenSigner::new(SECRET.to_vec())
            .verify(srv["token"].as_str().unwrap(), unix_now())
            .unwrap()
            .session_id
    );
    assert_eq!(e.gw.voice_occupants(LOBBY).len(), 1);
}

#[tokio::test]
async fn pushing_to_talk_is_required_where_voice_activity_is_denied() {
    let e = env();
    e.dir
        .perms
        .lock()
        .unwrap()
        .insert((u(MEMBER), ROOM_B), member_perms() - Permissions::USE_VAD);
    let mut a = client(&e, MEMBER).await;
    a.voice(1, Some(ROOM_B.get()), false, false).await;
    let srv = a.event("VOICE_SERVER_UPDATE").await;
    let claims = TokenSigner::new(SECRET.to_vec())
        .verify(srv["token"].as_str().unwrap(), unix_now())
        .unwrap();
    assert!(
        claims.ptt_required,
        "denying USE_VAD forces push-to-talk (D-046)"
    );
    assert!(claims.speak);
}

fn patch_mute(v: bool) -> ModPatch {
    ModPatch {
        mute: Some(v),
        ..ModPatch::default()
    }
}

#[tokio::test]
async fn moderators_mute_deafen_move_and_disconnect() {
    let e = env();
    let mut a = client(&e, MEMBER).await;
    let mut m = client(&e, MOD).await;
    a.voice(1, Some(LOBBY.get()), false, false).await;
    a.event("VOICE_SERVER_UPDATE").await;
    m.drain().await;
    e.relay.take();

    // Server mute.
    let out =
        e.gw.moderate_voice(u(MOD), G, u(MEMBER), patch_mute(true))
            .await
            .unwrap();
    assert_eq!(out.actions, vec![ModAction::Muted(true)]);
    assert!(out.state.unwrap().mute);
    let st = a.event("VOICE_STATE_UPDATE").await;
    assert_eq!(st["mute"], true);
    assert_eq!(names(&m.drain().await), ["VOICE_STATE_UPDATE"]);
    assert!(matches!(
        e.relay.take()[..],
        [VoiceCommand::SetFlags { speak: false, .. }]
    ));
    // Muting again changes nothing and says so.
    let again =
        e.gw.moderate_voice(u(MOD), G, u(MEMBER), patch_mute(true))
            .await
            .unwrap();
    assert!(again.actions.is_empty());
    assert!(a.drain().await.is_empty());

    // Server deafen.
    let out =
        e.gw.moderate_voice(
            u(MOD),
            G,
            u(MEMBER),
            ModPatch {
                deaf: Some(true),
                ..ModPatch::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(out.actions, vec![ModAction::Deafened(true)]);
    a.drain().await;
    e.relay.take();

    // Move: server mute survives, the target gets a new token, the old room is told.
    let out =
        e.gw.moderate_voice(
            u(MOD),
            G,
            u(MEMBER),
            ModPatch {
                channel: Some(Some(ROOM_B)),
                ..ModPatch::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        out.actions,
        vec![ModAction::Moved {
            from: LOBBY,
            to: ROOM_B
        }]
    );
    let st = a.event("VOICE_STATE_UPDATE").await;
    assert_eq!(st["channel_id"], "21");
    assert_eq!(st["mute"], true, "server mute follows the member");
    assert_eq!(st["deaf"], true);
    let srv = a.event("VOICE_SERVER_UPDATE").await;
    let claims = TokenSigner::new(SECRET.to_vec())
        .verify(srv["token"].as_str().unwrap(), unix_now())
        .unwrap();
    assert_eq!(claims.channel_id, ROOM_B);
    assert!(!claims.speak, "the token carries the server mute");
    assert!(claims.deaf);
    assert_eq!(
        e.relay.take(),
        vec![VoiceCommand::Disconnect {
            channel: LOBBY,
            user: u(MEMBER)
        }]
    );

    // Disconnect.
    let out =
        e.gw.moderate_voice(
            u(MOD),
            G,
            u(MEMBER),
            ModPatch {
                channel: Some(None),
                ..ModPatch::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(out.actions, vec![ModAction::Disconnected { from: ROOM_B }]);
    assert!(out.state.is_none());
    let st = a.event("VOICE_STATE_UPDATE").await;
    assert!(st["channel_id"].is_null());
    assert!(e.gw.voice_state(u(MEMBER)).is_none());
    assert_eq!(
        e.relay.take(),
        vec![VoiceCommand::Disconnect {
            channel: ROOM_B,
            user: u(MEMBER)
        }]
    );

    // Server mute does not survive leaving voice (in-memory state, documented).
    a.voice(1, Some(LOBBY.get()), false, false).await;
    let st = a.event("VOICE_STATE_UPDATE").await;
    assert_eq!(st["mute"], false);
}

#[tokio::test]
async fn moderation_is_permission_checked_server_side() {
    let e = env();
    let mut a = client(&e, MEMBER).await;
    let mut b = client(&e, FRIEND).await;
    a.voice(1, Some(LOBBY.get()), false, false).await;
    a.event("VOICE_SERVER_UPDATE").await;
    b.voice(1, Some(LOBBY.get()), false, false).await;
    b.event("VOICE_SERVER_UPDATE").await;
    e.relay.take();

    // An ordinary member cannot mute, deafen, move or disconnect anyone.
    for patch in [
        patch_mute(true),
        ModPatch {
            deaf: Some(true),
            ..ModPatch::default()
        },
        ModPatch {
            channel: Some(None),
            ..ModPatch::default()
        },
        ModPatch {
            channel: Some(Some(ROOM_B)),
            ..ModPatch::default()
        },
    ] {
        assert_eq!(
            e.gw.moderate_voice(u(FRIEND), G, u(MEMBER), patch).await,
            Err(ModError::MissingPermissions)
        );
    }
    assert!(!e.gw.voice_state(u(MEMBER)).unwrap().mute);
    assert!(e.relay.take().is_empty());

    // A stranger to the guild is told nothing about who is in voice.
    assert_eq!(
        e.gw.moderate_voice(u(OUTSIDER), G, u(MEMBER), patch_mute(true))
            .await,
        Err(ModError::MissingPermissions)
    );
    // Not in voice, a guild the moderator is not in, bad destinations.
    assert_eq!(
        e.gw.moderate_voice(u(MOD), G, u(MOD), patch_mute(true))
            .await,
        Err(ModError::NotInVoice)
    );
    assert_eq!(
        e.gw.moderate_voice(u(MOD), OTHER_GUILD, u(MEMBER), patch_mute(true))
            .await,
        Err(ModError::MissingPermissions)
    );
    for (to, err) in [
        (TEXT, ModError::NotVoiceChannel),
        (ChannelId::new(998), ModError::UnknownChannel),
        (ChannelId::new(999), ModError::UnknownChannel),
    ] {
        assert_eq!(
            e.gw.moderate_voice(
                u(MOD),
                G,
                u(MEMBER),
                ModPatch {
                    channel: Some(Some(to)),
                    ..ModPatch::default()
                }
            )
            .await,
            Err(err)
        );
    }
    // Moving into a channel where the moderator lacks Move Members is refused.
    e.dir
        .perms
        .lock()
        .unwrap()
        .insert((u(MOD), ROOM_B), member_perms());
    assert_eq!(
        e.gw.moderate_voice(
            u(MOD),
            G,
            u(MEMBER),
            ModPatch {
                channel: Some(Some(ROOM_B)),
                ..ModPatch::default()
            }
        )
        .await,
        Err(ModError::MissingPermissions)
    );
}

#[tokio::test(start_paused = true)]
async fn a_lost_session_is_removed_from_voice_after_the_grace_period() {
    let e = env();
    let mut a = client(&e, MEMBER).await;
    let mut b = client(&e, FRIEND).await;
    a.voice(1, Some(LOBBY.get()), false, false).await;
    a.event("VOICE_SERVER_UPDATE").await;
    b.drain().await;
    e.relay.take();

    drop(a);
    tokio::time::sleep(Duration::from_millis(100)).await;
    // Within the grace period the user stays (a quick reconnect resumes).
    tokio::time::advance(Duration::from_secs(10)).await;
    e.gw.sweep().await;
    assert!(e.gw.voice_state(u(MEMBER)).is_some());
    // Past it, they are gone and everyone is told.
    tokio::time::advance(Duration::from_secs(15)).await;
    e.gw.sweep().await;
    assert!(e.gw.voice_state(u(MEMBER)).is_none());
    let seen = b.drain().await;
    assert!(
        seen.iter()
            .any(|v| v["t"] == "VOICE_STATE_UPDATE" && v["d"]["channel_id"].is_null())
    );
    assert_eq!(
        e.relay.take(),
        vec![VoiceCommand::Disconnect {
            channel: LOBBY,
            user: u(MEMBER)
        }]
    );
}

async fn emit(bus: &InProcessBus, ev: DomainEvent) {
    for t in ev.topics() {
        bus.publish(t, ev.clone()).await.unwrap();
    }
}

async fn settle() {
    tokio::time::sleep(Duration::from_millis(80)).await;
}

#[tokio::test]
async fn leaving_the_guild_or_losing_access_removes_the_member_from_voice() {
    let e = env();
    let mut a = client(&e, MEMBER).await;
    let mut f = client(&e, FRIEND).await;
    a.voice(1, Some(LOBBY.get()), false, false).await;
    a.event("VOICE_SERVER_UPDATE").await;
    f.voice(1, Some(LOBBY.get()), false, false).await;
    f.event("VOICE_SERVER_UPDATE").await;

    // Permissions change: FRIEND loses CONNECT, so the channel update drops them.
    e.dir
        .perms
        .lock()
        .unwrap()
        .insert((u(FRIEND), LOBBY), Permissions::VIEW_CHANNEL);
    emit(
        &e.bus,
        DomainEvent::ChannelUpdated {
            guild: G,
            channel: LOBBY,
            payload: json!({"id": "20"}),
        },
    )
    .await;
    settle().await;
    assert!(e.gw.voice_state(u(FRIEND)).is_none());
    assert!(e.gw.voice_state(u(MEMBER)).is_some());

    // MEMBER stays when permissions are unchanged and gets refreshed flags.
    e.relay.take();
    emit(
        &e.bus,
        DomainEvent::RoleUpdated {
            guild: G,
            role: sr_core::RoleId::new(7),
            payload: json!({}),
        },
    )
    .await;
    settle().await;
    assert!(e.gw.voice_state(u(MEMBER)).is_some());
    assert!(matches!(
        e.relay.take()[..],
        [VoiceCommand::SetFlags { speak: true, .. }]
    ));

    // Leaving the guild ends the voice session.
    emit(
        &e.bus,
        DomainEvent::MemberLeft {
            guild: G,
            user: u(MEMBER),
        },
    )
    .await;
    settle().await;
    assert!(e.gw.voice_state(u(MEMBER)).is_none());
}

#[tokio::test]
async fn a_media_session_the_sfu_lost_clears_the_voice_state() {
    let e = env();
    let mut a = client(&e, MEMBER).await;
    let mut b = client(&e, FRIEND).await;
    a.voice(1, Some(LOBBY.get()), false, false).await;
    a.event("VOICE_SERVER_UPDATE").await;
    b.drain().await;
    // A report for another channel is stale (the user moved on) and is ignored.
    e.gw.voice_media_lost(u(MEMBER), ROOM_B).await;
    assert!(e.gw.voice_state(u(MEMBER)).is_some());
    e.gw.voice_media_lost(u(MEMBER), LOBBY).await;
    assert!(e.gw.voice_state(u(MEMBER)).is_none());
    let seen = b.drain().await;
    assert!(
        seen.iter()
            .any(|v| v["t"] == "VOICE_STATE_UPDATE" && v["d"]["channel_id"].is_null())
    );
}

#[tokio::test]
async fn deleting_a_channel_closes_its_room() {
    let e = env();
    let mut a = client(&e, MEMBER).await;
    a.voice(1, Some(LOBBY.get()), false, false).await;
    a.event("VOICE_SERVER_UPDATE").await;
    e.relay.take();
    emit(
        &e.bus,
        DomainEvent::ChannelDeleted {
            guild: G,
            channel: LOBBY,
            payload: json!({"id": "20"}),
            viewers: vec![u(MEMBER)],
        },
    )
    .await;
    settle().await;
    assert!(e.gw.voice_state(u(MEMBER)).is_none());
    let cmds = e.relay.take();
    assert!(cmds.contains(&VoiceCommand::CloseRoom { channel: LOBBY }));
    assert!(cmds.contains(&VoiceCommand::Disconnect {
        channel: LOBBY,
        user: u(MEMBER)
    }));
}

#[tokio::test]
async fn deleting_the_guild_closes_its_rooms() {
    let e = env();
    let mut a = client(&e, MEMBER).await;
    a.voice(1, Some(LOBBY.get()), false, false).await;
    a.event("VOICE_SERVER_UPDATE").await;
    e.relay.take();
    emit(&e.bus, DomainEvent::GuildDeleted { guild: G }).await;
    settle().await;
    assert!(e.gw.voice_state(u(MEMBER)).is_none());
    assert_eq!(
        e.relay.take(),
        vec![VoiceCommand::CloseRoom { channel: LOBBY }]
    );
}

#[tokio::test]
async fn voice_state_dispatches_need_the_guild_voice_states_intent() {
    assert_eq!(
        crate::protocol::intents::required_for("VOICE_STATE_UPDATE"),
        Some(crate::protocol::intents::GUILD_VOICE_STATES)
    );
    assert_eq!(crate::protocol::intents::GUILD_VOICE_STATES, 1 << 7);
    // VOICE_SERVER_UPDATE goes to the joiner's own session and needs no intent (Discord).
    assert_eq!(
        crate::protocol::intents::required_for("VOICE_SERVER_UPDATE"),
        None
    );
}

#[path = "voice_ext_tests.rs"]
mod ext;

#[path = "voice_e2ee_tests.rs"]
mod e2ee;

/// The `VoiceJoined` / `VoiceLeft` events on the guild topic, in order, as `(+|-, channel, user)`.
async fn voice_events(
    stream: &mut futures::stream::BoxStream<'static, DomainEvent>,
) -> Vec<(char, u64, u64)> {
    use futures::StreamExt;
    let mut out = Vec::new();
    while let Ok(Some(ev)) = tokio::time::timeout(Duration::from_millis(150), stream.next()).await {
        match ev {
            DomainEvent::VoiceJoined { channel, user, .. } => {
                out.push(('+', channel.get(), user.get()));
            }
            DomainEvent::VoiceLeft { channel, user, .. } => {
                out.push(('-', channel.get(), user.get()));
            }
            _ => {}
        }
    }
    out
}

#[tokio::test]
async fn voice_joins_moves_and_leaves_are_published_on_the_bus() {
    use sr_events::Topic;
    let e = env();
    let mut stream = e.bus.subscribe(Topic::Guild(G));
    let mut a = client(&e, MEMBER).await;
    let mut m = client(&e, MOD).await;

    // A fresh join; a state change in place (mute) says nothing.
    a.voice(1, Some(LOBBY.get()), false, false).await;
    a.event("VOICE_SERVER_UPDATE").await;
    assert_eq!(voice_events(&mut stream).await, [('+', 20, 1)]);
    a.voice(1, Some(LOBBY.get()), true, false).await;
    assert!(voice_events(&mut stream).await.is_empty());

    // Moving yourself is a leave then a join; a moderator's move reads the same.
    a.voice(1, Some(ROOM_B.get()), false, false).await;
    a.event("VOICE_SERVER_UPDATE").await;
    assert_eq!(
        voice_events(&mut stream).await,
        [('-', 20, 1), ('+', 21, 1)]
    );
    m.drain().await;
    e.gw.moderate_voice(
        u(MOD),
        G,
        u(MEMBER),
        ModPatch {
            channel: Some(Some(LOBBY)),
            ..ModPatch::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        voice_events(&mut stream).await,
        [('-', 21, 1), ('+', 20, 1)]
    );

    // Disconnecting, and then a lost session, both end with a leave.
    a.voice(1, None, false, false).await;
    assert_eq!(voice_events(&mut stream).await, [('-', 20, 1)]);
    assert!(e.gw.voice_leave(u(MEMBER)).await.is_none());
    assert!(
        voice_events(&mut stream).await.is_empty(),
        "nothing to leave"
    );
}
