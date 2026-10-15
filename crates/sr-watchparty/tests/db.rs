//! Postgres-backed tests of the Watch Party service: creation and joining rules, command ordering and
//! conflicts, shared control, the buffering rule, host handover, idle cleanup, the age gate, the queue,
//! limits and events. The catalog, channel permissions and age lookups are fakes; the clock is manual.
//! Run with a scratch Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-watchparty --test db -- --ignored --test-threads=1`

use std::collections::{HashMap, HashSet};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicI64, Ordering},
};

use async_trait::async_trait;
use serde_json::Value;
use sr_core::Clock;
use sr_db::{
    PgPool,
    accounts::{self, NewAccount},
    testing::TestDb,
};
use sr_watchparty::model::*;
use sr_watchparty::{
    AgeLookup, ChannelAccess, ChannelKind, ChannelPerms, GateError, PartyError, PartyEvents,
    PartyService, PlayGate, Resolved,
};

static SEQ: AtomicI64 = AtomicI64::new(0);
const T0: i64 = 1_700_000_000_000;

fn uid() -> i64 {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);
    base * 1000 + SEQ.fetch_add(1, Ordering::Relaxed) % 1000
}

async fn account(pool: &PgPool) -> i64 {
    let id = uid();
    let name = format!("t{id}");
    let ok = accounts::create_account(
        pool,
        &NewAccount {
            id,
            person_id: uid(),
            username: &name,
            display_name: None,
            email: &format!("{name}@example.test"),
            password_hash: "x",
            state: "active",
            email_verified: true,
            initial_credits: 0,
            flags: 0,
        },
        uid(),
    )
    .await
    .unwrap();
    assert!(ok);
    id
}

struct ManualClock(AtomicI64);

impl Clock for ManualClock {
    fn now_unix_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
struct Recorder(Mutex<Vec<(Vec<i64>, String, Value)>>);

#[async_trait]
impl PartyEvents for Recorder {
    async fn emit(&self, users: Vec<i64>, name: &'static str, payload: Value) {
        self.0
            .lock()
            .unwrap()
            .push((users, name.to_owned(), payload));
    }
}

impl Recorder {
    fn named(&self, name: &str) -> Vec<(Vec<i64>, Value)> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, n, _)| n == name)
            .map(|(u, _, p)| (u.clone(), p.clone()))
            .collect()
    }
    fn clear(&self) {
        self.0.lock().unwrap().clear();
    }
}

struct FakeGate;

#[async_trait]
impl PlayGate for FakeGate {
    async fn resolve_live(&self, adult: bool, channel_id: i64) -> Result<Resolved, GateError> {
        let restricted = match channel_id {
            77 => false,
            78 => true,
            _ => return Err(GateError::NotFound),
        };
        if restricted && !adult {
            return Err(GateError::AgeConfirmationRequired);
        }
        Ok(Resolved {
            title_id: channel_id,
            episode_id: 0,
            variant_id: None,
            asset_id: channel_id,
            title: "7 Toonami".to_owned(),
            episode_label: Some("Cowboy Bebop".to_owned()),
            duration_ms: None,
            age_restricted: restricted,
            live: true,
        })
    }

    async fn resolve(
        &self,
        adult: bool,
        title_id: i64,
        episode_id: Option<i64>,
        variant_id: Option<i64>,
    ) -> Result<Resolved, GateError> {
        let (episode, asset, name, label, restricted) = match (title_id, episode_id) {
            (1, None) => (0, 501, "Akira", None, false),
            (2, None) => (0, 502, "Perfect Blue", None, false),
            (3, None) => (0, 503, "Secret", None, true),
            (5, Some(11)) => (11, 511, "Dark", Some("S1 E1".to_owned()), false),
            (5, Some(12)) => (12, 512, "Dark", Some("S1 E2".to_owned()), false),
            _ => return Err(GateError::NotFound),
        };
        if restricted && !adult {
            return Err(GateError::AgeConfirmationRequired);
        }
        Ok(Resolved {
            title_id,
            episode_id: episode,
            variant_id,
            asset_id: asset,
            title: name.to_owned(),
            episode_label: label,
            duration_ms: Some(100_000),
            age_restricted: restricted,
            live: false,
        })
    }
}

#[derive(Default)]
struct FakeChannels {
    perms: Mutex<HashMap<(i64, i64), ChannelPerms>>,
    viewers: Mutex<HashMap<i64, Vec<i64>>>,
}

#[async_trait]
impl ChannelAccess for FakeChannels {
    async fn perms(&self, user: i64, channel: i64) -> Result<Option<ChannelPerms>, String> {
        Ok(self.perms.lock().unwrap().get(&(user, channel)).copied())
    }
    async fn viewers(&self, channel: i64) -> Result<Vec<i64>, String> {
        Ok(self
            .viewers
            .lock()
            .unwrap()
            .get(&channel)
            .cloned()
            .unwrap_or_default())
    }
}

#[derive(Default)]
struct FakeAge(Mutex<HashSet<i64>>);

#[async_trait]
impl AgeLookup for FakeAge {
    async fn confirmed(&self, user: i64) -> bool {
        self.0.lock().unwrap().contains(&user)
    }
}

struct Env {
    db: TestDb,
    svc: PartyService,
    clock: Arc<ManualClock>,
    events: Arc<Recorder>,
    chan: Arc<FakeChannels>,
    age: Arc<FakeAge>,
}

async fn env() -> Env {
    let db = TestDb::new().await.unwrap();
    let clock = Arc::new(ManualClock(AtomicI64::new(T0)));
    let events = Arc::new(Recorder::default());
    let chan = Arc::new(FakeChannels::default());
    let age = Arc::new(FakeAge::default());
    let svc = PartyService::new(db.pool().clone(), clock.clone(), Arc::new(uid))
        .with_play_gate(Arc::new(FakeGate))
        .with_channels(chan.clone())
        .with_age(age.clone())
        .with_events(events.clone());
    Env {
        db,
        svc,
        clock,
        events,
        chan,
        age,
    }
}

impl Env {
    fn pool(&self) -> &PgPool {
        self.db.pool()
    }
    fn advance(&self, ms: i64) {
        self.clock.0.fetch_add(ms, Ordering::SeqCst);
    }
    async fn user(&self) -> i64 {
        account(self.pool()).await
    }
    async fn adult(&self) -> i64 {
        let u = account(self.pool()).await;
        self.age.0.lock().unwrap().insert(u);
        u
    }
    async fn v(&self, user: i64) -> Viewer {
        self.svc.viewer(user).await
    }
    /// A guild with a voice channel everyone in `users` can view (and `starter` may start parties in).
    async fn channel(&self, owner: i64, users: &[i64], starter: i64, kind: ChannelKind) -> i64 {
        let guild = uid();
        let channel = uid();
        sqlx::query("INSERT INTO guilds (id, name, owner_id) VALUES ($1, 'Test guild', $2)")
            .bind(guild)
            .bind(owner)
            .execute(self.pool())
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO channels (id, guild_id, type, name) VALUES ($1, $2, 2, 'theatre')",
        )
        .bind(channel)
        .bind(guild)
        .execute(self.pool())
        .await
        .unwrap();
        for u in users.iter().chain([&starter]) {
            self.chan.perms.lock().unwrap().insert(
                (*u, channel),
                ChannelPerms {
                    guild_id: Some(guild),
                    kind,
                    view: true,
                    start: *u == starter,
                    control: false,
                    manage: false,
                    user_limit: None,
                },
            );
        }
        self.chan
            .viewers
            .lock()
            .unwrap()
            .insert(channel, users.iter().copied().chain([starter]).collect());
        channel
    }
}

fn create(title: i64) -> CreateInput {
    CreateInput {
        title_id: title,
        tv_channel_id: None,
        episode_id: None,
        variant_id: None,
        channel_id: None,
        start_ms: None,
        settings: None,
    }
}

fn cmd(action: Action) -> CommandInput {
    CommandInput {
        action,
        position_ms: None,
        rate: None,
        expected_version: None,
    }
}

fn seek(ms: i64) -> CommandInput {
    CommandInput {
        position_ms: Some(ms),
        ..cmd(Action::Seek)
    }
}

fn settings_control(mode: ControlMode) -> SettingsInput {
    SettingsInput {
        shared_control: Some(mode),
        ..SettingsInput::default()
    }
}

async fn join_all(e: &Env, party: &PartyView, users: &[i64]) {
    for u in users {
        e.svc
            .join(
                e.v(*u).await,
                party.id,
                JoinInput {
                    invite_code: party.invite_code.clone(),
                },
            )
            .await
            .unwrap();
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_private_party_needs_its_invite_code() {
    let e = env().await;
    let (host, guest, stranger) = (e.user().await, e.user().await, e.user().await);
    let party = e.svc.create(e.v(host).await, create(1)).await.unwrap();
    assert_eq!(party.binding, Binding::Private);
    assert!(party.invite_code.is_some());
    assert_eq!(party.media.title, "Akira");
    assert_eq!(party.media.asset_id, 501);
    assert!(!party.playback.playing);
    assert_eq!(party.participants.len(), 1);
    assert!(party.participants[0].is_host);
    assert!(party.you.unwrap().can_control);

    // A stranger sees nothing, with or without a wrong code.
    let g = e.v(guest).await;
    assert!(matches!(
        e.svc.get(g, party.id).await,
        Err(PartyError::NotFound(_))
    ));
    for code in [None, Some("nope".to_owned())] {
        assert!(matches!(
            e.svc
                .join(g, party.id, JoinInput { invite_code: code })
                .await,
            Err(PartyError::NotFound(_))
        ));
    }
    let joined = e
        .svc
        .join(
            g,
            party.id,
            JoinInput {
                invite_code: party.invite_code.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(joined.participants.len(), 2);
    let you = joined.you.unwrap();
    assert!(you.member && !you.host && !you.can_control);
    // Joining twice is a no-op.
    let again = e
        .svc
        .join(
            g,
            party.id,
            JoinInput {
                invite_code: party.invite_code.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(again.participants.len(), 2);
    // The unrelated stranger still cannot see it.
    assert!(e.svc.get(e.v(stranger).await, party.id).await.is_err());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn only_playable_published_titles_start_a_party() {
    let e = env().await;
    let host = e.user().await;
    assert!(matches!(
        e.svc.create(e.v(host).await, create(404)).await,
        Err(PartyError::NotFound(_))
    ));
    let mut episode = create(5);
    episode.episode_id = Some(11);
    let party = e.svc.create(e.v(host).await, episode).await.unwrap();
    assert_eq!(party.media.episode_label.as_deref(), Some("S1 E1"));
    assert_eq!(party.media.episode_id, 11);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn playback_arithmetic_and_versions_follow_the_server_clock() {
    let e = env().await;
    let host = e.user().await;
    let v = e.v(host).await;
    let party = e.svc.create(v, create(1)).await.unwrap();
    let id = party.id;
    assert_eq!(party.playback.version, 1);

    let p = e.svc.command(v, id, cmd(Action::Play)).await.unwrap();
    assert!(p.playing);
    assert_eq!(p.version, 2);
    e.advance(10_000);
    let p = e.svc.command(v, id, cmd(Action::Pause)).await.unwrap();
    assert!(!p.playing);
    assert_eq!(p.position_ms, 10_000);
    assert_eq!(p.version, 3);
    // A paused party does not move; resuming continues from the paused position.
    e.advance(60_000);
    let p = e.svc.command(v, id, cmd(Action::Play)).await.unwrap();
    assert_eq!(p.position_ms, 10_000);
    e.advance(5_000);
    let p = e.svc.command(v, id, seek(50_000)).await.unwrap();
    assert!(p.playing);
    assert_eq!(p.position_ms, 50_000);
    e.advance(2_000);
    let got = e.svc.get(v, id).await.unwrap().playback;
    assert_eq!(got.expected_position_ms, 52_000);
    assert_eq!(got.server_time_ms, e.clock.now_unix_ms());

    let p = e
        .svc
        .command(
            v,
            id,
            CommandInput {
                rate: Some(1.5),
                ..cmd(Action::Rate)
            },
        )
        .await
        .unwrap();
    assert!((p.rate - 1.5).abs() < f64::EPSILON);
    e.advance(2_000);
    assert_eq!(
        e.svc
            .get(v, id)
            .await
            .unwrap()
            .playback
            .expected_position_ms,
        55_000
    );
    // A no-op does not move the version.
    let before = e.svc.get(v, id).await.unwrap().playback.version;
    let p = e.svc.command(v, id, cmd(Action::Play)).await.unwrap();
    assert_eq!(p.version, before);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_stale_expected_version_is_a_conflict_carrying_the_current_state() {
    let e = env().await;
    let (host, guest) = (e.user().await, e.user().await);
    let hv = e.v(host).await;
    let party = e.svc.create(hv, create(1)).await.unwrap();
    join_all(&e, &party, &[guest]).await;
    e.svc
        .update_settings(hv, party.id, settings_control(ControlMode::Everyone))
        .await
        .unwrap();
    let gv = e.v(guest).await;
    let seen = e.svc.get(gv, party.id).await.unwrap().playback.version;
    // The host acts first; the guest's command carrying the old version is refused.
    e.svc.command(hv, party.id, seek(30_000)).await.unwrap();
    let err = e
        .svc
        .command(
            gv,
            party.id,
            CommandInput {
                expected_version: Some(seen),
                ..seek(70_000)
            },
        )
        .await
        .unwrap_err();
    match err {
        PartyError::VersionConflict { current } => {
            assert_eq!(current.position_ms, 30_000);
            assert!(current.version > seen);
        }
        other => panic!("expected a conflict, got {other:?}"),
    }
    // Without a token the last writer wins.
    let p = e.svc.command(gv, party.id, seek(70_000)).await.unwrap();
    assert_eq!(p.position_ms, 70_000);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn shared_control_is_enforced_by_the_server() {
    let e = env().await;
    let (host, guest, helper) = (e.user().await, e.user().await, e.user().await);
    let hv = e.v(host).await;
    let party = e.svc.create(hv, create(1)).await.unwrap();
    join_all(&e, &party, &[guest, helper]).await;
    let (gv, pv) = (e.v(guest).await, e.v(helper).await);

    // Off (the default): every command of a non-host is refused, and so is changing the media.
    for c in [cmd(Action::Play), cmd(Action::Pause), seek(5)] {
        assert_eq!(
            e.svc.command(gv, party.id, c).await.unwrap_err(),
            PartyError::Forbidden
        );
    }
    assert_eq!(
        e.svc
            .change_media(
                gv,
                party.id,
                ChangeMediaInput {
                    title_id: 2,
                    tv_channel_id: None,
                    episode_id: None,
                    variant_id: None,
                    start_ms: None
                }
            )
            .await
            .unwrap_err(),
        PartyError::Forbidden
    );
    // Settings are the host's.
    assert_eq!(
        e.svc
            .update_settings(gv, party.id, settings_control(ControlMode::Everyone))
            .await
            .unwrap_err(),
        PartyError::Forbidden
    );

    // Controllers: only the named one.
    e.svc
        .update_settings(hv, party.id, settings_control(ControlMode::Controllers))
        .await
        .unwrap();
    e.svc
        .set_controller(hv, party.id, helper, true)
        .await
        .unwrap();
    assert!(e.svc.command(pv, party.id, cmd(Action::Play)).await.is_ok());
    assert_eq!(
        e.svc
            .command(gv, party.id, cmd(Action::Pause))
            .await
            .unwrap_err(),
        PartyError::Forbidden
    );
    let view = e.svc.get(pv, party.id).await.unwrap();
    assert!(view.you.unwrap().can_control);
    assert!(
        !e.svc
            .get(gv, party.id)
            .await
            .unwrap()
            .you
            .unwrap()
            .can_control
    );

    // Everyone.
    e.svc
        .update_settings(hv, party.id, settings_control(ControlMode::Everyone))
        .await
        .unwrap();
    assert!(
        e.svc
            .command(gv, party.id, cmd(Action::Pause))
            .await
            .is_ok()
    );

    // A non-participant never controls.
    let outsider = e.v(e.user().await).await;
    assert!(matches!(
        e.svc.command(outsider, party.id, cmd(Action::Play)).await,
        Err(PartyError::NotFound(_))
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_channel_permission_grants_control_in_controllers_mode() {
    let e = env().await;
    let (host, member, plain) = (e.user().await, e.user().await, e.user().await);
    let channel = e
        .channel(host, &[member, plain], host, ChannelKind::Voice)
        .await;
    let hv = e.v(host).await;
    let mut input = create(1);
    input.channel_id = Some(channel);
    input.settings = Some(settings_control(ControlMode::Controllers));
    let party = e.svc.create(hv, input).await.unwrap();
    assert_eq!(party.binding, Binding::Channel);
    join_all(&e, &party, &[member, plain]).await;
    // Grant CONTROL_WATCH_PARTY to `member` in the channel.
    e.chan
        .perms
        .lock()
        .unwrap()
        .get_mut(&(member, channel))
        .unwrap()
        .control = true;
    assert!(
        e.svc
            .command(e.v(member).await, party.id, cmd(Action::Play))
            .await
            .is_ok()
    );
    assert_eq!(
        e.svc
            .command(e.v(plain).await, party.id, cmd(Action::Pause))
            .await
            .unwrap_err(),
        PartyError::Forbidden
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn starting_in_a_channel_needs_the_permission_and_one_party_per_channel() {
    let e = env().await;
    let (host, other) = (e.user().await, e.user().await);
    let channel = e.channel(host, &[other], host, ChannelKind::Text).await;
    let mut input = create(1);
    input.channel_id = Some(channel);
    // `other` can see the channel but not start a party in it.
    assert_eq!(
        e.svc
            .create(e.v(other).await, input.clone())
            .await
            .unwrap_err(),
        PartyError::Forbidden
    );
    // Someone who cannot even see it learns nothing.
    let blind = e.user().await;
    assert!(matches!(
        e.svc.create(e.v(blind).await, input.clone()).await,
        Err(PartyError::NotFound(_))
    ));
    let party = e.svc.create(e.v(host).await, input.clone()).await.unwrap();
    // The channel's viewers can look at it and find it by channel.
    let seen = e.svc.in_channel(e.v(other).await, channel).await.unwrap();
    assert_eq!(seen.id, party.id);
    assert!(!seen.you.unwrap().member);
    assert!(seen.invite_code.is_none());
    // A second party in the same channel is refused (after the host leaves the first by creating it
    // elsewhere is impossible: the unique index decides).
    let second = e.user().await;
    let guild_id = e.chan.perms.lock().unwrap()[&(host, channel)].guild_id;
    e.chan.perms.lock().unwrap().insert(
        (second, channel),
        ChannelPerms {
            guild_id,
            kind: ChannelKind::Text,
            view: true,
            start: true,
            control: false,
            manage: false,
            user_limit: None,
        },
    );
    assert!(matches!(
        e.svc.create(e.v(second).await, input).await,
        Err(PartyError::Conflict(_))
    ));
    // Joining a bound party needs the channel.
    let blind_join = e
        .svc
        .join(e.v(blind).await, party.id, JoinInput::default())
        .await;
    assert!(matches!(blind_join, Err(PartyError::NotFound(_))));
    assert!(
        e.svc
            .join(e.v(other).await, party.id, JoinInput::default())
            .await
            .is_ok()
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_server_is_limited_in_concurrent_parties() {
    let e = env().await;
    let host = e.user().await;
    let channel = e.channel(host, &[], host, ChannelKind::Text).await;
    // Two more channels in the same guild, so the guild limit (2) is what stops the third party.
    let guild: i64 = sqlx::query_scalar("SELECT guild_id FROM channels WHERE id = $1")
        .bind(channel)
        .fetch_one(e.pool())
        .await
        .unwrap();
    let mut channels = vec![channel];
    for _ in 0..2 {
        let c = uid();
        sqlx::query("INSERT INTO channels (id, guild_id, type, name) VALUES ($1, $2, 2, 'more')")
            .bind(c)
            .bind(guild)
            .execute(e.pool())
            .await
            .unwrap();
        e.chan.perms.lock().unwrap().insert(
            (host, c),
            ChannelPerms {
                guild_id: Some(guild),
                kind: ChannelKind::Text,
                view: true,
                start: true,
                control: false,
                manage: false,
                user_limit: None,
            },
        );
        channels.push(c);
    }
    // Three hosts so each creation is a different account (an account is in one party at a time).
    let hosts = [host, e.user().await, e.user().await];
    for h in &hosts[1..] {
        for c in &channels {
            let mut p = *e.chan.perms.lock().unwrap().get(&(host, *c)).unwrap();
            p.start = true;
            e.chan.perms.lock().unwrap().insert((*h, *c), p);
        }
    }
    for (h, c) in hosts.iter().zip(&channels).take(2) {
        let mut input = create(1);
        input.channel_id = Some(*c);
        e.svc.create(e.v(*h).await, input).await.unwrap();
    }
    let mut input = create(1);
    input.channel_id = Some(channels[2]);
    assert!(matches!(
        e.svc.create(e.v(hosts[2]).await, input).await,
        Err(PartyError::Conflict(m)) if m.contains("limit")
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn buffering_pauses_everyone_then_resumes_or_continues_without_the_straggler() {
    let e = env().await;
    let (host, slow, fast) = (e.user().await, e.user().await, e.user().await);
    let hv = e.v(host).await;
    let party = e.svc.create(hv, create(1)).await.unwrap();
    join_all(&e, &party, &[slow, fast]).await;
    e.svc
        .command(hv, party.id, cmd(Action::Play))
        .await
        .unwrap();
    e.advance(4_000);
    e.events.clear();

    // `slow` stalls: the server pauses everybody at the current position.
    e.svc
        .heartbeat(
            e.v(slow).await,
            party.id,
            HeartbeatInput {
                buffering: Some(true),
                ..HeartbeatInput::default()
            },
        )
        .await
        .unwrap();
    let paused = e.svc.get(hv, party.id).await.unwrap();
    assert!(paused.playback.auto_paused && !paused.playback.playing);
    assert_eq!(paused.playback.position_ms, 4_000);
    assert!(
        paused
            .participants
            .iter()
            .any(|p| p.buffering && p.user_id == slow)
    );
    let evs = e.events.named("WATCH_PARTY_PLAYBACK");
    assert!(evs.iter().any(|(users, p)| p["reason"] == "buffering"
        && users.contains(&host)
        && users.contains(&fast)));

    // The straggler recovers: playback resumes without a controller.
    e.advance(3_000);
    e.svc
        .heartbeat(
            e.v(slow).await,
            party.id,
            HeartbeatInput {
                buffering: Some(false),
                ..HeartbeatInput::default()
            },
        )
        .await
        .unwrap();
    let resumed = e.svc.get(hv, party.id).await.unwrap();
    assert!(resumed.playback.playing && !resumed.playback.auto_paused);
    assert_eq!(resumed.playback.position_ms, 4_000);

    // Stalls again and never recovers: after max_wait the party continues without them.
    e.svc
        .heartbeat(
            e.v(slow).await,
            party.id,
            HeartbeatInput {
                buffering: Some(true),
                ..HeartbeatInput::default()
            },
        )
        .await
        .unwrap();
    assert!(e.svc.get(hv, party.id).await.unwrap().playback.auto_paused);
    e.advance(9_000);
    e.svc.tick(party.id).await.unwrap();
    assert!(e.svc.get(hv, party.id).await.unwrap().playback.auto_paused);
    e.advance(2_000);
    e.svc.tick(party.id).await.unwrap();
    let cont = e.svc.get(hv, party.id).await.unwrap();
    assert!(cont.playback.playing && !cont.playback.auto_paused);
    assert!(cont.participants.iter().all(|p| !p.buffering));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn buffering_pause_can_be_switched_off() {
    let e = env().await;
    let (host, slow) = (e.user().await, e.user().await);
    let hv = e.v(host).await;
    let mut input = create(1);
    input.settings = Some(SettingsInput {
        auto_pause_buffering: Some(false),
        ..SettingsInput::default()
    });
    let party = e.svc.create(hv, input).await.unwrap();
    join_all(&e, &party, &[slow]).await;
    e.svc
        .command(hv, party.id, cmd(Action::Play))
        .await
        .unwrap();
    e.svc
        .heartbeat(
            e.v(slow).await,
            party.id,
            HeartbeatInput {
                buffering: Some(true),
                ..HeartbeatInput::default()
            },
        )
        .await
        .unwrap();
    let p = e.svc.get(hv, party.id).await.unwrap();
    assert!(p.playback.playing && !p.playback.auto_paused);
    assert!(p.participants.iter().any(|x| x.buffering));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_host_role_passes_on_and_empty_parties_end_after_five_minutes() {
    let e = env().await;
    let (host, a, b) = (e.user().await, e.user().await, e.user().await);
    let hv = e.v(host).await;
    let party = e.svc.create(hv, create(1)).await.unwrap();
    join_all(&e, &party, &[a]).await;
    e.advance(1_000);
    join_all(&e, &party, &[b]).await;
    // `b` is named a controller, so inherits before the longer-standing `a`.
    e.svc.set_controller(hv, party.id, b, true).await.unwrap();
    e.svc.leave(hv, party.id).await.unwrap();
    let after = e.svc.get(e.v(a).await, party.id).await.unwrap();
    assert_eq!(after.host_id, b);
    assert_eq!(after.participants.len(), 2);

    // Everybody leaves: the party idles, then ends after 5 minutes.
    e.svc.leave(e.v(a).await, party.id).await.unwrap();
    e.svc.leave(e.v(b).await, party.id).await.unwrap();
    e.advance(4 * 60_000);
    e.svc.sweep().await.unwrap();
    let still =
        sqlx::query_scalar::<_, Option<i64>>("SELECT ended_at_ms FROM watch_parties WHERE id = $1")
            .bind(party.id)
            .fetch_one(e.pool())
            .await
            .unwrap();
    assert!(still.is_none());
    e.advance(61_000);
    e.svc.sweep().await.unwrap();
    let ended =
        sqlx::query_scalar::<_, Option<i64>>("SELECT ended_at_ms FROM watch_parties WHERE id = $1")
            .bind(party.id)
            .fetch_one(e.pool())
            .await
            .unwrap();
    assert!(ended.is_some());
    assert!(matches!(
        e.svc
            .join(
                e.v(a).await,
                party.id,
                JoinInput {
                    invite_code: party.invite_code.clone()
                }
            )
            .await,
        Err(PartyError::NotFound(_))
    ));
    // Ended parties are removed a day later.
    e.advance(25 * 3_600_000);
    e.svc.sweep().await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM watch_parties WHERE id = $1")
        .bind(party.id)
        .fetch_one(e.pool())
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn persistent_channel_parties_survive_being_empty_and_the_first_joiner_hosts() {
    let e = env().await;
    let (host, next) = (e.user().await, e.user().await);
    let channel = e.channel(host, &[next], host, ChannelKind::Voice).await;
    let mut input = create(1);
    input.channel_id = Some(channel);
    input.settings = Some(SettingsInput {
        persistent: Some(true),
        ..SettingsInput::default()
    });
    let party = e.svc.create(e.v(host).await, input).await.unwrap();
    e.svc.leave(e.v(host).await, party.id).await.unwrap();
    e.advance(3_600_000);
    e.svc.sweep().await.unwrap();
    let joined = e
        .svc
        .join(e.v(next).await, party.id, JoinInput::default())
        .await
        .unwrap();
    assert_eq!(joined.host_id, next);
    // Private parties cannot be persistent.
    let mut p = create(1);
    p.settings = Some(SettingsInput {
        persistent: Some(true),
        ..SettingsInput::default()
    });
    assert!(matches!(
        e.svc.create(e.v(e.user().await).await, p).await,
        Err(PartyError::Invalid(_))
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn silent_participants_are_dropped_by_the_sweep() {
    let e = env().await;
    let (host, ghost) = (e.user().await, e.user().await);
    let hv = e.v(host).await;
    let party = e.svc.create(hv, create(1)).await.unwrap();
    join_all(&e, &party, &[ghost]).await;
    e.advance(60_000);
    e.svc
        .heartbeat(hv, party.id, HeartbeatInput::default())
        .await
        .unwrap();
    e.advance(40_000);
    e.svc.sweep().await.unwrap();
    let after = e.svc.get(hv, party.id).await.unwrap();
    assert_eq!(after.participants.len(), 1);
    assert_eq!(after.participants[0].user_id, host);
    let hb = e
        .svc
        .heartbeat(e.v(ghost).await, party.id, HeartbeatInput::default())
        .await
        .unwrap();
    assert!(!hb.member);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn restricted_titles_need_age_confirmation_from_everyone() {
    let e = env().await;
    let (adult, adult2, minor) = (e.adult().await, e.adult().await, e.user().await);
    let av = e.v(adult).await;
    // A restricted title cannot be started by an unconfirmed account.
    assert_eq!(
        e.svc.create(e.v(minor).await, create(3)).await.unwrap_err(),
        PartyError::AgeConfirmationRequired
    );
    // Nor can an unconfirmed account join a restricted party.
    let party = e.svc.create(av, create(3)).await.unwrap();
    assert!(party.media.age_restricted);
    assert_eq!(
        e.svc
            .join(
                e.v(minor).await,
                party.id,
                JoinInput {
                    invite_code: party.invite_code.clone()
                }
            )
            .await
            .unwrap_err(),
        PartyError::AgeConfirmationRequired
    );
    join_all(&e, &party, &[adult2]).await;

    // Switching an open party to a restricted title removes unconfirmed participants.
    let open = e.svc.create(e.v(adult2).await, create(1)).await.unwrap();
    join_all(&e, &open, &[minor]).await;
    e.svc
        .update_settings(
            e.v(adult2).await,
            open.id,
            settings_control(ControlMode::Controllers),
        )
        .await
        .unwrap();
    e.events.clear();
    let view = e
        .svc
        .change_media(
            e.v(adult2).await,
            open.id,
            ChangeMediaInput {
                title_id: 3,
                tv_channel_id: None,
                episode_id: None,
                variant_id: None,
                start_ms: None,
            },
        )
        .await
        .unwrap();
    assert!(view.media.age_restricted);
    assert_eq!(view.participants.len(), 1);
    assert!(e.svc.mine(e.v(minor).await).await.is_err());
    let removed = e.events.named("WATCH_PARTY_UPDATE");
    assert!(removed.iter().any(|(users, p)| p["type"] == "removed"
        && users == &vec![minor]
        && p["reason"] == "age_restricted"));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn restricted_party_cards_are_masked_for_unconfirmed_channel_viewers() {
    let e = env().await;
    let (adult, minor) = (e.adult().await, e.user().await);
    let channel = e.channel(adult, &[minor], adult, ChannelKind::Text).await;
    let mut input = create(3);
    input.channel_id = Some(channel);
    e.svc.create(e.v(adult).await, input).await.unwrap();
    let seen = e.svc.in_channel(e.v(minor).await, channel).await.unwrap();
    assert_eq!(seen.media.title, "Age-restricted content");
    assert_eq!(seen.media.asset_id, 0);
    // The created event reached the channel's viewers masked, the host unmasked.
    let evs = e.events.named("WATCH_PARTY_UPDATE");
    let to_minor = evs.iter().find(|(u, _)| u.contains(&minor)).unwrap();
    assert_eq!(
        to_minor.1["party"]["media"]["title"],
        "Age-restricted content"
    );
    let to_host = evs.iter().find(|(u, _)| u.contains(&adult)).unwrap();
    assert_eq!(to_host.1["party"]["media"]["title"], "Secret");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_queue_adds_reorders_removes_and_advances() {
    let e = env().await;
    let (host, guest) = (e.user().await, e.user().await);
    let hv = e.v(host).await;
    let party = e.svc.create(hv, create(1)).await.unwrap();
    join_all(&e, &party, &[guest]).await;
    let gv = e.v(guest).await;
    let add = |title: i64, episode: Option<i64>| QueueAddInput {
        title_id: title,
        episode_id: episode,
        variant_id: None,
    };
    // The default queue policy is controllers: a plain participant may not add.
    assert_eq!(
        e.svc
            .queue_add(gv, party.id, add(2, None))
            .await
            .unwrap_err(),
        PartyError::Forbidden
    );
    e.svc.queue_add(hv, party.id, add(2, None)).await.unwrap();
    e.svc
        .queue_add(hv, party.id, add(5, Some(11)))
        .await
        .unwrap();
    let view = e
        .svc
        .queue_add(hv, party.id, add(5, Some(12)))
        .await
        .unwrap();
    let ids: Vec<i64> = view.queue.iter().map(|q| q.id).collect();
    assert_eq!(view.queue.len(), 3);
    assert_eq!(
        view.queue.iter().map(|q| q.position).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    // An unplayable title cannot be queued.
    assert!(matches!(
        e.svc.queue_add(hv, party.id, add(404, None)).await,
        Err(PartyError::NotFound(_))
    ));
    // Reorder: the last entry goes first.
    let view = e
        .svc
        .queue_move(hv, party.id, ids[2], QueueMoveInput { position: 0 })
        .await
        .unwrap();
    assert_eq!(
        view.queue.iter().map(|q| q.id).collect::<Vec<_>>(),
        vec![ids[2], ids[0], ids[1]]
    );
    // Everyone may add when the policy says so, and the adder may remove their own entry.
    e.svc
        .update_settings(
            hv,
            party.id,
            SettingsInput {
                queue_policy: Some(ControlMode::Everyone),
                ..SettingsInput::default()
            },
        )
        .await
        .unwrap();
    let view = e.svc.queue_add(gv, party.id, add(1, None)).await.unwrap();
    let mine = view.queue.last().unwrap().id;
    e.svc
        .update_settings(
            hv,
            party.id,
            SettingsInput {
                queue_policy: Some(ControlMode::Host),
                ..SettingsInput::default()
            },
        )
        .await
        .unwrap();
    let view = e.svc.queue_remove(gv, party.id, mine).await.unwrap();
    assert_eq!(view.queue.len(), 3);

    // Skip: the first entry starts, paused-state reset, version moves on.
    let before = view.playback.version;
    let view = e.svc.next(hv, party.id).await.unwrap();
    assert_eq!(view.media.episode_id, 12);
    assert_eq!(view.queue.len(), 2);
    assert!(view.playback.version > before);
    assert!(view.playback.playing);
    assert_eq!(view.playback.position_ms, 0);

    // The end of the media advances the queue by itself.
    e.advance(100_000);
    let p = e
        .svc
        .command(gv, party.id, cmd(Action::Ended))
        .await
        .unwrap();
    assert!(p.version > before);
    let view = e.svc.get(hv, party.id).await.unwrap();
    assert_eq!(view.media.asset_id, 502);
    assert_eq!(view.queue.len(), 1);
    // An `ended` report that is not at the end changes nothing.
    let v1 = e.svc.get(hv, party.id).await.unwrap().playback.version;
    e.svc
        .command(gv, party.id, cmd(Action::Ended))
        .await
        .unwrap();
    assert_eq!(e.svc.get(hv, party.id).await.unwrap().playback.version, v1);
    // The queue ends: skipping with nothing queued is a conflict.
    e.svc.next(hv, party.id).await.unwrap();
    assert!(matches!(
        e.svc.next(hv, party.id).await,
        Err(PartyError::Conflict(_))
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn kicks_bans_and_host_transfers_are_the_hosts_and_audited() {
    let e = env().await;
    let (host, a, b) = (e.user().await, e.user().await, e.user().await);
    let hv = e.v(host).await;
    let party = e.svc.create(hv, create(1)).await.unwrap();
    join_all(&e, &party, &[a, b]).await;
    let (av, bv) = (e.v(a).await, e.v(b).await);
    assert_eq!(
        e.svc.kick(av, party.id, b).await.unwrap_err(),
        PartyError::Forbidden
    );
    assert_eq!(
        e.svc.end(av, party.id).await.unwrap_err(),
        PartyError::Forbidden
    );
    assert_eq!(
        e.svc.transfer_host(av, party.id, a).await.unwrap_err(),
        PartyError::Forbidden
    );

    e.events.clear();
    e.svc.kick(hv, party.id, b).await.unwrap();
    assert!(e.svc.get(bv, party.id).await.is_err());
    assert!(matches!(
        e.svc
            .join(
                bv,
                party.id,
                JoinInput {
                    invite_code: party.invite_code.clone()
                }
            )
            .await,
        Err(PartyError::NotFound(_))
    ));
    assert!(
        e.events
            .named("WATCH_PARTY_UPDATE")
            .iter()
            .any(|(u, p)| p["type"] == "removed" && u == &vec![b] && p["reason"] == "kicked")
    );

    let view = e.svc.transfer_host(hv, party.id, a).await.unwrap();
    assert_eq!(view.host_id, a);
    // The old host is just a participant now.
    assert_eq!(
        e.svc.kick(hv, party.id, a).await.unwrap_err(),
        PartyError::Forbidden
    );
    assert_eq!(
        e.svc.transfer_host(av, party.id, uid()).await.unwrap_err(),
        PartyError::Invalid("The new host must be in the party.".to_owned())
    );

    let log = e.svc.audit_log(av, party.id).await.unwrap();
    let actions: Vec<&str> = log.iter().map(|x| x.action.as_str()).collect();
    for want in ["created", "kicked", "host_changed"] {
        assert!(actions.contains(&want), "missing {want} in {actions:?}");
    }
    assert_eq!(
        e.svc.audit_log(hv, party.id).await.unwrap_err(),
        PartyError::Forbidden
    );

    e.svc.end(av, party.id).await.unwrap();
    assert!(matches!(
        e.svc.get(av, party.id).await.map(|v| v.ended),
        Ok(true)
    ));
    assert!(matches!(
        e.svc.command(av, party.id, cmd(Action::Play)).await,
        Err(PartyError::NotFound(_))
    ));
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_account_is_in_one_party_at_a_time() {
    let e = env().await;
    let (host, guest) = (e.user().await, e.user().await);
    let hv = e.v(host).await;
    let first = e.svc.create(hv, create(1)).await.unwrap();
    join_all(&e, &first, &[guest]).await;
    let second = e.svc.create(hv, create(2)).await.unwrap();
    // The host moved on: the first party passed to the guest, the host is only in the second.
    assert_eq!(e.svc.mine(hv).await.unwrap().id, second.id);
    let old = e.svc.get(e.v(guest).await, first.id).await.unwrap();
    assert_eq!(old.host_id, guest);
    assert_eq!(old.participants.len(), 1);
    // Joining the first again moves the guest... they are already in it; the host rejoins.
    e.svc
        .join(
            hv,
            first.id,
            JoinInput {
                invite_code: first.invite_code.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(e.svc.mine(hv).await.unwrap().id, first.id);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_full_party_refuses_and_commands_are_rate_limited() {
    let e = env().await;
    let (host, a, b) = (e.user().await, e.user().await, e.user().await);
    let hv = e.v(host).await;
    let mut input = create(1);
    input.settings = Some(SettingsInput {
        max_participants: Some(2),
        ..SettingsInput::default()
    });
    let party = e.svc.create(hv, input).await.unwrap();
    join_all(&e, &party, &[a]).await;
    assert!(matches!(
        e.svc
            .join(
                e.v(b).await,
                party.id,
                JoinInput {
                    invite_code: party.invite_code.clone()
                }
            )
            .await,
        Err(PartyError::Conflict(_))
    ));
    // 12 commands are allowed in 5 s on the frozen clock; the 13th is limited.
    for i in 0..12 {
        e.svc.command(hv, party.id, seek(i * 1_000)).await.unwrap();
    }
    assert_eq!(
        e.svc.command(hv, party.id, seek(1)).await.unwrap_err(),
        PartyError::RateLimited
    );
    e.advance(5_001);
    assert!(e.svc.command(hv, party.id, seek(1)).await.is_ok());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn reactions_and_messages_reach_participants_only_and_are_validated() {
    let e = env().await;
    let (host, guest, outsider) = (e.user().await, e.user().await, e.user().await);
    let hv = e.v(host).await;
    let party = e.svc.create(hv, create(1)).await.unwrap();
    join_all(&e, &party, &[guest]).await;
    e.events.clear();
    e.svc
        .react(
            e.v(guest).await,
            party.id,
            ReactionInput {
                emoji: "\u{1f37f}".to_owned(),
            },
        )
        .await
        .unwrap();
    e.svc
        .say(
            hv,
            party.id,
            MessageInput {
                content: "hello".to_owned(),
            },
        )
        .await
        .unwrap();
    let r = e.events.named("WATCH_PARTY_REACTION");
    assert_eq!(r.len(), 1);
    assert!(r[0].0.contains(&host) && r[0].0.contains(&guest) && !r[0].0.contains(&outsider));
    assert_eq!(r[0].1["emoji"], "\u{1f37f}");
    assert_eq!(
        e.events.named("WATCH_PARTY_MESSAGE")[0].1["content"],
        "hello"
    );
    assert!(matches!(
        e.svc
            .react(
                e.v(outsider).await,
                party.id,
                ReactionInput { emoji: "x".into() }
            )
            .await,
        Err(PartyError::NotFound(_))
    ));
    assert!(matches!(
        e.svc
            .react(
                hv,
                party.id,
                ReactionInput {
                    emoji: "<b>".into()
                }
            )
            .await,
        Err(PartyError::Invalid(_))
    ));
    // 8 reactions per 5 s.
    for _ in 0..8 {
        e.svc
            .react(hv, party.id, ReactionInput { emoji: "x".into() })
            .await
            .unwrap();
    }
    assert_eq!(
        e.svc
            .react(hv, party.id, ReactionInput { emoji: "x".into() })
            .await
            .unwrap_err(),
        PartyError::RateLimited
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn playback_events_carry_increasing_versions_and_the_server_clock() {
    let e = env().await;
    let (host, guest) = (e.user().await, e.user().await);
    let hv = e.v(host).await;
    let party = e.svc.create(hv, create(1)).await.unwrap();
    join_all(&e, &party, &[guest]).await;
    e.events.clear();
    e.svc
        .command(hv, party.id, cmd(Action::Play))
        .await
        .unwrap();
    e.advance(500);
    e.svc.command(hv, party.id, seek(20_000)).await.unwrap();
    e.advance(500);
    e.svc
        .command(hv, party.id, cmd(Action::Pause))
        .await
        .unwrap();
    let evs = e.events.named("WATCH_PARTY_PLAYBACK");
    assert_eq!(evs.len(), 3);
    let versions: Vec<i64> = evs
        .iter()
        .map(|(_, p)| p["playback"]["version"].as_i64().unwrap())
        .collect();
    assert!(versions.windows(2).all(|w| w[0] < w[1]), "{versions:?}");
    for (users, p) in &evs {
        assert!(users.contains(&guest));
        assert!(p["playback"]["server_time_ms"].as_i64().unwrap() >= T0);
        assert_eq!(p["by"], host.to_string());
    }
    assert_eq!(evs[0].1["reason"], "play");
    assert_eq!(evs[1].1["reason"], "seek");
    assert_eq!(evs[2].1["reason"], "pause");
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_live_tv_party_follows_the_live_edge_and_has_no_seeking() {
    let e = env().await;
    let (host, minor) = (e.adult().await, e.user().await);
    let v = e.v(host).await;
    let live = |channel: i64| CreateInput {
        tv_channel_id: Some(channel),
        ..create(0)
    };
    // Unknown channel, a title and a channel together, and neither.
    assert!(matches!(
        e.svc.create(v, live(99)).await,
        Err(PartyError::NotFound(_))
    ));
    assert!(matches!(
        e.svc
            .create(
                v,
                CreateInput {
                    tv_channel_id: Some(77),
                    ..create(1)
                }
            )
            .await,
        Err(PartyError::Invalid(_))
    ));
    assert!(matches!(
        e.svc.create(v, create(0)).await,
        Err(PartyError::Invalid(_))
    ));
    // A restricted programme needs the age confirmation.
    assert_eq!(
        e.svc
            .create(e.v(minor).await, live(78))
            .await
            .unwrap_err(),
        PartyError::AgeConfirmationRequired
    );

    let party = e.svc.create(v, live(77)).await.unwrap();
    assert!(party.media.live && party.playback.live);
    assert_eq!(party.media.title_id, 77);
    assert!(party.playback.playing, "a live party starts on the live edge");
    assert_eq!(party.playback.duration_ms, None);
    // Seeking and pausing are refused for everybody.
    assert!(e.svc.command(v, party.id, seek(10_000)).await.is_err());
    assert!(e.svc.command(v, party.id, cmd(Action::Pause)).await.is_err());
    // Switching to a film and back works for the host.
    let film = e
        .svc
        .change_media(
            v,
            party.id,
            ChangeMediaInput {
                title_id: 1,
                tv_channel_id: None,
                episode_id: None,
                variant_id: None,
                start_ms: None,
            },
        )
        .await
        .unwrap();
    assert!(!film.media.live);
    let back = e
        .svc
        .change_media(
            v,
            party.id,
            ChangeMediaInput {
                title_id: 0,
                tv_channel_id: Some(77),
                episode_id: None,
                variant_id: None,
                start_ms: None,
            },
        )
        .await
        .unwrap();
    assert!(back.media.live);
}
