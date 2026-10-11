//! Endpoint tests over the in-memory repositories with one-shot requests.

use super::automod_gate::{AutoModGate, MessageCheck};
use super::events::RecordingEvents;
use super::join::MemberJoiner;
use super::{AutoModGateHandle, SettingsDeps, router};
use crate::routes::messages::{self, FixedEntitlements, MessagesDeps};
use crate::{
    ApiSettings, AppState,
    auth::{BotIdentity, memory::MemoryBots},
    build_router,
};
use async_trait::async_trait;
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
use sr_chat::settings::repo::{InviteRepository, WebhookRepository};
use sr_chat::settings::service::SettingsService;
use sr_chat::{Member, Role};
use sr_core::permissions::Permissions;
use sr_core::{
    ApiError, ChannelId, Clock, GuildId, ManualClock, RoleId, SnowflakeGenerator, UserId,
};
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

/// Adds the member to the in-memory guild, standing in for the real join path.
struct TestJoiner(MemoryBackend);

#[async_trait]
impl MemberJoiner for TestJoiner {
    fn is_wired(&self) -> bool {
        true
    }
    async fn join(&self, guild: GuildId, user: UserId, _temporary: bool) -> Result<(), ApiError> {
        self.0.data().members.push(Member {
            guild_id: guild,
            user_id: user,
            nick: None,
            role_ids: vec![],
            joined_at_ms: T0,
            communication_disabled_until_ms: None,
            pending: false,
        });
        Ok(())
    }
}

struct Env {
    app: Router,
    backend: MemoryBackend,
    repo: Arc<MemorySettingsRepo>,
    events: Arc<RecordingEvents>,
    storage: Arc<ObjectStoreStorage>,
    deps: Arc<SettingsDeps>,
    guild: GuildId,
    ch: ChannelId,
    voice: ChannelId,
}

#[derive(Default)]
struct Opts {
    limits: Option<ExpressionLimits>,
    wired_joiner: bool,
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
    if opts.wired_joiner {
        deps = deps.with_joiner(Arc::new(TestJoiner(backend.clone())));
    }
    let deps = Arc::new(deps);

    let msgs = MessagesDeps::new(
        backend.clone(),
        Arc::new(backend.clone()),
        storage.clone(),
        bus,
        Arc::new(FixedEntitlements(1 << 20)),
        Arc::new(backend.clone()),
        None,
        dyn_clock.clone(),
        1,
        1,
    )
    .unwrap();
    let state = AppState::builder()
        .bots(bots)
        .clock(dyn_clock)
        .settings(settings)
        .extension(deps.clone())
        .extension(Arc::new(AutoModGateHandle(deps.clone())))
        .extension(Arc::new(msgs))
        .build();
    let app = build_router(state, router().merge(messages::router()));
    Env {
        app,
        backend,
        repo,
        events,
        storage,
        deps,
        guild,
        ch,
        voice,
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

#[tokio::test]
async fn accepting_invites() {
    // Without a wired join path the endpoint refuses before using the invite.
    let e = env().await;
    let (_, inv) = e
        .post(&e.invites_path(), "owner", json!({"max_uses": 1}))
        .await;
    let code = inv["code"].as_str().unwrap().to_owned();
    let r = e
        .post(&format!("/invites/{code}"), "carol", json!({}))
        .await;
    assert_eq!(code_of(&r), (StatusCode::FORBIDDEN, Some(40006)));
    assert_eq!(
        e.repo.get_invite(&code).await.unwrap().unwrap().uses,
        0,
        "a refused accept must not consume the invite"
    );

    let e = env_with(Opts {
        wired_joiner: true,
        ..Opts::default()
    })
    .await;
    let (_, inv) = e
        .post(&e.invites_path(), "owner", json!({"max_uses": 1}))
        .await;
    let code = inv["code"].as_str().unwrap().to_owned();
    let accept = format!("/invites/{code}");

    // Members just get the invite back, unused.
    let (s, _) = e.post(&accept, "bob", json!({})).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(e.repo.get_invite(&code).await.unwrap().unwrap().uses, 0);

    // A banned user is refused and the invite stays unused.
    e.repo.with(|s| {
        s.bans.insert((e.guild, UserId::new(CAROL)));
    });
    assert_eq!(
        code_of(&e.post(&accept, "carol", json!({})).await),
        (StatusCode::FORBIDDEN, Some(40007))
    );
    e.repo.with(|s| s.bans.clear());

    // Paused invites are refused (Security Actions).
    let (s, _) = e
        .patch(
            &e.g("/sr/community"),
            "owner",
            json!({"invites_paused": true}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        code_of(&e.post(&accept, "carol", json!({})).await).0,
        StatusCode::FORBIDDEN
    );
    e.patch(
        &e.g("/sr/community"),
        "owner",
        json!({"invites_paused": false}),
    )
    .await;

    // The use is consumed and the member is added; the next try finds it used up.
    let (s, joined) = e.post(&accept, "carol", json!({})).await;
    assert_eq!((s, joined["uses"].as_u64()), (StatusCode::OK, Some(1)));
    assert!(
        e.backend
            .data()
            .members
            .iter()
            .any(|m| m.user_id.get() == CAROL)
    );
    // A used-up invite is unknown to everyone, members included.
    assert_eq!(
        code_of(&e.post(&accept, "bob", json!({})).await),
        (StatusCode::NOT_FOUND, Some(10006))
    );
    assert_eq!(
        code_of(&e.get(&accept, "owner").await),
        (StatusCode::NOT_FOUND, Some(10006)),
        "used up"
    );
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
