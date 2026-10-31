//! Endpoint tests over the in-memory backend with one-shot requests.

use super::{FixedEntitlements, MessagesDeps, UploadEntitlements, router};
use crate::{
    ApiSettings, AppState,
    auth::{BotIdentity, memory::MemoryBots},
    build_router,
};
use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Method, Request, StatusCode},
};
use bytes::Bytes;
use futures::{StreamExt, stream::BoxStream};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sr_chat::guild_rules::plan_new_guild;
use sr_chat::memory::MemoryBackend;
use sr_chat::repo::GuildRepository;
use sr_chat::{Member, MessageType};
use sr_core::{ChannelId, Clock, GuildId, ManualClock, Permissions, SnowflakeGenerator, UserId};
use sr_events::{DomainEvent, EventBus, InProcessBus, Topic};
use sr_storage::{ObjectStorage, ObjectStoreStorage};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

const T0: i64 = 1_800_000_000_000;
const MIB: u64 = 1024 * 1024;
const OWNER: u64 = 1;
const BOB: u64 = 2;
const CAROL: u64 = 3; // not a guild member
const THUMBS_UP: &str = "%F0%9F%91%8D";

/// The owner is a patron (500 MiB); everyone else gets the standard limit. With `.0` set the
/// server is at level 2 (100 MiB for every member in it, D-238); DMs only ever see the perk.
struct Perks(bool);

#[async_trait]
impl UploadEntitlements for Perks {
    async fn max_upload_bytes(&self, user: UserId, guild: Option<GuildId>) -> u64 {
        let perk = if user.get() == OWNER {
            500 * MIB
        } else {
            30 * MIB
        };
        let level = if self.0 && guild.is_some() {
            100 * MIB
        } else {
            0
        };
        perk.max(level)
    }
}

/// Captures audit writes (LNG-SSET-056).
#[derive(Default)]
struct AuditLog(std::sync::Mutex<Vec<(sr_chat::settings::audit::AuditAction, Option<u64>, Value)>>);

#[async_trait]
impl crate::routes::guilds::backend::AuditSink for AuditLog {
    async fn record(
        &self,
        _guild: GuildId,
        _actor: UserId,
        _action: sr_chat::settings::audit::AuditAction,
        _target_id: Option<u64>,
        _changes: Vec<sr_chat::settings::audit::AuditChange>,
        _reason: Option<&str>,
    ) {
    }

    async fn record_with_options(
        &self,
        _guild: GuildId,
        _actor: UserId,
        action: sr_chat::settings::audit::AuditAction,
        target_id: Option<u64>,
        options: Value,
    ) {
        self.0.lock().unwrap().push((action, target_id, options));
    }
}

struct Env {
    app: Router,
    backend: MemoryBackend,
    clock: Arc<ManualClock>,
    bus: Arc<InProcessBus>,
    storage: Arc<ObjectStoreStorage>,
    /// Stand-in for the S3 multipart API behind large uploads (LNG-CMP-007).
    parts: Arc<sr_library::multipart::MemoryPartStore>,
    guild: GuildId,
    ch: ChannelId,
    deps: Arc<MessagesDeps>,
    jobs: Arc<rich_tests::RecordingScheduler>,
    web: Arc<rich_tests::FakeWeb>,
    audit: Arc<AuditLog>,
}

async fn env_with(entitlements: Arc<dyn UploadEntitlements>) -> Env {
    env_full(entitlements, None, None).await
}

async fn env_full(
    entitlements: Arc<dyn UploadEntitlements>,
    gate: Option<crate::routes::settings::AutoModGateHandle>,
    age: Option<Arc<dyn crate::routes::search::AgeGate>>,
) -> Env {
    env_build(entitlements, gate, age, None).await
}

async fn env_build(
    entitlements: Arc<dyn UploadEntitlements>,
    gate: Option<crate::routes::settings::AutoModGateHandle>,
    age: Option<Arc<dyn crate::routes::search::AgeGate>>,
    safety: Option<Arc<dyn super::ViewerSafety>>,
) -> Env {
    let clock = Arc::new(ManualClock::new(T0));
    let dyn_clock: Arc<dyn Clock> = clock.clone();
    let backend = MemoryBackend::new();
    let bus = Arc::new(InProcessBus::new());
    let storage = Arc::new(ObjectStoreStorage::in_memory());

    let gen_ids = SnowflakeGenerator::new(dyn_clock.clone(), 9, 9).unwrap();
    let bp = plan_new_guild(UserId::new(OWNER), "Club", T0, || gen_ids.generate()).unwrap();
    let (guild, ch) = (bp.guild.id, bp.channels[2].id);
    backend.insert_guild(&bp).await.unwrap();
    backend.data().members.push(Member {
        guild_id: guild,
        user_id: UserId::new(BOB),
        nick: None,
        role_ids: vec![],
        joined_at_ms: T0,
        communication_disabled_until_ms: None,
        pending: false,
        avatar_key: None,
        banner_key: None,
        bio: None,
        extras: Default::default(),
    });

    let bots = Arc::new(MemoryBots::new());
    for (tok, uid) in [("owner", OWNER), ("bob", BOB), ("carol", CAROL)] {
        bots.insert(
            tok,
            BotIdentity {
                user_id: i64::try_from(uid).unwrap(),
                application_id: 1,
            },
        );
    }
    // Rate limiting has its own tests; here it would only throttle the test client.
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let audit = Arc::new(AuditLog::default());
    let parts = Arc::new(sr_library::multipart::MemoryPartStore::new(storage.clone()));
    let deps = MessagesDeps::new(
        backend.clone(),
        Arc::new(backend.clone()),
        storage.clone(),
        bus.clone(),
        entitlements,
        Arc::new(backend.clone()),
        None,
        dyn_clock.clone(),
        1,
        1,
    )
    .unwrap()
    .with_cdn_signer(sr_storage::cdn::CdnSigner::new(
        b"messages-test-secret-0123456789ab".to_vec(),
    ))
    .with_audit(audit.clone())
    .with_multipart(sr_library::MultipartWiring::new(parts.clone()).with_part_size(16 * MIB));
    let deps = match age {
        Some(a) => deps.with_age_gate(a),
        None => deps,
    };
    let deps = match safety {
        Some(sf) => deps.with_viewer_safety(sf),
        None => deps,
    };
    let jobs = Arc::new(rich_tests::RecordingScheduler::default());
    let web = Arc::new(rich_tests::FakeWeb::default());
    let deps = Arc::new(deps.with_rich(super::RichDeps {
        store: Some(Arc::new(backend.clone())),
        scheduler: Some(jobs.clone()),
        unfurler: Some(Arc::new(web.unfurler())),
        gate: None,
    }));
    let mut state = AppState::builder()
        .bots(bots)
        .clock(dyn_clock)
        .settings(settings)
        .extension(deps.clone());
    if let Some(g) = gate {
        state = state.extension(Arc::new(g));
    }
    let state = state.build();
    let app = build_router(state, router());
    Env {
        app,
        backend,
        clock,
        bus,
        storage,
        parts,
        guild,
        ch,
        deps,
        jobs,
        web,
        audit,
    }
}

async fn env() -> Env {
    env_with(Arc::new(FixedEntitlements(30 * MIB))).await
}

#[tokio::test]
async fn user_objects_carry_sr_patron_and_a_flag_change_refreshes_members() {
    use sr_chat::repo::UserSummary;
    let e = env().await;
    let bob = UserId::new(BOB);
    let set = |patron: bool| {
        e.backend.data().users.insert(
            bob,
            UserSummary {
                id: bob,
                username: "bob".into(),
                display_name: None,
                avatar_key: None,
                banner_key: None,
                bot: false,
                primary_guild: None,
                patron,
            },
        );
    };
    // The flag is always present on the author object, true or false (D-256).
    set(false);
    let users = std::collections::HashMap::from([(bob, e.backend.data().users[&bob].clone())]);
    assert_eq!(
        serde_json::to_value(super::model::UserRef::new(bob, &users)).unwrap()["sr_patron"],
        json!(false)
    );
    let mut events = e.bus.subscribe(Topic::Guild(e.guild));
    set(true);
    e.deps.refresh_member_objects(bob).await;
    let ev = tokio::time::timeout(Duration::from_secs(2), events.next())
        .await
        .unwrap()
        .unwrap();
    let DomainEvent::MemberUpdated { user, payload, .. } = ev else {
        panic!("expected MemberUpdated, got {ev:?}");
    };
    assert_eq!(user, bob);
    assert_eq!(payload["guild_id"], e.guild.to_string());
    assert_eq!(payload["user"]["sr_patron"], json!(true));
}

impl Env {
    async fn call(
        &self,
        method: Method,
        path: &str,
        who: &str,
        body: Option<Value>,
    ) -> (StatusCode, HeaderMap, Value) {
        let uri = format!("/api/v1/channels/{}{path}", self.ch);
        let mut req = Request::builder()
            .method(method)
            .uri(uri)
            .header("authorization", format!("Bot {who}"));
        let body = match body {
            Some(v) => {
                req = req.header("content-type", "application/json");
                Body::from(v.to_string())
            }
            None => Body::empty(),
        };
        let resp = self
            .app
            .clone()
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let (parts, body) = resp.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        let json = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        (parts.status, parts.headers, json)
    }

    async fn get(&self, path: &str, who: &str) -> (StatusCode, Value) {
        let (s, _, v) = self.call(Method::GET, path, who, None).await;
        (s, v)
    }

    async fn post(&self, path: &str, who: &str, body: Value) -> (StatusCode, Value) {
        let (s, _, v) = self.call(Method::POST, path, who, Some(body)).await;
        (s, v)
    }

    async fn patch(&self, path: &str, who: &str, body: Value) -> (StatusCode, Value) {
        let (s, _, v) = self.call(Method::PATCH, path, who, Some(body)).await;
        (s, v)
    }

    async fn say(&self, who: &str, content: &str) -> Value {
        let (s, v) = self
            .post("/messages", who, json!({ "content": content }))
            .await;
        assert_eq!(s, StatusCode::OK, "{v}");
        v
    }

    async fn status(&self, method: Method, path: &str, who: &str) -> StatusCode {
        self.call(method, path, who, None).await.0
    }

    fn events(&self) -> BoxStream<'static, DomainEvent> {
        self.bus.subscribe(Topic::Channel(self.ch))
    }

    /// Remove a permission from `@everyone`.
    fn revoke_everyone(&self, p: Permissions) {
        let mut d = self.backend.data();
        let role = d.roles.iter_mut().find(|r| r.is_everyone()).unwrap();
        role.permissions.remove(p);
    }
}

async fn next_event(s: &mut BoxStream<'static, DomainEvent>) -> DomainEvent {
    tokio::time::timeout(Duration::from_secs(1), s.next())
        .await
        .expect("no event")
        .expect("stream ended")
}

async fn no_event(s: &mut BoxStream<'static, DomainEvent>) {
    assert!(
        tokio::time::timeout(Duration::from_millis(80), s.next())
            .await
            .is_err(),
        "unexpected event"
    );
}

fn ids(list: &Value) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_owned())
        .collect()
}

fn id_of(v: &Value) -> String {
    v["id"].as_str().unwrap().to_owned()
}

fn code(r: &(StatusCode, Value)) -> (StatusCode, Option<u64>) {
    (r.0, r.1["code"].as_u64())
}

fn chunk(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut v = u32::try_from(payload.len()).unwrap().to_be_bytes().to_vec();
    v.extend_from_slice(kind);
    v.extend_from_slice(payload);
    v.extend_from_slice(&[0; 4]); // CRC is not checked by the stripper
    v
}

/// A structurally valid PNG carrying a text chunk with a location secret.
fn png_with_secret() -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    v.extend(chunk(b"IHDR", &[0; 13]));
    v.extend(chunk(b"tEXt", b"GPS\0lat=51.5,lon=-0.12"));
    v.extend(chunk(b"IDAT", b"pixels"));
    v.extend(chunk(b"IEND", b""));
    v
}

#[tokio::test]
async fn create_get_and_list_have_discord_shape() {
    let e = env().await;
    let mut ev = e.events();
    let m = e.say("bob", "hello <@1> and <#2>").await;
    assert_eq!(m["content"], "hello <@1> and <#2>");
    assert_eq!(m["author"]["id"], "2");
    assert_eq!(m["channel_id"], e.ch.to_string());
    assert_eq!(m["guild_id"], e.guild.to_string());
    assert_eq!(m["type"], 0);
    assert_eq!(m["mentions"][0]["id"], "1");
    assert!(
        m["timestamp"]
            .as_str()
            .unwrap()
            .starts_with("2027-01-15T08:00:00")
    );
    assert!(m["edited_timestamp"].is_null());

    let id = id_of(&m);
    let (s, one) = e.get(&format!("/messages/{id}"), "owner").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(one["id"], id.as_str());
    let (_, list) = e.get("/messages", "owner").await;
    assert_eq!(ids(&list), vec![id.clone()]);

    match next_event(&mut ev).await {
        DomainEvent::MessageSent { data, author, .. } => {
            assert_eq!(author.get(), BOB);
            assert_eq!(data["id"], id.as_str());
            assert_eq!(data["content"], "hello <@1> and <#2>");
            // RDY-PERF-002: rendered once; the author's answer is the gateway payload.
            assert_eq!(data, m);
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn empty_and_invalid_bodies_are_rejected_in_discord_shape() {
    let e = env().await;
    let r = e.post("/messages", "bob", json!({})).await;
    assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50006)));
    let r = e
        .post("/messages", "bob", json!({"content": "x".repeat(2001)}))
        .await;
    assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50035)));
    assert!(r.1["errors"]["content"].is_object());
    let r = e.post("/messages", "bob", json!({"content": 5})).await;
    assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50035)));
    let r = e
        .post("/messages", "bob", json!({"content": "x", "poll": {}}))
        .await;
    assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50035)));
    let r = e.get("/messages?limit=0", "bob").await;
    assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50035)));
    let r = e.get("/messages?before=1&after=2", "bob").await;
    assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50035)));
}

#[tokio::test]
async fn unauthenticated_and_outsiders_are_refused() {
    let e = env().await;
    let req = Request::get(format!("/api/v1/channels/{}/messages", e.ch))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        e.app.clone().oneshot(req).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let r = e.get("/messages", "carol").await;
    assert_eq!(code(&r), (StatusCode::FORBIDDEN, Some(50001)));
    let r = e.post("/messages", "carol", json!({"content": "hi"})).await;
    assert_eq!(code(&r), (StatusCode::FORBIDDEN, Some(50001)));
    let req = Request::get("/api/v1/channels/not-an-id/messages")
        .header("authorization", "Bot bob")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        e.app.clone().oneshot(req).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn pagination_before_after_around() {
    let e = env().await;
    let mut all = Vec::new();
    for i in 0..7 {
        all.push(id_of(&e.say("bob", &format!("m{i}")).await));
    }
    let a = |i: usize| all[i].clone();
    let (_, v) = e.get("/messages?limit=3", "bob").await;
    assert_eq!(ids(&v), vec![a(6), a(5), a(4)]);
    let (_, v) = e
        .get(&format!("/messages?limit=2&before={}", a(4)), "bob")
        .await;
    assert_eq!(ids(&v), vec![a(3), a(2)]);
    // `after` returns the messages right after the cursor, still newest first.
    let (_, v) = e
        .get(&format!("/messages?limit=2&after={}", a(1)), "bob")
        .await;
    assert_eq!(ids(&v), vec![a(3), a(2)]);
    let (_, v) = e
        .get(&format!("/messages?limit=5&around={}", a(3)), "bob")
        .await;
    assert_eq!(ids(&v), vec![a(5), a(4), a(3), a(2), a(1)]);
    // Near the start the window keeps the target and fills from the newer side.
    let (_, v) = e
        .get(&format!("/messages?limit=4&around={}", a(0)), "bob")
        .await;
    assert_eq!(ids(&v), vec![a(2), a(1), a(0)]);
    let (_, v) = e.get("/messages", "bob").await;
    assert_eq!(v.as_array().unwrap().len(), 7);
}

#[tokio::test]
async fn history_requires_read_message_history() {
    let e = env().await;
    e.say("bob", "visible").await;
    e.revoke_everyone(Permissions::READ_MESSAGE_HISTORY);
    let (s, v) = e.get("/messages", "bob").await;
    assert_eq!((s, v), (StatusCode::OK, json!([])));
    let (_, v) = e.get("/messages", "owner").await;
    assert_eq!(v.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn nonce_dedupes_only_with_enforce_nonce() {
    let e = env().await;
    let mut ev = e.events();
    let body = json!({"content": "once", "nonce": 12345});
    // Without enforce_nonce a repeated nonce creates another message (Discord parity).
    let (s, a) = e.post("/messages", "bob", body.clone()).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(a["nonce"], "12345");
    let (s, b) = e.post("/messages", "bob", body).await;
    assert_eq!(s, StatusCode::OK);
    assert_ne!(a["id"], b["id"]);
    let (_, list) = e.get("/messages", "bob").await;
    assert_eq!(list.as_array().unwrap().len(), 2);
    next_event(&mut ev).await;
    next_event(&mut ev).await;

    // With enforce_nonce the original comes back with 200 and no new event.
    let strict = json!({"content": "x", "nonce": "n2", "enforce_nonce": true});
    let (s, first) = e.post("/messages", "bob", strict.clone()).await;
    assert_eq!(s, StatusCode::OK);
    next_event(&mut ev).await;
    let (s, again) = e.post("/messages", "bob", strict.clone()).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(first["id"], again["id"]);
    assert_eq!(again["nonce"], "n2");
    no_event(&mut ev).await;

    // Another author may reuse the nonce; so may the same author after the window.
    let (_, c) = e.post("/messages", "owner", strict.clone()).await;
    assert_ne!(c["id"], first["id"]);
    e.clock.advance_ms(6 * 60 * 1000);
    let (_, d) = e.post("/messages", "bob", strict).await;
    assert_ne!(d["id"], first["id"]);

    let r = e
        .post(
            "/messages",
            "bob",
            json!({"content": "x", "nonce": "x".repeat(26)}),
        )
        .await;
    assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50035)));
}

#[tokio::test]
async fn replies_reference_visible_messages_only() {
    let e = env().await;
    let first = e.say("owner", "question").await;
    let fid = id_of(&first);
    let (s, r) = e
        .post(
            "/messages",
            "bob",
            json!({"content": "answer", "message_reference": {"message_id": fid}}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(r["type"], MessageType::Reply.code());
    assert_eq!(r["message_reference"]["message_id"], fid.as_str());

    let missing = json!({"message_id": "999999999999"});
    let r = e
        .post(
            "/messages",
            "bob",
            json!({"content": "x", "message_reference": missing}),
        )
        .await;
    assert_eq!(code(&r), (StatusCode::NOT_FOUND, Some(10008)));
    let lenient = json!({"message_id": "999999999999", "fail_if_not_exists": false});
    let (s, v) = e
        .post(
            "/messages",
            "bob",
            json!({"content": "x", "message_reference": lenient}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["type"], 0);
    assert!(v.get("message_reference").is_none());
}

#[tokio::test]
async fn mentions_are_permission_gated() {
    let e = env().await;
    let m = e.say("bob", "@everyone <@1> <@&5>").await;
    assert_eq!(m["mention_everyone"], false);
    assert_eq!(m["mentions"][0]["id"], "1");
    assert_eq!(m["mention_roles"], json!([]));
    let m = e.say("owner", "@everyone hi").await;
    assert_eq!(m["mention_everyone"], true);
    let r = e
        .post(
            "/messages",
            "bob",
            json!({"content": (1..=120).map(|i| format!("<@{i}>")).collect::<String>()}),
        )
        .await;
    assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50035)));
}

#[tokio::test]
async fn age_restricted_channels_need_the_18_plus_answer() {
    // Only the owner has confirmed; bob has not (D-032, LNG-CHN-006).
    let confirmed = crate::routes::search::ConfirmedUsers([UserId::new(OWNER)].into());
    let e = env_full(
        Arc::new(FixedEntitlements(30 * MIB)),
        None,
        Some(Arc::new(confirmed)),
    )
    .await;
    // An ordinary channel is open to bob.
    e.say("bob", "hello").await;
    e.backend.data().channels.get_mut(&e.ch).unwrap().nsfw = true;
    for (method, path, body) in [
        (Method::GET, "/messages", None),
        (Method::POST, "/messages", Some(json!({"content": "x"}))),
        (Method::GET, "/pins", None),
    ] {
        let (s, _, v) = e.call(method.clone(), path, "bob", body.clone()).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{method} {path}: {v}");
        assert_eq!(v["code"], 900_011, "{method} {path}: {v}");
    }
    // The confirmed owner reads and writes as usual.
    e.say("owner", "adults only").await;
    assert_eq!(e.get("/messages", "owner").await.0, StatusCode::OK);
    // Answering yes (modelled by adding bob) opens the channel for him too.
    let both = crate::routes::search::ConfirmedUsers([UserId::new(OWNER), UserId::new(BOB)].into());
    let e2 = env_full(
        Arc::new(FixedEntitlements(30 * MIB)),
        None,
        Some(Arc::new(both)),
    )
    .await;
    e2.backend.data().channels.get_mut(&e2.ch).unwrap().nsfw = true;
    e2.say("bob", "now allowed").await;
}

#[tokio::test]
async fn slowmode_returns_retry_after() {
    let e = env().await;
    e.backend
        .data()
        .channels
        .get_mut(&e.ch)
        .unwrap()
        .rate_limit_per_user = 10;
    e.say("bob", "first").await;
    e.clock.advance_ms(4000);
    let (s, h, v) = e
        .call(
            Method::POST,
            "/messages",
            "bob",
            Some(json!({"content": "again"})),
        )
        .await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(v["code"], 20016);
    assert_eq!(v["retry_after"], 6.0);
    assert_eq!(v["global"], false);
    assert_eq!(h["retry-after"], "6");
    // Moderators bypass.
    e.say("owner", "a").await;
    e.say("owner", "b").await;
    e.clock.advance_ms(6000);
    e.say("bob", "now ok").await;
}

#[tokio::test]
async fn edit_rules_and_event() {
    let e = env().await;
    let m = e.say("bob", "typo").await;
    let path = format!("/messages/{}", id_of(&m));
    let mut ev = e.events();
    e.clock.advance_ms(1000);
    let (s, v) = e.patch(&path, "bob", json!({"content": "fixed"})).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["content"], "fixed");
    assert!(v["edited_timestamp"].is_string());
    match next_event(&mut ev).await {
        DomainEvent::MessageEdited { data, .. } => assert_eq!(data["content"], "fixed"),
        other => panic!("{other:?}"),
    }
    // Others cannot edit content (50005), even moderators.
    let r = e
        .patch(&path, "owner", json!({"content": "mine now"}))
        .await;
    assert_eq!(code(&r), (StatusCode::FORBIDDEN, Some(50005)));
    // Emptying the message is refused.
    let r = e.patch(&path, "bob", json!({"content": "  "})).await;
    assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50006)));
    let r = e
        .patch("/messages/42", "bob", json!({"content": "x"}))
        .await;
    assert_eq!(r.0, StatusCode::NOT_FOUND);
    assert_eq!(e.backend.data().edits.len(), 1);
}

#[tokio::test]
async fn delete_hides_message_and_publishes() {
    let e = env().await;
    let id = id_of(&e.say("bob", "oops").await);
    let oid = id_of(&e.say("owner", "mod's").await);
    let mut ev = e.events();

    assert_eq!(
        e.status(Method::DELETE, &format!("/messages/{oid}"), "bob")
            .await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.status(Method::DELETE, &format!("/messages/{id}"), "bob")
            .await,
        StatusCode::NO_CONTENT
    );
    match next_event(&mut ev).await {
        DomainEvent::MessageDeleted { message_id, .. } => assert_eq!(message_id.to_string(), id),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        e.get(&format!("/messages/{id}"), "bob").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.status(Method::DELETE, &format!("/messages/{id}"), "bob")
            .await,
        StatusCode::NOT_FOUND
    );
    let (_, list) = e.get("/messages", "bob").await;
    assert_eq!(ids(&list), vec![oid.clone()]);
    // Moderators delete anything; the message is purge-scheduled, not removed.
    assert_eq!(
        e.status(Method::DELETE, &format!("/messages/{oid}"), "owner")
            .await,
        StatusCode::NO_CONTENT
    );
    let d = e.backend.data();
    assert!(
        d.messages
            .iter()
            .all(|m| m.purge_after_ms == Some(T0 + 30 * 86_400_000))
    );
}

#[tokio::test]
async fn bulk_delete_rules() {
    let e = env().await;
    let mut made = Vec::new();
    for i in 0..4 {
        made.push(id_of(&e.say("bob", &format!("{i}")).await));
    }
    let mut ev = e.events();
    let body = json!({"messages": [made[0], made[1], made[2]]});
    let r = e.post("/messages/bulk-delete", "bob", body.clone()).await;
    assert_eq!(code(&r), (StatusCode::FORBIDDEN, Some(50013)));
    let r = e
        .post(
            "/messages/bulk-delete",
            "owner",
            json!({"messages": [made[0]]}),
        )
        .await;
    assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50035)));
    let (s, _) = e.post("/messages/bulk-delete", "owner", body).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    match next_event(&mut ev).await {
        DomainEvent::MessagesBulkDeleted { message_ids, .. } => assert_eq!(message_ids.len(), 3),
        other => panic!("{other:?}"),
    }
    let (_, list) = e.get("/messages", "bob").await;
    assert_eq!(ids(&list), vec![made[3].clone()]);

    // Older than 14 days is refused.
    e.clock.advance_ms(15 * 86_400_000);
    let fresh = id_of(&e.say("bob", "fresh").await);
    let r = e
        .post(
            "/messages/bulk-delete",
            "owner",
            json!({"messages": [made[3], fresh]}),
        )
        .await;
    assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50035)));
}

#[tokio::test]
async fn reactions_lifecycle() {
    let e = env().await;
    let id = id_of(&e.say("owner", "react to me").await);
    let base = format!("/messages/{id}/reactions");
    let mine = format!("{base}/{THUMBS_UP}/@me");
    let mut ev = e.events();

    assert_eq!(
        e.status(Method::PUT, &mine, "bob").await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.status(Method::PUT, &mine, "bob").await,
        StatusCode::NO_CONTENT
    ); // idempotent
    assert_eq!(
        e.status(Method::PUT, &mine, "owner").await,
        StatusCode::NO_CONTENT
    );
    match next_event(&mut ev).await {
        DomainEvent::ReactionAdded { user, emoji, .. } => {
            assert_eq!(user.get(), BOB);
            assert_eq!(emoji["name"], "\u{1F44D}");
            assert!(emoji["id"].is_null());
        }
        other => panic!("{other:?}"),
    }
    next_event(&mut ev).await; // owner's
    no_event(&mut ev).await; // the repeat published nothing

    let (_, one) = e.get(&format!("/messages/{id}"), "bob").await;
    assert_eq!(one["reactions"][0]["count"], 2);
    assert_eq!(one["reactions"][0]["me"], true);
    assert_eq!(one["reactions"][0]["emoji"]["name"], "\u{1F44D}");

    let (s, users) = e.get(&format!("{base}/{THUMBS_UP}"), "bob").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(users, json!([{"id": "1"}, {"id": "2"}]));
    let (_, users) = e
        .get(&format!("{base}/{THUMBS_UP}?limit=1&after=1"), "bob")
        .await;
    assert_eq!(users, json!([{"id": "2"}]));

    // Custom emoji work; bare words are not emoji.
    assert_eq!(
        e.status(Method::PUT, &format!("{base}/party:77/@me"), "bob")
            .await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.status(Method::PUT, &format!("{base}/smile/@me"), "bob")
            .await,
        StatusCode::NOT_FOUND
    );

    // Removing: own is free, someone else's needs MANAGE_MESSAGES.
    assert_eq!(
        e.status(Method::DELETE, &format!("{base}/{THUMBS_UP}/1"), "bob")
            .await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.status(Method::DELETE, &mine, "bob").await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.status(Method::DELETE, &mine, "bob").await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.status(Method::DELETE, &format!("{base}/{THUMBS_UP}/1"), "owner")
            .await,
        StatusCode::NO_CONTENT
    );

    // Clearing needs MANAGE_MESSAGES.
    assert_eq!(
        e.status(Method::DELETE, &base, "bob").await,
        StatusCode::FORBIDDEN
    );
    let party = format!("{base}/party:77");
    assert_eq!(
        e.status(Method::DELETE, &party, "bob").await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.status(Method::DELETE, &party, "owner").await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.status(Method::DELETE, &party, "owner").await,
        StatusCode::NOT_FOUND
    );
    e.status(Method::PUT, &mine, "bob").await;
    assert_eq!(
        e.status(Method::DELETE, &base, "owner").await,
        StatusCode::NO_CONTENT
    );
    let (_, one) = e.get(&format!("/messages/{id}"), "bob").await;
    assert!(one.get("reactions").is_none());
    // Outsiders see nothing.
    assert_eq!(
        e.status(Method::PUT, &mine, "carol").await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn reaction_limits_and_permissions() {
    let e = env().await;
    let id = id_of(&e.say("owner", "popular").await);
    for i in 0..20 {
        let em = format!("e{i:02}:{}", 1000 + i);
        assert_eq!(
            e.status(
                Method::PUT,
                &format!("/messages/{id}/reactions/{em}/@me"),
                "bob"
            )
            .await,
            StatusCode::NO_CONTENT
        );
    }
    let r = e
        .call(
            Method::PUT,
            &format!("/messages/{id}/reactions/e99:9999/@me"),
            "bob",
            None,
        )
        .await;
    assert_eq!(
        (r.0, r.2["code"].as_u64()),
        (StatusCode::BAD_REQUEST, Some(30010))
    );
    // Joining an existing emoji is still allowed at the cap.
    assert_eq!(
        e.status(
            Method::PUT,
            &format!("/messages/{id}/reactions/e00:1000/@me"),
            "owner"
        )
        .await,
        StatusCode::NO_CONTENT
    );
    // Without ADD_REACTIONS a new emoji is refused.
    e.revoke_everyone(Permissions::ADD_REACTIONS);
    let m2 = id_of(&e.say("owner", "another").await);
    assert_eq!(
        e.status(
            Method::PUT,
            &format!("/messages/{m2}/reactions/{THUMBS_UP}/@me"),
            "bob"
        )
        .await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn reaction_objects_carry_the_burst_shim_and_ignore_type() {
    let e = env().await;
    let id = id_of(&e.say("owner", "shim").await);
    let base = format!("/messages/{id}/reactions/{THUMBS_UP}");
    // `type` is accepted on every reaction route and treated as a normal reaction (D-231).
    assert_eq!(
        e.status(Method::PUT, &format!("{base}/@me?type=1"), "bob")
            .await,
        StatusCode::NO_CONTENT
    );
    let (_, one) = e.get(&format!("/messages/{id}"), "bob").await;
    let r = &one["reactions"][0];
    assert_eq!(r["count"], 1);
    assert_eq!(r["me"], true);
    assert_eq!(r["me_burst"], false);
    assert_eq!(
        r["count_details"],
        serde_json::json!({"burst": 0, "normal": 1})
    );
    assert_eq!(r["burst_colors"], serde_json::json!([]));
    assert_eq!(
        e.get(&format!("{base}?type=1"), "bob").await.1,
        serde_json::json!([])
    );
    assert_eq!(
        e.get(&format!("{base}?type=0"), "bob")
            .await
            .1
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        e.status(Method::DELETE, &format!("{base}/@me?type=1"), "bob")
            .await,
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn pins_flow() {
    let e = env().await;
    let id = id_of(&e.say("bob", "pin me").await);
    let mut ev = e.events();
    let pin = format!("/pins/{id}");

    assert_eq!(
        e.status(Method::PUT, &pin, "bob").await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.status(Method::PUT, &pin, "owner").await,
        StatusCode::NO_CONTENT
    );
    match next_event(&mut ev).await {
        DomainEvent::MessageSent { data, .. } => {
            assert_eq!(data["type"], 6);
            assert_eq!(data["message_reference"]["message_id"], id.as_str());
        }
        other => panic!("{other:?}"),
    }
    match next_event(&mut ev).await {
        DomainEvent::ChannelPinsUpdated { last_pin_ms, .. } => assert_eq!(last_pin_ms, Some(T0)),
        other => panic!("{other:?}"),
    }
    // Already pinned.
    assert_eq!(
        e.status(Method::PUT, &pin, "owner").await,
        StatusCode::BAD_REQUEST
    );

    let (s, legacy) = e.get("/pins", "bob").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(ids(&legacy), vec![id.clone()]);
    assert_eq!(legacy[0]["pinned"], true);
    let (_, page) = e.get("/messages/pins", "bob").await;
    assert_eq!(page["has_more"], false);
    assert_eq!(page["items"][0]["message"]["id"], id.as_str());
    assert!(
        page["items"][0]["pinned_at"]
            .as_str()
            .unwrap()
            .starts_with("2027-")
    );

    // The system message is in history.
    let (_, list) = e.get("/messages", "bob").await;
    assert_eq!(list[0]["type"], 6);

    assert_eq!(
        e.status(Method::DELETE, &pin, "bob").await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.status(Method::DELETE, &pin, "owner").await,
        StatusCode::NO_CONTENT
    );
    match next_event(&mut ev).await {
        DomainEvent::ChannelPinsUpdated { last_pin_ms, .. } => assert_eq!(last_pin_ms, None),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        e.status(Method::DELETE, &pin, "owner").await,
        StatusCode::NOT_FOUND
    );
    let (_, legacy) = e.get("/pins", "bob").await;
    assert_eq!(legacy, json!([]));
}

#[tokio::test]
async fn typing_publishes_for_members_only() {
    let e = env().await;
    let mut ev = e.events();
    assert_eq!(
        e.status(Method::POST, "/typing", "bob").await,
        StatusCode::NO_CONTENT
    );
    match next_event(&mut ev).await {
        DomainEvent::TypingStarted {
            user,
            timestamp,
            guild,
            ..
        } => {
            assert_eq!(user.get(), BOB);
            assert_eq!(timestamp, u64::try_from(T0 / 1000).unwrap());
            assert_eq!(guild, Some(e.guild));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        e.status(Method::POST, "/typing", "carol").await,
        StatusCode::FORBIDDEN
    );
    no_event(&mut ev).await;
}

fn slot_key(ch: ChannelId, slot: &Value, filename: &str) -> String {
    format!(
        "chat/{ch}/{}/{filename}",
        slot["upload_id"].as_str().unwrap()
    )
}

#[tokio::test]
async fn upload_slot_enforces_entitlement_limits() {
    let e = env_with(Arc::new(Perks(false))).await;
    let big = json!({"files": [{"filename": "movie.mkv", "file_size": 31 * MIB}]});
    let r = e.post("/attachments", "bob", big.clone()).await;
    assert_eq!(code(&r), (StatusCode::PAYLOAD_TOO_LARGE, Some(40005)));
    // Patrons may send up to 500 MiB; large files expire after 7 days.
    let (s, v) = e.post("/attachments", "owner", big).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let slot = &v["attachments"][0];
    assert_eq!(slot["max_size"], 500 * MIB);
    assert_eq!(slot["expires_after"], 7 * 24 * 3600);
    assert!(slot["upload_url"].as_str().unwrap().contains("movie.mkv"));

    let r = e
        .post(
            "/attachments",
            "owner",
            json!({"files": [{"filename": "x", "file_size": 501 * MIB}]}),
        )
        .await;
    assert_eq!(code(&r), (StatusCode::PAYLOAD_TOO_LARGE, Some(40005)));
    let r = e
        .post(
            "/attachments",
            "bob",
            json!({"files": [{"filename": "x", "file_size": 0}]}),
        )
        .await;
    assert_eq!(r.0, StatusCode::BAD_REQUEST);
    let r = e.post("/attachments", "bob", json!({"files": []})).await;
    assert_eq!(r.0, StatusCode::BAD_REQUEST);
    let r = e
        .post(
            "/attachments",
            "carol",
            json!({"files": [{"filename": "x", "file_size": 5}]}),
        )
        .await;
    assert_eq!(r.0, StatusCode::FORBIDDEN);
    // Slots are all-or-nothing: one oversized file rejects the batch.
    let r = e
        .post(
            "/attachments",
            "bob",
            json!({"files": [
                {"filename": "a", "file_size": 5},
                {"filename": "b", "file_size": 31 * MIB},
            ]}),
        )
        .await;
    assert_eq!(r.0, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn the_server_level_lifts_the_upload_limit_for_free_members_but_not_in_dms() {
    let e = env_with(Arc::new(Perks(true))).await;
    // A free member of a level-2 server may send 100 MiB there, and not a byte more.
    let ok = json!({"files": [{"filename": "clip.mkv", "file_size": 100 * MIB}]});
    let (s, v) = e.post("/attachments", "bob", ok).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["attachments"][0]["max_size"], 100 * MIB);
    let over = json!({"files": [{"filename": "clip.mkv", "file_size": 101 * MIB}]});
    let r = e.post("/attachments", "bob", over).await;
    assert_eq!(code(&r), (StatusCode::PAYLOAD_TOO_LARGE, Some(40005)));
    // The patron keeps the larger of the two.
    let (_, v) = e
        .post(
            "/attachments",
            "owner",
            json!({"files": [{"filename": "a", "file_size": 200 * MIB}]}),
        )
        .await;
    assert_eq!(v["attachments"][0]["max_size"], 500 * MIB);
    // The advertised number follows the same rule: server level in the server, perk alone in DMs.
    let bob = UserId::new(BOB);
    assert_eq!(
        e.deps.effective_upload_limit(bob, Some(e.guild)).await,
        100 * MIB
    );
    assert_eq!(e.deps.effective_upload_limit(bob, None).await, 30 * MIB);
    assert_eq!(
        e.deps
            .effective_upload_limit(UserId::new(OWNER), None)
            .await,
        500 * MIB
    );
}

/// Bob blocks media in age-restricted channels; nobody has friends.
struct BlockServers;

#[async_trait]
impl super::ViewerSafety for BlockServers {
    async fn sensitive_view(
        &self,
        viewer: UserId,
    ) -> (
        sr_chat::social::safety::SensitiveMedia,
        std::collections::HashSet<UserId>,
    ) {
        use sr_chat::social::safety::MediaFilter;
        let servers = if viewer.get() == BOB {
            MediaFilter::Block
        } else if viewer.get() == OWNER {
            MediaFilter::Blur
        } else {
            MediaFilter::Show
        };
        (
            sr_chat::social::safety::SensitiveMedia {
                friends: MediaFilter::Show,
                others: MediaFilter::Show,
                servers,
            },
            Default::default(),
        )
    }
}

#[tokio::test]
async fn age_restricted_media_is_filtered_per_viewer_on_the_list() {
    let all = crate::routes::search::ConfirmedUsers(
        [UserId::new(OWNER), UserId::new(BOB), UserId::new(CAROL)].into(),
    );
    let e = env_build(
        Arc::new(FixedEntitlements(30 * MIB)),
        None,
        Some(Arc::new(all)),
        Some(Arc::new(BlockServers)),
    )
    .await;
    let png = png_with_secret();
    let (s, v) = e
        .post(
            "/attachments",
            "carol",
            json!({"files": [{"id": "0", "filename": "cat.png", "file_size": png.len(), "content_type": "image/png"}]}),
        )
        .await;
    // Carol is not a member: use the owner as the author instead.
    assert_eq!(s, StatusCode::FORBIDDEN, "{v}");
    let (s, v) = e
        .post(
            "/attachments",
            "owner",
            json!({"files": [{"id": "0", "filename": "cat.png", "file_size": png.len(), "content_type": "image/png"}]}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let slot = v["attachments"][0].clone();
    e.storage
        .put(&slot_key(e.ch, &slot, "cat.png"), Bytes::from(png))
        .await
        .unwrap();
    e.backend.data().channels.get_mut(&e.ch).unwrap().nsfw = true;
    let (s, sent) = e
        .post(
            "/messages",
            "owner",
            json!({"content": "pic", "attachments": [{"id": "0", "uploaded_filename": slot["upload_filename"]}]}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{sent}");
    // The author sees the file as it is, whatever their own setting says.
    assert_eq!(sent["attachments"][0]["filename"], "cat.png");
    let (_, own) = e.get("/messages", "owner").await;
    assert_eq!(own[0]["attachments"][0]["filename"], "cat.png");
    assert!(own[0]["attachments"][0].get("sr_filtered").is_none());
    // Bob blocks: the attachment is listed without a URL and flagged.
    let (_, bob) = e.get("/messages", "bob").await;
    let a = &bob[0]["attachments"][0];
    assert_eq!(
        (a["url"].as_str(), a["sr_filtered"].as_bool()),
        (Some(""), Some(true)),
        "{bob}"
    );
    assert_eq!(a["id"], sent["attachments"][0]["id"]);
    // A single fetch goes through the same decorator.
    let id = sent["id"].as_str().unwrap();
    let (_, one) = e.get(&format!("/messages/{id}"), "bob").await;
    assert_eq!(one["attachments"][0]["sr_filtered"], true, "{one}");
}

#[tokio::test]
async fn declared_picture_size_and_blurhash_reach_the_attachment() {
    let e = env().await;
    let png = png_with_secret();
    let (s, v) = e
        .post(
            "/attachments",
            "bob",
            json!({"files": [{
                "id": "0", "filename": "cat.png", "file_size": png.len(),
                "content_type": "image/png", "width": 640, "height": 480,
                "blurhash": "LEHV6nWB2yk8pyo0adR*.7kCMdnj"
            }]}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let slot = v["attachments"][0].clone();
    e.storage
        .put(&slot_key(e.ch, &slot, "cat.png"), Bytes::from(png))
        .await
        .unwrap();
    let msg = json!({
        "content": "pic",
        "attachments": [{"id": "0", "uploaded_filename": slot["upload_filename"]}],
    });
    let (s, v) = e.post("/messages", "bob", msg).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let a = &v["attachments"][0];
    assert_eq!(a["width"], 640);
    assert_eq!(a["height"], 480);
    assert_eq!(a["blurhash"], "LEHV6nWB2yk8pyo0adR*.7kCMdnj");
    let (_, list) = e.get("/messages", "bob").await;
    assert_eq!(list[0]["attachments"][0]["blurhash"], a["blurhash"]);
}

#[tokio::test]
async fn bad_picture_size_or_blurhash_is_refused() {
    let e = env().await;
    for file in [
        json!({"filename": "a.png", "file_size": 10, "width": 100}),
        json!({"filename": "a.png", "file_size": 10, "width": 0, "height": 5}),
        json!({"filename": "a.png", "file_size": 10, "width": 99999, "height": 5}),
        json!({"filename": "a.png", "file_size": 10, "blurhash": "short"}),
        json!({"filename": "a.png", "file_size": 10, "blurhash": "has spaces in it!!!"}),
        json!({"filename": "a.png", "file_size": 10, "blurhash": "x".repeat(200)}),
    ] {
        let r = e
            .post("/attachments", "bob", json!({"files": [file.clone()]}))
            .await;
        assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50035)), "{file}");
    }
    // A file without any picture metadata is unaffected.
    let r = e
        .post(
            "/attachments",
            "bob",
            json!({"files": [{"filename": "a.txt", "file_size": 10}]}),
        )
        .await;
    assert_eq!(r.0, StatusCode::OK);
}

#[tokio::test]
async fn upload_then_message_sniffs_content_type() {
    let e = env().await;
    let png = png_with_secret();
    let mut ev = e.events();
    let (s, v) = e
        .post(
            "/attachments",
            "bob",
            json!({"files": [{
                "id": "0", "filename": "../cat.png", "file_size": png.len(),
                "content_type": "image/png"
            }]}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let slot = v["attachments"][0].clone();
    assert_eq!(slot["id"], "0");
    assert!(
        slot["upload_filename"]
            .as_str()
            .unwrap()
            .ends_with("/cat.png")
    );

    // Referencing before the PUT happened fails.
    let msg = json!({
        "content": "look",
        "attachments": [{"id": "0", "uploaded_filename": slot["upload_filename"]}],
    });
    let r = e.post("/messages", "bob", msg.clone()).await;
    assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50035)));

    e.storage
        .put(&slot_key(e.ch, &slot, "cat.png"), Bytes::from(png.clone()))
        .await
        .unwrap();
    // Another user cannot use the slot.
    let r = e.post("/messages", "owner", msg.clone()).await;
    assert_eq!(r.0, StatusCode::BAD_REQUEST);

    let (s, v) = e.post("/messages", "bob", msg.clone()).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let a = &v["attachments"][0];
    assert_eq!(a["filename"], "cat.png");
    assert_eq!(a["content_type"], "image/png");
    // Metadata was stripped from the stored object and the size updated.
    let stored = e
        .storage
        .get(&slot_key(e.ch, &slot, "cat.png"))
        .await
        .unwrap();
    assert!(!stored.windows(3).any(|w| w == b"GPS"));
    assert!(stored.len() < png.len());
    assert_eq!(a["size"], stored.len());
    assert!(a["url"].as_str().unwrap().starts_with("/cdn/attachments/"));
    match next_event(&mut ev).await {
        DomainEvent::MessageSent { data, .. } => {
            assert_eq!(data["attachments"][0]["content_type"], "image/png");
        }
        other => panic!("{other:?}"),
    }
    // History shows it too, and an upload cannot be attached twice.
    let (_, list) = e.get("/messages", "bob").await;
    assert_eq!(list[0]["attachments"][0]["id"], a["id"]);
    let r = e.post("/messages", "bob", msg).await;
    assert_eq!(r.0, StatusCode::BAD_REQUEST);
}

async fn delete(e: &Env, path: &str, who: &str) -> StatusCode {
    e.status(Method::DELETE, path, who).await
}

#[tokio::test]
async fn large_uploads_are_multipart_resumable_and_complete_from_the_stored_parts() {
    let e = env_with(Arc::new(Perks(false))).await;
    let size = 70 * MIB; // five parts of 16 MiB, the last one 6 MiB
    let (s, v) = e
        .post(
            "/attachments",
            "owner",
            json!({"files": [{"id": "0", "filename": "film.bin", "file_size": size}]}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let slot = v["attachments"][0].clone();
    assert_eq!(slot["upload_url"], "");
    let mp = &slot["multipart"];
    assert_eq!(mp["part_size"], 16 * MIB);
    assert_eq!(mp["part_count"], 5);
    assert_eq!(mp["parts"].as_array().unwrap().len(), 5);
    let up = slot["upload_id"].as_str().unwrap().to_owned();
    let mp_id = e
        .deps
        .store
        .attachment_multipart(up.parse().unwrap())
        .await
        .unwrap()
        .unwrap()
        .0;

    // Nothing is in yet: every part is missing. Another user cannot look.
    let (s, p) = e.get(&format!("/attachments/{up}/parts"), "owner").await;
    assert_eq!(s, StatusCode::OK, "{p}");
    assert_eq!(p["received"].as_array().unwrap().len(), 0);
    assert_eq!(p["missing"].as_array().unwrap().len(), 5);
    let r = e.get(&format!("/attachments/{up}/parts"), "bob").await;
    assert_eq!(r.0, StatusCode::NOT_FOUND);

    // Two parts arrive (one of them with the wrong size), then the connection drops.
    let part = |n: usize| Bytes::from(vec![n as u8; (16 * MIB) as usize]);
    e.parts.put_part(&mp_id, 1, part(1));
    e.parts.put_part(&mp_id, 2, Bytes::from_static(b"short"));
    let (_, p) = e.get(&format!("/attachments/{up}/parts"), "owner").await;
    assert_eq!(p["received"].as_array().unwrap().len(), 1);
    assert_eq!(p["received"][0]["part_number"], 1);
    let missing: Vec<u64> = p["missing"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["part_number"].as_u64().unwrap())
        .collect();
    assert_eq!(missing, vec![2, 3, 4, 5]);

    // Completing early is refused and leaves the slot usable.
    let r = e
        .post(&format!("/attachments/{up}/complete"), "owner", json!({}))
        .await;
    assert_eq!(r.0, StatusCode::BAD_REQUEST, "{}", r.1);

    // The rest arrives after the resume.
    for n in 2..=4 {
        e.parts.put_part(&mp_id, n as u32, part(n));
    }
    e.parts
        .put_part(&mp_id, 5, Bytes::from(vec![5u8; (6 * MIB) as usize]));
    let (s, a) = e
        .post(&format!("/attachments/{up}/complete"), "owner", json!({}))
        .await;
    assert_eq!(s, StatusCode::OK, "{a}");
    assert_eq!(a["size"], size);
    let stored = e
        .storage
        .head(&slot_key(e.ch, &slot, "film.bin"))
        .await
        .unwrap();
    assert_eq!(stored.size, size);
    // The multipart record is gone and the upload is no longer resumable.
    assert!(
        e.deps
            .store
            .attachment_multipart(up.parse().unwrap())
            .await
            .unwrap()
            .is_none()
    );
    let r = e.get(&format!("/attachments/{up}/parts"), "owner").await;
    assert_eq!(r.0, StatusCode::BAD_REQUEST);
    // Completing twice is harmless.
    let r = e
        .post(&format!("/attachments/{up}/complete"), "owner", json!({}))
        .await;
    assert_eq!(r.0, StatusCode::OK);
}

#[tokio::test]
async fn aborting_a_multipart_upload_frees_its_parts_and_small_files_stay_single_put() {
    let e = env_with(Arc::new(Perks(false))).await;
    let (_, v) = e
        .post(
            "/attachments",
            "owner",
            json!({"files": [
                {"id": "big", "filename": "big.bin", "file_size": 70 * MIB},
                {"id": "small", "filename": "small.bin", "file_size": 5 * MIB},
            ]}),
        )
        .await;
    let big = &v["attachments"][0];
    let small = &v["attachments"][1];
    assert!(small["multipart"].is_null());
    assert!(small["upload_url"].as_str().unwrap().contains("small.bin"));
    let up = big["upload_id"].as_str().unwrap();
    let mp_id = e
        .deps
        .store
        .attachment_multipart(up.parse().unwrap())
        .await
        .unwrap()
        .unwrap()
        .0;
    // Only the owner of the upload may cancel it.
    assert_eq!(
        delete(&e, &format!("/attachments/{up}"), "bob").await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        delete(&e, &format!("/attachments/{up}"), "owner").await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(e.parts.aborted(), vec![mp_id]);
    assert!(
        e.deps
            .store
            .attachment_multipart(up.parse().unwrap())
            .await
            .unwrap()
            .is_none()
    );
    // The aborted slot itself is gone: asking for its parts (or aborting it again) is a 404, which tells the
    // client to drop its resume ledger row (D-271).
    assert_eq!(
        e.get(&format!("/attachments/{up}/parts"), "owner").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        delete(&e, &format!("/attachments/{up}"), "owner").await,
        StatusCode::NOT_FOUND
    );
    // A single-PUT slot can be cancelled too.
    let small_id = small["upload_id"].as_str().unwrap();
    assert_eq!(
        delete(&e, &format!("/attachments/{small_id}"), "owner").await,
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn spoiler_uploads_carry_the_spoiler_prefix() {
    let e = env().await;
    let png = png_with_secret();
    // `spoiler: true` prefixes the name once; an already prefixed name is not doubled
    // and counts as a spoiler without the flag (Discord's convention, D-177).
    for (name, spoiler, expected) in [
        ("cat.png", Some(true), "SPOILER_cat.png"),
        ("SPOILER_cat.png", Some(true), "SPOILER_cat.png"),
        ("SPOILER_cat.png", None, "SPOILER_cat.png"),
        ("cat.png", Some(false), "cat.png"),
    ] {
        let mut file = json!({
            "id": "0", "filename": name, "file_size": png.len(), "content_type": "image/png"
        });
        if let Some(s) = spoiler {
            file["spoiler"] = json!(s);
        }
        let (s, v) = e
            .post("/attachments", "bob", json!({"files": [file]}))
            .await;
        assert_eq!(s, StatusCode::OK, "{v}");
        let slot = v["attachments"][0].clone();
        assert!(
            slot["upload_filename"]
                .as_str()
                .unwrap()
                .ends_with(&format!("/{expected}")),
            "{name}: {slot}"
        );
        e.storage
            .put(&slot_key(e.ch, &slot, expected), Bytes::from(png.clone()))
            .await
            .unwrap();
        let msg = json!({
            "content": "x",
            "attachments": [{"id": "0", "uploaded_filename": slot["upload_filename"]}],
        });
        let (s, v) = e.post("/messages", "bob", msg).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(v["attachments"][0]["filename"], expected, "{name}");
    }
}

#[tokio::test]
async fn completion_rejects_mismatched_content_and_sizes() {
    let e = env().await;
    // Declared as an image but the bytes are HTML: refused and deleted.
    let html = b"<html><script>alert(1)</script></html>";
    let (_, v) = e
        .post(
            "/attachments",
            "bob",
            json!({"files": [{
                "filename": "x.png", "file_size": html.len(), "content_type": "image/png"
            }]}),
        )
        .await;
    let slot = v["attachments"][0].clone();
    let key = slot_key(e.ch, &slot, "x.png");
    e.storage.put(&key, Bytes::from_static(html)).await.unwrap();
    let path = format!(
        "/attachments/{}/complete",
        slot["upload_id"].as_str().unwrap()
    );
    assert_eq!(
        e.post(&path, "bob", json!({})).await.0,
        StatusCode::BAD_REQUEST
    );
    assert!(e.storage.head(&key).await.is_err());

    // Size differing from the declaration is refused.
    let (_, v) = e
        .post(
            "/attachments",
            "bob",
            json!({"files": [{"filename": "a.bin", "file_size": 100}]}),
        )
        .await;
    let slot = v["attachments"][0].clone();
    e.storage
        .put(
            &slot_key(e.ch, &slot, "a.bin"),
            Bytes::from(png_with_secret()),
        )
        .await
        .unwrap();
    let path = format!(
        "/attachments/{}/complete",
        slot["upload_id"].as_str().unwrap()
    );
    assert_eq!(
        e.post(&path, "bob", json!({})).await.0,
        StatusCode::BAD_REQUEST
    );

    // Unknown bytes are served as opaque downloads whatever the client claimed.
    let (_, v) = e
        .post(
            "/attachments",
            "bob",
            json!({"files": [{"filename": "pic.svg", "file_size": html.len(),
                              "content_type": "text/html"}]}),
        )
        .await;
    let slot = v["attachments"][0].clone();
    e.storage
        .put(&slot_key(e.ch, &slot, "pic.svg"), Bytes::from_static(html))
        .await
        .unwrap();
    let path = format!(
        "/attachments/{}/complete",
        slot["upload_id"].as_str().unwrap()
    );
    let (s, v) = e.post(&path, "bob", json!({})).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["content_type"], "application/octet-stream");
    // Completion is idempotent; only the uploader may complete.
    assert_eq!(e.post(&path, "bob", json!({})).await.0, StatusCode::OK);
    assert_eq!(
        e.post(&path, "owner", json!({})).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn attachments_need_attach_files() {
    let e = env().await;
    e.revoke_everyone(Permissions::ATTACH_FILES);
    let r = e
        .post(
            "/attachments",
            "bob",
            json!({"files": [{"filename": "a", "file_size": 3}]}),
        )
        .await;
    assert_eq!(code(&r), (StatusCode::FORBIDDEN, Some(50013)));
}

#[tokio::test]
async fn openapi_fragment_documents_every_route() {
    let doc = serde_json::to_value(super::openapi()).unwrap();
    let paths = doc["paths"].as_object().unwrap();
    for p in [
        "/channels/{channel_id}/messages",
        "/channels/{channel_id}/messages/{message_id}",
        "/channels/{channel_id}/messages/bulk-delete",
        "/channels/{channel_id}/messages/pins",
        "/channels/{channel_id}/messages/{message_id}/reactions",
        "/channels/{channel_id}/messages/{message_id}/reactions/{emoji}",
        "/channels/{channel_id}/messages/{message_id}/reactions/{emoji}/@me",
        "/channels/{channel_id}/messages/{message_id}/reactions/{emoji}/{user_id}",
        "/channels/{channel_id}/pins",
        "/channels/{channel_id}/pins/{message_id}",
        "/channels/{channel_id}/typing",
        "/guilds/{guild_id}/bridge/members",
        "/channels/{channel_id}/attachments",
        "/channels/{channel_id}/attachments/{upload_id}/complete",
    ] {
        assert!(paths.contains_key(p), "missing {p}");
    }
    assert!(paths["/channels/{channel_id}/messages"]["post"].is_object());
    assert!(doc["components"]["schemas"]["MessageObject"].is_object());
}

#[tokio::test]
async fn allowed_mentions_gate_pings_and_tts_is_a_flag() {
    let e = env().await;
    // parse: [] pings nobody, though the text stays.
    let (s, m) = e
        .post(
            "/messages",
            "bob",
            json!({"content": "@everyone <@1>", "allowed_mentions": {"parse": []}}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(m["mention_everyone"], false);
    assert_eq!(m["mentions"], json!([]));
    // A listed user id may ping even when users are not parsed.
    let (_, m) = e
        .post(
            "/messages",
            "bob",
            json!({"content": "<@1> <@3>", "allowed_mentions": {"users": ["1"]}}),
        )
        .await;
    assert_eq!(m["mentions"][0]["id"], "1");
    assert_eq!(m["mentions"].as_array().unwrap().len(), 1);
    // Malformed allowed_mentions is a form error.
    let r = e
        .post(
            "/messages",
            "bob",
            json!({"content": "x", "allowed_mentions": {"parse": ["nope"]}}),
        )
        .await;
    assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50035)));
    // tts needs SEND_TTS_MESSAGES, which @everyone has by default (Discord).
    let (s, m) = e
        .post(
            "/messages",
            "bob",
            json!({"content": "hear me", "tts": true}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(m["tts"], true);
    let (_, m) = e
        .post(
            "/messages",
            "owner",
            json!({"content": "hear me", "tts": true}),
        )
        .await;
    assert_eq!(m["tts"], true);
}

#[tokio::test]
async fn embeds_are_stored_and_echoed() {
    let e = env().await;
    let body = json!({
        "content": "",
        "embeds": [{"title": "Hello", "type": "rich", "fields": [{"name": "a", "value": "b"}]}]
    });
    let (s, m) = e.post("/messages", "bob", body).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(m["embeds"][0]["title"], "Hello");
    assert_eq!(m["embeds"][0]["fields"][0]["name"], "a");
    let (_, list) = e.get("/messages", "bob").await;
    assert_eq!(list[0]["embeds"][0]["title"], "Hello");
}

#[tokio::test]
async fn third_party_embed_images_get_a_proxy_url_when_rendered() {
    let e = env().await;
    let body = json!({
        "content": "",
        "embeds": [{
            "title": "pic",
            "image": {"url": "https://img.example/a.png", "proxy_url": "https://evil.example/x"},
            "author": {"name": "a", "icon_url": "https://img.example/i.png"}
        }]
    });
    let (s, m) = e.post("/messages", "bob", body).await;
    assert_eq!(s, StatusCode::OK, "{m}");
    for emb in [
        &m["embeds"][0],
        &e.get("/messages", "bob").await.1[0]["embeds"][0],
    ] {
        // The original URL stays (the client shows it only behind "Load image"); the
        // proxied form is added, and a client-sent proxy_url never survives.
        assert_eq!(emb["image"]["url"], "https://img.example/a.png");
        let p = emb["image"]["proxy_url"].as_str().unwrap();
        assert!(p.starts_with("/cdn/external/"), "{p}");
        assert!(
            emb["author"]["proxy_icon_url"]
                .as_str()
                .unwrap()
                .starts_with("/cdn/external/")
        );
    }
}

#[tokio::test]
async fn dm_events_go_once_to_each_recipients_user_topic() {
    let e = env().await;
    let dm = ChannelId::new(777);
    {
        let mut d = e.backend.data();
        d.channels.insert(
            dm,
            sr_chat::Channel::new(dm, None, sr_chat::ChannelType::Dm),
        );
        d.recipients.insert((dm, UserId::new(OWNER)));
        d.recipients.insert((dm, UserId::new(BOB)));
    }
    let mut owner = e.bus.subscribe(Topic::User(UserId::new(OWNER)));
    let mut bob = e.bus.subscribe(Topic::User(UserId::new(BOB)));
    let mut carol = e.bus.subscribe(Topic::User(UserId::new(CAROL)));
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/channels/{dm}/messages"))
        .header("authorization", "Bot bob")
        .header("content-type", "application/json")
        .body(Body::from(json!({ "content": "psst" }).to_string()))
        .unwrap();
    let resp = e.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    for s in [&mut owner, &mut bob] {
        assert!(matches!(
            next_event(s).await,
            DomainEvent::MessageSent { guild: None, .. }
        ));
        no_event(s).await;
    }
    no_event(&mut carol).await;
}

// ---- AutoMod gate on message create ---------------------------------------------

/// Blocks "badword", alerts on "alertword", errors on "boom".
struct FakeGate;

#[async_trait]
impl crate::routes::settings::AutoModGate for FakeGate {
    async fn evaluate(
        &self,
        msg: &crate::routes::settings::MessageCheck,
    ) -> Result<crate::routes::settings::AutoModVerdict, crate::HttpError> {
        use crate::routes::settings::AutoModVerdict;
        use crate::routes::settings::automod_gate::{Alert, Block};
        let mut v = AutoModVerdict::default();
        if msg.content.contains("boom") {
            return Err(sr_core::ErrorCode::SrInternal.into());
        }
        if msg.content.contains("badword") {
            v.block = Some(Block {
                rule_id: 7,
                rule_name: "words".into(),
                custom_message: None,
            });
        }
        if msg.content.contains("alertword") || msg.content.contains("badword") {
            v.alerts.push(Alert {
                channel_id: ChannelId::new(99),
                rule_id: 7,
                trigger: sr_chat::settings::automod::TriggerKind::Keyword,
                rule_name: "words".into(),
                matched: "word".into(),
                user_id: msg.author,
                source_channel_id: msg.channel,
                content: msg.content.clone(),
            });
        }
        Ok(v)
    }
}

async fn gated_env() -> Env {
    env_full(
        Arc::new(FixedEntitlements(30 * MIB)),
        Some(crate::routes::settings::AutoModGateHandle(Arc::new(
            FakeGate,
        ))),
        None,
    )
    .await
}

#[tokio::test]
async fn automod_block_rejects_with_200000_and_stores_nothing() {
    let e = gated_env().await;
    let mut guild_events = e.bus.subscribe(Topic::Guild(e.guild));
    let r = e
        .post("/messages", "bob", json!({ "content": "a badword here" }))
        .await;
    assert_eq!(
        code(&r),
        (StatusCode::BAD_REQUEST, Some(200_000)),
        "{}",
        r.1
    );
    // Not stored: the channel history is empty.
    let (_, list) = e.get("/messages", "bob").await;
    assert!(ids(&list).is_empty(), "{list}");
    // The alert action of the blocking rule is still announced, without a message id.
    match next_event(&mut guild_events).await {
        DomainEvent::AutoModActionExecuted { guild, payload } => {
            assert_eq!(guild, e.guild);
            assert_eq!(payload["action"]["type"], 2);
            assert!(payload.get("message_id").is_none());
        }
        other => panic!("unexpected {other:?}"),
    }
    // Clean messages still go through.
    e.say("bob", "hello").await;
}

#[tokio::test]
async fn automod_alert_publishes_the_execution_event_after_storing() {
    let e = gated_env().await;
    let mut guild_events = e.bus.subscribe(Topic::Guild(e.guild));
    let m = e.say("bob", "an alertword appears").await;
    let ev = loop {
        let ev = next_event(&mut guild_events).await;
        if matches!(ev, DomainEvent::AutoModActionExecuted { .. }) {
            break ev;
        }
    };
    match ev {
        DomainEvent::AutoModActionExecuted { payload, .. } => {
            assert_eq!(payload["message_id"], m["id"]);
            assert_eq!(payload["action"]["metadata"]["channel_id"], "99");
            assert_eq!(payload["rule_id"], "7");
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn a_failing_automod_check_lets_the_message_through() {
    let e = gated_env().await;
    e.say("bob", "boom goes the check").await;
}

#[tokio::test]
async fn messages_work_without_a_registered_gate() {
    let e = env().await;
    e.say("bob", "a badword here").await;
}

mod rich_tests;

#[path = "tests/external_pg.rs"]
mod external_pg;

#[tokio::test]
async fn moderator_message_actions_are_audited() {
    use sr_chat::settings::audit::AuditAction;
    let e = env().await;
    let bobs = id_of(&e.say("bob", "mine").await);
    let owners = id_of(&e.say("owner", "mod's").await);
    let bobs2 = id_of(&e.say("bob", "again").await);
    let bobs3 = id_of(&e.say("bob", "and again").await);
    let bobs4 = id_of(&e.say("bob", "last").await);
    let pin = format!("/pins/{bobs}");

    e.status(Method::PUT, &pin, "owner").await;
    e.status(Method::DELETE, &pin, "owner").await;
    // The author deleting their own message is not an audit event; a moderator is.
    e.status(Method::DELETE, &format!("/messages/{owners}"), "owner")
        .await;
    e.status(Method::DELETE, &format!("/messages/{bobs2}"), "bob")
        .await;
    e.status(Method::DELETE, &format!("/messages/{bobs3}"), "owner")
        .await;
    let (s, _) = e
        .post(
            "/messages/bulk-delete",
            "owner",
            json!({ "messages": [bobs, bobs4] }),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);

    let log = e.audit.0.lock().unwrap().clone();
    let ch = e.ch.to_string();
    assert_eq!(
        log,
        vec![
            (
                AuditAction::MessagePin,
                Some(BOB),
                json!({ "channel_id": ch, "message_id": bobs })
            ),
            (
                AuditAction::MessageUnpin,
                Some(BOB),
                json!({ "channel_id": ch, "message_id": bobs })
            ),
            (
                AuditAction::MessageDelete,
                Some(BOB),
                json!({ "channel_id": ch, "count": 1 })
            ),
            (
                AuditAction::MessageBulkDelete,
                Some(e.ch.get()),
                json!({ "count": 2 })
            ),
        ]
    );
}

/// D-235: removing a webhook message by a moderator is logged against its creator and names the
/// webhook in `options`.
#[tokio::test]
async fn deleting_a_webhook_message_notes_the_webhook_in_the_audit_entry() {
    use sr_chat::settings::audit::AuditAction;
    let e = env().await;
    let posted = id_of(&e.say("bob", "deploy done").await);
    e.backend.data().webhook_authors.insert(
        sr_core::MessageId::new(posted.parse().unwrap()),
        sr_chat::rich_repo::WebhookAuthor {
            webhook_id: 4242,
            username: "Deploys".into(),
            avatar_url: None,
        },
    );
    e.status(Method::DELETE, &format!("/messages/{posted}"), "owner")
        .await;
    let log = e.audit.0.lock().unwrap().clone();
    assert_eq!(
        log,
        vec![(
            AuditAction::MessageDelete,
            Some(BOB),
            json!({ "channel_id": e.ch.to_string(), "count": 1, "webhook_id": "4242" })
        )]
    );
}

/// BRG-GW-001: a message that came in over the Discord bridge shows the Discord user as its author, carries
/// `sr_bridge`, and its attachment links turn into "expired" once their `ex` has passed (nothing is re-hosted).
#[tokio::test]
async fn a_bridged_message_carries_sr_bridge_and_its_links_expire() {
    use sr_chat::rich_repo::{BridgeAuthor, BridgeLink};
    let e = env().await;
    let posted = id_of(&e.say("bob", "hello from discord").await);
    e.backend.data().bridge_authors.insert(
        sr_core::MessageId::new(posted.parse().unwrap()),
        BridgeAuthor {
            platform: "discord".into(),
            author_name: "Dana".into(),
            author_avatar: Some("https://cdn.discordapp.com/avatars/7/abc.png".into()),
            discord_user_id: Some("7".into()),
            links: vec![
                BridgeLink {
                    filename: "old.png".into(),
                    url: "https://cdn.discordapp.com/attachments/1/2/old.png?ex=1".into(),
                    expires_at_ms: Some(T0 - 1),
                },
                BridgeLink {
                    filename: "new.png".into(),
                    url: "https://cdn.discordapp.com/attachments/1/2/new.png?ex=2".into(),
                    expires_at_ms: Some(T0 + 3_600_000),
                },
            ],
        },
    );
    let (s, m) = e.get(&format!("/messages/{posted}"), "owner").await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert_eq!(m["author"]["username"], "Dana", "{m}");
    assert_eq!(m["author"]["bot"], true);
    // The author keeps the integration account's id: only the shown name and avatar change.
    assert_eq!(m["author"]["id"], BOB.to_string());
    let b = &m["sr_bridge"];
    assert_eq!(b["platform"], "discord", "{m}");
    assert_eq!(b["author_name"], "Dana");
    assert_eq!(b["member_id"], "7");
    let links = b["links"].as_array().unwrap();
    assert_eq!(links[0]["filename"], "old.png");
    assert_eq!(links[0]["expired"], true);
    assert!(links[0].get("url").is_none(), "an expired link is withheld");
    assert_eq!(links[1]["expired"], false);
    assert!(links[1]["url"].as_str().unwrap().contains("new.png"));
    // A native message has no `sr_bridge`.
    let other = id_of(&e.say("bob", "plain").await);
    let (_, p) = e.get(&format!("/messages/{other}"), "owner").await;
    assert!(p.get("sr_bridge").is_none(), "{p}");
}

/// BRG-GW-001: the Discord members bridged into a server are listed for any member of it, and are not
/// accounts (display data only).
#[tokio::test]
async fn bridged_members_are_listed_for_server_members_only() {
    use sr_chat::rich_repo::BridgedMember;
    let e = env().await;
    e.backend.data().bridged_members = vec![
        BridgedMember {
            discord_user_id: "7".into(),
            display_name: "Dana".into(),
            avatar_url: None,
            role_name: Some("Mod".into()),
            role_color: Some(0x00ff_0000),
            bot: false,
            presence: Some("idle".into()),
        },
        BridgedMember {
            discord_user_id: "8".into(),
            display_name: "Robo".into(),
            avatar_url: None,
            role_name: None,
            role_color: None,
            bot: true,
            presence: None,
        },
    ];
    let list = |who: &'static str| {
        let app = e.app.clone();
        let uri = format!("/api/v1/guilds/{}/bridge/members", e.guild);
        async move {
            let req = Request::get(uri)
                .header("authorization", format!("Bot {who}"))
                .body(Body::empty())
                .unwrap();
            let resp = app.oneshot(req).await.unwrap();
            let status = resp.status();
            let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap();
            (
                status,
                serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
            )
        }
    };
    let (s, v) = list("bob").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let m = &v[0];
    assert_eq!(m["id"], "7");
    assert_eq!(m["display_name"], "Dana");
    assert_eq!(m["role_name"], "Mod");
    assert_eq!(m["role_color"], 0x00ff_0000);
    assert_eq!(m["presence"], "idle");
    assert!(m["avatar"].is_null());
    assert_eq!(v[1]["bot"], true);
    assert!(v[1]["presence"].is_null(), "unknown presence is null");
    // Not a member of the server: no list.
    let (s, _) = list("carol").await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

/// Wave 35 (commander default): a `<@id>` is looked up among the server's bridged Discord members first, so an id
/// that collides with an account here never pings that account.
#[tokio::test]
async fn a_bridged_member_id_that_collides_with_an_account_never_pings_it() {
    use sr_chat::rich_repo::BridgedMember;
    let e = env().await;
    // Without a bridged member of that id, `<@2>` is bob and pings him.
    let m = e.say("owner", &format!("hi <@{BOB}>")).await;
    assert_eq!(m["mentions"][0]["id"], BOB.to_string(), "{m}");
    // A Discord member with the same id is bridged into the server: the mention is theirs, bob is not pinged.
    e.backend.data().bridged_members = vec![BridgedMember {
        discord_user_id: BOB.to_string(),
        display_name: "Dana on Discord".into(),
        avatar_url: None,
        role_name: None,
        role_color: None,
        bot: false,
        presence: None,
    }];
    let m = e.say("owner", &format!("hi <@{BOB}>")).await;
    assert_eq!(m["mentions"], json!([]), "{m}");
    assert_eq!(
        m["content"],
        format!("hi <@{BOB}>"),
        "the text keeps the token"
    );
}

#[test]
fn custom_emoji_tokens_are_found_in_text() {
    use super::rich_model::custom_emoji_ids;
    assert_eq!(
        custom_emoji_ids("a <:wave:12> b <a:dance:34> <:bad:x> <::5> <@1> <:"),
        vec![12, 34]
    );
}

#[tokio::test]
async fn a_finished_call_becomes_a_call_message_with_its_details() {
    let e = env().await;
    let mut events = e.events();
    super::call_record::post_call_record(
        &e.deps,
        e.ch,
        UserId::new(OWNER),
        &[UserId::new(OWNER), UserId::new(BOB)],
        T0,
        T0 + 90_000,
    )
    .await
    .unwrap();

    // Announced like any message.
    let DomainEvent::MessageSent { data, author, .. } = next_event(&mut events).await else {
        panic!("expected MESSAGE_CREATE");
    };
    assert_eq!(author, UserId::new(OWNER));
    assert_eq!(data["type"], 3);
    assert_eq!(data["content"], "");
    assert_eq!(data["call"]["participants"], json!(["1", "2"]));
    assert_eq!(
        data["call"]["ended_timestamp"],
        "2027-01-15T08:01:30.000000+00:00"
    );

    // And listed the same way, as a system message nobody can edit.
    let (s, list) = e.get("/messages", "bob").await;
    assert_eq!(s, StatusCode::OK);
    let m = list.as_array().unwrap().first().unwrap().clone();
    assert_eq!(m["type"], 3);
    assert_eq!(m["call"]["participants"], json!(["1", "2"]));
    let id = m["id"].as_str().unwrap();
    let (s, _) = e
        .patch(&format!("/messages/{id}"), "owner", json!({"content": "x"}))
        .await;
    assert!(s.is_client_error(), "call records are not editable");
}
