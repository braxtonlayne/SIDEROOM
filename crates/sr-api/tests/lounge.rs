//! Phase 10 lounge backend flows over the in-memory repositories, driven through the real
//! routers with `oneshot`: server tags (LNG-GLD-011), per-server profiles (LNG-GLD-012),
//! vanity URLs (LNG-INV-004), membership screening and onboarding answers (LNG-GLD-008),
//! temporary memberships (LNG-INV-005) and the Server Directory (LNG-GLD-013).
//!
//! Everybody signs in with a browser session, because joining and staff routes refuse bots.

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use futures::{StreamExt, stream::BoxStream};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sr_admin::{AccountState, AdminService, Caller, SYSTEM_USER_ID, memory::MemoryAdminRepo};
use sr_api::{
    ApiSettings, AppState,
    auth::{
        csrf,
        memory::{MemoryBots, MemorySessions},
    },
    build_router,
    routes::{
        admin::{self, AdminDeps},
        auth::{
            AccountRecord, AccountState as AuthAccountState, AuthService, AuthStore,
            mail::MemoryMailer, memory::MemoryAuthStore,
        },
        directory::{self, DirectoryDeps},
        guilds, settings,
        social::{self, DynRepo as SocialDynRepo, SocialDeps},
    },
};
use sr_auth::{
    repo::{SessionRecord, SessionRepository},
    session::SessionToken,
};
use sr_chat::memory::MemoryRepo;
use sr_chat::repo::UserSummary;
use sr_chat::settings::directory::MemoryDirectory;
use sr_chat::settings::memory::{MemoryInviteJoin, MemorySettingsRepo};
use sr_chat::social::repo::DmRepository;
use sr_chat::{ChatLimits, ChatService, Member};
use sr_core::PlatformPermissions as P;
use sr_core::{ChannelId, Clock, GuildId, ManualClock, SnowflakeGenerator, UserId};
use sr_events::{DomainEvent, EventBus, InProcessBus, Topic};
use sr_storage::{ObjectStorage, ObjectStoreStorage};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tower::ServiceExt;

const T0: i64 = 1_800_000_000_000;
const OWNER: i64 = 1;
const BOB: i64 = 2;
const CAROL: i64 = 3;
const DAVE: i64 = 4;
/// Staff with `MANAGE_DIRECTORY`.
const STAFF: i64 = 11;
/// Staff without it.
const BASIC: i64 = 12;
const ADMIN_OWNER: i64 = 10;

/// A 1x1 PNG.
const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

struct World {
    app: Router,
    repo: MemoryRepo,
    settings: Arc<MemorySettingsRepo>,
    joiner: Arc<settings::InviteMemberJoiner>,
    bus: Arc<InProcessBus>,
    storage: Arc<ObjectStoreStorage>,
    features: sr_features::FeatureService,
    social: Arc<SocialDeps>,
    settings_deps: Arc<settings::SettingsDeps>,
    voice: Arc<FakeVoice>,
    notified: Arc<Notified>,
    clock: Arc<ManualClock>,
    /// The platform admin store (staff, and the server list behind `/admin/guilds`).
    admin_repo: Arc<MemoryAdminRepo>,
    tokens: HashMap<i64, String>,
    auth: Arc<MemoryAuthStore>,
}

/// Voice side of the event clock: occupancy, live stages and a log of stage calls.
#[derive(Default)]
struct FakeVoice {
    occupants: std::sync::Mutex<HashMap<ChannelId, usize>>,
    live: std::sync::Mutex<Vec<ChannelId>>,
    log: std::sync::Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl settings::EventWorld for FakeVoice {
    async fn view(&self, _channels: &[ChannelId]) -> settings::event_world::VoiceView {
        settings::event_world::VoiceView {
            occupants: self.occupants.lock().unwrap().clone(),
            live_stages: self.live.lock().unwrap().iter().copied().collect(),
        }
    }
    async fn start_stage(
        &self,
        _guild: GuildId,
        channel: ChannelId,
        topic: &str,
        _by: UserId,
        _event_id: u64,
    ) -> Result<(), String> {
        self.log.lock().unwrap().push(format!("start {topic}"));
        self.live.lock().unwrap().push(channel);
        Ok(())
    }
    async fn end_stage(&self, channel: ChannelId) {
        self.log.lock().unwrap().push("end".into());
        self.live.lock().unwrap().retain(|c| *c != channel);
    }
}

/// Records the start notifications: event name and the members told.
#[derive(Default)]
struct Notified(std::sync::Mutex<Vec<(String, Vec<UserId>)>>);

#[async_trait::async_trait]
impl settings::EventStartNotifier for Notified {
    async fn event_started(
        &self,
        event: &sr_chat::settings::scheduled_events::ScheduledEvent,
        interested: &[UserId],
    ) {
        self.0
            .lock()
            .unwrap()
            .push((event.name.clone(), interested.to_vec()));
    }
}

async fn world() -> World {
    let clock = Arc::new(ManualClock::new(T0));
    let dyn_clock: Arc<dyn Clock> = clock.clone();
    let ids = SnowflakeGenerator::new(dyn_clock.clone(), 1, 1).unwrap();
    let repo = MemoryRepo::default();
    let bus = Arc::new(InProcessBus::new());
    let storage = Arc::new(ObjectStoreStorage::in_memory());
    let svc = ChatService::new(repo.clone(), dyn_clock.clone(), ids, ChatLimits::default());
    let guild_api = guilds::GuildApi(guilds::backend::backend(svc, bus.clone()));

    // Chat users and their display fields.
    for (id, name) in [
        (OWNER, "owner"),
        (BOB, "bob"),
        (CAROL, "carol"),
        (DAVE, "dave"),
    ] {
        repo.store.lock().unwrap().users.insert(
            UserId::new(id as u64),
            UserSummary {
                id: UserId::new(id as u64),
                username: name.into(),
                display_name: None,
                avatar_key: None,
                banner_key: None,
                bot: false,
                primary_guild: None,
            },
        );
    }

    // Settings: invites, community, joining.
    let settings_repo = Arc::new(MemorySettingsRepo::default());
    let counter = Arc::new(AtomicU64::new(0x9e37_79b9_7f4a_7c15));
    let entropy: settings::Entropy = Arc::new(move || {
        counter
            .fetch_add(0x1234_5678_9abc_def1, Ordering::SeqCst)
            .wrapping_mul(0xff51_afd7_ed55_8ccd)
    });
    let joiner = Arc::new(settings::InviteMemberJoiner::new(
        Arc::new(MemoryInviteJoin {
            chat: repo.clone(),
            settings: settings_repo.clone(),
        }),
        Arc::new(repo.clone()),
        bus.clone(),
    ));
    let voice = Arc::new(FakeVoice::default());
    let notified = Arc::new(Notified::default());
    let settings_deps = Arc::new(
        settings::SettingsDeps::new(
            settings_repo.clone(),
            Arc::new(repo.clone()),
            storage.clone(),
            dyn_clock.clone(),
            entropy,
            1,
            5,
        )
        .unwrap()
        .with_events(Arc::new(settings::events::BusSettingsEvents::new(
            bus.clone(),
        )))
        .with_joiner(joiner.clone())
        .with_event_world(voice.clone())
        .with_event_notifier(notified.clone()),
    );

    // Staff: STAFF may manage the directory, BASIC may not.
    let admin_repo = Arc::new(MemoryAdminRepo::default());
    admin_repo.add_account(
        SYSTEM_USER_ID,
        "sideroom",
        AccountState::Deactivated,
        None,
        None,
    );
    admin_repo.add_account(ADMIN_OWNER, "admin-owner", AccountState::Active, None, None);
    for (id, name) in [(STAFF, "staff"), (BASIC, "basic")] {
        admin_repo.add_account(id, name, AccountState::Active, None, Some(ADMIN_OWNER));
    }
    admin_repo.add_role(
        1,
        "Directory",
        2,
        P::STAFF_BASIC | P::MANAGE_DIRECTORY | P::MANAGE_GUILDS_GLOBAL,
    );
    admin_repo.add_role(2, "Basic", 1, P::STAFF_BASIC);
    admin_repo.set_owner(ADMIN_OWNER);
    let admin_svc = AdminService::new(admin_repo.clone()).with_extras(admin_repo.clone());
    let op = Caller::operator(SYSTEM_USER_ID);
    admin_svc
        .appoint(&op, STAFF, &[1], Some("t"))
        .await
        .unwrap();
    admin_svc
        .appoint(&op, BASIC, &[2], Some("t"))
        .await
        .unwrap();

    // Sessions for everybody.
    let sessions = Arc::new(MemorySessions::new());
    let mut tokens = HashMap::new();
    for (n, uid) in [OWNER, BOB, CAROL, DAVE, STAFF, BASIC]
        .into_iter()
        .enumerate()
    {
        let t = SessionToken::generate().unwrap();
        sessions
            .create(SessionRecord {
                id: 100 + n as i64,
                user_id: uid,
                token_hash: t.hash(),
                created_at: 0,
                last_seen_at: i64::MAX / 4,
                expires_at: i64::MAX / 4,
                device_name: None,
            })
            .await
            .unwrap();
        tokens.insert(uid, t.expose().to_owned());
    }

    // Accounts and the auth service, for the server 2FA requirement (G-94).
    let auth_store = Arc::new(MemoryAuthStore::new(sessions.clone()));
    for (uid, name) in [
        (OWNER, "owner"),
        (BOB, "bob"),
        (CAROL, "carol"),
        (DAVE, "dave"),
    ] {
        auth_store.add_account(AccountRecord {
            id: uid,
            person_id: uid,
            username: name.to_owned(),
            display_name: None,
            email: format!("{name}@x.test"),
            email_verified: true,
            password_hash: "unused".into(),
            state: AuthAccountState::Active,
            age_confirmed: None,
            age_confirmed_at: None,
            locale: "en-US".into(),
            flags: 0,
        });
    }
    let auth_ids = Arc::new(AtomicU64::new(50_000));
    let auth_service = AuthService::new(
        auth_store.clone(),
        Arc::new(MemoryMailer::new()),
        Arc::new(move || i64::try_from(auth_ids.fetch_add(1, Ordering::SeqCst)).unwrap()),
    );

    // Direct messages: group DM invites (G-47) and icons (G-50).
    let social_repo = Arc::new(sr_chat::social::memory::MemoryRepo::default());
    for uid in [OWNER, BOB, CAROL, DAVE] {
        social_repo.add_user(UserId::new(uid as u64));
    }
    let social_deps = Arc::new(
        SocialDeps::new(
            SocialDynRepo(social_repo.clone() as Arc<dyn sr_chat::social::repo::SocialRepository>),
            Arc::new(repo.clone()),
            bus.clone(),
            dyn_clock.clone(),
            sr_chat::social::SocialLimits::default(),
            1,
            7,
        )
        .unwrap(),
    );
    let mut feature_registry = sr_features::FeatureRegistry::new();
    feature_registry
        .register(sr_features::DirectoryHiddenCategoriesFeature)
        .unwrap();
    let features = sr_features::FeatureService::new(
        Arc::new(feature_registry),
        Arc::new(sr_features::InMemorySettingsStore::new()),
        None,
    );
    let mut api_settings = ApiSettings::default();
    api_settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(api_settings)
        .sessions(sessions)
        .bots(Arc::new(MemoryBots::new()))
        .clock(dyn_clock)
        .extension(Arc::new(guild_api))
        .extension(Arc::new(auth_service))
        .extension(settings_deps.clone())
        .extension(social_deps.clone())
        .extension(Arc::new(
            DirectoryDeps::new(Arc::new(MemoryDirectory::default()))
                .with_features(features.clone()),
        ))
        .extension(Arc::new(AdminDeps::new(admin_svc)))
        .extension(Arc::new(sr_api::routes::media::MediaDeps::new(
            storage.clone(),
        )))
        .build();
    let app = build_router(
        state,
        guilds::router()
            .merge(settings::router())
            .merge(social::router())
            .merge(directory::router())
            .merge(admin::router()),
    );
    World {
        app,
        repo,
        settings: settings_repo,
        joiner,
        bus,
        storage,
        features,
        social: social_deps,
        settings_deps,
        voice,
        notified,
        clock,
        admin_repo,
        tokens,
        auth: auth_store,
    }
}

impl World {
    async fn call(
        &self,
        who: i64,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let raw = &self.tokens[&who];
        let mut req = Request::builder()
            .method(method.clone())
            .uri(format!("/api/v1{path}"))
            .header("origin", "http://localhost:8080")
            .header("cookie", format!("sr_session={raw}"));
        if method != Method::GET {
            req = req.header("x-sr-csrf", csrf::token_for(raw));
        }
        let body = match body {
            Some(b) => {
                req = req.header("content-type", "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let resp = self
            .app
            .clone()
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        (status, json)
    }

    async fn get(&self, who: i64, path: &str) -> (StatusCode, Value) {
        self.call(who, Method::GET, path, None).await
    }
    async fn post(&self, who: i64, path: &str, body: Value) -> (StatusCode, Value) {
        self.call(who, Method::POST, path, Some(body)).await
    }
    async fn patch(&self, who: i64, path: &str, body: Value) -> (StatusCode, Value) {
        self.call(who, Method::PATCH, path, Some(body)).await
    }
    async fn put(&self, who: i64, path: &str, body: Value) -> (StatusCode, Value) {
        self.call(who, Method::PUT, path, Some(body)).await
    }
    async fn delete(&self, who: i64, path: &str) -> (StatusCode, Value) {
        self.call(who, Method::DELETE, path, None).await
    }

    /// The owner creates a server; BOB joins it directly. Returns its id and `#general`.
    async fn guild(&self, name: &str) -> (String, String) {
        let (s, g) = self.post(OWNER, "/guilds", json!({ "name": name })).await;
        assert_eq!(s, StatusCode::CREATED, "{g}");
        let gid = g["id"].as_str().unwrap().to_owned();
        let general = g["channels"][2]["id"].as_str().unwrap().to_owned();
        self.add_member(&gid, BOB);
        (gid, general)
    }

    fn add_member(&self, gid: &str, user: i64) {
        self.repo.store.lock().unwrap().members.push(Member {
            guild_id: GuildId::new(gid.parse().unwrap()),
            user_id: UserId::new(user as u64),
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

    /// Give `user` a confirmed authenticator (a second factor on the account).
    async fn enrol(&self, user: i64) {
        self.auth.totp_put_pending(user, &[7u8; 20]).await.unwrap();
        self.auth.totp_confirm(user, 0).await.unwrap();
    }

    /// `user` proves the second factor on their session now (sessions are numbered from 100).
    async fn prove(&self, session: i64) {
        let now = self.clock.now_unix_ms() / 1000;
        self.auth
            .session_mark_mfa_verified(session, now)
            .await
            .unwrap();
    }

    fn is_member(&self, gid: &str, user: i64) -> bool {
        self.repo
            .store
            .lock()
            .unwrap()
            .members
            .iter()
            .any(|m| m.guild_id.to_string() == gid && m.user_id == UserId::new(user as u64))
    }

    fn events(&self, gid: &str) -> BoxStream<'static, DomainEvent> {
        self.bus
            .subscribe(Topic::Guild(GuildId::new(gid.parse().unwrap())))
    }

    /// Turn Community on for the server (the safety checks need levels 1 and 2).
    async fn community(&self, gid: &str, general: &str, extra: Value) {
        let (s, e) = self
            .patch(
                OWNER,
                &format!("/guilds/{gid}"),
                json!({ "verification_level": 1, "explicit_content_filter": 2 }),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{e}");
        let mut body = json!({
            "community_enabled": true,
            "rules_channel_id": general,
            "public_updates_channel_id": general,
        });
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().cloned().unwrap_or_default());
        let (s, e) = self
            .patch(OWNER, &format!("/guilds/{gid}/sr/community"), body)
            .await;
        assert_eq!(s, StatusCode::OK, "{e}");
    }

    /// `user` joins through a fresh invite of the server's `#general`.
    async fn join(&self, gid: &str, general: &str, user: i64) -> (StatusCode, Value) {
        let (s, inv) = self
            .post(OWNER, &format!("/channels/{general}/invites"), json!({}))
            .await;
        assert_eq!(s, StatusCode::OK, "{inv}");
        let code = inv["code"].as_str().unwrap().to_owned();
        let _ = gid;
        self.post(user, &format!("/invites/{code}"), json!({}))
            .await
    }
}

async fn next_event(s: &mut BoxStream<'static, DomainEvent>) -> Option<DomainEvent> {
    tokio::time::timeout(Duration::from_millis(200), s.next())
        .await
        .ok()
        .flatten()
}

// ------------------------------------------------------------------ LNG-GLD-011

#[tokio::test]
async fn server_tag_is_shown_next_to_members_who_opt_in() {
    // One implementation of server tags survives the phase-10 integration: the
    // `guild_tags` / `user_primary_guilds` one (LNG-SSET-011/012). The lounge surface
    // (`primary_guild` on member user references, LNG-GLD-011) reads from it.
    let w = world().await;
    let (gid, _) = w.guild("Club House").await;
    let tag = format!("/guilds/{gid}/tag");

    // The duplicate lounge routes and guild fields are gone (`tag-badges` now only matches
    // `/guilds/{guild_id}` and is not a snowflake).
    assert_eq!(
        w.get(BOB, "/guilds/tag-badges").await.0,
        StatusCode::BAD_REQUEST
    );
    let (s, _) = w
        .patch(
            BOB,
            "/users/@me/primary-guild",
            json!({ "identity_guild_id": gid }),
        )
        .await;
    assert_eq!(s, StatusCode::METHOD_NOT_ALLOWED);
    let (s, g) = w
        .patch(OWNER, &format!("/guilds/{gid}"), json!({ "tag": "SR1" }))
        .await;
    assert_eq!(s, StatusCode::OK, "{g}");
    assert!(
        g.get("tag").is_none() && g.get("tag_badge").is_none(),
        "{g}"
    );

    // Managers set the tag; members adopt it.
    let (s, _) = w.put(BOB, &tag, json!({ "tag": "SR1" })).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, t) = w
        .put(
            OWNER,
            &tag,
            json!({ "tag": "SR1", "badge": 8, "color": 0x00AA_33FF }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{t}");
    let (_, m) = w.get(OWNER, &format!("/guilds/{gid}/members/{BOB}")).await;
    assert!(m["user"].get("primary_guild").is_none(), "{m}");
    let (s, u) = w
        .put(BOB, "/users/@me/primary-guild", json!({ "guild_id": gid }))
        .await;
    assert_eq!(s, StatusCode::OK, "{u}");
    assert_eq!(u["primary_guild"]["tag"], "SR1");
    assert_eq!(u["primary_guild"]["badge"], 8);
    let (s, _) = w
        .put(
            CAROL,
            "/users/@me/primary-guild",
            json!({ "guild_id": gid }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "carol is not a member");

    // Member references carry what the user lookup resolves (on Postgres that is the
    // `guild_tags` join, see sr-db `server_tag_adoption_shows_only_for_members_while_allowed`).
    w.repo
        .store
        .lock()
        .unwrap()
        .users
        .get_mut(&UserId::new(BOB as u64))
        .unwrap()
        .primary_guild = Some(sr_chat::settings::tags::PrimaryGuild {
        guild_id: GuildId::new(gid.parse().unwrap()),
        tag: "SR1".into(),
        badge: 8,
        color: 0x00AA_33FF,
    });
    let (_, m) = w.get(OWNER, &format!("/guilds/{gid}/members/{BOB}")).await;
    let pg = &m["user"]["primary_guild"];
    assert_eq!(
        (
            &pg["identity_guild_id"],
            &pg["tag"],
            &pg["badge"],
            &pg["color"]
        ),
        (&json!(gid), &json!("SR1"), &json!(8), &json!(0x00AA_33FF)),
        "{m}"
    );
    assert_eq!(pg["identity_enabled"], true);
}

// ------------------------------------------------------------------ LNG-GLD-012

#[tokio::test]
async fn members_edit_their_own_per_server_profile() {
    let w = world().await;
    let (gid, _) = w.guild("Club House").await;
    let uri = format!("/guilds/{gid}/members/@me");

    let (s, m) = w
        .patch(
            BOB,
            &uri,
            json!({ "nick": "Bobby", "bio": " hello there ", "avatar": PNG, "banner": PNG }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert_eq!(
        (m["nick"].as_str(), m["bio"].as_str()),
        (Some("Bobby"), Some("hello there"))
    );
    let avatar = m["avatar"].as_str().unwrap().to_owned();
    let banner = m["banner"].as_str().unwrap().to_owned();
    assert!(
        w.storage
            .get(&format!("avatars/{BOB}/{avatar}"))
            .await
            .is_ok()
    );
    assert!(
        w.storage
            .get(&format!("avatars/{BOB}/{banner}"))
            .await
            .is_ok()
    );

    // Everybody sees it on the member object, and bob can read his own.
    let (_, seen) = w.get(OWNER, &format!("/guilds/{gid}/members/{BOB}")).await;
    assert_eq!(seen["avatar"], m["avatar"]);
    assert_eq!(seen["bio"], "hello there");
    let (s, mine) = w.get(BOB, &format!("/users/@me/guilds/{gid}/member")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(mine["nick"], "Bobby");

    // The same picture in another server is a different object.
    let (other, _) = w.guild("Second").await;
    let (s, m2) = w
        .patch(
            BOB,
            &format!("/guilds/{other}/members/@me"),
            json!({ "avatar": PNG }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{m2}");
    assert_ne!(m2["avatar"], m["avatar"]);

    // Replacing or clearing deletes the old object but not the other server's.
    let (s, m3) = w
        .patch(BOB, &uri, json!({ "avatar": null, "bio": null }))
        .await;
    assert_eq!(s, StatusCode::OK, "{m3}");
    assert!(m3["avatar"].is_null() && m3.get("bio").is_none(), "{m3}");
    assert!(
        w.storage
            .get(&format!("avatars/{BOB}/{avatar}"))
            .await
            .is_err()
    );
    let other_avatar = m2["avatar"].as_str().unwrap();
    assert!(
        w.storage
            .get(&format!("avatars/{BOB}/{other_avatar}"))
            .await
            .is_ok()
    );
    assert_eq!(m3["banner"], m["banner"], "untouched fields stay");

    // Bad input fails the whole edit.
    let (s, e) = w.patch(BOB, &uri, json!({ "bio": "x".repeat(191) })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    let (s, e) = w
        .patch(BOB, &uri, json!({ "nick": "ok", "avatar": "not an image" }))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    let (_, again) = w.get(BOB, &format!("/users/@me/guilds/{gid}/member")).await;
    assert_eq!(again["nick"], "Bobby", "nothing was written");
    // Outsiders have no profile here.
    let (s, _) = w.patch(CAROL, &uri, json!({ "bio": "hi" })).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn members_set_per_server_pronouns_theme_effect_and_decoration() {
    let w = world().await;
    let (gid, _) = w.guild("Club House").await;
    let uri = format!("/guilds/{gid}/members/@me");
    let (s, m) = w
        .patch(
            BOB,
            &uri,
            json!({
                "pronouns": " they/them ", "theme_colors": [0x00AA_BBCC, 0x0011_2233],
                "profile_effect": "aurora",
            }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert_eq!(m["pronouns"], "they/them");
    assert_eq!(m["theme_colors"], json!([0x00AA_BBCC, 0x0011_2233]));
    assert_eq!(m["profile_effect"], "aurora");
    // Everyone sees them on the member object.
    let (_, seen) = w.get(OWNER, &format!("/guilds/{gid}/members/{BOB}")).await;
    assert_eq!(seen["pronouns"], "they/them");
    // Bad values fail the edit; null clears.
    for bad in [
        json!({ "pronouns": "x".repeat(41) }),
        json!({ "theme_colors": [0x0100_0000, 1] }),
        json!({ "profile_effect": "Not A Slug" }),
        // Well formed but not in the profile catalogue (G-48).
        json!({ "profile_effect": "neon-halo" }),
        json!({ "avatar_decoration": "halo-1" }),
    ] {
        let (s, e) = w.patch(BOB, &uri, bad).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    }
    let (s, m) = w
        .patch(BOB, &uri, json!({ "pronouns": null, "theme_colors": null }))
        .await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert!(
        m.get("pronouns").is_none() && m.get("theme_colors").is_none(),
        "{m}"
    );
    assert_eq!(m["profile_effect"], "aurora", "untouched fields stay");
}

// ------------------------------------------------------------------ LNG-INV-004

#[tokio::test]
async fn vanity_url_works_like_a_free_invite_that_never_expires() {
    let w = world().await;
    let (gid, _) = w.guild("Club House").await;
    let uri = format!("/guilds/{gid}/vanity-url");

    let (s, v) = w.get(OWNER, &uri).await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["code"].is_null() && v["uses"] == 0, "{v}");
    assert_eq!(w.get(BOB, &uri).await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        w.patch(BOB, &uri, json!({ "code": "club" })).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        w.get(CAROL, "/invites/club-house").await.0,
        StatusCode::NOT_FOUND
    );

    for bad in ["ab", "has space", "-lead", "login"] {
        let (s, e) = w.patch(OWNER, &uri, json!({ "code": bad })).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}: {e}");
    }
    let (s, v) = w.patch(OWNER, &uri, json!({ "code": "club-house" })).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["code"], "club-house");
    let (_, g) = w.get(OWNER, &format!("/guilds/{gid}")).await;
    assert_eq!(g["vanity_url_code"], "club-house");

    // Codes are unique, whatever the case, and may not shadow a real invite.
    let (second, second_general) = w.guild("Other").await;
    let (s, e) = w
        .patch(
            OWNER,
            &format!("/guilds/{second}/vanity-url"),
            json!({ "code": "CLUB-house" }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    let (_, inv) = w
        .post(
            OWNER,
            &format!("/channels/{second_general}/invites"),
            json!({}),
        )
        .await;
    let taken = inv["code"].as_str().unwrap();
    let (s, e) = w.patch(OWNER, &uri, json!({ "code": taken })).await;
    assert_eq!(
        s,
        StatusCode::BAD_REQUEST,
        "an invite already has this text: {e}"
    );

    // Any member can look it up; the invite object carries the splash and the code.
    let (s, preview) = w.get(CAROL, "/invites/club-house?with_counts=true").await;
    assert_eq!(s, StatusCode::OK, "{preview}");
    assert_eq!(preview["guild"]["vanity_url_code"], "club-house");
    assert!(preview["guild"].get("splash").is_some());
    assert_eq!(preview["max_age"], 0);
    assert_eq!(preview["approximate_member_count"], 2);
    // Joining uses no invite row, counts the use and never runs out.
    let (s, joined) = w.post(CAROL, "/invites/club-house", json!({})).await;
    assert_eq!(s, StatusCode::OK, "{joined}");
    assert!(w.is_member(&gid, CAROL));
    assert_eq!(joined["uses"], 1);
    let (s, joined) = w.post(DAVE, "/invites/Club-House", json!({})).await;
    assert_eq!(s, StatusCode::OK, "{joined}");
    assert_eq!(joined["uses"], 2);
    let (_, v) = w.get(OWNER, &uri).await;
    assert_eq!(v["uses"], 2);
    // It is not a stored invite: it cannot be revoked as one.
    assert_eq!(
        w.delete(OWNER, "/invites/club-house").await.0,
        StatusCode::NOT_FOUND
    );

    // Releasing it frees the code.
    let (s, v) = w.patch(OWNER, &uri, json!({ "code": null })).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert!(v["code"].is_null());
    assert_eq!(
        w.get(CAROL, "/invites/club-house").await.0,
        StatusCode::NOT_FOUND
    );
    let (s, v) = w
        .patch(
            OWNER,
            &format!("/guilds/{second}/vanity-url"),
            json!({ "code": "club-house" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
}

// ------------------------------------------------------------------ LNG-GLD-008

#[tokio::test]
async fn membership_screening_holds_new_members_until_they_agree() {
    let w = world().await;
    let (gid, general) = w.guild("Club House").await;
    let g = format!("/guilds/{gid}");

    // Screening needs rules; it only applies to a Community.
    let (s, e) = w
        .patch(
            OWNER,
            &format!("{g}/sr/community"),
            json!({ "screening_enabled": true }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "no rules, no screening: {e}");
    let (s, v) = w.get(BOB, &format!("{g}/member-verification")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["enabled"], false);
    w.community(
        &gid,
        &general,
        json!({
            "screening_enabled": true,
            "screening_description": "Welcome!",
            "screening_rules": [" Be kind ", "", "No spam"],
        }),
    )
    .await;
    let (_, c) = w.get(OWNER, &format!("{g}/sr/community")).await;
    assert_eq!(c["screening_rules"], json!(["Be kind", "No spam"]));

    // A new member is pending and sees the form.
    let (s, _) = w.join(&gid, &general, CAROL).await;
    assert_eq!(s, StatusCode::OK);
    let (_, m) = w.get(OWNER, &format!("{g}/members/{CAROL}")).await;
    assert_eq!(m["pending"], true, "{m}");
    let (s, form) = w.get(CAROL, &format!("{g}/member-verification")).await;
    assert_eq!(s, StatusCode::OK, "{form}");
    assert_eq!(form["enabled"], true);
    assert_eq!(form["description"], "Welcome!");
    assert_eq!(form["form_fields"][0]["field_type"], "TERMS");
    assert_eq!(
        form["form_fields"][0]["values"],
        json!(["Be kind", "No spam"])
    );

    // Not agreeing keeps them pending; agreeing releases them (and announces it).
    let mut events = w.events(&gid);
    let (s, e) = w
        .put(
            CAROL,
            &format!("{g}/requests/@me"),
            json!({ "agreed": false }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    let (s, m) = w
        .put(
            CAROL,
            &format!("{g}/requests/@me"),
            json!({ "agreed": true }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert_eq!(m["pending"], false);
    let mut updated = false;
    while let Some(ev) = next_event(&mut events).await {
        updated |= matches!(ev, DomainEvent::MemberUpdated { user, .. } if user == UserId::new(CAROL as u64));
    }
    assert!(updated, "GUILD_MEMBER_UPDATE after accepting");
    // Accepting again is harmless; an outsider cannot accept for a server they are not in.
    assert_eq!(
        w.put(CAROL, &format!("{g}/requests/@me"), json!({}))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        w.put(DAVE, &format!("{g}/requests/@me"), json!({})).await.0,
        StatusCode::FORBIDDEN
    );

    // Switching Community off switches screening off with it.
    let (s, c) = w
        .patch(
            OWNER,
            &format!("{g}/sr/community"),
            json!({ "community_enabled": false }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{c}");
    assert_eq!(c["screening_enabled"], false);
    let (_, m) = w.join(&gid, &general, DAVE).await;
    let _ = m;
    let (_, d) = w.get(OWNER, &format!("{g}/members/{DAVE}")).await;
    assert_eq!(d["pending"], false, "{d}");
}

#[tokio::test]
async fn onboarding_answers_grant_safe_roles_and_unlock_channels() {
    let w = world().await;
    let (gid, general) = w.guild("Club House").await;
    let g = format!("/guilds/{gid}");
    let role = |name: &str, perms: &str| json!({ "name": name, "permissions": perms });
    let (_, gamer) = w
        .post(OWNER, &format!("{g}/roles"), role("gamer", "0"))
        .await;
    // ADMINISTRATOR = 8, KICK_MEMBERS = 2.
    let (_, boss) = w
        .post(OWNER, &format!("{g}/roles"), role("boss", "8"))
        .await;
    let gamer_id = gamer["id"].as_str().unwrap().to_owned();
    let boss_id = boss["id"].as_str().unwrap().to_owned();
    let (_, side) = w
        .post(
            OWNER,
            &format!("{g}/channels"),
            json!({ "name": "games", "type": 0 }),
        )
        .await;
    let games = side["id"].as_str().unwrap().to_owned();

    // The form: a required single-select question.
    let (s, o) = w
        .put(
            OWNER,
            &format!("{g}/onboarding"),
            json!({
                "enabled": false,
                "default_channel_ids": [general],
                "prompts": [{
                    "title": "What do you play?",
                    "single_select": true,
                    "required": true,
                    "in_onboarding": true,
                    "options": [
                        { "title": "Games", "channel_ids": [games], "role_ids": [gamer_id, boss_id] },
                        { "title": "Nothing" },
                    ],
                }],
            }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{o}");
    let opt_games = o["prompts"][0]["options"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let opt_none = o["prompts"][0]["options"][1]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Onboarding has to be on.
    let answer = format!("{g}/onboarding-responses");
    let (s, e) = w
        .post(BOB, &answer, json!({ "option_ids": [opt_games] }))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "off: {e}");
    let (s, _) = w
        .patch(
            OWNER,
            &format!("{g}/sr/community"),
            json!({ "onboarding_enabled": true }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);

    // Bad answers.
    for (who, body) in [
        (BOB, json!({ "option_ids": [] })),
        (BOB, json!({ "option_ids": ["999"] })),
        (BOB, json!({ "option_ids": [opt_games, opt_none] })),
        (BOB, json!({ "option_ids": ["x"] })),
    ] {
        let (s, e) = w.post(who, &answer, body.clone()).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{body}: {e}");
    }
    assert_eq!(
        w.post(CAROL, &answer, json!({ "option_ids": [opt_games] }))
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    // A good answer grants the harmless role, never the powerful one, and lists channels.
    let (s, r) = w
        .post(BOB, &answer, json!({ "option_ids": [opt_games] }))
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["role_ids"], json!([gamer_id]));
    assert_eq!(r["channel_ids"], json!([general, games]));
    let (_, m) = w.get(OWNER, &format!("{g}/members/{BOB}")).await;
    assert_eq!(m["roles"], json!([gamer_id]));
    // Answering again changes nothing more.
    let (_, r) = w
        .post(BOB, &answer, json!({ "option_ids": [opt_games] }))
        .await;
    assert_eq!(r["role_ids"], json!([]));
}

// ------------------------------------------------------------------ LNG-INV-005

#[tokio::test]
async fn temporary_members_leave_when_they_go_offline_unless_they_hold_a_role() {
    let w = world().await;
    let (gid, general) = w.guild("Club House").await;
    let g = format!("/guilds/{gid}");
    let (_, role) = w
        .post(OWNER, &format!("{g}/roles"), json!({ "name": "regular" }))
        .await;
    let role_id: sr_core::RoleId = role["id"].as_str().unwrap().parse().unwrap();

    let invite = |temporary: bool| {
        let general = general.clone();
        let w = &w;
        async move {
            let (s, inv) = w
                .post(
                    OWNER,
                    &format!("/channels/{general}/invites"),
                    json!({ "temporary": temporary }),
                )
                .await;
            assert_eq!(s, StatusCode::OK, "{inv}");
            assert_eq!(inv["temporary"], temporary);
            inv["code"].as_str().unwrap().to_owned()
        }
    };
    let temp = invite(true).await;
    let perm = invite(false).await;
    assert_eq!(
        w.post(CAROL, &format!("/invites/{temp}"), json!({}))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        w.post(DAVE, &format!("/invites/{temp}"), json!({})).await.0,
        StatusCode::OK
    );
    // Bob is already in; a normal invite never marks anyone.
    assert_eq!(
        w.post(BOB, &format!("/invites/{perm}"), json!({})).await.0,
        StatusCode::OK
    );
    // Dave earns a role before disconnecting and stays.
    w.repo
        .store
        .lock()
        .unwrap()
        .members
        .iter_mut()
        .find(|m| m.user_id == UserId::new(DAVE as u64))
        .unwrap()
        .role_ids
        .push(role_id);

    let mut events = w.events(&gid);
    let gone = w.joiner.drop_temporary(UserId::new(CAROL as u64)).await;
    assert_eq!(gone, vec![GuildId::new(gid.parse().unwrap())]);
    assert!(!w.is_member(&gid, CAROL));
    assert!(matches!(
        next_event(&mut events).await,
        Some(DomainEvent::MemberLeft { user, .. }) if user == UserId::new(CAROL as u64)
    ));
    assert!(
        w.joiner
            .drop_temporary(UserId::new(DAVE as u64))
            .await
            .is_empty()
    );
    assert!(
        w.is_member(&gid, DAVE),
        "a role makes the membership permanent"
    );
    assert!(
        w.joiner
            .drop_temporary(UserId::new(BOB as u64))
            .await
            .is_empty()
    );
    assert!(w.is_member(&gid, BOB));
    assert!(w.settings.with(|s| {
        s.temporary
            .contains(&(GuildId::new(gid.parse().unwrap()), UserId::new(DAVE as u64)))
    }));
}

// ------------------------------------------------------------------ LNG-GLD-013

#[tokio::test]
async fn server_directory_listing_approval_search_and_join() {
    let w = world().await;
    let (gid, general) = w.guild("Chess Club").await;
    let listing = format!("/guilds/{gid}/directory");

    // A server needs Community, a known category and MANAGE_GUILD.
    let (s, e) = w
        .put(OWNER, &listing, json!({ "category": "gaming" }))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "no community: {e}");
    w.community(&gid, &general, json!({})).await;
    assert_eq!(
        w.put(BOB, &listing, json!({ "category": "gaming" }))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let (s, e) = w.put(OWNER, &listing, json!({ "category": "nope" })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    assert_eq!(w.get(OWNER, &listing).await.0, StatusCode::NOT_FOUND);

    let (s, l) = w
        .put(
            OWNER,
            &listing,
            json!({ "category": "gaming", "description": "Boards and clocks" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{l}");
    assert_eq!(
        (l["status"].as_str(), l["category"].as_str()),
        (Some("pending"), Some("gaming"))
    );
    assert_eq!(
        w.get(BOB, &listing).await.0,
        StatusCode::FORBIDDEN,
        "listing state is for managers"
    );

    // Pending listings are invisible to members; categories are listed.
    let (s, cats) = w.get(CAROL, "/directory/categories").await;
    assert_eq!(s, StatusCode::OK);
    assert!(cats.as_array().unwrap().iter().any(|c| c == "gaming"));
    assert_eq!(w.get(CAROL, "/directory").await.1, json!([]));
    assert_eq!(
        w.get(CAROL, &format!("/directory/{gid}")).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        w.post(CAROL, &format!("/directory/{gid}/join"), json!({}))
            .await
            .0,
        StatusCode::NOT_FOUND
    );

    // Staff: only MANAGE_DIRECTORY works the queue.
    assert_eq!(
        w.get(BASIC, "/admin/directory").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        w.get(OWNER, "/admin/directory").await.0,
        StatusCode::FORBIDDEN
    );
    let (s, queue) = w.get(STAFF, "/admin/directory").await;
    assert_eq!(s, StatusCode::OK, "{queue}");
    assert_eq!(queue[0]["guild_id"], gid);
    assert_eq!(queue[0]["guild_name"], "Chess Club");
    assert_eq!(queue[0]["community_enabled"], true);
    assert_eq!(
        w.get(STAFF, "/admin/directory?status=bogus").await.0,
        StatusCode::BAD_REQUEST
    );
    let decide = |body: Value| {
        let w = &w;
        let gid = gid.clone();
        async move {
            w.patch(STAFF, &format!("/admin/directory/{gid}"), body)
                .await
        }
    };
    assert_eq!(
        decide(json!({ "decision": "maybe" })).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        w.patch(
            BASIC,
            &format!("/admin/directory/{gid}"),
            json!({ "decision": "approve" })
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (s, d) = decide(json!({ "decision": "reject", "note": "add a description" })).await;
    assert_eq!(s, StatusCode::OK, "{d}");
    assert_eq!(
        (d["status"].as_str(), d["decision_note"].as_str()),
        (Some("rejected"), Some("add a description"))
    );
    let (_, l) = w.get(OWNER, &listing).await;
    assert_eq!(l["decision_note"], "add a description");
    assert_eq!(
        decide(json!({ "decision": "remove" })).await.0,
        StatusCode::BAD_REQUEST,
        "only approved ones"
    );

    // Asking again after a rejection, then approval and featuring.
    let (_, l) = w
        .put(
            OWNER,
            &listing,
            json!({ "category": "gaming", "description": "Boards and clocks" }),
        )
        .await;
    assert_eq!(l["status"], "pending");
    let (s, d) = decide(json!({ "decision": "approve", "featured": true })).await;
    assert_eq!(s, StatusCode::OK, "{d}");
    assert_eq!(
        (d["status"].as_str(), d["featured"].as_bool()),
        (Some("approved"), Some(true))
    );

    // Members find it, with the member count; filters and text search work.
    let (s, found) = w.get(CAROL, "/directory?query=boards").await;
    assert_eq!(s, StatusCode::OK, "{found}");
    assert_eq!(found.as_array().unwrap().len(), 1);
    assert_eq!(found[0]["id"], gid);
    assert_eq!(found[0]["name"], "Chess Club");
    assert_eq!(found[0]["description"], "Boards and clocks");
    assert_eq!(found[0]["category"], "gaming");
    assert_eq!(found[0]["featured"], true);
    assert_eq!(found[0]["approximate_member_count"], 2);
    assert_eq!(found[0]["joined"], false);
    assert_eq!(
        w.get(CAROL, "/directory?query=chess+clocks")
            .await
            .1
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(w.get(CAROL, "/directory?query=poker").await.1, json!([]));
    assert_eq!(w.get(CAROL, "/directory?category=music").await.1, json!([]));
    assert_eq!(
        w.get(CAROL, "/directory?category=bogus").await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(w.get(BOB, "/directory").await.1[0]["joined"], true);
    assert_eq!(
        w.get(CAROL, &format!("/directory/{gid}")).await.0,
        StatusCode::OK
    );

    // Joining from the directory needs no invite and respects bans.
    let (s, m) = w
        .post(CAROL, &format!("/directory/{gid}/join"), json!({}))
        .await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert!(w.is_member(&gid, CAROL));
    let (s, again) = w
        .post(CAROL, &format!("/directory/{gid}/join"), json!({}))
        .await;
    assert_eq!(s, StatusCode::OK, "{again}");
    assert_eq!(
        w.put(OWNER, &format!("/guilds/{gid}/bans/{DAVE}"), json!({}))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    // The settings store keeps its own copy of the bans in this in-memory setup.
    w.settings.with(|s| {
        s.bans
            .insert((GuildId::new(gid.parse().unwrap()), UserId::new(DAVE as u64)))
    });
    assert_eq!(
        w.post(DAVE, &format!("/directory/{gid}/join"), json!({}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    // Editing the blurb sends an approved listing back to review and hides it.
    let (_, l) = w
        .put(
            OWNER,
            &listing,
            json!({ "category": "gaming", "description": "New text" }),
        )
        .await;
    assert_eq!(l["status"], "pending");
    assert_eq!(w.get(BOB, "/directory").await.1, json!([]));
    decide(json!({ "decision": "approve" })).await;
    assert_eq!(
        w.get(BOB, "/directory").await.1.as_array().unwrap().len(),
        1
    );
    // Taking Community away hides it too; the manager can withdraw.
    let (s, _) = w
        .patch(
            OWNER,
            &format!("/guilds/{gid}/sr/community"),
            json!({ "community_enabled": false }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(w.get(BOB, "/directory").await.1, json!([]));
    assert_eq!(w.delete(BOB, &listing).await.0, StatusCode::FORBIDDEN);
    assert_eq!(w.delete(OWNER, &listing).await.0, StatusCode::NO_CONTENT);
    assert_eq!(w.delete(OWNER, &listing).await.0, StatusCode::NOT_FOUND);
    // Staff can take down an approved listing.
    w.community(&gid, &general, json!({})).await;
    w.put(OWNER, &listing, json!({ "category": "music" })).await;
    decide(json!({ "decision": "approve" })).await;
    let (s, d) = decide(json!({ "decision": "remove", "note": "spam" })).await;
    assert_eq!(s, StatusCode::OK, "{d}");
    assert_eq!(d["status"], "removed");
    assert_eq!(w.get(BOB, "/directory").await.1, json!([]));
}

#[tokio::test]
async fn hidden_directory_categories_are_not_offered_or_accepted() {
    let w = world().await;
    let (gid, general) = w.guild("Chess Club").await;
    w.community(&gid, &general, json!({})).await;
    let listing = format!("/guilds/{gid}/directory");
    let hide = |cats: Value| {
        w.features.set_settings(
            sr_features::HIDDEN_CATEGORIES_ID,
            &sr_features::ScopeRef::Platform,
            json!({ "categories": cats }).as_object().unwrap().clone(),
        )
    };
    hide(json!(["gaming"])).await.unwrap();
    let (_, cats) = w.get(CAROL, "/directory/categories").await;
    assert!(!cats.as_array().unwrap().iter().any(|c| c == "gaming"));
    assert!(cats.as_array().unwrap().iter().any(|c| c == "music"));
    let (s, e) = w
        .put(OWNER, &listing, json!({ "category": "gaming" }))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    assert_eq!(
        w.get(CAROL, "/directory?category=gaming").await.0,
        StatusCode::BAD_REQUEST
    );

    // A listing made before the category was hidden stops showing.
    hide(json!([])).await.unwrap();
    assert_eq!(
        w.put(OWNER, &listing, json!({ "category": "gaming" }))
            .await
            .0,
        StatusCode::OK
    );
    let (s, _) = w
        .patch(
            STAFF,
            &format!("/admin/directory/{gid}"),
            json!({ "decision": "approve" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        w.get(CAROL, "/directory").await.1.as_array().unwrap().len(),
        1
    );
    hide(json!(["gaming"])).await.unwrap();
    assert_eq!(w.get(CAROL, "/directory").await.1, json!([]));
    assert_eq!(
        w.get(CAROL, &format!("/directory/{gid}")).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        w.post(CAROL, &format!("/directory/{gid}/join"), json!({}))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

/// Owner, Bob and Carol in one group DM; returns its id.
async fn group_dm(w: &World) -> String {
    for other in [BOB, CAROL] {
        let (a, b) = (UserId::new(OWNER as u64), UserId::new(other as u64));
        w.social.service.send_friend_request(a, b).await.unwrap();
        w.social.service.accept_friend_request(b, a).await.unwrap();
    }
    let (s, g) = w
        .post(
            OWNER,
            "/users/@me/channels",
            json!({ "recipients": [BOB.to_string(), CAROL.to_string()], "name": "crew" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{g}");
    g["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn group_dm_invites_create_resolve_join_and_revoke() {
    let w = world().await;
    let dm = group_dm(&w).await;
    let mut dave_events = w.bus.subscribe(Topic::User(UserId::new(DAVE as u64)));

    // Only the group owner makes links.
    let (s, _) = w
        .post(BOB, &format!("/channels/{dm}/invites"), json!({}))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, inv) = w
        .post(OWNER, &format!("/channels/{dm}/invites"), json!({}))
        .await;
    assert_eq!(s, StatusCode::OK, "{inv}");
    assert_eq!(inv["type"], 1);
    assert!(inv["guild"].is_null(), "no server behind a group link");
    assert!(
        inv.as_object().unwrap().contains_key("guild"),
        "guild is null, not absent"
    );
    assert_eq!(inv["channel"]["id"], dm.as_str());
    assert_eq!(inv["channel"]["type"], 3);
    assert_eq!(inv["max_age"], 86_400);
    assert_eq!(inv["max_uses"], 0);
    assert_eq!(inv["approximate_member_count"], 3);
    let code = inv["code"].as_str().unwrap().to_owned();

    // The owner lists live links; a stranger resolves one, then joins, then joins again.
    let (_, list) = w.get(OWNER, &format!("/channels/{dm}/invites")).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    let (s, seen) = w
        .get(DAVE, &format!("/invites/{code}?with_counts=true"))
        .await;
    assert_eq!(s, StatusCode::OK, "{seen}");
    assert_eq!(seen["channel"]["name"], "crew");
    let (s, joined) = w.post(DAVE, &format!("/invites/{code}"), json!({})).await;
    assert_eq!(s, StatusCode::OK, "{joined}");
    assert_eq!(joined["approximate_member_count"], 4);
    let (s, again) = w.post(DAVE, &format!("/invites/{code}"), json!({})).await;
    assert_eq!(s, StatusCode::OK, "{again}");
    assert_eq!(again["approximate_member_count"], 4, "no second seat");
    // Dave heard about the group, and sees it in his list.
    let mut names = Vec::new();
    while let Ok(Some(ev)) =
        tokio::time::timeout(Duration::from_millis(50), dave_events.next()).await
    {
        if let DomainEvent::UserScoped { name, .. } = ev {
            names.push(name);
        }
    }
    assert!(names.contains(&"CHANNEL_CREATE".to_owned()), "{names:?}");
    let (_, mine) = w.get(DAVE, "/users/@me/channels").await;
    assert!(
        mine.as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == dm.as_str()),
        "{mine}"
    );

    // Revoking is the owner's call; afterwards the link is gone for everybody.
    assert_eq!(
        w.delete(BOB, &format!("/invites/{code}")).await.0,
        StatusCode::FORBIDDEN
    );
    let (s, gone) = w.delete(OWNER, &format!("/invites/{code}")).await;
    assert_eq!(s, StatusCode::OK, "{gone}");
    assert_eq!(
        w.get(CAROL, &format!("/invites/{code}")).await.0,
        StatusCode::NOT_FOUND
    );
    let (_, list) = w.get(OWNER, &format!("/channels/{dm}/invites")).await;
    assert_eq!(list, json!([]));
    assert_eq!(
        w.post(CAROL, "/invites/zzzzzzzz", json!({})).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn group_dm_icon_is_stored_through_the_media_pipeline() {
    let w = world().await;
    let dm = group_dm(&w).await;
    let (s, g) = w
        .patch(
            BOB,
            &format!("/users/@me/channels/{dm}"),
            json!({ "icon": PNG }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{g}");
    let hash = g["icon"].as_str().unwrap().to_owned();
    assert_eq!(hash.len(), 32, "a content hash, not the upload");
    let key = format!("channel-icons/{dm}/{hash}");
    assert!(w.storage.get(&key).await.is_ok(), "stored under {key}");

    // Anything that is not an image data URI is refused, and nothing changes.
    for bad in [json!("not-a-uri"), json!("data:text/plain;base64,aGk=")] {
        let (s, _) = w
            .patch(
                BOB,
                &format!("/users/@me/channels/{dm}"),
                json!({ "icon": bad }),
            )
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }
    let (_, back) = w.get(BOB, "/users/@me/channels").await;
    let me = back
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == dm.as_str())
        .unwrap();
    assert_eq!(me["icon"], hash.as_str());

    // null removes the icon and deletes the stored object.
    let (s, g3) = w
        .patch(
            OWNER,
            &format!("/users/@me/channels/{dm}"),
            json!({ "icon": null }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{g3}");
    assert!(g3["icon"].is_null());
    assert!(
        w.storage.get(&key).await.is_err(),
        "the old icon was removed"
    );
}

#[tokio::test]
async fn server_safety_fields_widget_and_mfa_level() {
    let w = world().await;
    let (gid, general) = w.guild("Safe Club").await;
    // The new fields are on the server object and default to off.
    let (_, g) = w.get(OWNER, &format!("/guilds/{gid}")).await;
    assert_eq!(
        (
            g["mfa_level"].as_u64(),
            g["system_channel_flags"].as_u64(),
            g["widget_enabled"].as_bool()
        ),
        (Some(0), Some(0), Some(false))
    );

    // system_channel_flags: PATCH with a manager; undefined bits are refused.
    let (s, g) = w
        .patch(
            OWNER,
            &format!("/guilds/{gid}"),
            json!({ "system_channel_flags": 5 }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{g}");
    assert_eq!(g["system_channel_flags"], 5);
    assert_eq!(
        w.patch(
            OWNER,
            &format!("/guilds/{gid}"),
            json!({ "system_channel_flags": 64 })
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        w.patch(
            BOB,
            &format!("/guilds/{gid}"),
            json!({ "system_channel_flags": 1 })
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    // Widget settings: any member reads, a manager writes, the channel must be the server's.
    let (s, wd) = w.get(BOB, &format!("/guilds/{gid}/widget")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(wd, json!({ "enabled": false, "channel_id": null }));
    let (s, wd) = w
        .patch(
            OWNER,
            &format!("/guilds/{gid}/widget"),
            json!({ "enabled": true, "channel_id": general }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{wd}");
    assert_eq!(wd, json!({ "enabled": true, "channel_id": general }));
    assert_eq!(
        w.patch(
            OWNER,
            &format!("/guilds/{gid}/widget"),
            json!({ "channel_id": "424242" })
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        w.patch(
            BOB,
            &format!("/guilds/{gid}/widget"),
            json!({ "enabled": false })
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    // The 2FA requirement is the owner's alone.
    assert_eq!(
        w.post(BOB, &format!("/guilds/{gid}/mfa"), json!({ "level": 1 }))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        w.post(OWNER, &format!("/guilds/{gid}/mfa"), json!({ "level": 2 }))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    // Switching it on needs the owner to have a second factor of their own.
    let (s, m) = w
        .post(OWNER, &format!("/guilds/{gid}/mfa"), json!({ "level": 1 }))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{m}");
    w.enrol(OWNER).await;
    let (s, m) = w
        .post(OWNER, &format!("/guilds/{gid}/mfa"), json!({ "level": 1 }))
        .await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert_eq!(m, json!({ "level": 1 }));
    let (_, g) = w.get(OWNER, &format!("/guilds/{gid}")).await;
    assert_eq!(g["mfa_level"], 1);
}

#[tokio::test]
async fn community_dm_and_spam_protection_fields_are_patchable() {
    let w = world().await;
    let (gid, _) = w.guild("Spam Club").await;
    let (s, c) = w
        .patch(
            OWNER,
            &format!("/guilds/{gid}/sr/community"),
            json!({ "dm_spam_filter": true, "explicit_media_filter_dms": true, "hide_suspicious_links": true, "warn_new_account_days": 7 }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{c}");
    assert_eq!(
        (
            c["dm_spam_filter"].as_bool(),
            c["warn_new_account_days"].as_u64()
        ),
        (Some(true), Some(7))
    );
    assert_eq!(
        w.patch(
            OWNER,
            &format!("/guilds/{gid}/sr/community"),
            json!({ "warn_new_account_days": 366 })
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        w.patch(
            BOB,
            &format!("/guilds/{gid}/sr/community"),
            json!({ "dm_spam_filter": false })
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (_, c) = w.get(OWNER, &format!("/guilds/{gid}/sr/community")).await;
    assert_eq!(c["hide_suspicious_links"], true);
}

// ---- scheduled events ----

fn at(offset_ms: i64) -> String {
    sr_api::routes::guilds::dto::iso8601(T0 + offset_ms)
}

const HOUR: i64 = 3_600_000;

async fn voice_channel(w: &World, gid: &str) -> String {
    let (_, chans) = w.get(OWNER, &format!("/guilds/{gid}/channels")).await;
    chans
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == 2)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn scheduled_events_validate_and_follow_discord_shapes() {
    let w = world().await;
    let (gid, general) = w.guild("Event Club").await;
    w.add_member(&gid, CAROL);
    let voice = voice_channel(&w, &gid).await;
    let path = format!("/guilds/{gid}/scheduled-events");
    let external = json!({
        "name": "Park meetup",
        "entity_type": 3,
        "entity_metadata": { "location": "the park" },
        "scheduled_start_time": at(HOUR),
        "scheduled_end_time": at(2 * HOUR),
        "privacy_level": 2,
    });
    // Everybody may create events by default (CREATE_EVENTS); the shape follows Discord.
    let (s, ev) = w.post(BOB, &path, external.clone()).await;
    assert_eq!(s, StatusCode::OK, "{ev}");
    assert_eq!(ev["status"], 1);
    assert_eq!(ev["entity_type"], 3);
    assert_eq!(ev["entity_metadata"]["location"], "the park");
    assert_eq!(ev["privacy_level"], 2);
    assert_eq!(ev["creator_id"], BOB.to_string());
    assert_eq!(ev["creator"]["username"], "bob");
    assert!(ev["channel_id"].is_null());
    assert_eq!(ev["user_count"], 0);

    // Validation: past start, ends before it starts, external without location or end time, a
    // channel on an external event, a text channel for a voice event, a bad privacy level.
    let bad = |patch: Value| {
        let mut b = external.clone();
        for (k, v) in patch.as_object().unwrap() {
            b[k] = v.clone();
        }
        b
    };
    for body in [
        bad(json!({ "scheduled_start_time": at(-HOUR) })),
        bad(json!({ "scheduled_end_time": at(HOUR) })),
        bad(json!({ "entity_metadata": null })),
        bad(json!({ "scheduled_end_time": null })),
        bad(json!({ "channel_id": voice })),
        bad(json!({ "privacy_level": 1 })),
        bad(json!({ "name": "  " })),
        bad(json!({ "scheduled_start_time": "tomorrow" })),
        bad(json!({ "entity_type": 9 })),
        json!({ "name": "Movie", "entity_type": 2, "channel_id": general, "scheduled_start_time": at(HOUR) }),
        json!({ "name": "Movie", "entity_type": 2, "scheduled_start_time": at(HOUR) }),
        json!({ "name": "Stage", "entity_type": 1, "channel_id": voice, "scheduled_start_time": at(HOUR) }),
    ] {
        let (s, e) = w.post(BOB, &path, body.clone()).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{body} -> {e}");
    }

    // A voice event in a voice channel.
    let (s, v) = w
        .post(
            CAROL,
            &path,
            json!({ "name": "Movie night", "entity_type": 2, "channel_id": voice, "scheduled_start_time": at(3 * HOUR) }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["channel_id"], voice.as_str());
    assert!(v["entity_metadata"].is_null());

    // The list is soonest first, counts on request, and outsiders see nothing.
    let (s, _) = w.get(DAVE, &path).await;
    assert_ne!(s, StatusCode::OK, "not a member");
    let (s, list) = w.get(BOB, &format!("{path}?with_user_count=true")).await;
    assert_eq!(s, StatusCode::OK);
    let names: Vec<_> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Park meetup", "Movie night"]);
    assert_eq!(list[0]["user_count"], 0);
    let (_, plain) = w.get(BOB, &path).await;
    assert!(plain[0].get("user_count").is_none());
}

#[tokio::test]
async fn scheduled_event_rsvp_status_and_permissions() {
    let w = world().await;
    let (gid, _) = w.guild("RSVP Club").await;
    w.add_member(&gid, CAROL);
    let mut bus = w.events(&gid);
    let path = format!("/guilds/{gid}/scheduled-events");
    let (_, ev) = w
        .post(
            BOB,
            &path,
            json!({
                "name": "Quiz", "entity_type": 3, "entity_metadata": { "location": "online" },
                "scheduled_start_time": at(HOUR), "scheduled_end_time": at(2 * HOUR)
            }),
        )
        .await;
    let eid = ev["id"].as_str().unwrap().to_owned();
    let one = format!("{path}/{eid}");

    // RSVP: put and delete are idempotent; counts and the list of users follow.
    for who in [CAROL, BOB] {
        assert_eq!(
            w.put(who, &format!("{one}/users/@me"), json!({})).await.0,
            StatusCode::NO_CONTENT
        );
    }
    assert_eq!(
        w.put(CAROL, &format!("{one}/users/@me"), json!({})).await.0,
        StatusCode::NO_CONTENT
    );
    let (_, got) = w.get(CAROL, &format!("{one}?with_user_count=true")).await;
    assert_eq!(got["user_count"], 2);
    assert_eq!(got["user_rsvp"], true);
    let (_, users) = w.get(BOB, &format!("{one}/users?with_member=true")).await;
    let ids: Vec<_> = users
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["user"]["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ids, [BOB.to_string(), CAROL.to_string()]);
    assert!(users[0]["member"]["joined_at"].is_string());
    let (_, after_bob) = w
        .get(BOB, &format!("{one}/users?after={BOB}&limit=1"))
        .await;
    assert_eq!(after_bob.as_array().unwrap().len(), 1);
    assert_eq!(
        w.delete(CAROL, &format!("{one}/users/@me")).await.0,
        StatusCode::NO_CONTENT
    );
    let (_, got) = w.get(CAROL, &format!("{one}?with_user_count=true")).await;
    assert_eq!(
        (got["user_count"].as_u64(), got["user_rsvp"].as_bool()),
        (Some(1), Some(false))
    );

    // Only the creator (CREATE_EVENTS) or a MANAGE_EVENTS holder changes or deletes it.
    assert_eq!(
        w.patch(CAROL, &one, json!({ "name": "Hijack" })).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(w.delete(CAROL, &one).await.0, StatusCode::FORBIDDEN);
    let (s, ev) = w.patch(BOB, &one, json!({ "name": "Pub quiz" })).await;
    assert_eq!(s, StatusCode::OK, "{ev}");
    assert_eq!(ev["name"], "Pub quiz");
    let (s, ev) = w
        .patch(OWNER, &one, json!({ "description": "Teams of four" }))
        .await;
    assert_eq!(s, StatusCode::OK, "{ev}");

    // Status: scheduled -> active -> completed; no skipping, nothing after the end.
    assert_eq!(
        w.patch(BOB, &one, json!({ "status": 3 })).await.0,
        StatusCode::BAD_REQUEST,
        "a scheduled event cannot end before it started"
    );
    let (s, ev) = w.patch(BOB, &one, json!({ "status": 2 })).await;
    assert_eq!((s, ev["status"].as_u64()), (StatusCode::OK, Some(2)));
    assert_eq!(
        w.patch(BOB, &one, json!({ "status": 4 })).await.0,
        StatusCode::BAD_REQUEST,
        "a running event cannot be canceled"
    );
    assert_eq!(
        w.patch(BOB, &one, json!({ "scheduled_start_time": at(5 * HOUR) }))
            .await
            .0,
        StatusCode::BAD_REQUEST,
        "a running event keeps its start"
    );
    let (s, ev) = w.patch(BOB, &one, json!({ "status": 3 })).await;
    assert_eq!((s, ev["status"].as_u64()), (StatusCode::OK, Some(3)));
    assert_eq!(
        w.patch(BOB, &one, json!({ "name": "again" })).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        w.put(CAROL, &format!("{one}/users/@me"), json!({})).await.0,
        StatusCode::BAD_REQUEST,
        "no RSVP after the end"
    );
    let (_, list) = w.get(BOB, &path).await;
    assert_eq!(list, json!([]), "ended events leave the list");
    assert_eq!(
        w.get(BOB, &one).await.1["status"],
        3,
        "but can still be read"
    );

    // Deleting removes it for good, and unknown ids answer 10070.
    assert_eq!(w.delete(OWNER, &one).await.0, StatusCode::NO_CONTENT);
    let (s, gone) = w.get(BOB, &one).await;
    assert_eq!(
        (s, gone["code"].as_u64()),
        (StatusCode::NOT_FOUND, Some(10070))
    );

    // The gateway heard about all of it, in order.
    let mut seen = Vec::new();
    while let Ok(Some(ev)) = tokio::time::timeout(Duration::from_millis(50), bus.next()).await {
        if let DomainEvent::ScheduledEvent { kind, .. } = ev {
            seen.push(kind.dispatch_name());
        }
    }
    assert_eq!(seen.first(), Some(&"GUILD_SCHEDULED_EVENT_CREATE"));
    assert!(seen.contains(&"GUILD_SCHEDULED_EVENT_USER_ADD"), "{seen:?}");
    assert!(
        seen.contains(&"GUILD_SCHEDULED_EVENT_USER_REMOVE"),
        "{seen:?}"
    );
    assert_eq!(seen.last(), Some(&"GUILD_SCHEDULED_EVENT_DELETE"));
    assert!(
        seen.iter()
            .filter(|n| **n == "GUILD_SCHEDULED_EVENT_UPDATE")
            .count()
            >= 3,
        "{seen:?}"
    );
}

#[tokio::test]
async fn scheduled_events_start_and_end_by_the_clock_and_are_audited() {
    let w = world().await;
    let (gid, _) = w.guild("Clock Club").await;
    let path = format!("/guilds/{gid}/scheduled-events");
    let (_, ev) = w
        .post(
            BOB,
            &path,
            json!({
                "name": "Stream party", "entity_type": 3, "entity_metadata": { "location": "online" },
                "scheduled_start_time": at(HOUR), "scheduled_end_time": at(2 * HOUR)
            }),
        )
        .await;
    let eid = ev["id"].as_str().unwrap().to_owned();
    sr_api::routes::settings::scheduled_events::advance(&w.settings_deps).await;
    assert_eq!(w.get(BOB, &format!("{path}/{eid}")).await.1["status"], 1);

    w.clock.advance_ms(HOUR + 1);
    sr_api::routes::settings::scheduled_events::advance(&w.settings_deps).await;
    assert_eq!(w.get(BOB, &format!("{path}/{eid}")).await.1["status"], 2);
    w.clock.advance_ms(HOUR);
    sr_api::routes::settings::scheduled_events::advance(&w.settings_deps).await;
    assert_eq!(w.get(BOB, &format!("{path}/{eid}")).await.1["status"], 3);

    // Create and update are in the audit log (action 100 and 101).
    let (_, log) = w.get(OWNER, &format!("/guilds/{gid}/audit-logs")).await;
    let actions: Vec<u64> = log["audit_log_entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["action_type"].as_u64())
        .collect();
    assert!(actions.contains(&100), "{log}");
}

#[tokio::test]
async fn group_dm_invite_is_refused_to_a_joiner_who_blocks_or_is_blocked_by_the_owner() {
    let w = world().await;
    let dm = group_dm(&w).await;
    let (_, inv) = w
        .post(OWNER, &format!("/channels/{dm}/invites"), json!({}))
        .await;
    let code = inv["code"].as_str().unwrap().to_owned();
    let (owner, dave) = (UserId::new(OWNER as u64), UserId::new(DAVE as u64));

    // The joiner blocked the owner: the link does not exist for them.
    w.social.service.block(dave, owner).await.unwrap();
    let (s, _) = w.post(DAVE, &format!("/invites/{code}"), json!({})).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // So does the other direction.
    w.social.service.unblock(dave, owner).await.unwrap();
    w.social.service.block(owner, dave).await.unwrap();
    let (s, _) = w.post(DAVE, &format!("/invites/{code}"), json!({})).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    // Nobody joined meanwhile; once the block is gone the same link works.
    w.social.service.unblock(owner, dave).await.unwrap();
    let (s, joined) = w.post(DAVE, &format!("/invites/{code}"), json!({})).await;
    assert_eq!(s, StatusCode::OK, "{joined}");
    assert_eq!(joined["approximate_member_count"], 4);
}

#[tokio::test]
async fn group_dm_invite_stops_at_ten_recipients() {
    let w = world().await;
    let dm = group_dm(&w).await;
    let (_, inv) = w
        .post(OWNER, &format!("/channels/{dm}/invites"), json!({}))
        .await;
    let code = inv["code"].as_str().unwrap().to_owned();
    // Seven more seats fill the group (owner, Bob, Carol and seven).
    let fillers: Vec<UserId> = (100..107).map(UserId::new).collect();
    let dm_id: ChannelId = dm.parse().unwrap();
    w.social
        .service
        .repo()
        .add_recipients(dm_id, &fillers)
        .await
        .unwrap();
    let (s, body) = w.post(DAVE, &format!("/invites/{code}"), json!({})).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{body}");
    let (_, mine) = w.get(DAVE, "/users/@me/channels").await;
    assert!(
        !mine
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == dm.as_str()),
        "no seat was taken"
    );
}

async fn stage_channel(w: &World, gid: &str) -> String {
    let (s, c) = w
        .post(
            OWNER,
            &format!("/guilds/{gid}/channels"),
            json!({ "name": "town-hall", "type": 13 }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{c}");
    c["id"].as_str().unwrap().to_owned()
}

async fn audit_actions(w: &World, gid: &str) -> Vec<u64> {
    let (_, log) = w.get(OWNER, &format!("/guilds/{gid}/audit-logs")).await;
    log["audit_log_entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["action_type"].as_u64())
        .collect()
}

#[tokio::test]
async fn event_cover_is_stored_replaced_and_removed_with_the_event() {
    let w = world().await;
    let (gid, _) = w.guild("Cover Club").await;
    let path = format!("/guilds/{gid}/scheduled-events");
    let body = |image: Value| {
        json!({
            "name": "Picnic", "entity_type": 3, "entity_metadata": { "location": "park" },
            "scheduled_start_time": at(HOUR), "scheduled_end_time": at(2 * HOUR), "image": image
        })
    };
    // Only JPEG, PNG and GIF, 2 MiB at most, and it must be an image.
    for bad in [
        json!("data:image/webp;base64,AAAA"),
        json!("data:text/plain;base64,aGk="),
        json!("not-a-data-uri"),
        json!(format!("data:image/png;base64,{}", "A".repeat(3_000_000))),
    ] {
        let (s, e) = w.post(BOB, &path, body(bad.clone())).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad} -> {e}");
        assert!(e["errors"]["image"].is_object(), "{e}");
    }
    let (s, ev) = w.post(BOB, &path, body(json!(PNG))).await;
    assert_eq!(s, StatusCode::OK, "{ev}");
    let eid = ev["id"].as_str().unwrap().to_owned();
    let hash = ev["image"].as_str().unwrap().to_owned();
    let key = format!("guild-events/{eid}/{hash}");
    assert!(w.storage.get(&key).await.is_ok(), "stored under {key}");
    assert_eq!(w.get(BOB, &format!("{path}/{eid}")).await.1["image"], hash);

    // A second cover replaces the first object; null removes the cover.
    let png2 = "data:image/gif;base64,R0lGODlhAQABAIAAAP///wAAACwAAAAAAQABAAACAkQBADs=";
    let (s, ev2) = w
        .patch(BOB, &format!("{path}/{eid}"), json!({ "image": png2 }))
        .await;
    assert_eq!(s, StatusCode::OK, "{ev2}");
    let hash2 = ev2["image"].as_str().unwrap().to_owned();
    assert_ne!(hash, hash2);
    assert!(w.storage.get(&key).await.is_err(), "the old cover is gone");
    let key2 = format!("guild-events/{eid}/{hash2}");
    assert!(w.storage.get(&key2).await.is_ok());
    // A bad cover on an edit changes nothing.
    let (s, _) = w
        .patch(BOB, &format!("{path}/{eid}"), json!({ "image": "nope" }))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(w.get(BOB, &format!("{path}/{eid}")).await.1["image"], hash2);
    let (_, cleared) = w
        .patch(BOB, &format!("{path}/{eid}"), json!({ "image": null }))
        .await;
    assert!(cleared["image"].is_null(), "{cleared}");
    assert!(w.storage.get(&key2).await.is_err());

    // Deleting an event takes its cover along.
    let (_, ev3) = w.post(BOB, &path, body(json!(PNG))).await;
    let eid3 = ev3["id"].as_str().unwrap().to_owned();
    let key3 = format!("guild-events/{eid3}/{}", ev3["image"].as_str().unwrap());
    assert!(w.storage.get(&key3).await.is_ok());
    assert_eq!(
        w.delete(BOB, &format!("{path}/{eid3}")).await.0,
        StatusCode::NO_CONTENT
    );
    assert!(w.storage.get(&key3).await.is_err());
}

#[tokio::test]
async fn a_member_joining_the_voice_channel_starts_the_event_and_interested_members_are_told() {
    let w = world().await;
    let (gid, _) = w.guild("Voice Club").await;
    w.add_member(&gid, CAROL);
    let voice = voice_channel(&w, &gid).await;
    let (g, ch) = (
        GuildId::new(gid.parse().unwrap()),
        ChannelId::new(voice.parse().unwrap()),
    );
    let path = format!("/guilds/{gid}/scheduled-events");
    let (_, ev) = w
        .post(
            BOB,
            &path,
            json!({
                "name": "Movie night", "entity_type": 2, "channel_id": voice,
                "scheduled_start_time": at(HOUR)
            }),
        )
        .await;
    let eid = ev["id"].as_str().unwrap().to_owned();
    assert_eq!(
        w.put(CAROL, &format!("{path}/{eid}/users/@me"), json!({}))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    let mut events = w.events(&gid);

    // Before the start time a join does nothing, and the clock never starts a voice event.
    settings::scheduled_events::on_voice_joined(&w.settings_deps, g, ch).await;
    assert_eq!(w.get(BOB, &format!("{path}/{eid}")).await.1["status"], 1);
    w.clock.advance_ms(HOUR);
    settings::scheduled_events::advance(&w.settings_deps).await;
    assert_eq!(w.get(BOB, &format!("{path}/{eid}")).await.1["status"], 1);
    assert!(w.notified.0.lock().unwrap().is_empty());

    // The first join at the start time starts it: UPDATE dispatched, audit 101, Carol told.
    settings::scheduled_events::on_voice_joined(&w.settings_deps, g, ch).await;
    let (_, now) = w.get(BOB, &format!("{path}/{eid}")).await;
    assert_eq!(now["status"], 2, "{now}");
    let mut saw_update = false;
    while let Ok(Some(ev)) = tokio::time::timeout(Duration::from_millis(50), events.next()).await {
        if let DomainEvent::ScheduledEvent { kind, payload, .. } = ev
            && kind.dispatch_name() == "GUILD_SCHEDULED_EVENT_UPDATE"
            && payload["status"] == 2
        {
            saw_update = true;
        }
    }
    assert!(saw_update, "GUILD_SCHEDULED_EVENT_UPDATE with status 2");
    assert!(audit_actions(&w, &gid).await.contains(&101));
    let told = w.notified.0.lock().unwrap().clone();
    assert_eq!(
        told,
        vec![("Movie night".to_owned(), vec![UserId::new(CAROL as u64)])]
    );

    // A second join announces nothing more; an empty channel ends it after 15 minutes.
    settings::scheduled_events::on_voice_joined(&w.settings_deps, g, ch).await;
    assert_eq!(w.notified.0.lock().unwrap().len(), 1);
    settings::scheduled_events::advance(&w.settings_deps).await;
    w.clock.advance_ms(15 * 60_000);
    settings::scheduled_events::advance(&w.settings_deps).await;
    assert_eq!(w.get(BOB, &format!("{path}/{eid}")).await.1["status"], 3);
}

#[tokio::test]
async fn a_stage_event_opens_its_stage_instance_and_ends_with_it() {
    let w = world().await;
    let (gid, _) = w.guild("Stage Club").await;
    let stage = stage_channel(&w, &gid).await;
    w.add_member(&gid, CAROL);
    let path = format!("/guilds/{gid}/scheduled-events");
    let (s, ev) = w
        .post(
            OWNER,
            &path,
            json!({
                "name": "Town hall", "entity_type": 1, "channel_id": stage,
                "scheduled_start_time": at(HOUR)
            }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{ev}");
    let eid = ev["id"].as_str().unwrap().to_owned();
    assert!(ev["entity_id"].is_null());
    // Carol is interested; she is told when it starts.
    w.put(CAROL, &format!("{path}/{eid}/users/@me"), json!({}))
        .await;

    // Starting by hand opens the stage (topic = event name) and links it as entity_id.
    let (s, started) = w
        .patch(OWNER, &format!("{path}/{eid}"), json!({ "status": 2 }))
        .await;
    assert_eq!(s, StatusCode::OK, "{started}");
    assert_eq!(started["entity_id"], stage.as_str());
    assert_eq!(w.voice.log.lock().unwrap().as_slice(), ["start Town hall"]);
    assert_eq!(w.notified.0.lock().unwrap().len(), 1);

    // Someone is in the channel; the event runs. The stage instance goes: the event completes.
    w.voice
        .occupants
        .lock()
        .unwrap()
        .insert(ChannelId::new(stage.parse().unwrap()), 2);
    settings::scheduled_events::advance(&w.settings_deps).await;
    assert_eq!(w.get(OWNER, &format!("{path}/{eid}")).await.1["status"], 2);
    w.voice.live.lock().unwrap().clear();
    settings::scheduled_events::advance(&w.settings_deps).await;
    let (_, done) = w.get(OWNER, &format!("{path}/{eid}")).await;
    assert_eq!(done["status"], 3, "{done}");
    assert!(done["entity_id"].is_null());

    // Ending a running stage event by hand deletes its instance too.
    let (_, ev2) = w
        .post(
            OWNER,
            &path,
            json!({
                "name": "Q and A", "entity_type": 1, "channel_id": stage,
                "scheduled_start_time": at(2 * HOUR)
            }),
        )
        .await;
    let eid2 = ev2["id"].as_str().unwrap().to_owned();
    w.patch(OWNER, &format!("{path}/{eid2}"), json!({ "status": 2 }))
        .await;
    assert_eq!(w.voice.live.lock().unwrap().len(), 1);
    let (s, _) = w
        .patch(OWNER, &format!("{path}/{eid2}"), json!({ "status": 3 }))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert!(w.voice.live.lock().unwrap().is_empty());
    assert_eq!(w.voice.log.lock().unwrap().last().unwrap(), "end");
}

/// Both error codes of the server 2FA requirement, as `(status, code)`.
fn mfa_code(r: &(StatusCode, Value)) -> (u16, i64) {
    (r.0.as_u16(), r.1["code"].as_i64().unwrap_or(-1))
}

#[tokio::test]
async fn elevated_routes_need_a_second_factor_on_a_2fa_server() {
    let w = world().await;
    let (gid, general) = w.guild("Strict Club").await;
    w.add_member(&gid, DAVE);
    // Make the requirement apply (the owner enrols first, as the route demands).
    w.enrol(OWNER).await;
    let (s, m) = w
        .post(OWNER, &format!("/guilds/{gid}/mfa"), json!({ "level": 1 }))
        .await;
    assert_eq!(s, StatusCode::OK, "{m}");
    w.clock.advance_ms(16 * 60 * 1000);
    // Remove the owner's authenticator again for the "not enrolled" half.
    w.auth.totp_delete(OWNER).await.unwrap();
    let (_, inv) = w
        .post(OWNER, &format!("/channels/{general}/invites"), json!({}))
        .await;
    let code = inv["code"].as_str().unwrap().to_owned();

    let routes: Vec<(Method, String, Option<Value>)> = vec![
        (
            Method::DELETE,
            format!("/guilds/{gid}/members/{DAVE}"),
            None,
        ),
        (
            Method::PATCH,
            format!("/guilds/{gid}/members/{BOB}"),
            Some(json!({ "communication_disabled_until": "2099-01-01T00:00:00.000000+00:00" })),
        ),
        (
            Method::POST,
            format!("/channels/{general}/webhooks"),
            Some(json!({ "name": "hook" })),
        ),
        (
            Method::POST,
            format!("/guilds/{gid}/emojis"),
            Some(json!({ "name": "blob", "image": PNG })),
        ),
        (Method::DELETE, format!("/guilds/{gid}/emojis/1"), None),
        (Method::DELETE, format!("/guilds/{gid}/stickers/1"), None),
        (
            Method::DELETE,
            format!("/guilds/{gid}/soundboard-sounds/1"),
            None,
        ),
        (
            Method::DELETE,
            format!("/guilds/{gid}/auto-moderation/rules/1"),
            None,
        ),
        (
            Method::PATCH,
            format!("/guilds/{gid}/sr/community"),
            Some(json!({ "dm_spam_filter": true })),
        ),
        (Method::DELETE, format!("/invites/{code}"), None),
    ];

    // No second factor on the account: 900123 everywhere.
    for (m, p, b) in &routes {
        let r = w.call(OWNER, m.clone(), p, b.clone()).await;
        assert_eq!(mfa_code(&r), (403, 900123), "{m} {p}: {}", r.1);
    }
    // Members without the permission still get the plain permission error where the route
    // checks the permission first.
    let r = w
        .call(
            BOB,
            Method::PATCH,
            &format!("/guilds/{gid}/sr/community"),
            Some(json!({ "dm_spam_filter": true })),
        )
        .await;
    assert_eq!(mfa_code(&r).1, 50013, "{}", r.1);

    // Enrolled but not proved on this session: 900004.
    w.enrol(OWNER).await;
    for (m, p, b) in &routes {
        let r = w.call(OWNER, m.clone(), p, b.clone()).await;
        assert_eq!(mfa_code(&r), (403, 900004), "{m} {p}: {}", r.1);
    }

    // Proved: none of them is refused for the second factor any more.
    w.prove(100).await;
    for (m, p, b) in &routes {
        let r = w.call(OWNER, m.clone(), p, b.clone()).await;
        assert!(
            !matches!(mfa_code(&r).1, 900004 | 900123),
            "{m} {p}: {}",
            r.1
        );
    }

    // Webhook edit and delete are gated too once the window closes.
    let (s, hook) = w
        .post(
            OWNER,
            &format!("/channels/{general}/webhooks"),
            json!({ "name": "again" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{hook}");
    let hid = hook["id"].as_str().unwrap().to_owned();
    w.clock.advance_ms(16 * 60 * 1000);
    let r = w
        .patch(OWNER, &format!("/webhooks/{hid}"), json!({ "name": "x" }))
        .await;
    assert_eq!(mfa_code(&r), (403, 900004), "{}", r.1);
    let r = w.delete(OWNER, &format!("/webhooks/{hid}")).await;
    assert_eq!(mfa_code(&r), (403, 900004), "{}", r.1);
}

#[tokio::test]
async fn guild_counts_come_with_with_counts_only() {
    let w = world().await;
    let (gid, _) = w.guild("Chess Club").await;
    let (s, plain) = w.get(OWNER, &format!("/guilds/{gid}")).await;
    assert_eq!(s, StatusCode::OK, "{plain}");
    assert!(plain.get("approximate_member_count").is_none());
    let (s, counted) = w
        .get(OWNER, &format!("/guilds/{gid}?with_counts=true"))
        .await;
    assert_eq!(s, StatusCode::OK, "{counted}");
    assert_eq!(counted["approximate_member_count"], 2, "owner and bob");
    // No presence source is wired in this world, so no online count.
    assert!(counted.get("approximate_presence_count").is_none());
}

#[tokio::test]
async fn staff_list_transfer_and_delete_servers_with_a_reason() {
    let w = world().await;
    let (gid, _) = w.guild("Chess Club").await;
    let id: i64 = gid.parse().unwrap();
    w.admin_repo.add_guild(id, "Chess Club", OWNER, 2);
    let path = format!("/admin/guilds/{gid}");

    // Only staff with MANAGE_GUILDS_GLOBAL see the list.
    assert_eq!(w.get(BASIC, "/admin/guilds").await.0, StatusCode::FORBIDDEN);
    assert_eq!(w.get(OWNER, "/admin/guilds").await.0, StatusCode::FORBIDDEN);
    let (s, page) = w.get(STAFF, "/admin/guilds?q=chess").await;
    assert_eq!(s, StatusCode::OK, "{page}");
    assert_eq!(page["items"][0]["id"], gid.as_str());

    // A reason is required for both changes.
    let (s, _) = w
        .post(
            STAFF,
            &format!("{path}/transfer-ownership"),
            json!({"new_owner_id": BOB.to_string()}),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(w.delete(STAFF, &path).await.0, StatusCode::BAD_REQUEST);

    // Transfer to a member: the chat stack now says BOB owns it.
    let (s, v) = w
        .post(
            STAFF,
            &format!("{path}/transfer-ownership"),
            json!({"new_owner_id": BOB.to_string(), "reason": "owner asked on the help desk"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (_, g) = w.get(BOB, &format!("/guilds/{gid}")).await;
    assert_eq!(g["owner_id"], BOB.to_string());
    // A non-member cannot become the owner.
    let (s, _) = w
        .post(
            STAFF,
            &format!("{path}/transfer-ownership"),
            json!({"new_owner_id": CAROL.to_string(), "reason": "test"}),
        )
        .await;
    assert_ne!(s, StatusCode::OK);

    // Delete acts as the owner (the admin store here is a seeded copy, so use a fresh server)
    // and the server is gone for its members.
    let (other, _) = w.guild("Gardening").await;
    w.admin_repo
        .add_guild(other.parse().unwrap(), "Gardening", OWNER, 2);
    let other_path = format!("/admin/guilds/{other}");
    let (s, _) = w
        .call(
            STAFF,
            Method::DELETE,
            &other_path,
            Some(json!({"reason": "illegal content, see case 4"})),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(
        w.get(BOB, &format!("/guilds/{other}")).await.0,
        StatusCode::NOT_FOUND
    );
}
