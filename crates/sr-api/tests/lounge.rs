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
        directory::{self, DirectoryDeps},
        guilds, settings,
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
use sr_chat::{ChatLimits, ChatService, Member};
use sr_core::PlatformPermissions as P;
use sr_core::{Clock, GuildId, ManualClock, SnowflakeGenerator, UserId};
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
    tokens: HashMap<i64, String>,
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
        .with_joiner(joiner.clone()),
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
    admin_repo.add_role(1, "Directory", 2, P::STAFF_BASIC | P::MANAGE_DIRECTORY);
    admin_repo.add_role(2, "Basic", 1, P::STAFF_BASIC);
    admin_repo.set_owner(ADMIN_OWNER);
    let admin_svc = AdminService::new(admin_repo.clone());
    let op = Caller::operator(SYSTEM_USER_ID);
    admin_svc.appoint(&op, STAFF, &[1], Some("t")).await.unwrap();
    admin_svc.appoint(&op, BASIC, &[2], Some("t")).await.unwrap();

    // Sessions for everybody.
    let sessions = Arc::new(MemorySessions::new());
    let mut tokens = HashMap::new();
    for (n, uid) in [OWNER, BOB, CAROL, DAVE, STAFF, BASIC].into_iter().enumerate() {
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

    let mut api_settings = ApiSettings::default();
    api_settings.rate_limit.enabled = false;
    let state = AppState::builder()
        .settings(api_settings)
        .sessions(sessions)
        .bots(Arc::new(MemoryBots::new()))
        .clock(dyn_clock)
        .extension(Arc::new(guild_api))
        .extension(settings_deps)
        .extension(Arc::new(DirectoryDeps::new(Arc::new(
            MemoryDirectory::default(),
        ))))
        .extension(Arc::new(AdminDeps::new(admin_svc)))
        .extension(Arc::new(sr_api::routes::media::MediaDeps::new(
            storage.clone(),
        )))
        .build();
    let app = build_router(
        state,
        guilds::router()
            .merge(settings::router())
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
        tokens,
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
        });
    }

    fn is_member(&self, gid: &str, user: i64) -> bool {
        self.repo.store.lock().unwrap().members.iter().any(|m| {
            m.guild_id.to_string() == gid && m.user_id == UserId::new(user as u64)
        })
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
        self.post(user, &format!("/invites/{code}"), json!({})).await
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
    let w = world().await;
    let (gid, _) = w.guild("Club House").await;

    let (s, badges) = w.get(BOB, "/guilds/tag-badges").await;
    assert_eq!(s, StatusCode::OK);
    assert!(badges.as_array().unwrap().iter().any(|b| b == "crown"));

    // Setting a tag takes MANAGE_GUILD; it is validated; a badge needs a tag.
    let uri = format!("/guilds/{gid}");
    let (s, _) = w.patch(BOB, &uri, json!({ "tag": "ZZ" })).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    for bad in [json!({"tag": "x"}), json!({"tag": "toolong"}), json!({"tag_badge": "crown"})] {
        let (s, e) = w.patch(OWNER, &uri, bad).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    }
    let (s, e) = w
        .patch(OWNER, &uri, json!({ "tag": "SR1", "tag_badge": "crown" }))
        .await;
    assert_eq!(s, StatusCode::OK, "{e}");
    assert_eq!((e["tag"].as_str(), e["tag_badge"].as_str()), (Some("SR1"), Some("crown")));
    let (s, e) = w.patch(OWNER, &uri, json!({ "tag_badge": "unicorn" })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");

    // Nobody shows it until they choose to.
    let (_, m) = w.get(OWNER, &format!("{uri}/members/{BOB}")).await;
    assert!(m["user"].get("primary_guild").is_none(), "{m}");

    // Bob opts in; non-members and tagless servers are refused.
    let (s, u) = w
        .patch(BOB, "/users/@me/primary-guild", json!({ "identity_guild_id": gid }))
        .await;
    assert_eq!(s, StatusCode::OK, "{u}");
    assert_eq!(u["primary_guild"]["tag"], "SR1");
    assert_eq!(u["primary_guild"]["badge"], "crown");
    assert_eq!(u["primary_guild"]["identity_guild_id"], gid);
    assert_eq!(u["primary_guild"]["identity_enabled"], true);
    let (_, m) = w.get(OWNER, &format!("{uri}/members/{BOB}")).await;
    assert_eq!(m["user"]["primary_guild"]["tag"], "SR1");
    let (s, _) = w
        .patch(CAROL, "/users/@me/primary-guild", json!({ "identity_guild_id": gid }))
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "carol is not a member");
    let (other, _) = w.guild("No Tag").await;
    let (s, e) = w
        .patch(OWNER, "/users/@me/primary-guild", json!({ "identity_guild_id": other }))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");

    // Opting out hides it again; removing the tag removes the badge too.
    let (s, u) = w
        .patch(BOB, "/users/@me/primary-guild", json!({ "identity_guild_id": null }))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert!(u.get("primary_guild").is_none(), "{u}");
    let (s, e) = w.patch(OWNER, &uri, json!({ "tag": null })).await;
    assert_eq!(s, StatusCode::OK, "{e}");
    assert!(e.get("tag").is_none() && e.get("tag_badge").is_none(), "{e}");
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
    assert_eq!((m["nick"].as_str(), m["bio"].as_str()), (Some("Bobby"), Some("hello there")));
    let avatar = m["avatar"].as_str().unwrap().to_owned();
    let banner = m["banner"].as_str().unwrap().to_owned();
    assert!(w.storage.get(&format!("avatars/{BOB}/{avatar}")).await.is_ok());
    assert!(w.storage.get(&format!("avatars/{BOB}/{banner}")).await.is_ok());

    // Everybody sees it on the member object, and bob can read his own.
    let (_, seen) = w.get(OWNER, &format!("/guilds/{gid}/members/{BOB}")).await;
    assert_eq!(seen["avatar"], m["avatar"]);
    assert_eq!(seen["bio"], "hello there");
    let (s, mine) = w.get(BOB, &format!("/users/@me/guilds/{gid}/member")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(mine["nick"], "Bobby");

    // The same picture in another server is a different object.
    let (other, _) = w.guild("Second").await;
    let (s, m2) = w.patch(BOB, &format!("/guilds/{other}/members/@me"), json!({ "avatar": PNG })).await;
    assert_eq!(s, StatusCode::OK, "{m2}");
    assert_ne!(m2["avatar"], m["avatar"]);

    // Replacing or clearing deletes the old object but not the other server's.
    let (s, m3) = w.patch(BOB, &uri, json!({ "avatar": null, "bio": null })).await;
    assert_eq!(s, StatusCode::OK, "{m3}");
    assert!(m3["avatar"].is_null() && m3.get("bio").is_none(), "{m3}");
    assert!(w.storage.get(&format!("avatars/{BOB}/{avatar}")).await.is_err());
    let other_avatar = m2["avatar"].as_str().unwrap();
    assert!(w.storage.get(&format!("avatars/{BOB}/{other_avatar}")).await.is_ok());
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
    assert_eq!(w.patch(BOB, &uri, json!({ "code": "club" })).await.0, StatusCode::FORBIDDEN);
    assert_eq!(w.get(CAROL, "/invites/club-house").await.0, StatusCode::NOT_FOUND);

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
        .patch(OWNER, &format!("/guilds/{second}/vanity-url"), json!({ "code": "CLUB-house" }))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    let (_, inv) = w
        .post(OWNER, &format!("/channels/{second_general}/invites"), json!({}))
        .await;
    let taken = inv["code"].as_str().unwrap();
    let (s, e) = w
        .patch(OWNER, &uri, json!({ "code": taken }))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "an invite already has this text: {e}");

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
    assert_eq!(w.delete(OWNER, "/invites/club-house").await.0, StatusCode::NOT_FOUND);

    // Releasing it frees the code.
    let (s, v) = w.patch(OWNER, &uri, json!({ "code": null })).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert!(v["code"].is_null());
    assert_eq!(w.get(CAROL, "/invites/club-house").await.0, StatusCode::NOT_FOUND);
    let (s, v) = w
        .patch(OWNER, &format!("/guilds/{second}/vanity-url"), json!({ "code": "club-house" }))
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
        .patch(OWNER, &format!("{g}/sr/community"), json!({ "screening_enabled": true }))
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
    assert_eq!(form["form_fields"][0]["values"], json!(["Be kind", "No spam"]));

    // Not agreeing keeps them pending; agreeing releases them (and announces it).
    let mut events = w.events(&gid);
    let (s, e) = w.put(CAROL, &format!("{g}/requests/@me"), json!({ "agreed": false })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    let (s, m) = w.put(CAROL, &format!("{g}/requests/@me"), json!({ "agreed": true })).await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert_eq!(m["pending"], false);
    let mut updated = false;
    while let Some(ev) = next_event(&mut events).await {
        updated |= matches!(ev, DomainEvent::MemberUpdated { user, .. } if user == UserId::new(CAROL as u64));
    }
    assert!(updated, "GUILD_MEMBER_UPDATE after accepting");
    // Accepting again is harmless; an outsider cannot accept for a server they are not in.
    assert_eq!(w.put(CAROL, &format!("{g}/requests/@me"), json!({})).await.0, StatusCode::OK);
    assert_eq!(w.put(DAVE, &format!("{g}/requests/@me"), json!({})).await.0, StatusCode::FORBIDDEN);

    // Switching Community off switches screening off with it.
    let (s, c) = w
        .patch(OWNER, &format!("{g}/sr/community"), json!({ "community_enabled": false }))
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
    let (_, gamer) = w.post(OWNER, &format!("{g}/roles"), role("gamer", "0")).await;
    // ADMINISTRATOR = 8, KICK_MEMBERS = 2.
    let (_, boss) = w.post(OWNER, &format!("{g}/roles"), role("boss", "8")).await;
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
    let opt_games = o["prompts"][0]["options"][0]["id"].as_str().unwrap().to_owned();
    let opt_none = o["prompts"][0]["options"][1]["id"].as_str().unwrap().to_owned();

    // Onboarding has to be on.
    let answer = format!("{g}/onboarding-responses");
    let (s, e) = w.post(BOB, &answer, json!({ "option_ids": [opt_games] })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "off: {e}");
    let (s, _) = w
        .patch(OWNER, &format!("{g}/sr/community"), json!({ "onboarding_enabled": true }))
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
    assert_eq!(w.post(CAROL, &answer, json!({ "option_ids": [opt_games] })).await.0, StatusCode::FORBIDDEN);

    // A good answer grants the harmless role, never the powerful one, and lists channels.
    let (s, r) = w.post(BOB, &answer, json!({ "option_ids": [opt_games] })).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["role_ids"], json!([gamer_id]));
    assert_eq!(r["channel_ids"], json!([general, games]));
    let (_, m) = w.get(OWNER, &format!("{g}/members/{BOB}")).await;
    assert_eq!(m["roles"], json!([gamer_id]));
    // Answering again changes nothing more.
    let (_, r) = w.post(BOB, &answer, json!({ "option_ids": [opt_games] })).await;
    assert_eq!(r["role_ids"], json!([]));
}

// ------------------------------------------------------------------ LNG-INV-005

#[tokio::test]
async fn temporary_members_leave_when_they_go_offline_unless_they_hold_a_role() {
    let w = world().await;
    let (gid, general) = w.guild("Club House").await;
    let g = format!("/guilds/{gid}");
    let (_, role) = w.post(OWNER, &format!("{g}/roles"), json!({ "name": "regular" })).await;
    let role_id: sr_core::RoleId = role["id"].as_str().unwrap().parse().unwrap();

    let invite = |temporary: bool| {
        let general = general.clone();
        let w = &w;
        async move {
            let (s, inv) = w
                .post(OWNER, &format!("/channels/{general}/invites"), json!({ "temporary": temporary }))
                .await;
            assert_eq!(s, StatusCode::OK, "{inv}");
            assert_eq!(inv["temporary"], temporary);
            inv["code"].as_str().unwrap().to_owned()
        }
    };
    let temp = invite(true).await;
    let perm = invite(false).await;
    assert_eq!(w.post(CAROL, &format!("/invites/{temp}"), json!({})).await.0, StatusCode::OK);
    assert_eq!(w.post(DAVE, &format!("/invites/{temp}"), json!({})).await.0, StatusCode::OK);
    // Bob is already in; a normal invite never marks anyone.
    assert_eq!(w.post(BOB, &format!("/invites/{perm}"), json!({})).await.0, StatusCode::OK);
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
    assert!(w.joiner.drop_temporary(UserId::new(DAVE as u64)).await.is_empty());
    assert!(w.is_member(&gid, DAVE), "a role makes the membership permanent");
    assert!(w.joiner.drop_temporary(UserId::new(BOB as u64)).await.is_empty());
    assert!(w.is_member(&gid, BOB));
    assert!(w.settings.with(|s| s.temporary.contains(&(GuildId::new(gid.parse().unwrap()), UserId::new(DAVE as u64)))));
}

// ------------------------------------------------------------------ LNG-GLD-013

#[tokio::test]
async fn server_directory_listing_approval_search_and_join() {
    let w = world().await;
    let (gid, general) = w.guild("Chess Club").await;
    let listing = format!("/guilds/{gid}/directory");

    // A server needs Community, a known category and MANAGE_GUILD.
    let (s, e) = w.put(OWNER, &listing, json!({ "category": "gaming" })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "no community: {e}");
    w.community(&gid, &general, json!({})).await;
    assert_eq!(w.put(BOB, &listing, json!({ "category": "gaming" })).await.0, StatusCode::FORBIDDEN);
    let (s, e) = w.put(OWNER, &listing, json!({ "category": "nope" })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    assert_eq!(w.get(OWNER, &listing).await.0, StatusCode::NOT_FOUND);

    let (s, l) = w
        .put(OWNER, &listing, json!({ "category": "gaming", "description": "Boards and clocks" }))
        .await;
    assert_eq!(s, StatusCode::OK, "{l}");
    assert_eq!((l["status"].as_str(), l["category"].as_str()), (Some("pending"), Some("gaming")));
    assert_eq!(w.get(BOB, &listing).await.0, StatusCode::FORBIDDEN, "listing state is for managers");

    // Pending listings are invisible to members; categories are listed.
    let (s, cats) = w.get(CAROL, "/directory/categories").await;
    assert_eq!(s, StatusCode::OK);
    assert!(cats.as_array().unwrap().iter().any(|c| c == "gaming"));
    assert_eq!(w.get(CAROL, "/directory").await.1, json!([]));
    assert_eq!(w.get(CAROL, &format!("/directory/{gid}")).await.0, StatusCode::NOT_FOUND);
    assert_eq!(w.post(CAROL, &format!("/directory/{gid}/join"), json!({})).await.0, StatusCode::NOT_FOUND);

    // Staff: only MANAGE_DIRECTORY works the queue.
    assert_eq!(w.get(BASIC, "/admin/directory").await.0, StatusCode::FORBIDDEN);
    assert_eq!(w.get(OWNER, "/admin/directory").await.0, StatusCode::FORBIDDEN);
    let (s, queue) = w.get(STAFF, "/admin/directory").await;
    assert_eq!(s, StatusCode::OK, "{queue}");
    assert_eq!(queue[0]["guild_id"], gid);
    assert_eq!(queue[0]["guild_name"], "Chess Club");
    assert_eq!(queue[0]["community_enabled"], true);
    assert_eq!(w.get(STAFF, "/admin/directory?status=bogus").await.0, StatusCode::BAD_REQUEST);
    let decide = |body: Value| {
        let w = &w;
        let gid = gid.clone();
        async move { w.patch(STAFF, &format!("/admin/directory/{gid}"), body).await }
    };
    assert_eq!(decide(json!({ "decision": "maybe" })).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(
        w.patch(BASIC, &format!("/admin/directory/{gid}"), json!({ "decision": "approve" })).await.0,
        StatusCode::FORBIDDEN
    );
    let (s, d) = decide(json!({ "decision": "reject", "note": "add a description" })).await;
    assert_eq!(s, StatusCode::OK, "{d}");
    assert_eq!((d["status"].as_str(), d["decision_note"].as_str()), (Some("rejected"), Some("add a description")));
    let (_, l) = w.get(OWNER, &listing).await;
    assert_eq!(l["decision_note"], "add a description");
    assert_eq!(decide(json!({ "decision": "remove" })).await.0, StatusCode::BAD_REQUEST, "only approved ones");

    // Asking again after a rejection, then approval and featuring.
    let (_, l) = w.put(OWNER, &listing, json!({ "category": "gaming", "description": "Boards and clocks" })).await;
    assert_eq!(l["status"], "pending");
    let (s, d) = decide(json!({ "decision": "approve", "featured": true })).await;
    assert_eq!(s, StatusCode::OK, "{d}");
    assert_eq!((d["status"].as_str(), d["featured"].as_bool()), (Some("approved"), Some(true)));

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
    assert_eq!(w.get(CAROL, "/directory?query=chess+clocks").await.1.as_array().unwrap().len(), 1);
    assert_eq!(w.get(CAROL, "/directory?query=poker").await.1, json!([]));
    assert_eq!(w.get(CAROL, "/directory?category=music").await.1, json!([]));
    assert_eq!(w.get(CAROL, "/directory?category=bogus").await.0, StatusCode::BAD_REQUEST);
    assert_eq!(w.get(BOB, "/directory").await.1[0]["joined"], true);
    assert_eq!(w.get(CAROL, &format!("/directory/{gid}")).await.0, StatusCode::OK);

    // Joining from the directory needs no invite and respects bans.
    let (s, m) = w.post(CAROL, &format!("/directory/{gid}/join"), json!({})).await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert!(w.is_member(&gid, CAROL));
    let (s, again) = w.post(CAROL, &format!("/directory/{gid}/join"), json!({})).await;
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
    assert_eq!(w.post(DAVE, &format!("/directory/{gid}/join"), json!({})).await.0, StatusCode::FORBIDDEN);

    // Editing the blurb sends an approved listing back to review and hides it.
    let (_, l) = w.put(OWNER, &listing, json!({ "category": "gaming", "description": "New text" })).await;
    assert_eq!(l["status"], "pending");
    assert_eq!(w.get(BOB, "/directory").await.1, json!([]));
    decide(json!({ "decision": "approve" })).await;
    assert_eq!(w.get(BOB, "/directory").await.1.as_array().unwrap().len(), 1);
    // Taking Community away hides it too; the manager can withdraw.
    let (s, _) = w
        .patch(OWNER, &format!("/guilds/{gid}/sr/community"), json!({ "community_enabled": false }))
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
