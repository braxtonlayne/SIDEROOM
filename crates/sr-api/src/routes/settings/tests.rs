//! Endpoint tests over the in-memory repositories with one-shot requests.

use super::automod_gate::{AutoModGate, MessageCheck};
use super::events::RecordingEvents;
use super::join::InviteMemberJoiner;
use super::{AutoModGateHandle, SettingsDeps, router};
use crate::routes::messages::{self, FixedEntitlements, MessagesDeps};
use crate::{
    ApiSettings, AppState,
    auth::{
        BotIdentity, csrf,
        memory::{MemoryBots, MemorySessions},
    },
    build_router,
};
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use data_encoding::BASE64;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sr_chat::guild_rules::plan_new_guild;
use sr_chat::memory::MemoryBackend;
use sr_chat::repo::GuildRepository;
use sr_chat::settings::expressions::ExpressionLimits;
use sr_chat::settings::memory::MemorySettingsRepo;
use sr_chat::settings::repo::WebhookRepository;
use sr_chat::settings::service::SettingsService;
use sr_chat::{Member, Role};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, Clock, GuildId, ManualClock, RoleId, SnowflakeGenerator, UserId};
use sr_events::InProcessBus;
use sr_storage::{ObjectStorage, ObjectStoreStorage};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tower::ServiceExt;

const T0: i64 = 1_800_000_000_000;
const OWNER: u64 = 1;
const BOB: u64 = 2; // member without roles
const CAROL: u64 = 3; // not a member
const MOD: u64 = 4; // member with a management role

struct Env {
    app: Router,
    backend: MemoryBackend,
    repo: Arc<MemorySettingsRepo>,
    events: Arc<RecordingEvents>,
    storage: Arc<ObjectStoreStorage>,
    deps: Arc<SettingsDeps>,
    sessions: Arc<MemorySessions>,
    clock: Arc<ManualClock>,
    bus: Arc<InProcessBus>,
    guild: GuildId,
    ch: ChannelId,
    voice: ChannelId,
    /// The Server Directory listings behind `GET /emojis/{id}`.
    directory: Arc<sr_chat::settings::directory::MemoryDirectory>,
    actions: Arc<sr_features::testing::FakeActions>,
    /// The applications behind application emoji (D-237).
    apps: Arc<sr_apps::memory::MemoryAppsRepo>,
    apps_svc: sr_apps::AppsService,
}

struct NoPassword;

#[async_trait::async_trait]
impl crate::routes::developer::PasswordConfirm for NoPassword {
    async fn confirm(
        &self,
        _now: i64,
        _user: i64,
        _password: &str,
    ) -> crate::routes::messages::error::MsgResult<()> {
        Ok(())
    }
}

#[derive(Default)]
struct Opts {
    limits: Option<ExpressionLimits>,
    wired_joiner: bool,
    timeouter: Option<Arc<dyn super::MemberTimeouter>>,
    presence: Option<Arc<dyn super::PresenceCounter>>,
    voice: Option<Arc<dyn crate::routes::voice::VoiceBackend>>,
    /// Register the per-server feature routes over fake feature actions.
    features: bool,
}

async fn env() -> Env {
    env_with(Opts::default()).await
}

async fn env_with(opts: Opts) -> Env {
    let clock = Arc::new(ManualClock::new(T0));
    let dyn_clock: Arc<dyn Clock> = clock.clone();
    let backend = MemoryBackend::new();
    let bus = Arc::new(InProcessBus::new());
    let storage = Arc::new(ObjectStoreStorage::in_memory());

    let gen_ids = SnowflakeGenerator::new(dyn_clock.clone(), 9, 9).unwrap();
    let bp = plan_new_guild(UserId::new(OWNER), "Club", T0, || gen_ids.generate()).unwrap();
    let (guild, ch, voice) = (bp.guild.id, bp.channels[2].id, bp.channels[3].id);
    backend.insert_guild(&bp).await.unwrap();
    for user in [BOB, MOD] {
        backend.data().members.push(Member {
            guild_id: guild,
            user_id: UserId::new(user),
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
    }
    let mod_role = RoleId::new(gen_ids.generate().get());
    {
        let mut d = backend.data();
        d.roles.push(Role {
            id: mod_role,
            guild_id: guild,
            name: "mods".into(),
            color: 0,
            hoist: false,
            position: 1,
            permissions: Permissions::MANAGE_GUILD
                | Permissions::MANAGE_WEBHOOKS
                | Permissions::MANAGE_GUILD_EXPRESSIONS
                | Permissions::VIEW_AUDIT_LOG
                | Permissions::MANAGE_CHANNELS,
            mentionable: false,
            managed: false,
            unicode_emoji: None,
            icon_key: None,
        });
        for m in d.members.iter_mut().filter(|m| m.user_id.get() == MOD) {
            m.role_ids.push(mod_role);
        }
    }

    let actions = Arc::new(sr_features::testing::FakeActions::new());
    let sessions = Arc::new(MemorySessions::new());
    let bots = Arc::new(MemoryBots::new());
    for (tok, uid) in [
        ("owner", OWNER),
        ("bob", BOB),
        ("carol", CAROL),
        ("mod", MOD),
    ] {
        bots.insert(
            tok,
            BotIdentity {
                user_id: i64::try_from(uid).unwrap(),
                application_id: 1,
            },
        );
    }
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;

    let repo = Arc::new(MemorySettingsRepo::default());
    let counter = Arc::new(AtomicU64::new(0x9e37_79b9_7f4a_7c15));
    let entropy: super::Entropy = Arc::new(move || {
        counter
            .fetch_add(0x1234_5678_9abc_def1, Ordering::SeqCst)
            .wrapping_mul(0xff51_afd7_ed55_8ccd)
    });
    let events = Arc::new(RecordingEvents::default());
    let mut deps = SettingsDeps::new(
        repo.clone(),
        Arc::new(backend.clone()),
        storage.clone(),
        dyn_clock.clone(),
        entropy.clone(),
        1,
        5,
    )
    .unwrap()
    .with_events(events.clone());
    if let Some(limits) = opts.limits {
        let ids = SnowflakeGenerator::new(dyn_clock.clone(), 1, 6).unwrap();
        deps = deps.with_service(
            SettingsService::new(
                super::DynRepo(repo.clone()),
                dyn_clock.clone(),
                ids,
                entropy,
            )
            .with_limits(limits),
        );
    }
    if let Some(p) = opts.presence {
        deps = deps.with_presence(p);
    }
    if let Some(t) = opts.timeouter {
        deps = deps.with_timeouter(t);
    }
    if opts.wired_joiner {
        deps = deps.with_joiner(Arc::new(InviteMemberJoiner::new(
            Arc::new(sr_chat::settings::memory::MemoryInviteJoin {
                chat: backend.clone(),
                settings: repo.clone(),
            }),
            Arc::new(backend.clone()),
            bus.clone(),
        )));
    }
    let deps = Arc::new(deps);

    let msgs = MessagesDeps::new(
        backend.clone(),
        Arc::new(backend.clone()),
        storage.clone(),
        bus.clone(),
        Arc::new(FixedEntitlements(1 << 20)),
        Arc::new(backend.clone()),
        None,
        dyn_clock.clone(),
        1,
        1,
    )
    .unwrap()
    .with_rich(crate::routes::messages::rich::RichDeps {
        store: Some(Arc::new(backend.clone())),
        ..Default::default()
    })
    .with_cdn_signer(sr_storage::cdn::CdnSigner::new(
        b"webhook-avatar-test-secret-0123456".to_vec(),
    ));
    let directory = Arc::new(sr_chat::settings::directory::MemoryDirectory::default());
    let apps = Arc::new(sr_apps::memory::MemoryAppsRepo::new());
    let app_ids = Arc::new(AtomicU64::new(900_000));
    let apps_svc = sr_apps::AppsService::new(
        apps.clone(),
        dyn_clock.clone(),
        Arc::new(move || i64::try_from(app_ids.fetch_add(1, Ordering::SeqCst)).unwrap()),
    );
    let dev = crate::routes::developer::DevDeps::new(
        apps_svc.clone(),
        Arc::new(backend.clone()),
        bus.clone(),
        Arc::new(NoPassword),
        dyn_clock.clone(),
        "https://side.test",
    );
    // The guild backend behind the alert buttons (kick, ban, timeout), writing audit rows into
    // the same settings store.
    let chat_ids = SnowflakeGenerator::new(dyn_clock.clone(), 3, 3).unwrap();
    let chat_svc = sr_chat::ChatService::new(
        backend.clone(),
        dyn_clock.clone(),
        chat_ids,
        sr_chat::ChatLimits::default(),
    );
    let audit_ids = SnowflakeGenerator::new(dyn_clock.clone(), 4, 4).unwrap();
    let audit_counter = Arc::new(AtomicU64::new(7));
    let audit_service = Arc::new(SettingsService::new(
        super::DynRepo(repo.clone()),
        dyn_clock.clone(),
        audit_ids,
        Arc::new(move || audit_counter.fetch_add(1, Ordering::SeqCst)),
    ));
    let guild_api = crate::routes::guilds::GuildApi(
        crate::routes::guilds::backend::backend_with_audit(chat_svc, bus.clone(), audit_service),
    );
    let mut builder = AppState::builder()
        .bots(bots)
        .sessions(sessions.clone())
        .clock(dyn_clock.clone())
        .settings(settings)
        .extension(deps.clone())
        .extension(Arc::new(AutoModGateHandle(deps.clone())))
        .extension(Arc::new(guild_api))
        .extension(Arc::new(crate::routes::media::MediaDeps::new(
            storage.clone(),
        )))
        .extension(Arc::new(msgs))
        .extension(Arc::new(crate::routes::cdn::CdnDeps::new(
            storage.clone(),
            sr_storage::cdn::CdnSigner::new(b"emoji-url-test-secret-0123456789".to_vec()),
        )))
        .extension(Arc::new(crate::routes::directory::DirectoryDeps::new(
            directory.clone(),
        )))
        .extension(Arc::new(dev));
    if let Some(v) = opts.voice {
        builder = builder.extension(Arc::new(crate::routes::voice::VoiceApi(v)));
    }
    if opts.features {
        actions.put_channel(sr_features::actions::ChannelInfo {
            id: ch,
            guild: Some(guild),
            kind: 0,
            name: "general".into(),
            nsfw: false,
            parent: None,
            rate_limit_per_user: 5,
            everyone_can_view: true,
        });
        let gf = features_tests::wire(&backend, dyn_clock.clone(), bus.clone(), actions.clone());
        builder = builder.extension(Arc::new(gf));
    }
    let state = builder.build();
    let app = build_router(
        state,
        router()
            .merge(messages::router())
            .merge(crate::routes::directory::router()),
    );
    Env {
        app,
        backend,
        repo,
        events,
        storage,
        deps,
        sessions,
        clock,
        bus,
        guild,
        ch,
        voice,
        directory,
        actions,
        apps,
        apps_svc,
    }
}

impl Env {
    async fn call(
        &self,
        method: Method,
        path: &str,
        who: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/v1{path}"));
        if let Some(who) = who {
            req = req.header("authorization", format!("Bot {who}"));
        }
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
        (parts.status, json)
    }

    async fn get(&self, path: &str, who: &str) -> (StatusCode, Value) {
        self.call(Method::GET, path, Some(who), None).await
    }
    async fn post(&self, path: &str, who: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::POST, path, Some(who), Some(body)).await
    }
    async fn patch(&self, path: &str, who: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::PATCH, path, Some(who), Some(body)).await
    }
    async fn put(&self, path: &str, who: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::PUT, path, Some(who), Some(body)).await
    }
    async fn delete(&self, path: &str, who: &str) -> (StatusCode, Value) {
        self.call(Method::DELETE, path, Some(who), None).await
    }

    /// `POST /invites/{code}` as a signed-in browser session of `user` (bots may
    /// not join by invite).
    async fn accept(&self, user: u64, code: &str) -> (StatusCode, Value) {
        use sr_auth::{
            repo::{SessionRecord, SessionRepository},
            session::SessionToken,
        };
        let t = SessionToken::generate().unwrap();
        let now = self.clock.now_unix_ms() / 1000;
        self.sessions
            .create(SessionRecord {
                id: i64::try_from(user).unwrap(),
                user_id: i64::try_from(user).unwrap(),
                token_hash: t.hash(),
                created_at: now - 10,
                last_seen_at: now,
                expires_at: now + 100_000,
                device_name: None,
            })
            .await
            .unwrap();
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/api/v1/invites/{code}"))
            .header("cookie", format!("sr_session={}", t.expose()))
            .header("x-sr-csrf", csrf::token_for(t.expose()))
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let (parts, body) = resp.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (parts.status, json)
    }

    fn g(&self, rest: &str) -> String {
        format!("/guilds/{}{rest}", self.guild)
    }

    fn invites_path(&self) -> String {
        format!("/channels/{}/invites", self.ch)
    }

    /// Audit action numbers, newest first.
    async fn audit_actions(&self) -> Vec<u64> {
        let (s, v) = self.get(&self.g("/audit-logs"), "owner").await;
        assert_eq!(s, StatusCode::OK, "{v}");
        v["audit_log_entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["action_type"].as_u64().unwrap())
            .collect()
    }

    fn revoke_everyone(&self, p: Permissions) {
        let mut d = self.backend.data();
        let role = d.roles.iter_mut().find(|r| r.is_everyone()).unwrap();
        role.permissions.remove(p);
    }

    fn mod_role(&self) -> String {
        let d = self.backend.data();
        d.roles
            .iter()
            .find(|r| r.name == "mods")
            .unwrap()
            .id
            .to_string()
    }

    fn check(&self, author: u64, text: &str) -> MessageCheck {
        MessageCheck {
            guild: self.guild,
            channel: self.ch,
            author: UserId::new(author),
            content: text.to_owned(),
            ..Default::default()
        }
    }
}

fn code_of(r: &(StatusCode, Value)) -> (StatusCode, Option<u64>) {
    (r.0, r.1["code"].as_u64())
}

fn png_uri(extra: usize) -> String {
    let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    v.extend(std::iter::repeat_n(0u8, extra));
    format!("data:image/png;base64,{}", BASE64.encode(&v))
}

fn gif_uri(frames: usize) -> String {
    let mut v = b"GIF89a".to_vec();
    for _ in 0..frames {
        v.extend_from_slice(&[0x21, 0xF9, 0, 0]);
    }
    format!("data:image/gif;base64,{}", BASE64.encode(&v))
}

// ---- cross-cutting ---------------------------------------------------------------

#[tokio::test]
async fn requires_authentication() {
    let e = env().await;
    let r = e.call(Method::GET, &e.g("/invites"), None, None).await;
    assert_eq!(r.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn non_members_and_unknown_guilds_are_refused() {
    let e = env().await;
    assert_eq!(
        code_of(&e.get(&e.g("/audit-logs"), "carol").await),
        (StatusCode::FORBIDDEN, Some(50001))
    );
    assert_eq!(
        code_of(&e.get("/guilds/123/audit-logs", "owner").await),
        (StatusCode::NOT_FOUND, Some(10004))
    );
    assert_eq!(
        e.get("/guilds/abc/audit-logs", "owner").await.0,
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn openapi_documents_every_settings_path() {
    let doc = serde_json::to_value(crate::routes::openapi()).unwrap();
    let paths = doc["paths"].as_object().unwrap();
    for p in [
        "/channels/{channel_id}/invites",
        "/invites/{code}",
        "/guilds/{guild_id}/invites",
        "/guilds/{guild_id}/audit-logs",
        "/guilds/{guild_id}/emojis",
        "/guilds/{guild_id}/stickers/{sticker_id}",
        "/guilds/{guild_id}/auto-moderation/rules",
        "/channels/{channel_id}/webhooks",
        "/webhooks/{webhook_id}/{token}",
        "/guilds/templates/{code}",
        "/guilds/{guild_id}/onboarding",
        "/guilds/{guild_id}/welcome-screen",
        "/guilds/{guild_id}/sr/community",
    ] {
        assert!(paths.contains_key(p), "missing {p}");
    }
}

// ---- invites ---------------------------------------------------------------------

#[tokio::test]
async fn invite_defaults_external_and_overrides() {
    let e = env().await;
    let path = e.invites_path();

    // Internal default: 24 hours, unlimited uses.
    let (s, inv) = e.post(&path, "bob", json!({})).await;
    assert_eq!(s, StatusCode::OK, "{inv}");
    assert_eq!(inv["max_age"], 86_400);
    assert_eq!(inv["max_uses"], 0);
    assert_eq!(inv["code"].as_str().unwrap().len(), 8);
    assert_eq!(inv["guild"]["name"], "Club");
    assert_eq!(inv["channel"]["name"], "general");
    assert_eq!(inv["inviter"]["id"], BOB.to_string());
    assert!(inv["expires_at"].is_string());

    // An empty body is allowed (Discord).
    let (s, _) = e.call(Method::POST, &path, Some("bob"), None).await;
    assert_eq!(s, StatusCode::OK);

    // "Copy link" (external) is single use (Q134)...
    let (_, ext) = e.post(&path, "bob", json!({"external": true})).await;
    assert_eq!(ext["max_uses"], 1);
    assert_eq!(ext["max_age"], 86_400);
    // ...unless the caller says otherwise.
    let (_, ov) = e
        .post(
            &path,
            "bob",
            json!({"external": true, "max_uses": 5, "max_age": 0}),
        )
        .await;
    assert_eq!(
        (ov["max_uses"].as_u64(), ov["max_age"].as_u64()),
        (Some(5), Some(0))
    );
    assert!(ov["expires_at"].is_null());

    // Out-of-range options are a form error.
    let r = e.post(&path, "bob", json!({"max_uses": 101})).await;
    assert_eq!(code_of(&r), (StatusCode::BAD_REQUEST, Some(50035)));
    assert!(r.1["errors"]["max_uses"].is_object());

    assert!(e.events.names().contains(&"INVITE_CREATE"));
}

#[tokio::test]
async fn invite_permissions_and_lifecycle() {
    let e = env().await;
    let path = e.invites_path();

    assert_eq!(
        code_of(&e.post(&path, "carol", json!({})).await),
        (StatusCode::FORBIDDEN, Some(50001))
    );
    e.revoke_everyone(Permissions::CREATE_INSTANT_INVITE);
    assert_eq!(
        code_of(&e.post(&path, "bob", json!({})).await),
        (StatusCode::FORBIDDEN, Some(50013))
    );

    let (_, inv) = e.post(&path, "owner", json!({})).await;
    let code = inv["code"].as_str().unwrap().to_owned();

    // Listing needs MANAGE_GUILD (guild) or MANAGE_CHANNELS (channel).
    assert_eq!(
        e.get(&e.g("/invites"), "bob").await.0,
        StatusCode::FORBIDDEN
    );
    let (s, list) = e.get(&e.g("/invites"), "mod").await;
    assert_eq!((s, list.as_array().unwrap().len()), (StatusCode::OK, 1));
    let (s, by_channel) = e.get(&path, "mod").await;
    assert_eq!(
        (s, by_channel.as_array().unwrap().len()),
        (StatusCode::OK, 1)
    );

    // Anyone signed in can resolve a code.
    let (s, got) = e.get(&format!("/invites/{code}"), "carol").await;
    assert_eq!(
        (s, got["code"].as_str()),
        (StatusCode::OK, Some(code.as_str()))
    );
    assert_eq!(
        code_of(&e.get("/invites/nope1234", "carol").await),
        (StatusCode::NOT_FOUND, Some(10006))
    );
    assert_eq!(
        code_of(&e.get("/invites/bad%20code", "carol").await),
        (StatusCode::NOT_FOUND, Some(10006))
    );

    // Revoking needs MANAGE_GUILD (or MANAGE_CHANNELS on the channel).
    assert_eq!(
        e.delete(&format!("/invites/{code}"), "bob").await.0,
        StatusCode::FORBIDDEN
    );
    let (s, gone) = e.delete(&format!("/invites/{code}"), "mod").await;
    assert_eq!(
        (s, gone["code"].as_str()),
        (StatusCode::OK, Some(code.as_str()))
    );
    assert_eq!(
        code_of(&e.get(&format!("/invites/{code}"), "owner").await),
        (StatusCode::NOT_FOUND, Some(10006))
    );
    assert!(e.events.names().contains(&"INVITE_DELETE"));
    // InviteDelete (42) after InviteCreate (40), newest first.
    assert_eq!(e.audit_actions().await, vec![42, 40]);
}

#[tokio::test]
async fn expired_invites_are_unknown_and_unlisted() {
    let e = env().await;
    let path = e.invites_path();
    let (_, short) = e.post(&path, "owner", json!({"max_age": 60})).await;
    let (_, forever) = e.post(&path, "owner", json!({"max_age": 0})).await;
    e.repo.with(|s| {
        for i in s.invites.iter_mut().filter(|i| i.max_age_secs == 60) {
            i.created_at_ms -= 61_000;
        }
    });
    assert_eq!(
        code_of(
            &e.get(
                &format!("/invites/{}", short["code"].as_str().unwrap()),
                "bob"
            )
            .await
        ),
        (StatusCode::NOT_FOUND, Some(10006))
    );
    let (_, list) = e.get(&e.g("/invites"), "owner").await;
    let codes: Vec<_> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["code"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(codes, vec![forever["code"].as_str().unwrap().to_owned()]);
}

async fn wired() -> Env {
    env_with(Opts {
        wired_joiner: true,
        ..Opts::default()
    })
    .await
}

async fn new_invite(e: &Env, body: Value) -> String {
    let (s, inv) = e.post(&e.invites_path(), "owner", body).await;
    assert_eq!(s, StatusCode::OK, "{inv}");
    inv["code"].as_str().unwrap().to_owned()
}

fn uses(e: &Env, code: &str) -> u32 {
    e.repo.with(|s| {
        s.invites
            .iter()
            .find(|i| i.code == code)
            .map(|i| i.uses)
            .unwrap()
    })
}

fn is_member(e: &Env, user: u64) -> bool {
    e.backend
        .data()
        .members
        .iter()
        .any(|m| m.guild_id == e.guild && m.user_id.get() == user)
}

#[tokio::test]
async fn accepting_without_a_wired_joiner_is_unavailable_and_keeps_the_use() {
    let e = env().await;
    let code = new_invite(&e, json!({"max_uses": 1})).await;
    let r = e.accept(CAROL, &code).await;
    assert_eq!(code_of(&r), (StatusCode::FORBIDDEN, Some(40006)));
    assert_eq!(uses(&e, &code), 0, "a refused accept must not consume");
}

#[tokio::test]
async fn bots_cannot_join_by_invite() {
    let e = wired().await;
    let code = new_invite(&e, json!({})).await;
    let r = e
        .post(&format!("/invites/{code}"), "carol", json!({}))
        .await;
    assert_eq!(r.0, StatusCode::FORBIDDEN, "{}", r.1);
    assert_eq!(r.1["code"], 20001);
    assert!(!is_member(&e, CAROL));
    assert_eq!(uses(&e, &code), 0);
}

#[tokio::test]
async fn accepting_adds_the_member_consumes_one_use_and_emits_member_joined() {
    use futures::StreamExt;
    use sr_events::{DomainEvent, EventBus, Topic};
    let e = wired().await;
    let code = new_invite(&e, json!({"max_uses": 5})).await;
    let mut guild_events = e.bus.subscribe(Topic::Guild(e.guild));

    let (s, inv) = e.accept(CAROL, &code).await;
    assert_eq!(s, StatusCode::OK, "{inv}");
    assert_eq!(inv["uses"], 1);
    assert_eq!(inv["guild"]["id"], e.guild.to_string());
    assert_eq!(inv["channel"]["id"], e.ch.to_string());
    assert!(is_member(&e, CAROL));
    assert_eq!(uses(&e, &code), 1);
    // `@everyone` is implicit: no explicit role assignments.
    assert!(
        e.backend
            .data()
            .members
            .iter()
            .find(|m| m.user_id.get() == CAROL)
            .unwrap()
            .role_ids
            .is_empty()
    );
    // A normal join writes no audit entry (only INVITE_CREATE from the setup).
    assert_eq!(e.audit_actions().await, vec![40]);

    let ev = tokio::time::timeout(std::time::Duration::from_secs(1), guild_events.next())
        .await
        .unwrap()
        .unwrap();
    let DomainEvent::MemberJoined {
        guild,
        user,
        payload,
    } = ev
    else {
        panic!("expected MemberJoined, got {ev:?}");
    };
    assert_eq!((guild, user.get()), (e.guild, CAROL));
    assert_eq!(payload["guild_id"], e.guild.to_string());
    assert_eq!(payload["user"]["id"], CAROL.to_string());
    assert_eq!(payload["roles"], json!([]));
    assert_eq!(payload["pending"], false);
    assert!(payload["joined_at"].is_string());
}

#[tokio::test]
async fn accepting_when_already_a_member_is_idempotent() {
    let e = wired().await;
    let code = new_invite(&e, json!({"max_uses": 3})).await;
    // Existing members get the invite back, unused.
    let (s, inv) = e.accept(BOB, &code).await;
    assert_eq!((s, inv["uses"].as_u64()), (StatusCode::OK, Some(0)));
    assert_eq!(uses(&e, &code), 0);
    // Joining twice consumes one use and adds one membership row.
    assert_eq!(e.accept(CAROL, &code).await.0, StatusCode::OK);
    let (s, inv) = e.accept(CAROL, &code).await;
    assert_eq!((s, inv["uses"].as_u64()), (StatusCode::OK, Some(1)));
    assert_eq!(uses(&e, &code), 1);
    assert_eq!(
        e.backend
            .data()
            .members
            .iter()
            .filter(|m| m.user_id.get() == CAROL)
            .count(),
        1
    );
}

#[tokio::test]
async fn max_uses_are_enforced() {
    let e = wired().await;
    let code = new_invite(&e, json!({"max_uses": 1})).await;
    assert_eq!(e.accept(CAROL, &code).await.0, StatusCode::OK);
    // A used-up invite is unknown to the next user, and nobody is added.
    let r = e.accept(5, &code).await;
    assert_eq!(code_of(&r), (StatusCode::NOT_FOUND, Some(10006)));
    assert!(!is_member(&e, 5));
    assert_eq!(uses(&e, &code), 1);
    assert_eq!(
        code_of(&e.get(&format!("/invites/{code}"), "owner").await),
        (StatusCode::NOT_FOUND, Some(10006)),
        "used up"
    );
}

#[tokio::test]
async fn expired_and_revoked_invites_are_refused() {
    let e = wired().await;
    let timed = new_invite(&e, json!({"max_age": 60})).await;
    let revoked = new_invite(&e, json!({})).await;
    assert_eq!(
        e.delete(&format!("/invites/{revoked}"), "owner").await.0,
        StatusCode::OK
    );
    assert_eq!(
        code_of(&e.accept(CAROL, &revoked).await),
        (StatusCode::NOT_FOUND, Some(10006))
    );
    e.clock.advance_ms(61_000);
    assert_eq!(
        code_of(&e.accept(CAROL, &timed).await),
        (StatusCode::NOT_FOUND, Some(10006))
    );
    assert_eq!(
        code_of(&e.accept(CAROL, "nope1234").await),
        (StatusCode::NOT_FOUND, Some(10006))
    );
    assert!(!is_member(&e, CAROL));
    assert_eq!(uses(&e, &timed), 0);
}

#[tokio::test]
async fn paused_invites_and_bans_refuse_without_consuming() {
    let e = wired().await;
    let code = new_invite(&e, json!({"max_uses": 1})).await;

    e.repo.with(|s| {
        s.bans.insert((e.guild, UserId::new(CAROL)));
    });
    assert_eq!(
        code_of(&e.accept(CAROL, &code).await),
        (StatusCode::FORBIDDEN, Some(40007))
    );
    e.repo.with(|s| s.bans.clear());

    let (s, _) = e
        .patch(
            &e.g("/sr/community"),
            "owner",
            json!({"invites_paused": true}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(e.accept(CAROL, &code).await.0, StatusCode::FORBIDDEN);
    assert!(!is_member(&e, CAROL));
    assert_eq!(uses(&e, &code), 0, "refusals keep the use");

    e.patch(
        &e.g("/sr/community"),
        "owner",
        json!({"invites_paused": false}),
    )
    .await;
    let (s, joined) = e.accept(CAROL, &code).await;
    assert_eq!((s, joined["uses"].as_u64()), (StatusCode::OK, Some(1)));
    assert!(is_member(&e, CAROL));
}

// ---- audit log -------------------------------------------------------------------

#[tokio::test]
async fn audit_log_filters_pages_and_checks_permission() {
    let e = env().await;
    let path = e.invites_path();
    e.post(&path, "owner", json!({})).await; // 40 by owner
    e.post(&path, "bob", json!({})).await; // 40 by bob
    let (_, emoji) = e
        .post(
            &e.g("/emojis"),
            "owner",
            json!({"name": "party", "image": png_uri(4)}),
        )
        .await;
    assert!(emoji["id"].is_string());

    // VIEW_AUDIT_LOG is required.
    assert_eq!(
        code_of(&e.get(&e.g("/audit-logs"), "bob").await),
        (StatusCode::FORBIDDEN, Some(50013))
    );
    let (s, all) = e.get(&e.g("/audit-logs"), "mod").await;
    assert_eq!(s, StatusCode::OK, "{all}");
    let entries = all["audit_log_entries"].as_array().unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0]["action_type"], 60); // newest first
    assert_eq!(entries[0]["changes"][0]["key"], "name");
    assert_eq!(entries[0]["changes"][0]["new_value"], "party");
    assert!(entries[0]["changes"][0].get("old_value").is_none());
    for key in [
        "users",
        "webhooks",
        "integrations",
        "threads",
        "guild_scheduled_events",
        "auto_moderation_rules",
    ] {
        assert!(all[key].is_array(), "{key}");
    }

    // Filter by actor and action.
    let by_bob = e
        .get(&e.g(&format!("/audit-logs?user_id={BOB}")), "mod")
        .await
        .1;
    assert_eq!(by_bob["audit_log_entries"].as_array().unwrap().len(), 1);
    let by_action = e.get(&e.g("/audit-logs?action_type=60"), "mod").await.1;
    assert_eq!(by_action["audit_log_entries"].as_array().unwrap().len(), 1);

    // Page with `before` and `limit`.
    let first = e.get(&e.g("/audit-logs?limit=1"), "mod").await.1;
    let cursor = first["audit_log_entries"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let next = e
        .get(&e.g(&format!("/audit-logs?limit=1&before={cursor}")), "mod")
        .await
        .1;
    assert_eq!(next["audit_log_entries"].as_array().unwrap().len(), 1);
    assert_ne!(next["audit_log_entries"][0]["id"], cursor);

    // Bad filters are form errors.
    for q in [
        "limit=0",
        "limit=101",
        "action_type=9999",
        "user_id=x",
        "before=x",
    ] {
        assert_eq!(
            code_of(&e.get(&e.g(&format!("/audit-logs?{q}")), "mod").await),
            (StatusCode::BAD_REQUEST, Some(50035)),
            "{q}"
        );
    }
}

// ---- emoji -----------------------------------------------------------------------

#[tokio::test]
async fn emoji_crud_roles_and_storage() {
    let e = env().await;
    let base = e.g("/emojis");

    let (s, emoji) = e
        .post(&base, "mod", json!({"name": "party", "image": png_uri(8)}))
        .await;
    assert_eq!(s, StatusCode::CREATED, "{emoji}");
    assert_eq!(emoji["name"], "party");
    assert_eq!(emoji["animated"], false);
    assert_eq!(emoji["available"], true);
    assert_eq!(emoji["user"]["id"], MOD.to_string());
    let id = emoji["id"].as_str().unwrap().to_owned();
    assert_eq!(e.storage.list("emoji/").await.unwrap().len(), 1);

    // An animated GIF is tracked separately.
    let (_, gif) = e
        .post(&base, "mod", json!({"name": "spin", "image": gif_uri(3)}))
        .await;
    assert_eq!(gif["animated"], true);

    // Members read; outsiders and unprivileged members cannot write.
    let (s, list) = e.get(&base, "bob").await;
    assert_eq!((s, list.as_array().unwrap().len()), (StatusCode::OK, 2));
    assert_eq!(e.get(&base, "carol").await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        code_of(
            &e.post(&base, "bob", json!({"name": "nope", "image": png_uri(1)}))
                .await
        ),
        (StatusCode::FORBIDDEN, Some(50013))
    );
    assert_eq!(
        e.get(&format!("{base}/{id}"), "bob").await.1["name"],
        "party"
    );

    // Bad name, bad image, unknown role.
    for body in [
        json!({"name": "a", "image": png_uri(1)}),
        json!({"name": "ok_name", "image": "http://x/y.png"}),
        json!({"name": "ok_name", "image": "data:image/svg+xml;base64,PHN2Zy8+"}),
        json!({"name": "ok_name", "image": png_uri(1), "roles": ["123"]}),
    ] {
        assert_eq!(
            code_of(&e.post(&base, "mod", body.clone()).await),
            (StatusCode::BAD_REQUEST, Some(50035)),
            "{body}"
        );
    }

    // Rename and restrict to a role.
    let role = e.mod_role();
    let (s, patched) = e
        .patch(
            &format!("{base}/{id}"),
            "mod",
            json!({"name": "party2", "roles": [role]}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{patched}");
    assert_eq!(patched["name"], "party2");
    assert_eq!(patched["roles"], json!([role]));
    let (_, cleared) = e
        .patch(&format!("{base}/{id}"), "mod", json!({"roles": []}))
        .await;
    assert_eq!(cleared["roles"], json!([]));

    // Delete removes the row and the stored image.
    assert_eq!(
        e.delete(&format!("{base}/{id}"), "mod").await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        code_of(&e.delete(&format!("{base}/{id}"), "mod").await),
        (StatusCode::NOT_FOUND, Some(10014))
    );
    assert_eq!(e.storage.list("emoji/").await.unwrap().len(), 1);

    assert!(e.events.names().contains(&"GUILD_EMOJIS_UPDATE"));
    assert_eq!(e.audit_actions().await[..3], [62, 61, 61]);
}

#[tokio::test]
async fn emoji_slots_and_size_limits() {
    let e = env_with(Opts {
        limits: Some(ExpressionLimits {
            max_static_emoji: 2,
            max_animated_emoji: 1,
            max_emoji_bytes: 64,
            ..ExpressionLimits::default()
        }),
        ..Opts::default()
    })
    .await;
    let base = e.g("/emojis");
    for name in ["one", "two"] {
        let (s, v) = e
            .post(&base, "mod", json!({"name": name, "image": png_uri(4)}))
            .await;
        assert_eq!(s, StatusCode::CREATED, "{v}");
    }
    // Static slots are full (30008); animated slots are separate.
    assert_eq!(
        code_of(
            &e.post(&base, "mod", json!({"name": "three", "image": png_uri(4)}))
                .await
        ),
        (StatusCode::BAD_REQUEST, Some(30008))
    );
    assert_eq!(
        e.post(&base, "mod", json!({"name": "spin", "image": gif_uri(2)}))
            .await
            .0,
        StatusCode::CREATED
    );
    assert_eq!(
        code_of(
            &e.post(&base, "mod", json!({"name": "spin2", "image": gif_uri(2)}))
                .await
        ),
        (StatusCode::BAD_REQUEST, Some(30008))
    );
    // Oversized image.
    let r = e
        .post(&base, "mod", json!({"name": "big", "image": png_uri(500)}))
        .await;
    assert_eq!(code_of(&r), (StatusCode::BAD_REQUEST, Some(50035)));
    assert!(r.1["errors"]["image"].is_object());
    // A full slot refused the upload before anything reached storage.
    assert_eq!(e.storage.list("emoji/").await.unwrap().len(), 3);
}

// ---- stickers --------------------------------------------------------------------

#[tokio::test]
async fn sticker_crud_formats_and_limit() {
    let e = env_with(Opts {
        limits: Some(ExpressionLimits {
            max_stickers: 2,
            ..ExpressionLimits::default()
        }),
        ..Opts::default()
    })
    .await;
    let base = e.g("/stickers");
    let (s, st) = e
        .post(
            &base,
            "mod",
            json!({"name": "flame", "tags": "fire", "description": "hot", "file": png_uri(8)}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{st}");
    assert_eq!(
        (st["type"].as_u64(), st["format_type"].as_u64()),
        (Some(2), Some(1))
    );
    assert_eq!(st["guild_id"], e.guild.to_string());
    let id = st["id"].as_str().unwrap().to_owned();

    let (_, gif) = e
        .post(
            &base,
            "mod",
            json!({"name": "dance", "tags": "dance", "file": gif_uri(2)}),
        )
        .await;
    assert_eq!(gif["format_type"], 4);
    let lottie = format!(
        "data:application/json;base64,{}",
        BASE64.encode(br#"{"v":"5.7.0","layers":[]}"#)
    );
    // Slots are full after two.
    assert_eq!(
        code_of(
            &e.post(
                &base,
                "mod",
                json!({"name": "wave", "tags": "wave", "file": lottie})
            )
            .await
        ),
        (StatusCode::BAD_REQUEST, Some(30039))
    );
    assert_eq!(e.storage.list("sticker/").await.unwrap().len(), 2);

    // Validation and permissions.
    for body in [
        json!({"name": "x", "tags": "t", "file": png_uri(1)}),
        json!({"name": "okay", "tags": "", "file": png_uri(1)}),
        json!({"name": "okay", "tags": "t", "file": "data:image/png;base64,AAAA"}),
    ] {
        assert_eq!(
            code_of(&e.post(&base, "mod", body.clone()).await).0,
            StatusCode::BAD_REQUEST,
            "{body}"
        );
    }
    assert_eq!(
        e.post(
            &base,
            "bob",
            json!({"name": "okay", "tags": "t", "file": png_uri(1)})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    // Edit and read back; clearing the description with null.
    let (s, edited) = e
        .patch(
            &format!("{base}/{id}"),
            "mod",
            json!({"name": "flame2", "description": null, "tags": "fire,hot"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{edited}");
    assert_eq!(edited["name"], "flame2");
    assert!(edited["description"].is_null());
    assert_eq!(edited["tags"], "fire,hot");
    assert_eq!(
        e.get(&format!("{base}/{id}"), "bob").await.1["name"],
        "flame2"
    );
    assert_eq!(e.get(&base, "bob").await.1.as_array().unwrap().len(), 2);

    assert_eq!(
        e.delete(&format!("{base}/{id}"), "mod").await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.get(&format!("{base}/{id}"), "bob").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(e.storage.list("sticker/").await.unwrap().len(), 1);
    assert!(e.events.names().contains(&"GUILD_STICKERS_UPDATE"));
    let actions = e.audit_actions().await;
    assert!(actions.contains(&90) && actions.contains(&91) && actions.contains(&92));
}

// ---- AutoMod ---------------------------------------------------------------------

fn keyword_rule(name: &str, words: &[&str]) -> Value {
    json!({
        "name": name,
        "event_type": 1,
        "trigger_type": 1,
        "trigger_metadata": {"keyword_filter": words},
        "actions": [{"type": 1, "metadata": {"custom_message": "no"}}],
        "enabled": true,
    })
}

#[tokio::test]
async fn automod_rule_crud_validation_and_quota() {
    let e = env().await;
    let base = e.g("/auto-moderation/rules");

    let (s, rule) = e
        .post(&base, "owner", keyword_rule("words", &["heck*"]))
        .await;
    assert_eq!(s, StatusCode::CREATED, "{rule}");
    assert_eq!(rule["trigger_type"], 1);
    assert_eq!(rule["event_type"], 1);
    assert_eq!(rule["trigger_metadata"]["keyword_filter"], json!(["heck*"]));
    assert_eq!(rule["actions"][0]["type"], 1);
    assert_eq!(rule["actions"][0]["metadata"]["custom_message"], "no");
    assert_eq!(rule["creator_id"], OWNER.to_string());
    let id = rule["id"].as_str().unwrap().to_owned();

    // MANAGE_GUILD only.
    assert_eq!(e.get(&base, "bob").await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        e.post(&base, "bob", keyword_rule("x", &["a"])).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(e.get(&base, "mod").await.1.as_array().unwrap().len(), 1);
    assert_eq!(
        e.get(&format!("{base}/{id}"), "mod").await.0,
        StatusCode::OK
    );

    // Invalid regexes, missing fields, unknown types and foreign channels.
    let mut bad_regex = keyword_rule("re", &[]);
    bad_regex["trigger_metadata"] = json!({"regex_patterns": ["("]});
    let mut alert = keyword_rule("alert", &["a"]);
    alert["actions"] = json!([{"type": 2, "metadata": {"channel_id": "999"}}]);
    let mut timeout = json!({"name": "t", "event_type": 1, "trigger_type": 3,
        "actions": [{"type": 3, "metadata": {"duration_seconds": 60}}]});
    timeout["enabled"] = json!(true);
    for (body, label) in [
        (bad_regex, "regex"),
        (json!({"name": "n"}), "missing"),
        (
            json!({"name": "n", "event_type": 1, "trigger_type": 77, "actions": []}),
            "type",
        ),
        (alert, "foreign channel"),
        (timeout, "timeout on spam"),
    ] {
        assert_eq!(
            code_of(&e.post(&base, "owner", body).await),
            (StatusCode::BAD_REQUEST, Some(50035)),
            "{label}"
        );
    }

    // Six keyword rules per guild.
    for i in 0..5 {
        let (s, v) = e
            .post(&base, "owner", keyword_rule(&format!("r{i}"), &["zzz"]))
            .await;
        assert_eq!(s, StatusCode::CREATED, "{v}");
    }
    assert_eq!(
        code_of(&e.post(&base, "owner", keyword_rule("7th", &["zzz"])).await).0,
        StatusCode::BAD_REQUEST
    );

    // Modify: rename, disable, new keywords; the trigger type cannot change.
    let (s, patched) = e
        .patch(
            &format!("{base}/{id}"),
            "owner",
            json!({"name": "renamed", "enabled": false,
                   "trigger_metadata": {"keyword_filter": ["darn"]},
                   "exempt_channels": [e.ch.to_string()]}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{patched}");
    assert_eq!(patched["name"], "renamed");
    assert_eq!(patched["enabled"], false);
    assert_eq!(
        patched["trigger_metadata"]["keyword_filter"],
        json!(["darn"])
    );
    assert_eq!(patched["exempt_channels"], json!([e.ch.to_string()]));
    assert_eq!(
        code_of(
            &e.patch(&format!("{base}/{id}"), "owner", json!({"trigger_type": 3}))
                .await
        )
        .0,
        StatusCode::BAD_REQUEST
    );

    assert_eq!(
        e.delete(&format!("{base}/{id}"), "owner").await.0,
        StatusCode::NO_CONTENT
    );
    assert!(
        e.get(&format!("{base}/{id}"), "owner")
            .await
            .0
            .is_client_error()
    );
    assert!(
        e.delete(&format!("{base}/{id}"), "owner")
            .await
            .0
            .is_client_error()
    );

    let names = e.events.names();
    for n in [
        "AUTO_MODERATION_RULE_CREATE",
        "AUTO_MODERATION_RULE_UPDATE",
        "AUTO_MODERATION_RULE_DELETE",
    ] {
        assert!(names.contains(&n), "{n}");
    }
    let actions = e.audit_actions().await;
    assert!(actions.contains(&140) && actions.contains(&141) && actions.contains(&142));
}

#[tokio::test]
async fn automod_gate_blocks_folds_text_and_respects_exemptions() {
    let e = env().await;
    let base = e.g("/auto-moderation/rules");
    let (_, rule) = e
        .post(&base, "owner", keyword_rule("words", &["heck*"]))
        .await;
    let rule_id = rule["id"].as_str().unwrap().to_owned();

    // A plain hit blocks with the custom message.
    let v = e
        .deps
        .evaluate(&e.check(BOB, "oh heckin no"))
        .await
        .unwrap();
    let block = v.block.expect("blocked");
    assert_eq!(block.custom_message.as_deref(), Some("no"));
    assert_eq!(block.rule_id.to_string(), rule_id);

    // Open question 136: fullwidth, Cyrillic look-alikes and zero-width splits.
    for evasion in ["ＨＥＣＫing", "hеck", "he\u{200B}ck now"] {
        let v = e.deps.evaluate(&e.check(BOB, evasion)).await.unwrap();
        assert!(v.block.is_some(), "{evasion}");
    }
    assert!(
        e.deps
            .evaluate(&e.check(BOB, "hello there"))
            .await
            .unwrap()
            .is_clean()
    );

    // Administrators and server managers are never filtered.
    assert!(
        e.deps
            .evaluate(&e.check(OWNER, "heckin"))
            .await
            .unwrap()
            .is_clean()
    );
    assert!(
        e.deps
            .evaluate(&e.check(MOD, "heckin"))
            .await
            .unwrap()
            .is_clean()
    );

    // The block was audited (143) and announced.
    assert!(e.audit_actions().await.contains(&143));
    assert!(
        e.events
            .names()
            .contains(&"AUTO_MODERATION_ACTION_EXECUTION")
    );

    // Exempt channels, then disabling.
    let (_, _) = e
        .patch(
            &format!("{base}/{rule_id}"),
            "owner",
            json!({"exempt_channels": [e.ch.to_string()]}),
        )
        .await;
    assert!(
        e.deps
            .evaluate(&e.check(BOB, "heckin"))
            .await
            .unwrap()
            .is_clean()
    );
    e.patch(
        &format!("{base}/{rule_id}"),
        "owner",
        json!({"exempt_channels": [], "enabled": false}),
    )
    .await;
    assert!(
        e.deps
            .evaluate(&e.check(BOB, "heckin"))
            .await
            .unwrap()
            .is_clean()
    );
    e.patch(
        &format!("{base}/{rule_id}"),
        "owner",
        json!({"enabled": true}),
    )
    .await;
    assert!(
        e.deps
            .evaluate(&e.check(BOB, "heckin"))
            .await
            .unwrap()
            .block
            .is_some()
    );

    // Deleting the rule takes effect at once (the compiled cache is invalidated).
    e.delete(&format!("{base}/{rule_id}"), "owner").await;
    assert!(
        e.deps
            .evaluate(&e.check(BOB, "heckin"))
            .await
            .unwrap()
            .is_clean()
    );
}

#[tokio::test]
async fn automod_spam_window_alerts_and_timeouts() {
    let e = env().await;
    let base = e.g("/auto-moderation/rules");
    let (s, spam) = e
        .post(
            &base,
            "owner",
            json!({"name": "spam", "event_type": 1, "trigger_type": 3, "enabled": true,
                   "actions": [{"type": 2, "metadata": {"channel_id": e.voice.to_string()}}]}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{spam}");
    let (s, mention) = e
        .post(
            &base,
            "owner",
            json!({"name": "mentions", "event_type": 1, "trigger_type": 5, "enabled": true,
                   "trigger_metadata": {"mention_total_limit": 2},
                   "actions": [{"type": 3, "metadata": {"duration_seconds": 600}}]}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{mention}");

    // The fourth identical message from one member trips the window.
    for _ in 0..3 {
        assert!(
            e.deps
                .evaluate(&e.check(BOB, "buy my stuff"))
                .await
                .unwrap()
                .is_clean()
        );
    }
    let v = e
        .deps
        .evaluate(&e.check(BOB, "buy  MY stuff"))
        .await
        .unwrap();
    assert_eq!(v.matches[0].matched, "duplicate messages");
    assert_eq!(v.alerts.len(), 1);
    assert_eq!(v.alerts[0].channel_id, e.voice);
    assert_eq!(v.alerts[0].user_id.get(), BOB);
    assert!(v.block.is_none());
    // A non-member cannot be evaluated at all.
    assert!(
        e.deps
            .evaluate(&e.check(CAROL, "buy my stuff"))
            .await
            .is_err()
    );

    // Mention spam times the author out.
    let mut m = e.check(BOB, "hey all");
    m.mentioned_user_ids = (10..13).map(UserId::new).collect();
    let v = e.deps.evaluate(&m).await.unwrap();
    assert_eq!(v.timeout_secs, Some(600));
}

// ---- webhooks --------------------------------------------------------------------

#[tokio::test]
async fn webhook_lifecycle_token_hash_and_execute() {
    let e = env().await;
    let base = format!("/channels/{}/webhooks", e.ch);

    assert_eq!(
        code_of(&e.post(&base, "bob", json!({"name": "Deploys"})).await),
        (StatusCode::FORBIDDEN, Some(50013))
    );
    let (s, wh) = e.post(&base, "mod", json!({"name": "Deploys"})).await;
    assert_eq!(s, StatusCode::OK, "{wh}");
    let id = wh["id"].as_str().unwrap().to_owned();
    let token = wh["token"].as_str().unwrap().to_owned();
    assert_eq!(token.len(), 68);
    assert_eq!(wh["type"], 1);
    assert_eq!(wh["url"], format!("/api/v1/webhooks/{id}/{token}"));

    // Only the hash is stored (open question 137).
    let stored = e
        .repo
        .get_webhook(id.parse().unwrap())
        .await
        .unwrap()
        .unwrap();
    let hash = stored.token_hash.unwrap();
    assert_ne!(hash, token);
    assert_eq!(hash, sr_chat::settings::webhooks::hash_token(&token));

    // Listings never show the token.
    let (_, list) = e.get(&e.g("/webhooks"), "mod").await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert!(list[0].get("token").is_none());
    assert_eq!(
        e.get(&e.g("/webhooks"), "bob").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(e.get(&base, "mod").await.1.as_array().unwrap().len(), 1);
    assert_eq!(
        e.get(&format!("/webhooks/{id}"), "mod").await.1["name"],
        "Deploys"
    );
    assert_eq!(
        e.get(&format!("/webhooks/{id}"), "bob").await.0,
        StatusCode::FORBIDDEN
    );

    // Execution needs no session, only the token.
    let exec = format!("/webhooks/{id}/{token}");
    let (s, _) = e
        .call(
            Method::POST,
            &exec,
            None,
            Some(json!({"content": "build ok"})),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, msg) = e
        .call(
            Method::POST,
            &format!("{exec}?wait=true"),
            None,
            Some(json!({"content": "deployed", "embeds": [{"title": "v2"}]})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{msg}");
    assert_eq!(msg["content"], "deployed");
    assert_eq!(msg["channel_id"], e.ch.to_string());
    let (_, history) = e.get(&format!("/channels/{}/messages", e.ch), "bob").await;
    assert_eq!(history.as_array().unwrap().len(), 2);

    // Bad token, unknown id, empty payload, threads.
    assert_eq!(
        code_of(
            &e.call(
                Method::POST,
                &format!("/webhooks/{id}/{}", "x".repeat(68)),
                None,
                Some(json!({"content": "hi"}))
            )
            .await
        ),
        (StatusCode::NOT_FOUND, Some(10015))
    );
    assert_eq!(
        e.call(
            Method::POST,
            &format!("/webhooks/1/{token}"),
            None,
            Some(json!({"content": "hi"}))
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.call(Method::POST, &exec, None, Some(json!({}))).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        e.call(
            Method::POST,
            &format!("{exec}?thread_id=5"),
            None,
            Some(json!({"content": "hi"}))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );

    // Token-authenticated read and rename.
    let (s, got) = e.call(Method::GET, &exec, None, None).await;
    assert_eq!((s, got["name"].as_str()), (StatusCode::OK, Some("Deploys")));
    assert!(got.get("token").is_none() && got.get("user").is_none());
    let (s, renamed) = e
        .call(Method::PATCH, &exec, None, Some(json!({"name": "CI"})))
        .await;
    assert_eq!((s, renamed["name"].as_str()), (StatusCode::OK, Some("CI")));

    // Moving needs the management endpoint and a channel of the same guild.
    let (s, moved) = e
        .patch(
            &format!("/webhooks/{id}"),
            "mod",
            json!({"channel_id": e.voice.to_string()}),
        )
        .await;
    assert_eq!(
        (s, moved["channel_id"].as_str()),
        (StatusCode::OK, Some(e.voice.to_string().as_str())),
        "{moved}"
    );
    assert_eq!(
        e.patch(
            &format!("/webhooks/{id}"),
            "mod",
            json!({"channel_id": "999"})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );

    // Deleting by token ends execution.
    assert_eq!(
        e.call(Method::DELETE, &exec, None, None).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.call(Method::POST, &exec, None, Some(json!({"content": "late"})))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert!(e.events.names().contains(&"WEBHOOKS_UPDATE"));
    assert_eq!(e.audit_actions().await[..3], [52, 51, 51]);
}

fn square_png(w: u32, h: u32) -> String {
    let img = image::RgbaImage::from_fn(w, h, |x, y| {
        image::Rgba([(x % 255) as u8, (y % 255) as u8, 9, 255])
    });
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    format!(
        "data:image/png;base64,{}",
        data_encoding::BASE64.encode(&out)
    )
}

#[tokio::test]
async fn webhook_avatar_is_stored_replaced_and_removed() {
    let e = env().await;
    let base = format!("/channels/{}/webhooks", e.ch);

    // A refused request (no permission, bad name, bad image) leaves nothing behind.
    assert_eq!(
        code_of(
            &e.post(
                &base,
                "bob",
                json!({"name": "A", "avatar": square_png(40, 20)})
            )
            .await
        ),
        (StatusCode::FORBIDDEN, Some(50013))
    );
    let (s, _) = e
        .post(&base, "mod", json!({"name": "A", "avatar": "not an image"}))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = e
        .post(
            &base,
            "mod",
            json!({"name": "", "avatar": square_png(40, 20)}),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(e.storage.list("avatars").await.unwrap().is_empty());

    // Create with an avatar: cropped to a square, keyed by the webhook id.
    let (s, wh) = e
        .post(
            &base,
            "mod",
            json!({"name": "Deploys", "avatar": square_png(300, 200)}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{wh}");
    let id = wh["id"].as_str().unwrap().to_owned();
    let h1 = wh["avatar"].as_str().unwrap().to_owned();
    assert!(sr_storage::media::is_valid_hash(&h1));
    let key1 = format!("avatars/{id}/{h1}");
    let stored = e.storage.get(&key1).await.unwrap();
    let dims = image::ImageReader::new(std::io::Cursor::new(&stored))
        .with_guessed_format()
        .unwrap()
        .into_dimensions()
        .unwrap();
    assert_eq!(dims, (200, 200));
    let (_, list) = e.get(&e.g("/webhooks"), "mod").await;
    assert_eq!(list[0]["avatar"], h1.as_str());

    // A name-only edit keeps the avatar.
    let (_, v) = e
        .patch(
            &format!("/webhooks/{id}"),
            "mod",
            json!({"name": "Deploys 2"}),
        )
        .await;
    assert_eq!(v["avatar"], h1.as_str());

    // Replace: the old object goes away.
    let (s, v) = e
        .patch(
            &format!("/webhooks/{id}"),
            "mod",
            json!({"avatar": square_png(64, 64)}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let h2 = v["avatar"].as_str().unwrap().to_owned();
    assert_ne!(h1, h2);
    assert!(e.storage.head(&key1).await.is_err());
    assert!(e.storage.head(&format!("avatars/{id}/{h2}")).await.is_ok());

    // Remove: null clears it.
    let (s, v) = e
        .patch(&format!("/webhooks/{id}"), "mod", json!({"avatar": null}))
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert!(v["avatar"].is_null());
    assert!(e.storage.head(&format!("avatars/{id}/{h2}")).await.is_err());

    // Deleting the webhook deletes its image too.
    let (_, v) = e
        .patch(
            &format!("/webhooks/{id}"),
            "mod",
            json!({"avatar": square_png(32, 32)}),
        )
        .await;
    let h3 = v["avatar"].as_str().unwrap().to_owned();
    assert_eq!(
        e.delete(&format!("/webhooks/{id}"), "mod").await.0,
        StatusCode::NO_CONTENT
    );
    assert!(e.storage.head(&format!("avatars/{id}/{h3}")).await.is_err());
}

#[tokio::test]
async fn webhooks_are_capped_per_channel_and_deletable_by_managers() {
    let e = env().await;
    let base = format!("/channels/{}/webhooks", e.ch);
    let mut first = String::new();
    for i in 0..15 {
        let (s, wh) = e
            .post(&base, "mod", json!({"name": format!("hook {i}")}))
            .await;
        assert_eq!(s, StatusCode::OK, "{wh}");
        if i == 0 {
            first = wh["id"].as_str().unwrap().to_owned();
        }
    }
    assert_eq!(
        code_of(&e.post(&base, "mod", json!({"name": "16th"})).await).0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        e.post(&base, "mod", json!({"name": "@everyone"})).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        e.delete(&format!("/webhooks/{first}"), "bob").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.delete(&format!("/webhooks/{first}"), "mod").await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        code_of(&e.get(&format!("/webhooks/{first}"), "mod").await),
        (StatusCode::NOT_FOUND, Some(10015))
    );
    assert_eq!(
        e.post(&base, "mod", json!({"name": "again"})).await.0,
        StatusCode::OK
    );
}

// ---- templates -------------------------------------------------------------------

#[tokio::test]
async fn template_create_preview_sync_and_delete() {
    let e = env().await;
    let base = e.g("/templates");

    assert_eq!(
        e.post(&base, "bob", json!({"name": "Starter"})).await.0,
        StatusCode::FORBIDDEN
    );
    let (s, t) = e
        .post(
            &base,
            "mod",
            json!({"name": "Starter", "description": "A start"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{t}");
    let code = t["code"].as_str().unwrap().to_owned();
    assert_eq!(code.len(), 16);
    assert_eq!(t["is_dirty"], false);
    assert_eq!(t["source_guild_id"], e.guild.to_string());
    assert_eq!(
        t["serialized_source_guild"]["channels"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert!(
        t["serialized_source_guild"]["roles"]
            .as_array()
            .unwrap()
            .len()
            >= 2
    );
    assert!(t["serialized_source_guild"]["settings"]["system_channel_local_id"].is_u64());

    // One template per guild.
    assert_eq!(
        code_of(&e.post(&base, "mod", json!({"name": "Second"})).await),
        (StatusCode::BAD_REQUEST, Some(50035))
    );

    // Anyone signed in can preview by code, without the dirty flag.
    let (s, preview) = e.get(&format!("/guilds/templates/{code}"), "carol").await;
    assert_eq!(
        (s, preview["name"].as_str()),
        (StatusCode::OK, Some("Starter"))
    );
    assert!(preview["is_dirty"].is_null());
    assert!(
        e.get("/guilds/templates/nope", "carol")
            .await
            .0
            .is_client_error()
    );

    // Changing the guild makes it dirty until synced.
    e.backend
        .data()
        .guilds
        .get_mut(&e.guild)
        .unwrap()
        .preferred_locale = "de".into();
    let (_, list) = e.get(&base, "mod").await;
    assert_eq!(list[0]["is_dirty"], true);
    let (s, synced) = e.put(&format!("{base}/{code}"), "mod", json!({})).await;
    assert_eq!(
        (s, synced["is_dirty"].clone()),
        (StatusCode::OK, json!(false))
    );
    assert_eq!(
        e.get(&format!("/guilds/templates/{code}"), "carol").await.1["serialized_source_guild"]["settings"]
            ["preferred_locale"],
        "de"
    );

    let (s, edited) = e
        .patch(
            &format!("{base}/{code}"),
            "mod",
            json!({"name": "Renamed", "description": null}),
        )
        .await;
    assert_eq!(
        (s, edited["name"].as_str()),
        (StatusCode::OK, Some("Renamed"))
    );
    assert!(edited["description"].is_null());
    assert_eq!(
        e.patch(&format!("{base}/{code}"), "mod", json!({"name": ""}))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );

    assert_eq!(
        e.delete(&format!("{base}/{code}"), "bob").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.delete(&format!("{base}/{code}"), "mod").await.0,
        StatusCode::OK
    );
    assert!(
        e.get(&format!("/guilds/templates/{code}"), "carol")
            .await
            .0
            .is_client_error()
    );
    assert_eq!(e.get(&base, "mod").await.1, json!([]));
}

// ---- community -------------------------------------------------------------------

#[tokio::test]
async fn community_settings_and_safety_checks() {
    let e = env().await;
    let url = e.g("/sr/community");
    assert_eq!(e.get(&url, "bob").await.0, StatusCode::FORBIDDEN);
    let (s, c) = e.get(&url, "mod").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(c["community_enabled"], false);
    assert_eq!(c["preferred_locale"], "en-US");
    assert_eq!(c["invites_paused"], false);

    let (s, c) = e
        .patch(
            &url,
            "mod",
            json!({"description": "Our club", "age_restricted": true,
                   "raid_alert_joins_per_minute": 30, "preferred_locale": "de"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{c}");
    assert_eq!(c["description"], "Our club");
    assert_eq!(c["age_restricted"], true);
    assert_eq!(c["raid_alert_joins_per_minute"], 30);
    assert_eq!(e.audit_actions().await[0], 1); // GuildUpdate

    // Enabling needs both channels, belonging to the guild...
    let general = e.ch.to_string();
    for body in [
        json!({"community_enabled": true}),
        json!({"community_enabled": true, "rules_channel_id": general}),
        json!({"rules_channel_id": "999"}),
    ] {
        assert_eq!(
            code_of(&e.patch(&url, "mod", body.clone()).await),
            (StatusCode::BAD_REQUEST, Some(50035)),
            "{body}"
        );
    }
    // ...and the verification and content-filter levels.
    let both = json!({"community_enabled": true, "rules_channel_id": general,
                      "public_updates_channel_id": e.voice.to_string()});
    assert_eq!(
        code_of(&e.patch(&url, "mod", both.clone()).await).0,
        StatusCode::BAD_REQUEST
    );
    {
        let mut d = e.backend.data();
        let g = d.guilds.get_mut(&e.guild).unwrap();
        g.verification_level = 1;
        g.explicit_content_filter = 2;
    }
    let (s, c) = e.patch(&url, "mod", both).await;
    assert_eq!(s, StatusCode::OK, "{c}");
    assert_eq!(c["community_enabled"], true);
    assert_eq!(c["rules_channel_id"], general);

    // Turning it off also turns off the dependent features.
    e.patch(
        &url,
        "mod",
        json!({"onboarding_enabled": true, "guide_enabled": true}),
    )
    .await;
    let (_, c) = e
        .patch(&url, "mod", json!({"community_enabled": false}))
        .await;
    assert_eq!(
        (
            c["community_enabled"].clone(),
            c["onboarding_enabled"].clone(),
            c["guide_enabled"].clone()
        ),
        (json!(false), json!(false), json!(false))
    );
}

#[tokio::test]
async fn onboarding_and_welcome_screen() {
    let e = env().await;
    let url = e.g("/onboarding");
    let role = e.mod_role();

    // Disabled onboarding may carry questions; ids are assigned.
    let (s, o) = e
        .put(
            &url,
            "mod",
            json!({"enabled": false, "default_channel_ids": [],
                   "prompts": [{"title": "Interests", "single_select": false, "required": true,
                                "options": [{"title": "Manga", "channel_ids": [e.ch.to_string()],
                                             "role_ids": [role]}]}]}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{o}");
    assert!(o["prompts"][0]["id"].is_string());
    assert!(o["prompts"][0]["options"][0]["id"].is_string());
    assert_eq!(o["prompts"][0]["in_onboarding"], true);
    let (s, got) = e.get(&url, "bob").await;
    assert_eq!(
        (s, got["prompts"].as_array().unwrap().len()),
        (StatusCode::OK, 1)
    );
    assert_eq!(e.get(&url, "carol").await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        e.put(&url, "bob", json!({"enabled": false})).await.0,
        StatusCode::FORBIDDEN
    );

    // Enabling needs 7 default channels (5 of them open to everyone); a guild
    // with four channels can never satisfy it.
    assert_eq!(
        code_of(
            &e.put(
                &url,
                "mod",
                json!({"enabled": true, "default_channel_ids": [e.ch.to_string()]})
            )
            .await
        ),
        (StatusCode::BAD_REQUEST, Some(50035))
    );
    // Unknown role, foreign channel.
    assert_eq!(
        e.put(
            &url,
            "mod",
            json!({"prompts": [{"title": "q", "options": [{"title": "a", "role_ids": ["5"]}]}]})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        e.put(&url, "mod", json!({"default_channel_ids": ["999"]}))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );

    // Welcome screen: hidden from members until enabled.
    let ws = e.g("/welcome-screen");
    assert_eq!(e.get(&ws, "bob").await.0, StatusCode::FORBIDDEN);
    assert_eq!(e.get(&ws, "mod").await.0, StatusCode::OK);
    let card = |ch: &str| json!({"channel_id": ch, "description": "Say hi", "emoji_name": "wave"});
    let six: Vec<_> = (0..6).map(|_| card(&e.ch.to_string())).collect();
    assert_eq!(
        e.patch(&ws, "mod", json!({"welcome_channels": six}))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        e.patch(&ws, "mod", json!({"welcome_channels": [card("999")]}))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let (s, w) = e
        .patch(
            &ws,
            "mod",
            json!({"enabled": true, "description": "Welcome!", "welcome_channels": [card(&e.ch.to_string())]}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{w}");
    assert_eq!(w["welcome_channels"][0]["emoji_name"], "wave");
    let (s, seen) = e.get(&ws, "bob").await;
    assert_eq!(
        (s, seen["description"].as_str()),
        (StatusCode::OK, Some("Welcome!"))
    );
    assert_eq!(e.get(&ws, "carol").await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn community_changes_publish_a_full_guild_update() {
    let e = env().await;
    let (s, _) = e
        .patch(
            &e.g("/sr/community"),
            "mod",
            json!({"description": "Our club", "preferred_locale": "de"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let events = e.events.0.lock().unwrap();
    let Some(super::events::SettingsEvent::CommunityChanged { guild, payload }) = events
        .iter()
        .rev()
        .find(|ev| ev.name() == "GUILD_UPDATE")
        .cloned()
    else {
        panic!("no GUILD_UPDATE published");
    };
    assert_eq!(guild, e.guild);
    assert_eq!(payload["id"], e.guild.to_string());
    assert_eq!(payload["description"], "Our club");
    assert_eq!(payload["preferred_locale"], "de");
    assert!(payload["roles"].is_array());
    assert_eq!(payload["features"], json!([]));
}

#[tokio::test]
async fn welcome_screen_change_publishes_a_guild_update_with_its_feature_flag() {
    let e = env().await;
    let (s, _) = e
        .patch(
            &e.g("/welcome-screen"),
            "mod",
            json!({"enabled": true, "description": "Welcome!"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let events = e.events.0.lock().unwrap();
    let Some(super::events::SettingsEvent::CommunityChanged { payload, .. }) = events
        .iter()
        .rev()
        .find(|ev| ev.name() == "GUILD_UPDATE")
        .cloned()
    else {
        panic!("no GUILD_UPDATE published");
    };
    assert_eq!(payload["features"], json!(["WELCOME_SCREEN_ENABLED"]));
}

// ---- AutoMod timeout action -------------------------------------------------------

#[derive(Default)]
struct RecordingTimeouter(std::sync::Mutex<Vec<(GuildId, UserId, i64)>>);

#[async_trait::async_trait]
impl super::MemberTimeouter for RecordingTimeouter {
    async fn timeout_member(
        &self,
        guild: GuildId,
        user: UserId,
        until_ms: i64,
    ) -> Result<Option<i64>, sr_core::ApiError> {
        self.0.lock().unwrap().push((guild, user, until_ms));
        Ok(Some(until_ms))
    }
}

#[tokio::test]
async fn blocked_message_still_times_the_author_out() {
    let rec = Arc::new(RecordingTimeouter::default());
    let e = env_with(Opts {
        timeouter: Some(rec.clone()),
        ..Opts::default()
    })
    .await;
    let (s, rule) = e
        .post(
            &e.g("/auto-moderation/rules"),
            "owner",
            json!({"name": "slurs", "event_type": 1, "trigger_type": 1, "enabled": true,
                   "trigger_metadata": {"keyword_filter": ["badword"]},
                   "actions": [{"type": 1}, {"type": 3, "metadata": {"duration_seconds": 60}}]}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{rule}");

    let r = e
        .post(
            &format!("/channels/{}/messages", e.ch),
            "bob",
            json!({"content": "a badword"}),
        )
        .await;
    assert_eq!(
        code_of(&r),
        (StatusCode::BAD_REQUEST, Some(200_000)),
        "{}",
        r.1
    );
    assert_eq!(
        *rec.0.lock().unwrap(),
        vec![(e.guild, UserId::new(BOB), T0 + 60_000)]
    );
    // A clean message neither blocks nor times anyone out.
    let (s, _) = e
        .post(
            &format!("/channels/{}/messages", e.ch),
            "bob",
            json!({"content": "hello"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(rec.0.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn admin_timeouter_sets_the_deadline_and_announces_the_member() {
    use futures::StreamExt;
    use sr_chat::memory::MemoryRepo;
    use sr_chat::repo::GuildRepository;
    use sr_events::{DomainEvent, EventBus, Topic};

    let repo = Arc::new(MemoryRepo::default());
    let ids = SnowflakeGenerator::new(Arc::new(ManualClock::new(T0)), 9, 8).unwrap();
    let bp = plan_new_guild(UserId::new(OWNER), "Club", T0, || ids.generate()).unwrap();
    let guild = bp.guild.id;
    repo.insert_guild(&bp).await.unwrap();
    repo.store.lock().unwrap().members.push(Member {
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
    let bus = Arc::new(InProcessBus::new());
    let mut events = bus.subscribe(Topic::Guild(guild));
    let t = super::AdminMemberTimeouter::new(repo.clone(), bus.clone());
    use super::MemberTimeouter;

    let bob = UserId::new(BOB);
    assert_eq!(
        t.timeout_member(guild, bob, T0 + 5_000).await.unwrap(),
        Some(T0 + 5_000)
    );
    let ev = tokio::time::timeout(std::time::Duration::from_secs(2), events.next())
        .await
        .unwrap()
        .unwrap();
    let DomainEvent::MemberUpdated { user, payload, .. } = ev else {
        panic!("expected MemberUpdated, got {ev:?}");
    };
    assert_eq!(user, bob);
    assert_eq!(payload["guild_id"], guild.to_string());
    assert!(payload["communication_disabled_until"].is_string());
    // A shorter timeout never shortens a running one; non-members are ignored.
    assert_eq!(
        t.timeout_member(guild, bob, T0 + 1_000).await.unwrap(),
        None
    );
    assert_eq!(
        t.timeout_member(guild, UserId::new(CAROL), T0 + 1_000)
            .await
            .unwrap(),
        None
    );
    let stored = repo.get_member(guild, bob).await.unwrap().unwrap();
    assert_eq!(stored.communication_disabled_until_ms, Some(T0 + 5_000));
}

// ---- invite counts ----------------------------------------------------------------

struct OnlineUsers(Vec<u64>);

impl super::PresenceCounter for OnlineUsers {
    fn count_online(&self, users: &[UserId]) -> u64 {
        users.iter().filter(|u| self.0.contains(&u.get())).count() as u64
    }
}

#[tokio::test]
async fn invite_counts_only_with_with_counts() {
    let e = env_with(Opts {
        presence: Some(Arc::new(OnlineUsers(vec![BOB, CAROL]))),
        ..Opts::default()
    })
    .await;
    let (_, inv) = e.post(&e.invites_path(), "bob", json!({})).await;
    let code = inv["code"].as_str().unwrap();

    let (s, plain) = e.get(&format!("/invites/{code}"), "carol").await;
    assert_eq!(s, StatusCode::OK, "{plain}");
    assert!(plain.get("approximate_member_count").is_none());
    assert!(plain.get("approximate_presence_count").is_none());
    assert!(plain["expires_at"].is_string());

    let (s, counted) = e
        .get(&format!("/invites/{code}?with_counts=true"), "carol")
        .await;
    assert_eq!(s, StatusCode::OK, "{counted}");
    // Owner, bob and mod are members; carol is not, and does not count.
    assert_eq!(counted["approximate_member_count"], 3);
    assert_eq!(counted["approximate_presence_count"], 1);
}

#[tokio::test]
async fn invite_counts_omit_presence_without_a_presence_source() {
    let e = env().await;
    let (_, inv) = e.post(&e.invites_path(), "bob", json!({})).await;
    let code = inv["code"].as_str().unwrap();
    let (s, counted) = e
        .get(&format!("/invites/{code}?with_counts=true"), "carol")
        .await;
    assert_eq!(s, StatusCode::OK, "{counted}");
    assert_eq!(counted["approximate_member_count"], 3);
    assert!(counted.get("approximate_presence_count").is_none());
}

// ---- signed emoji and sticker URLs ----------------------------------------------

#[tokio::test]
async fn emoji_url_endpoint_mints_signed_variants() {
    let e = env().await;
    let (_, emoji) = e
        .post(
            &e.g("/emojis"),
            "mod",
            json!({"name": "party", "image": png_uri(8)}),
        )
        .await;
    let id = emoji["id"].as_str().unwrap().to_owned();
    let url = format!("{}/{id}/url", e.g("/emojis"));
    let signer = sr_storage::cdn::CdnSigner::new(b"emoji-url-test-secret-0123456789".to_vec());
    let now = u64::try_from(T0 / 1000).unwrap();

    // Plain: a signed URL for the stored object.
    let (s, v) = e.get(&url, "bob").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let u = v["url"].as_str().unwrap();
    let (path, query) = u.split_once('?').unwrap();
    // The object key uses its own id, not the emoji id.
    assert!(
        path.starts_with(&format!("/cdn/emoji/{}/", e.guild)) && path.ends_with(".png"),
        "{path}"
    );
    assert_eq!(signer.verify_query(path, query, now), Ok(()));
    assert!(v["expires_at"].as_u64().unwrap() > now);

    // With size and format: both are covered by the signature.
    let (s, v) = e.get(&format!("{url}?size=64&format=WebP"), "bob").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (path, query) = v["url"].as_str().unwrap().split_once('?').unwrap();
    assert!(query.starts_with("size=64&format=webp&ex="), "{query}");
    assert_eq!(signer.verify_query(path, query, now), Ok(()));
    assert!(
        signer
            .verify_query(path, &query.replace("size=64", "size=128"), now)
            .is_err()
    );

    // Bad parameters and access.
    for bad in ["?size=100", "?size=8192", "?format=avif"] {
        assert_eq!(
            e.get(&format!("{url}{bad}"), "bob").await.0,
            StatusCode::BAD_REQUEST,
            "{bad}"
        );
    }
    assert_eq!(e.get(&url, "carol").await.0, StatusCode::FORBIDDEN);
    let missing = format!("{}/1/url", e.g("/emojis"));
    assert_eq!(e.get(&missing, "bob").await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn webhook_token_can_be_reset_but_never_read_back() {
    let e = env().await;
    let base = format!("/channels/{}/webhooks", e.ch);
    let (_, wh) = e.post(&base, "mod", json!({"name": "Deploys"})).await;
    let id = wh["id"].as_str().unwrap().to_owned();
    let old = wh["token"].as_str().unwrap().to_owned();

    // No read path ever returns the token or url again.
    for (_, v) in [
        e.get(&format!("/webhooks/{id}"), "mod").await,
        e.get(&e.g("/webhooks"), "mod").await,
    ] {
        let one = if v.is_array() { v[0].clone() } else { v };
        assert!(
            one.get("token").is_none() && one.get("url").is_none(),
            "{one}"
        );
    }

    // Managers only; unknown webhooks are 404.
    let reset = format!("/webhooks/{id}/reset-token");
    assert_eq!(
        e.post(&reset, "bob", json!({})).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.post("/webhooks/12345/reset-token", "mod", json!({}))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (s, fresh) = e.post(&reset, "mod", json!({})).await;
    assert_eq!(s, StatusCode::OK, "{fresh}");
    let new = fresh["token"].as_str().unwrap().to_owned();
    assert_eq!(new.len(), 68);
    assert_ne!(new, old);
    assert_eq!(fresh["url"], format!("/api/v1/webhooks/{id}/{new}"));
    assert_eq!(fresh["name"], "Deploys");

    // Only the new token's hash is stored, and the old URL stops working at once.
    let stored = e
        .repo
        .get_webhook(id.parse().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.token_hash.unwrap(),
        sr_chat::settings::webhooks::hash_token(&new)
    );
    let run = |t: String| {
        let (e, path) = (&e, format!("/webhooks/{id}/{t}"));
        async move {
            e.call(Method::POST, &path, None, Some(json!({"content": "hi"})))
                .await
        }
    };
    assert_eq!(run(old.clone()).await.0, StatusCode::NOT_FOUND);
    assert_eq!(run(new.clone()).await.0, StatusCode::NO_CONTENT);
    // The audit log says the token changed, without the value.
    let (_, log) = e.get(&e.g("/audit-logs"), "owner").await;
    let text = log.to_string();
    assert!(!text.contains(&old) && !text.contains(&new));
    assert!(e.audit_actions().await.contains(&51));
}

// ---- phase 10: join notice, announcement following and publishing ------------------

fn add_channel(e: &Env, id: u64, kind: sr_chat::ChannelType, name: &str, flags: u32) -> ChannelId {
    let cid = ChannelId::new(id);
    let mut c = sr_chat::Channel::new(cid, Some(e.guild), kind);
    c.name = Some(name.to_owned());
    c.flags = flags;
    e.backend.data().channels.insert(cid, c);
    cid
}

async fn say(e: &Env, channel: ChannelId, who: &str, text: &str) -> Value {
    let (s, m) = e
        .post(
            &format!("/channels/{channel}/messages"),
            who,
            json!({ "content": text }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{m}");
    m
}

async fn history(e: &Env, channel: ChannelId) -> Vec<Value> {
    let (s, v) = e
        .get(&format!("/channels/{channel}/messages?limit=50"), "owner")
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    v.as_array().unwrap().clone()
}

#[tokio::test]
async fn joining_posts_the_notice_in_the_system_channel_once() {
    let e = wired().await;
    let code = new_invite(&e, json!({})).await;
    assert_eq!(e.accept(CAROL, &code).await.0, StatusCode::OK);
    let notices = |h: &[Value]| h.iter().filter(|m| m["type"] == 7).count();
    let h = history(&e, e.ch).await;
    let m = h.iter().find(|m| m["type"] == 7).expect("join notice");
    assert_eq!(m["author"]["id"], CAROL.to_string());
    assert_eq!(notices(&h), 1);
    // Joining again does not announce again, and other channels stay quiet.
    assert_eq!(e.accept(CAROL, &code).await.0, StatusCode::OK);
    assert_eq!(notices(&history(&e, e.ch).await), 1);
    assert_eq!(notices(&history(&e, e.voice).await), 0);
}

#[tokio::test]
async fn following_an_announcement_channel_and_publishing_to_followers() {
    use futures::StreamExt;
    use sr_events::{DomainEvent, EventBus, Topic};
    let e = env().await;
    let news = add_channel(&e, 7_001, sr_chat::ChannelType::Announcement, "news", 0);
    let follow = format!("/channels/{news}/followers");
    let target = json!({ "webhook_channel_id": e.ch.to_string() });

    // Only announcement channels can be followed, into text channels, by managers.
    let (s, v) = e
        .post(
            &format!("/channels/{}/followers", e.ch),
            "owner",
            target.clone(),
        )
        .await;
    assert_eq!(
        (s, v["code"].as_u64()),
        (StatusCode::BAD_REQUEST, Some(50024)),
        "{v}"
    );
    let (s, _) = e.post(&follow, "bob", target.clone()).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "bob lacks MANAGE_WEBHOOKS");
    let (s, v) = e
        .post(
            &follow,
            "owner",
            json!({ "webhook_channel_id": e.voice.to_string() }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{v}");
    let (s, v) = e
        .post(
            &follow,
            "owner",
            json!({ "webhook_channel_id": news.to_string() }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "no following yourself: {v}");

    let (s, f) = e.post(&follow, "mod", target.clone()).await;
    assert_eq!(s, StatusCode::OK, "{f}");
    assert_eq!(f["channel_id"], news.to_string());
    let webhook = f["webhook_id"].as_str().unwrap().to_owned();
    // Following twice is the same subscription, and it shows up as a follower webhook.
    let (_, again) = e.post(&follow, "owner", target.clone()).await;
    assert_eq!(again["webhook_id"], webhook);
    let (_, hooks) = e
        .get(&format!("/channels/{}/webhooks", e.ch), "owner")
        .await;
    let hook = hooks
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["id"] == webhook)
        .expect("follower webhook");
    assert_eq!(hook["type"], 2);
    assert!(hook["name"].as_str().unwrap().contains("#news"));

    // Publishing: the owner posts, then publishes by hand.
    let mut general = e.bus.subscribe(Topic::Channel(e.ch));
    let post = say(&e, news, "owner", "Server maintenance tonight").await;
    let src = post["id"].as_str().unwrap().to_owned();
    assert_eq!(post["flags"].as_u64().unwrap() & 1, 0);
    let (s, done) = e
        .post(
            &format!("/channels/{news}/messages/{src}/crosspost"),
            "owner",
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{done}");
    assert_eq!(done["flags"].as_u64().unwrap() & 1, 1, "CROSSPOSTED");
    let copies: Vec<Value> = history(&e, e.ch)
        .await
        .into_iter()
        .filter(|m| m["flags"].as_u64().unwrap_or(0) & 2 != 0)
        .collect();
    assert_eq!(copies.len(), 1, "one copy per follower");
    assert_eq!(copies[0]["content"], "Server maintenance tonight");
    assert_eq!(copies[0]["author"]["id"], OWNER.to_string());
    assert_eq!(copies[0]["message_reference"]["message_id"], src);
    assert_eq!(
        copies[0]["message_reference"]["channel_id"],
        news.to_string()
    );
    assert_eq!(copies[0]["mention_everyone"], false);
    let ev = tokio::time::timeout(std::time::Duration::from_secs(1), general.next())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(ev, DomainEvent::MessageSent { channel, .. } if channel == e.ch));

    // Once only; text channels have nothing to publish; others' messages need MANAGE_MESSAGES.
    let (s, v) = e
        .post(
            &format!("/channels/{news}/messages/{src}/crosspost"),
            "owner",
            json!({}),
        )
        .await;
    assert_eq!(
        (s, v["code"].as_u64()),
        (StatusCode::BAD_REQUEST, Some(40033)),
        "{v}"
    );
    let plain = say(&e, e.ch, "owner", "just chat").await;
    let (s, v) = e
        .post(
            &format!(
                "/channels/{}/messages/{}/crosspost",
                e.ch,
                plain["id"].as_str().unwrap()
            ),
            "owner",
            json!({}),
        )
        .await;
    assert_eq!(
        (s, v["code"].as_u64()),
        (StatusCode::BAD_REQUEST, Some(50024)),
        "{v}"
    );
    let second = say(&e, news, "owner", "second").await;
    let second = second["id"].as_str().unwrap().to_owned();
    let (s, _) = e
        .post(
            &format!("/channels/{news}/messages/{second}/crosspost"),
            "bob",
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let mine = say(&e, news, "bob", "bob wrote this").await;
    let (s, _) = e
        .post(
            &format!(
                "/channels/{news}/messages/{}/crosspost",
                mine["id"].as_str().unwrap()
            ),
            "bob",
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "authors may publish their own");

    // Unfollowing is deleting the follower webhook.
    let (s, _) = e.delete(&format!("/webhooks/{webhook}"), "owner").await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let before = history(&e, e.ch).await.len();
    let (s, _) = e
        .post(
            &format!("/channels/{news}/messages/{second}/crosspost"),
            "owner",
            json!({}),
        )
        .await;
    assert_eq!(
        s,
        StatusCode::OK,
        "publishing without followers still marks the message"
    );
    assert_eq!(history(&e, e.ch).await.len(), before, "nothing was copied");
}

#[tokio::test]
async fn auto_publish_sends_every_message_to_followers() {
    let e = env().await;
    let news = add_channel(
        &e,
        7_002,
        sr_chat::ChannelType::Announcement,
        "auto-news",
        sr_chat::threads::channel_flags::AUTO_PUBLISH,
    );
    let quiet = add_channel(&e, 7_003, sr_chat::ChannelType::Announcement, "quiet", 0);
    for source in [news, quiet] {
        let (s, v) = e
            .post(
                &format!("/channels/{source}/followers"),
                "owner",
                json!({ "webhook_channel_id": e.ch.to_string() }),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{v}");
    }
    let sent = say(&e, news, "owner", "auto announcement").await;
    assert_eq!(
        sent["flags"].as_u64().unwrap() & 1,
        0,
        "the response is the message as it was sent"
    );
    let h = history(&e, e.ch).await;
    assert_eq!(
        h.iter()
            .filter(|m| m["content"] == "auto announcement")
            .count(),
        1,
        "published as it was sent"
    );
    let (_, src) = e
        .get(
            &format!("/channels/{news}/messages/{}", sent["id"].as_str().unwrap()),
            "owner",
        )
        .await;
    assert_eq!(src["flags"].as_u64().unwrap() & 1, 1, "marked as published");
    // A channel without the flag publishes on request only.
    say(&e, quiet, "owner", "manual only").await;
    assert!(
        history(&e, e.ch)
            .await
            .iter()
            .all(|m| m["content"] != "manual only")
    );
}

#[tokio::test]
async fn server_tags_are_managed_by_managers_and_adopted_by_members() {
    let e = env().await;
    let url = e.g("/tag");
    let me = "/users/@me/primary-guild";

    // No tag yet.
    assert_eq!(e.get(&url, "bob").await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        e.put(me, "bob", json!({"guild_id": e.guild.to_string()}))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    // Only managers set it, and the text is validated.
    let body = json!({"tag": "SIDE", "badge": 3, "color": 0xAA00FF});
    assert_eq!(
        e.put(&url, "bob", body.clone()).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.put(&url, "mod", json!({"tag": "toolong"})).await.0,
        StatusCode::BAD_REQUEST
    );
    let (s, t) = e.put(&url, "mod", body).await;
    assert_eq!(s, StatusCode::OK, "{t}");
    assert_eq!(t["tag"], "SIDE");
    assert_eq!(t["members_can_display"], true);
    assert_eq!(e.get(&url, "bob").await.1["badge"], 3);
    // Non-members cannot read the tag or adopt it.
    assert_eq!(e.get(&url, "carol").await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        e.put(me, "carol", json!({"guild_id": e.guild.to_string()}))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );

    // A member adopts it.
    let (s, p) = e
        .put(me, "bob", json!({"guild_id": e.guild.to_string()}))
        .await;
    assert_eq!(s, StatusCode::OK, "{p}");
    assert_eq!(p["primary_guild"]["tag"], "SIDE");
    assert_eq!(p["primary_guild"]["identity_enabled"], true);
    assert_eq!(
        p["primary_guild"]["identity_guild_id"],
        e.guild.to_string().as_str()
    );
    assert_eq!(e.get(me, "bob").await.1["primary_guild"]["tag"], "SIDE");

    // Turning "members can display" off hides it and blocks new adoption.
    e.put(
        &url,
        "mod",
        json!({"tag": "SIDE", "members_can_display": false}),
    )
    .await;
    assert!(e.get(me, "bob").await.1["primary_guild"].is_null());

    // Removing the tag is audited; adoption is cleared with null.
    assert_eq!(e.delete(&url, "mod").await.0, StatusCode::NO_CONTENT);
    assert_eq!(e.get(&url, "bob").await.0, StatusCode::NOT_FOUND);
    let (s, p) = e.put(me, "bob", json!({"guild_id": null})).await;
    assert_eq!(s, StatusCode::OK);
    assert!(p["primary_guild"].is_null());
    assert!(e.audit_actions().await.contains(&1));
}

#[tokio::test]
async fn insights_need_a_manager_or_the_insights_permission() {
    let e = env().await;
    let url = e.g("/sr/insights?days=7");
    assert_eq!(e.get(&url, "bob").await.0, StatusCode::FORBIDDEN);
    assert_eq!(e.get(&url, "carol").await.0, StatusCode::FORBIDDEN);
    let (s, body) = e.get(&url, "mod").await;
    assert_eq!(s, StatusCode::OK, "{body}");
    // Nothing computed yet.
    assert_eq!(body["days"], json!([]));
    assert_eq!(body["top_channels"], json!([]));
}

#[tokio::test]
async fn server_guide_is_edited_by_managers_and_read_by_members_while_enabled() {
    let e = env().await;
    let url = e.g("/sr/guide");
    let body = json!({
        "welcome_message": "Welcome aboard",
        "todos": [{"title": "Say hi", "channel_id": e.ch.to_string()}],
        "resources": [{"title": "Rules", "content": "Be kind."}],
    });
    assert_eq!(
        e.put(&url, "bob", body.clone()).await.0,
        StatusCode::FORBIDDEN
    );
    let (s, g) = e
        .put(
            &url,
            "mod",
            json!({"todos": [{"title": "x", "channel_id": "123"}]}),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{g}");
    let (s, g) = e.put(&url, "mod", body).await;
    assert_eq!(s, StatusCode::OK, "{g}");
    assert!(g["todos"][0]["id"].is_string());
    assert_eq!(g["resources"][0]["content"], "Be kind.");
    assert_eq!(g["enabled"], false);

    // Hidden from members until the guide is switched on.
    assert_eq!(e.get(&url, "bob").await.0, StatusCode::FORBIDDEN);
    assert_eq!(e.get(&url, "mod").await.0, StatusCode::OK);
    let (s, _) = e
        .patch(&e.g("/sr/community"), "mod", json!({"guide_enabled": true}))
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, g) = e.get(&url, "bob").await;
    assert_eq!(s, StatusCode::OK, "{g}");
    assert_eq!(g["welcome_message"], "Welcome aboard");
    assert_eq!(g["enabled"], true);
    assert_eq!(e.get(&url, "carol").await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn membership_applications_flow() {
    let e = env().await;
    let form = e.g("/sr/applications/form");
    let apps = e.g("/sr/applications");

    // Closed: outsiders cannot see the form, cannot apply.
    assert_eq!(e.get(&form, "carol").await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        e.post(&apps, "carol", json!({"answers": []})).await.0,
        StatusCode::BAD_REQUEST
    );
    // Only managers set it up.
    let body = json!({"enabled": true, "description": "Tell us about you",
        "questions": [{"label": "Why join?", "kind": "paragraph", "required": true},
                      {"label": "Handle", "kind": "short"}]});
    assert_eq!(
        e.put(&form, "bob", body.clone()).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.put(
            &form,
            "mod",
            json!({"questions": [{"label": "x", "kind": "huge"}]})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let (s, f) = e.put(&form, "mod", body).await;
    assert_eq!(s, StatusCode::OK, "{f}");
    let q1 = f["questions"][0]["id"].as_str().unwrap().to_owned();
    assert_eq!(f["questions"][1]["kind"], "short");

    // Open: an outsider reads the form and applies; members cannot.
    assert_eq!(
        e.get(&form, "carol").await.1["description"],
        "Tell us about you"
    );
    assert_eq!(
        e.post(
            &apps,
            "bob",
            json!({"answers": [{"question_id": q1, "value": "x"}]})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        e.post(&apps, "carol", json!({"answers": []})).await.0,
        StatusCode::BAD_REQUEST,
        "a required question is unanswered"
    );
    let (s, a) = e
        .post(
            &apps,
            "carol",
            json!({"answers": [{"question_id": q1, "value": "I like manga"}]}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{a}");
    assert_eq!(a["status"], "pending");
    let app_id = a["id"].as_str().unwrap().to_owned();
    // One at a time.
    assert_eq!(
        e.post(
            &apps,
            "carol",
            json!({"answers": [{"question_id": q1, "value": "again"}]})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        e.get(&format!("{apps}/@me"), "carol").await.1["id"],
        app_id.as_str()
    );

    // The queue is for managers.
    assert_eq!(e.get(&apps, "bob").await.0, StatusCode::FORBIDDEN);
    let (s, q) = e.get(&format!("{apps}?status=pending"), "mod").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(q.as_array().unwrap().len(), 1);
    assert_eq!(q[0]["answers"][0]["value"], "I like manga");
    assert_eq!(
        e.get(&format!("{apps}?status=bogus"), "mod").await.0,
        StatusCode::BAD_REQUEST
    );

    // Reject, then apply again and get approved; a decided application stays decided.
    let decision = format!("{apps}/{app_id}/decision");
    assert_eq!(
        e.post(&decision, "bob", json!({"approve": false})).await.0,
        StatusCode::FORBIDDEN
    );
    let (s, r) = e
        .post(
            &decision,
            "mod",
            json!({"approve": false, "reason": "not now"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["status"], "rejected");
    assert_eq!(r["rejection_reason"], "not now");
    assert_eq!(
        e.post(&decision, "mod", json!({"approve": true})).await.0,
        StatusCode::NOT_FOUND
    );
    let (_, a2) = e
        .post(
            &apps,
            "carol",
            json!({"answers": [{"question_id": q1, "value": "second try"}]}),
        )
        .await;
    let (s, ok) = e
        .post(
            &format!("{apps}/{}/decision", a2["id"].as_str().unwrap()),
            "mod",
            json!({"approve": true}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{ok}");
    assert_eq!(ok["status"], "approved");
    assert_eq!(ok["reviewed_by"], MOD.to_string().as_str());
    // G-33: both decisions are in the guild audit log (action 200) with status and application id.
    let (s, log) = e.get(&e.g("/audit-logs"), "owner").await;
    assert_eq!(s, StatusCode::OK, "{log}");
    let decided: Vec<_> = log["audit_log_entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| x["action_type"] == 200)
        .collect();
    assert_eq!(decided.len(), 2, "{log}");
    assert_eq!(decided[0]["options"]["status"], "approved");
    assert_eq!(decided[1]["options"]["status"], "rejected");
    assert_eq!(decided[1]["options"]["application_id"], app_id.as_str());
}

#[path = "soundboard_tests.rs"]
mod soundboard_tests;

// ---- Staff Server invites (ADM-STF-003) ---------------------------------------------

#[tokio::test]
async fn nobody_creates_or_redeems_invites_to_the_staff_server() {
    let e = wired().await;
    // An invite made while the server was ordinary...
    let old = new_invite(&e, json!({"max_uses": 3})).await;
    // ...stops working once the server is the Staff Server.
    e.backend
        .data()
        .guilds
        .get_mut(&e.guild)
        .unwrap()
        .is_staff_guild = true;
    let (s, body) = e.post(&e.invites_path(), "owner", json!({})).await;
    assert_eq!(
        (s, body["code"].as_u64()),
        (StatusCode::FORBIDDEN, Some(40006)),
        "{body}"
    );
    assert!(body["message"].as_str().unwrap().contains("Staff Server"));
    let r = e.accept(CAROL, &old).await;
    assert_eq!(code_of(&r), (StatusCode::FORBIDDEN, Some(40006)));
    assert!(!is_member(&e, CAROL));
    assert_eq!(uses(&e, &old), 0, "a refusal keeps the use");
    // Other servers are unaffected.
    e.backend
        .data()
        .guilds
        .get_mut(&e.guild)
        .unwrap()
        .is_staff_guild = false;
    assert_eq!(e.accept(CAROL, &old).await.0, StatusCode::OK);
}

#[path = "features_tests.rs"]
mod features_tests;

#[tokio::test]
async fn emoji_popout_and_url_work_for_non_members_and_name_only_listed_servers() {
    use sr_chat::settings::directory::{DirectoryEntry, DirectoryRepository, DirectoryStatus};
    use sr_chat::settings::repo::CommunityRepository;
    let e = env().await;
    let (_, emoji) = e
        .post(
            &e.g("/emojis"),
            "mod",
            json!({"name": "party", "image": png_uri(8)}),
        )
        .await;
    let id = emoji["id"].as_str().unwrap().to_owned();
    let path = format!("/sr/emojis/{id}");

    // Anyone signed in may draw it; a private server stays unnamed to outsiders.
    let (s, v) = e.get(&path, "carol").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(
        (v["name"].as_str(), v["animated"].as_bool()),
        (Some("party"), Some(false))
    );
    assert!(v["guild"].is_null(), "{v}");
    let (s, u) = e.get(&format!("{path}/url?size=64"), "carol").await;
    assert_eq!(s, StatusCode::OK, "{u}");
    assert!(u["url"].as_str().unwrap().starts_with("/cdn/emoji/"));
    assert_eq!(
        e.get(&format!("{path}/url?size=100"), "carol").await.0,
        StatusCode::BAD_REQUEST
    );

    // Members see the server (not a Directory listing).
    let (_, v) = e.get(&path, "bob").await;
    assert_eq!(v["guild"]["id"], e.guild.to_string());
    assert!(v["guild"]["listing"].is_null());

    // An approved Directory listing of a Community server names it for everyone.
    let mut c = e.repo.get_community(e.guild).await.unwrap();
    c.community_enabled = true;
    e.repo.put_community(&c).await.unwrap();
    e.directory
        .put_listing(&DirectoryEntry {
            guild_id: e.guild,
            status: DirectoryStatus::Approved,
            category: "gaming".into(),
            description: String::new(),
            featured: false,
            requested_by: UserId::new(OWNER),
            requested_at_ms: T0,
            decided_by: None,
            decided_at_ms: None,
            decision_note: None,
        })
        .await
        .unwrap();
    let (_, v) = e.get(&path, "carol").await;
    assert_eq!(v["guild"]["id"], e.guild.to_string(), "{v}");
    assert_eq!(v["guild"]["listing"], e.guild.to_string());

    assert_eq!(
        e.get("/sr/emojis/12345", "bob").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.get("/sr/emojis/abc", "bob").await.0,
        StatusCode::BAD_REQUEST
    );
}

/// D-237: an application emoji resolves by id for any session (the popout names the application),
/// and its `<:name:id>` renders only in messages sent by that application's bot.
#[tokio::test]
async fn application_emoji_resolve_by_id_and_render_only_for_their_bot() {
    use sr_apps::service::NewAppEmoji;
    let e = env().await;
    e.apps.add_account(1, "owner");
    let created = e
        .apps_svc
        .create_application(1, "Helper", "x")
        .await
        .unwrap();
    let app = created.application.id;
    let bot = created.application.bot_user_id;
    let emoji = e
        .apps_svc
        .add_app_emoji(
            1,
            app,
            NewAppEmoji {
                id: e.apps_svc.new_media_id(),
                name: "wave".into(),
                animated: false,
                size_bytes: 1,
                media_key: "emoji/app/1/1.png".into(),
            },
        )
        .await
        .unwrap();
    let path = format!("/sr/emojis/{}", emoji.id);

    // Any signed-in caller resolves it; the owner is the application, not a server.
    let (s, v) = e.get(&path, "carol").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["name"], "wave");
    assert!(v["guild"].is_null(), "{v}");
    assert_eq!(v["application"]["id"], app.to_string());
    assert_eq!(v["application"]["name"], "Helper");
    let (s, u) = e.get(&format!("{path}/url?size=64"), "carol").await;
    assert_eq!(s, StatusCode::OK, "{u}");
    assert!(u["url"].as_str().unwrap().starts_with("/cdn/emoji/app/"));

    // Rendering: a person's message with the token keeps it as text.
    let post = |who: &'static str| {
        let e = &e;
        let emoji_id = emoji.id;
        async move {
            e.post(
                &format!("/channels/{}/messages", e.ch),
                who,
                json!({"content": format!("hi <:wave:{emoji_id}>")}),
            )
            .await
        }
    };
    {
        let mut d = e.backend.data();
        d.app_emojis.insert(emoji.id as u64, app);
    }
    let (s, m) = post("bob").await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert_eq!(m["sr_text_emoji"], json!([emoji.id.to_string()]), "{m}");

    // The application's bot sends the same token: it renders (no hint).
    e.backend.data().bot_apps.insert(UserId::new(BOB), app);
    let (s, m) = post("bob").await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert!(m.get("sr_text_emoji").is_none(), "{m}");
    let _ = bot;

    assert_eq!(
        e.get(&format!("/sr/emojis/{}", emoji.id + 1), "bob")
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn audit_log_can_be_filtered_by_target() {
    let e = env().await;
    let mut ids = Vec::new();
    for name in ["alpha", "beta"] {
        let (s, emoji) = e
            .post(
                &e.g("/emojis"),
                "mod",
                json!({"name": name, "image": png_uri(8)}),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{emoji}");
        ids.push(emoji["id"].as_str().unwrap().to_owned());
    }
    let (_, all) = e.get(&e.g("/audit-logs"), "owner").await;
    assert!(all["audit_log_entries"].as_array().unwrap().len() >= 2);

    let (s, only) = e
        .get(&e.g(&format!("/audit-logs?target_id={}", ids[0])), "owner")
        .await;
    assert_eq!(s, StatusCode::OK, "{only}");
    let entries = only["audit_log_entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1, "{only}");
    assert_eq!(entries[0]["target_id"], ids[0].as_str());

    let (_, none) = e.get(&e.g("/audit-logs?target_id=12345"), "owner").await;
    assert!(none["audit_log_entries"].as_array().unwrap().is_empty());
    assert_eq!(
        e.get(&e.g("/audit-logs?target_id=abc"), "owner").await.0,
        StatusCode::BAD_REQUEST
    );
}

// ---- webhook author (DEV-BOT-006) -------------------------------------------------

#[tokio::test]
async fn webhook_messages_show_the_webhook_or_the_per_call_override() {
    let e = env().await;
    let (_, wh) = e
        .post(
            &format!("/channels/{}/webhooks", e.ch),
            "mod",
            json!({"name": "Deploys"}),
        )
        .await;
    let (id, token) = (wh["id"].as_str().unwrap(), wh["token"].as_str().unwrap());
    let exec = format!("/webhooks/{id}/{token}?wait=true");

    // No override: the webhook name, the webhook id as author id, marked as a bot.
    let (s, m) = e
        .call(Method::POST, &exec, None, Some(json!({"content": "plain"})))
        .await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert_eq!(m["webhook_id"], id);
    assert_eq!(m["author"]["id"], id);
    assert_eq!(m["author"]["username"], "Deploys");
    assert_eq!(m["author"]["bot"], true);
    assert!(m["author"].get("avatar_url").is_none());

    // Override name and avatar for one message; the avatar is served through the image proxy.
    let (s, m) = e
        .call(
            Method::POST,
            &exec,
            None,
            Some(json!({
                "content": "custom",
                "username": "  Build Bot ",
                "avatar_url": "https://img.example/a.png"
            })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert_eq!(m["author"]["username"], "Build Bot");
    assert_eq!(m["author"]["global_name"], "Build Bot");
    let avatar = m["author"]["avatar_url"].as_str().unwrap();
    assert!(avatar.starts_with("/cdn/external/"), "{avatar}");
    assert!(
        !avatar.contains("img.example"),
        "never the raw URL: {avatar}"
    );

    // The history shows the same authors (also for a reader who is not the creator).
    let (_, history) = e.get(&format!("/channels/{}/messages", e.ch), "bob").await;
    let names: Vec<_> = history
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["author"]["username"].as_str().unwrap().to_owned())
        .collect();
    assert!(names.contains(&"Build Bot".to_owned()) && names.contains(&"Deploys".to_owned()));

    // Bad overrides are refused before anything is posted.
    for body in [
        json!({"content": "x", "username": "n".repeat(81)}),
        json!({"content": "x", "avatar_url": "http://img.example/a.png"}),
        json!({"content": "x", "avatar_url": "javascript:alert(1)"}),
    ] {
        let (s, _) = e.call(Method::POST, &exec, None, Some(body)).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }
    let (_, history) = e.get(&format!("/channels/{}/messages", e.ch), "bob").await;
    assert_eq!(history.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn a_webhook_posts_into_a_thread_of_its_channel() {
    let e = env().await;
    let (_, wh) = e
        .post(
            &format!("/channels/{}/webhooks", e.ch),
            "mod",
            json!({"name": "Deploys"}),
        )
        .await;
    let (id, token) = (wh["id"].as_str().unwrap(), wh["token"].as_str().unwrap());
    let thread = sr_core::ChannelId::new(7_700_001);
    let other = sr_core::ChannelId::new(7_700_002);
    {
        let mut d = e.backend.data();
        for (cid, parent) in [(thread, Some(e.ch)), (other, None)] {
            let mut c =
                sr_chat::Channel::new(cid, Some(e.guild), sr_chat::ChannelType::PublicThread);
            c.name = Some("t".into());
            c.parent_id = parent;
            c.thread = Some(sr_chat::threads::model::ThreadInfo::new(T0, 1440));
            d.channels.insert(cid, c);
        }
    }
    let exec = |q: &str| format!("/webhooks/{id}/{token}?wait=true{q}");

    // A thread of the webhook's channel: the message lands there, not in the channel.
    let (s, m) = e
        .call(
            Method::POST,
            &exec(&format!("&thread_id={thread}")),
            None,
            Some(json!({"content": "in thread", "username": "Build Bot"})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert_eq!(m["channel_id"], thread.to_string());
    assert_eq!(m["author"]["username"], "Build Bot");
    assert_eq!(m["webhook_id"], id);

    // A thread of some other channel, a plain channel and junk are refused.
    for bad in [
        other.to_string(),
        e.ch.to_string(),
        "x".to_owned(),
        "0".to_owned(),
    ] {
        let (s, v) = e
            .call(
                Method::POST,
                &exec(&format!("&thread_id={bad}")),
                None,
                Some(json!({"content": "no"})),
            )
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}: {v}");
    }
    let (_, history) = e.get(&format!("/channels/{}/messages", e.ch), "bob").await;
    assert!(
        history.as_array().unwrap().is_empty(),
        "nothing leaked into the parent channel"
    );
}

#[tokio::test]
async fn following_a_channel_posts_a_follow_added_notice_in_the_receiving_channel() {
    let e = env().await;
    let news = add_channel(&e, 7_101, sr_chat::ChannelType::Announcement, "news", 0);
    let (s, f) = e
        .post(
            &format!("/channels/{news}/followers"),
            "owner",
            json!({ "webhook_channel_id": e.ch.to_string() }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{f}");
    let notices: Vec<Value> = history(&e, e.ch)
        .await
        .into_iter()
        .filter(|m| m["type"] == 12)
        .collect();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0]["content"], "news");
    assert_eq!(notices[0]["author"]["id"], OWNER.to_string());
}

// ---- alert messages with moderation buttons (D-243, D-244) -----------------------

impl Env {
    /// A request as a signed-in browser session of `user` (alert buttons are for people).
    async fn as_user(
        &self,
        user: u64,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        use sr_auth::{
            repo::{SessionRecord, SessionRepository},
            session::SessionToken,
        };
        let t = SessionToken::generate().unwrap();
        let now = self.clock.now_unix_ms() / 1000;
        self.sessions
            .create(SessionRecord {
                id: i64::try_from(user).unwrap(),
                user_id: i64::try_from(user).unwrap(),
                token_hash: t.hash(),
                created_at: now - 10,
                last_seen_at: now,
                expires_at: now + 100_000,
                device_name: None,
            })
            .await
            .unwrap();
        let mut req = Request::builder()
            .method(method.clone())
            .uri(format!("/api/v1{path}"))
            .header("cookie", format!("sr_session={}", t.expose()));
        if method != Method::GET {
            req = req.header("x-sr-csrf", csrf::token_for(t.expose()));
        }
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
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (parts.status, json)
    }

    /// Add permissions to the `mods` role (MOD's role).
    fn grant_mods(&self, p: Permissions) {
        let mut d = self.backend.data();
        d.roles
            .iter_mut()
            .find(|r| r.name == "mods")
            .unwrap()
            .permissions |= p;
    }

    /// Press a button on a card in `channel`.
    async fn press(
        &self,
        who: u64,
        channel: ChannelId,
        card: &Value,
        custom_id: &str,
        values: &[&str],
    ) -> (StatusCode, Value) {
        self.as_user(
            who,
            Method::POST,
            &self.g("/alert-actions"),
            Some(json!({
                "channel_id": channel.to_string(),
                "message_id": card["id"],
                "custom_id": custom_id,
                "values": values,
            })),
        )
        .await
    }
}

fn card_ids(card: &Value) -> Vec<String> {
    card["components"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|r| r["components"].as_array().into_iter().flatten())
        .filter_map(|c| c["custom_id"].as_str().map(str::to_owned))
        .collect()
}

fn card_field(card: &Value, name: &str) -> Option<String> {
    card["embeds"][0]["fields"]
        .as_array()?
        .iter()
        .find(|f| f["name"] == name)
        .and_then(|f| f["value"].as_str().map(str::to_owned))
}

/// The cards (type 24) in a channel, newest first.
async fn cards(e: &Env, channel: ChannelId) -> Vec<Value> {
    history(e, channel)
        .await
        .into_iter()
        .filter(|m| m["type"] == 24)
        .collect()
}

#[tokio::test]
async fn a_join_burst_pauses_invites_and_posts_a_resume_card() {
    let e = wired().await;
    let alerts = add_channel(&e, 7_000_001, sr_chat::ChannelType::Text, "safety", 0);
    let code = new_invite(&e, json!({})).await;
    // Three members are here (owner, bob, mod); more than four in a minute is a burst.
    let (s, c) = e
        .patch(
            &e.g("/sr/community"),
            "owner",
            json!({
                "raid_alert_joins_per_minute": 4,
                "safety_alerts_channel_id": alerts.to_string(),
            }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{c}");
    assert_eq!(c["raid_auto_pause_invites"], true);

    assert_eq!(e.accept(10, &code).await.0, StatusCode::OK);
    assert!(
        cards(&e, alerts).await.is_empty(),
        "four joins is not a burst"
    );
    assert_eq!(e.accept(11, &code).await.0, StatusCode::OK);

    // The alert: a card from the system user with a Resume invites button.
    let found = cards(&e, alerts).await;
    assert_eq!(found.len(), 1, "{found:?}");
    let card = &found[0];
    assert_eq!(card["author"]["id"], "1");
    assert_eq!(
        card_ids(card),
        [format!("sr:incident:resume-invites:{}", e.guild)],
        "{card}"
    );
    assert_eq!(
        card_field(card, "Joins in the last minute").as_deref(),
        Some("5")
    );
    // Invites are paused for 24 hours by itself and the audit row names the module.
    let (raid, until) = e.repo.with(|s| {
        let c = &s.community[&e.guild];
        (c.raid_detected_at_ms, c.invites_disabled_until_ms)
    });
    assert_eq!((raid, until), (Some(T0), Some(T0 + 24 * 3_600_000)));
    assert_eq!(
        code_of(&e.accept(12, &code).await),
        (StatusCode::FORBIDDEN, Some(40006))
    );
    let (_, log) = e.get(&e.g("/audit-logs?action_type=1"), "owner").await;
    assert!(
        log["audit_log_entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["options"]["module"] == "community"),
        "{log}"
    );

    // Resume invites is checked at press time: a plain member is refused, a manager is not.
    let id = format!("sr:incident:resume-invites:{}", e.guild);
    assert_eq!(
        e.press(BOB, alerts, card, &id, &[]).await.0,
        StatusCode::FORBIDDEN
    );
    // A button id the card does not carry is a stale copy.
    let (s, r) = e
        .press(MOD, alerts, card, "sr:incident:resume-invites:1", &[])
        .await;
    assert_eq!((s, r["ok"].as_bool()), (StatusCode::OK, Some(false)), "{r}");
    let (s, r) = e.press(MOD, alerts, card, &id, &[]).await;
    assert_eq!((s, r["ok"].as_bool()), (StatusCode::OK, Some(true)), "{r}");
    assert_eq!(
        e.repo
            .with(|s| s.community[&e.guild].invites_disabled_until_ms),
        None
    );
    let after = &cards(&e, alerts).await[0];
    assert!(card_ids(after).is_empty(), "the button is gone");
    assert_eq!(
        card_field(after, "Handled").as_deref(),
        Some(format!("Invites resumed by <@{MOD}>").as_str())
    );
    assert_eq!(
        e.accept(12, &code).await.0,
        StatusCode::OK,
        "invites are open again"
    );
    // A stale copy gets the polite answer.
    let (s, r) = e.press(MOD, alerts, card, &id, &[]).await;
    assert_eq!((s, r["ok"].as_bool()), (StatusCode::OK, Some(false)), "{r}");

    // A second burst inside the cooldown posts nothing more.
    assert_eq!(e.accept(13, &code).await.0, StatusCode::OK);
    assert_eq!(cards(&e, alerts).await.len(), 1);

    // With the automatic pause switched off, the alert posts but offers no button.
    e.clock.advance_ms(11 * 60_000);
    e.repo.with(|s| {
        s.community
            .get_mut(&e.guild)
            .unwrap()
            .raid_auto_pause_invites = false;
    });
    for user in 14..=18 {
        assert_eq!(e.accept(user, &code).await.0, StatusCode::OK);
    }
    let found = cards(&e, alerts).await;
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(card_ids(&found[0]).is_empty());
    assert_eq!(
        e.repo
            .with(|s| s.community[&e.guild].invites_disabled_until_ms),
        None
    );
}

fn alert_rule(channel: ChannelId, block: bool) -> Value {
    let mut actions = vec![json!({"type": 2, "metadata": {"channel_id": channel.to_string()}})];
    if block {
        actions.push(json!({"type": 1, "metadata": {"custom_message": "no"}}));
    }
    json!({
        "name": "no bad words", "event_type": 1, "trigger_type": 1,
        "trigger_metadata": {"keyword_filter": ["badword"]},
        "actions": actions, "enabled": true,
    })
}

#[tokio::test]
async fn automod_alerts_are_cards_whose_buttons_check_the_presser_when_pressed() {
    let e = env().await;
    let log = add_channel(&e, 7_000_002, sr_chat::ChannelType::Text, "mod-log", 0);
    let (s, rule) = e
        .post(
            &e.g("/auto-moderation/rules"),
            "owner",
            alert_rule(log, false),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{rule}");

    let flagged = say(&e, e.ch, "bob", "this has a BADWORD in it").await;
    let found = cards(&e, log).await;
    assert_eq!(found.len(), 1, "{found:?}");
    let card = &found[0];
    let id = card["id"].as_str().unwrap().to_owned();
    assert_eq!(card["author"]["id"], "1");
    assert_eq!(
        card["flags"].as_u64().unwrap() & (1 << 30),
        1 << 30,
        "SYSTEM tag"
    );
    assert_eq!(card_field(card, "Rule").as_deref(), Some("no bad words"));
    assert_eq!(card_field(card, "Trigger").as_deref(), Some("Keyword"));
    assert_eq!(card_field(card, "Channel"), Some(format!("<#{}>", e.ch)));
    assert_eq!(card_field(card, "Member"), Some(format!("<@{BOB}>")));
    assert_eq!(card_field(card, "Blocked").as_deref(), Some("No"));
    assert_eq!(
        card_ids(card),
        [
            format!("sr:automod:delete:{id}"),
            format!("sr:automod:kick:{id}"),
            format!("sr:automod:ban:{id}"),
            format!("sr:automod:resolve:{id}"),
            format!("sr:automod:timeout:{id}"),
        ]
    );
    let select = &card["components"][1]["components"][0];
    let values: Vec<&str> = select["options"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|o| o["value"].as_str())
        .collect();
    assert_eq!(values, ["60", "300", "600", "3600", "86400", "604800"]);
    // People press; a bot token is refused.
    let delete = format!("sr:automod:delete:{id}");
    assert_eq!(
        e.post(
            &e.g("/alert-actions"),
            "mod",
            json!({"channel_id": log.to_string(), "message_id": id, "custom_id": delete})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    // Delete message needs MANAGE_MESSAGES now, whatever the card says.
    let (s, r) = e.press(MOD, log, card, &delete, &[]).await;
    assert_eq!(
        (s, r["code"].as_u64()),
        (StatusCode::FORBIDDEN, Some(50013)),
        "{r}"
    );
    assert_eq!(
        e.press(BOB, log, card, &delete, &[]).await.0,
        StatusCode::FORBIDDEN
    );
    assert!(
        history(&e, e.ch)
            .await
            .iter()
            .any(|m| m["id"] == flagged["id"])
    );
    e.grant_mods(Permissions::MANAGE_MESSAGES);
    let (s, r) = e.press(MOD, log, card, &delete, &[]).await;
    assert_eq!((s, r["ok"].as_bool()), (StatusCode::OK, Some(true)), "{r}");
    assert!(
        !history(&e, e.ch)
            .await
            .iter()
            .any(|m| m["id"] == flagged["id"]),
        "the flagged message is gone"
    );
    let card = cards(&e, log).await.remove(0);
    assert!(!card_ids(&card).contains(&delete));
    assert_eq!(
        card_field(&card, "Handled"),
        Some(format!("Deleted by <@{MOD}>"))
    );
    // Pressing the removed button again is a polite no.
    let (s, r) = e.press(MOD, log, &card, &delete, &[]).await;
    assert_eq!((s, r["ok"].as_bool()), (StatusCode::OK, Some(false)), "{r}");

    // Timeout: the choice must be one of the six.
    let timeout = format!("sr:automod:timeout:{id}");
    let (s, _) = e.press(MOD, log, &card, &timeout, &["3600"]).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "MODERATE_MEMBERS is missing");
    e.grant_mods(Permissions::MODERATE_MEMBERS);
    assert_eq!(
        e.press(MOD, log, &card, &timeout, &["5"]).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        e.press(MOD, log, &card, &timeout, &[]).await.0,
        StatusCode::BAD_REQUEST
    );
    let (s, r) = e.press(MOD, log, &card, &timeout, &["3600"]).await;
    assert_eq!((s, r["ok"].as_bool()), (StatusCode::OK, Some(true)), "{r}");
    let until = e
        .backend
        .data()
        .members
        .iter()
        .find(|m| m.user_id.get() == BOB)
        .and_then(|m| m.communication_disabled_until_ms);
    assert_eq!(until, Some(T0 + 3_600_000));
    let card = cards(&e, log).await.remove(0);
    assert_eq!(
        card_field(&card, "Handled"),
        Some(format!(
            "Deleted by <@{MOD}>\nTimed out by <@{MOD}> for 1 h"
        ))
    );

    // Kick carries the AutoMod reason into the audit row, naming the presser.
    let kick = format!("sr:automod:kick:{id}");
    assert_eq!(
        e.press(MOD, log, &card, &kick, &[]).await.0,
        StatusCode::FORBIDDEN
    );
    e.grant_mods(Permissions::KICK_MEMBERS);
    let (s, r) = e.press(MOD, log, &card, &kick, &[]).await;
    assert_eq!((s, r["ok"].as_bool()), (StatusCode::OK, Some(true)), "{r}");
    assert!(!is_member(&e, BOB));
    let (_, audit) = e.get(&e.g("/audit-logs?action_type=20"), "owner").await;
    let entry = &audit["audit_log_entries"][0];
    assert_eq!(entry["user_id"], MOD.to_string(), "{audit}");
    assert_eq!(entry["reason"], "AutoMod alert: no bad words");

    // Mark resolved closes the card.
    let card = cards(&e, log).await.remove(0);
    let resolve = format!("sr:automod:resolve:{id}");
    let (s, r) = e.press(MOD, log, &card, &resolve, &[]).await;
    assert_eq!((s, r["ok"].as_bool()), (StatusCode::OK, Some(true)), "{r}");
    let card = cards(&e, log).await.remove(0);
    assert!(card_ids(&card).is_empty());
    assert!(
        card_field(&card, "Handled")
            .unwrap()
            .ends_with(&format!("Resolved by <@{MOD}>"))
    );
    let (s, r) = e.press(MOD, log, &card, &resolve, &[]).await;
    assert_eq!((s, r["ok"].as_bool()), (StatusCode::OK, Some(false)), "{r}");
}

#[tokio::test]
async fn a_blocked_message_still_gets_an_alert_card_without_a_delete_button() {
    let e = env().await;
    let log = add_channel(&e, 7_000_003, sr_chat::ChannelType::Text, "mod-log", 0);
    let (s, rule) = e
        .post(
            &e.g("/auto-moderation/rules"),
            "owner",
            alert_rule(log, true),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{rule}");
    let (s, _) = e
        .post(
            &format!("/channels/{}/messages", e.ch),
            "bob",
            json!({"content": "badword"}),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let found = cards(&e, log).await;
    assert_eq!(found.len(), 1);
    let card = &found[0];
    assert_eq!(card_field(card, "Blocked").as_deref(), Some("Yes"));
    let id = card["id"].as_str().unwrap();
    assert!(!card_ids(card).contains(&format!("sr:automod:delete:{id}")));

    // Ban: the reason reaches the ban and the audit row.
    e.grant_mods(Permissions::BAN_MEMBERS);
    let ban = format!("sr:automod:ban:{id}");
    let (s, r) = e.press(MOD, log, card, &ban, &[]).await;
    assert_eq!((s, r["ok"].as_bool()), (StatusCode::OK, Some(true)), "{r}");
    assert!(
        e.backend
            .data()
            .bans
            .iter()
            .any(|b| b.guild_id == e.guild && b.user_id == UserId::new(BOB))
    );
    let (_, audit) = e.get(&e.g("/audit-logs?action_type=22"), "owner").await;
    assert_eq!(
        audit["audit_log_entries"][0]["reason"],
        "AutoMod alert: no bad words"
    );
    let card = cards(&e, log).await.remove(0);
    assert_eq!(
        card_field(&card, "Handled"),
        Some(format!("Banned by <@{MOD}>"))
    );
}
