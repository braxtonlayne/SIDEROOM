//! Guild endpoints over the in-memory repository, driven with `oneshot`.

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use futures::{StreamExt, stream::BoxStream};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sr_api::{
    ApiSettings, AppState,
    auth::{
        BearerIdentity, BotIdentity,
        memory::{MemoryBearers, MemoryBots},
    },
    build_router, openapi,
    rate_limit::RateLimitConfig,
    routes::guilds,
};
use sr_chat::memory::MemoryRepo;
use sr_chat::settings::audit::AuditAction;
use sr_chat::{ChatLimits, ChatService, Member};
use sr_core::{Clock, GuildId, ManualClock, SnowflakeGenerator, UserId};
use sr_events::{DomainEvent, EventBus, InProcessBus, Topic};
use std::{sync::Arc, time::Duration};
use tower::ServiceExt;

const T0: i64 = 1_800_000_000_000;

struct Fx {
    app: Router,
    repo: MemoryRepo,
    bus: Arc<InProcessBus>,
    storage: Arc<sr_storage::ObjectStoreStorage>,
}

fn fixture() -> Fx {
    fixture_with(None)
}

fn fixture_with(audit: Option<Arc<dyn guilds::backend::AuditSink>>) -> Fx {
    let clock: Arc<dyn Clock> = Arc::new(ManualClock::new(T0));
    let ids = SnowflakeGenerator::new(clock.clone(), 1, 1).unwrap();
    let repo = MemoryRepo::default();
    let svc = ChatService::new(repo.clone(), clock.clone(), ids, ChatLimits::default());
    let bus = Arc::new(InProcessBus::new());
    let backend = match audit {
        Some(a) => guilds::backend::backend_with_audit(svc, bus.clone(), a),
        None => guilds::backend::backend(svc, bus.clone()),
    };

    let bots = Arc::new(MemoryBots::new());
    for (token, user) in [("owner", 1), ("bob", 2), ("carol", 3), ("stranger", 9)] {
        bots.insert(
            token,
            BotIdentity {
                user_id: user,
                application_id: 100 + user,
            },
        );
    }
    let bearers = Arc::new(MemoryBearers::new());
    bearers.insert(
        "oauth-guilds",
        BearerIdentity {
            user_id: 2,
            scopes: vec!["guilds".into()],
            application_id: 0,
            expires_at: i64::MAX,
        },
    );
    bearers.insert(
        "oauth-identify",
        BearerIdentity {
            user_id: 2,
            scopes: vec!["identify".into()],
            application_id: 0,
            expires_at: i64::MAX,
        },
    );
    let settings = ApiSettings {
        rate_limit: RateLimitConfig {
            enabled: false,
            ..RateLimitConfig::default()
        },
        ..ApiSettings::default()
    };
    let storage = Arc::new(sr_storage::ObjectStoreStorage::in_memory());
    let mut doc = openapi::base();
    doc.merge(guilds::openapi());
    let state = AppState::builder()
        .extension(Arc::new(guilds::GuildApi(backend)))
        .extension(Arc::new(sr_api::routes::media::MediaDeps::new(
            storage.clone(),
        )))
        .bots(bots)
        .bearers(bearers)
        .clock(clock)
        .settings(settings)
        .openapi(doc)
        .build();
    Fx {
        app: build_router(state, guilds::router()),
        repo,
        bus,
        storage,
    }
}

async fn call(
    f: &Fx,
    method: Method,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(format!("/api/v1{uri}"));
    if let Some(t) = token {
        let scheme = if t.starts_with("oauth") {
            "Bearer"
        } else {
            "Bot"
        };
        req = req.header(header::AUTHORIZATION, format!("{scheme} {t}"));
    }
    let body = match body {
        Some(b) => {
            req = req.header(header::CONTENT_TYPE, "application/json");
            Body::from(b.to_string())
        }
        None => Body::empty(),
    };
    let resp = f
        .app
        .clone()
        .oneshot(req.body(body).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, v)
}

async fn get(f: &Fx, uri: &str, t: &str) -> (StatusCode, Value) {
    call(f, Method::GET, uri, Some(t), None).await
}
async fn post(f: &Fx, uri: &str, t: &str, b: Value) -> (StatusCode, Value) {
    call(f, Method::POST, uri, Some(t), Some(b)).await
}
async fn patch(f: &Fx, uri: &str, t: &str, b: Value) -> (StatusCode, Value) {
    call(f, Method::PATCH, uri, Some(t), Some(b)).await
}
async fn put(f: &Fx, uri: &str, t: &str, b: Value) -> (StatusCode, Value) {
    call(f, Method::PUT, uri, Some(t), Some(b)).await
}
async fn del(f: &Fx, uri: &str, t: &str) -> (StatusCode, Value) {
    call(f, Method::DELETE, uri, Some(t), None).await
}

/// Create the guild as `owner` and add bob (2) and carol (3) as plain members.
async fn guild(f: &Fx) -> (String, Value) {
    let (s, g) = post(f, "/guilds", "owner", json!({ "name": "Club House" })).await;
    assert_eq!(s, StatusCode::CREATED, "{g}");
    let id = g["id"].as_str().unwrap().to_owned();
    for u in [2, 3] {
        f.repo.store.lock().unwrap().members.push(Member {
            guild_id: GuildId::new(id.parse().unwrap()),
            user_id: UserId::new(u),
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
    (id, g)
}

async fn next_event(s: &mut BoxStream<'static, DomainEvent>) -> Option<DomainEvent> {
    tokio::time::timeout(Duration::from_millis(200), s.next())
        .await
        .ok()
        .flatten()
}

#[tokio::test]
async fn guild_lifecycle_and_shapes() {
    let f = fixture();
    let (gid, created) = guild(&f).await;
    // Discord shape: string ids, @everyone with id == guild id, default layout.
    assert_eq!(created["name"], "Club House");
    assert_eq!(created["owner_id"], "1");
    assert_eq!(created["roles"][0]["id"], gid);
    assert_eq!(created["roles"][0]["name"], "@everyone");
    assert!(created["roles"][0]["permissions"].is_string());
    assert_eq!(created["channels"].as_array().unwrap().len(), 4);
    assert_eq!(created["channels"][0]["type"], 4);

    let (s, g) = get(&f, &format!("/guilds/{gid}"), "bob").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(g["id"], gid);

    let (s, g) = patch(
        &f,
        &format!("/guilds/{gid}"),
        "owner",
        json!({ "name": "Renamed", "description": "hello", "verification_level": 2 }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{g}");
    assert_eq!(
        (g["name"].as_str(), g["verification_level"].as_u64()),
        (Some("Renamed"), Some(2))
    );

    let (s, mine) = get(&f, "/users/@me/guilds", "bob").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(mine[0]["id"], gid);
    assert_eq!(mine[0]["owner"], false);
    let (_, mine) = get(&f, "/users/@me/guilds", "owner").await;
    assert_eq!(mine[0]["owner"], true);

    // /api/v10 alias.
    let req = Request::get(format!("/api/v10/guilds/{gid}"))
        .header(header::AUTHORIZATION, "Bot bob")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        f.app.clone().oneshot(req).await.unwrap().status(),
        StatusCode::OK
    );

    // Leave, owner cannot.
    let (s, _) = del(&f, &format!("/users/@me/guilds/{gid}"), "carol").await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, e) = del(&f, &format!("/users/@me/guilds/{gid}"), "owner").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(e["code"], 50035);

    let (s, _) = del(&f, &format!("/guilds/{gid}"), "bob").await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = del(&f, &format!("/guilds/{gid}"), "owner").await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, e) = get(&f, &format!("/guilds/{gid}"), "owner").await;
    assert_eq!(
        (s, e["code"].as_u64()),
        (StatusCode::NOT_FOUND, Some(10004))
    );
}

#[tokio::test]
async fn auth_and_permission_errors_use_discord_codes() {
    let f = fixture();
    let (gid, _) = guild(&f).await;
    let uri = format!("/guilds/{gid}");

    let (s, e) = call(&f, Method::GET, &uri, None, None).await;
    assert_eq!(
        (s, e["code"].as_u64()),
        (StatusCode::UNAUTHORIZED, Some(40001))
    );
    // Non-members: Missing Access.
    let (s, e) = get(&f, &uri, "stranger").await;
    assert_eq!(
        (s, e["code"].as_u64()),
        (StatusCode::FORBIDDEN, Some(50001))
    );
    // Members without MANAGE_GUILD: Missing Permissions.
    let (s, e) = patch(&f, &uri, "bob", json!({ "name": "nope" })).await;
    assert_eq!(
        (s, e["code"].as_u64()),
        (StatusCode::FORBIDDEN, Some(50013))
    );
    // Bad ids and bodies.
    let (s, e) = get(&f, "/guilds/not-a-number", "bob").await;
    assert_eq!(
        (s, e["code"].as_u64()),
        (StatusCode::BAD_REQUEST, Some(50035))
    );
    let (s, e) = patch(&f, &uri, "owner", json!({ "name": "x" })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(e["errors"]["name"]["_errors"].is_array());
    // Bearer tokens may only list guilds, and only with the scope.
    let (s, _) = get(&f, &uri, "oauth-guilds").await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, mine) = get(&f, "/users/@me/guilds", "oauth-guilds").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(mine.as_array().unwrap().len(), 1);
    let (s, _) = get(&f, "/users/@me/guilds", "oauth-identify").await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn channels_categories_overwrites_and_reorder() {
    let f = fixture();
    let (gid, created) = guild(&f).await;
    let cat_id = created["channels"][0]["id"].as_str().unwrap().to_owned();

    // Only managers create channels.
    let (s, _) = post(
        &f,
        &format!("/guilds/{gid}/channels"),
        "bob",
        json!({"name": "x"}),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    let (s, cat) = post(
        &f,
        &format!("/guilds/{gid}/channels"),
        "owner",
        json!({ "name": "Staff Area", "type": 4 }),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{cat}");
    assert_eq!(cat["type"], 4);
    let staff_cat = cat["id"].as_str().unwrap().to_owned();

    // A private text channel inside the category, hidden from @everyone.
    let (s, ch) = post(
        &f,
        &format!("/guilds/{gid}/channels"),
        "owner",
        json!({
            "name": "Secret Plans",
            "type": 0,
            "topic": "shh",
            "parent_id": staff_cat,
            "permission_overwrites": [
                { "id": gid, "type": 0, "allow": "0", "deny": "1024" }
            ],
        }),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{ch}");
    assert_eq!(ch["name"], "secret-plans");
    assert_eq!(ch["parent_id"], staff_cat);
    assert_eq!(ch["permission_overwrites"][0]["deny"], "1024");
    let ch_id = ch["id"].as_str().unwrap().to_owned();

    // Bob cannot see it, in lists or directly; the owner can.
    let (_, list) = get(&f, &format!("/guilds/{gid}/channels"), "bob").await;
    assert!(
        list.as_array()
            .unwrap()
            .iter()
            .all(|c| c["id"].as_str() != Some(&ch_id))
    );
    let (s, e) = get(&f, &format!("/channels/{ch_id}"), "bob").await;
    assert_eq!(
        (s, e["code"].as_u64()),
        (StatusCode::FORBIDDEN, Some(50001))
    );
    let (s, _) = get(&f, &format!("/channels/{ch_id}"), "owner").await;
    assert_eq!(s, StatusCode::OK);

    // Granting bob access with a member overwrite.
    let (s, _) = put(
        &f,
        &format!("/channels/{ch_id}/permissions/2"),
        "owner",
        json!({ "type": 1, "allow": "1024", "deny": "0" }),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, view) = get(&f, &format!("/channels/{ch_id}"), "bob").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(view["permission_overwrites"].as_array().unwrap().len(), 2);
    // Bob cannot edit overwrites.
    let (s, _) = put(
        &f,
        &format!("/channels/{ch_id}/permissions/3"),
        "bob",
        json!({ "type": 1, "allow": "1024", "deny": "0" }),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = del(&f, &format!("/channels/{ch_id}/permissions/2"), "owner").await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = get(&f, &format!("/channels/{ch_id}"), "bob").await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Modify: rename, clear topic, move out of the category.
    let (s, m) = patch(
        &f,
        &format!("/channels/{ch_id}"),
        "owner",
        json!({ "name": "Open Plans", "topic": null, "parent_id": cat_id, "position": 7 }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert_eq!(m["name"], "open-plans");
    assert_eq!(m["topic"], Value::Null);
    assert_eq!(m["parent_id"], cat_id);
    assert_eq!(m["position"], 7);
    // Invalid parent (a text channel).
    let (s, _) = patch(
        &f,
        &format!("/channels/{ch_id}"),
        "owner",
        json!({ "parent_id": ch_id }),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Bulk reorder.
    let (s, _) = patch(
        &f,
        &format!("/guilds/{gid}/channels"),
        "owner",
        json!([
            { "id": ch_id, "position": 1, "parent_id": staff_cat, "lock_permissions": false },
            { "id": staff_cat, "position": 0 },
        ]),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, view) = get(&f, &format!("/channels/{ch_id}"), "owner").await;
    assert_eq!(
        (view["position"].as_u64(), view["parent_id"].as_str()),
        (Some(1), Some(staff_cat.as_str()))
    );

    // Delete a category: children survive without a parent.
    let (s, deleted) = del(&f, &format!("/channels/{staff_cat}"), "owner").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(deleted["id"], staff_cat);
    let (_, view) = get(&f, &format!("/channels/{ch_id}"), "owner").await;
    assert_eq!(view["parent_id"], Value::Null);
    let (s, e) = get(&f, &format!("/channels/{staff_cat}"), "owner").await;
    assert_eq!(
        (s, e["code"].as_u64()),
        (StatusCode::NOT_FOUND, Some(10003))
    );
}

#[tokio::test]
async fn roles_members_and_hierarchy() {
    let f = fixture();
    let (gid, _) = guild(&f).await;
    let roles_uri = format!("/guilds/{gid}/roles");

    let (_, mods) = post(
        &f,
        &roles_uri,
        "owner",
        json!({ "name": "mods", "permissions": "268435458", "hoist": true }),
    )
    .await;
    let (s, low) = post(
        &f,
        &roles_uri,
        "owner",
        json!({ "name": "member+", "color": 255 }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{low}");
    let (low_id, mods_id) = (low["id"].as_str().unwrap(), mods["id"].as_str().unwrap());
    assert_eq!(mods["permissions"], "268435458");
    assert_eq!(mods["position"], 1);

    let (_, list) = get(&f, &roles_uri, "bob").await;
    assert_eq!(list.as_array().unwrap().len(), 3);
    let (s, _) = post(&f, &roles_uri, "bob", json!({ "name": "evil" })).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Assign, then the new moderator acts within the hierarchy.
    let (s, _) = put(
        &f,
        &format!("/guilds/{gid}/members/2/roles/{mods_id}"),
        "owner",
        json!({}),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, bob) = get(&f, &format!("/guilds/{gid}/members/2"), "carol").await;
    assert_eq!(bob["roles"][0], mods_id);
    assert_eq!(bob["user"]["id"], "2");
    assert!(
        bob["joined_at"]
            .as_str()
            .unwrap()
            .starts_with("2027-01-15T")
    );
    // Bob (KICK|MANAGE_ROLES via 0x10000002 = KICK_MEMBERS + MANAGE_ROLES) may not hand out his own rank.
    let (s, _) = put(
        &f,
        &format!("/guilds/{gid}/members/3/roles/{mods_id}"),
        "bob",
        json!({}),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = put(
        &f,
        &format!("/guilds/{gid}/members/3/roles/{low_id}"),
        "bob",
        json!({}),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = del(
        &f,
        &format!("/guilds/{gid}/members/3/roles/{low_id}"),
        "bob",
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);

    // Modify role, reorder, delete.
    let (s, r) = patch(
        &f,
        &format!("{roles_uri}/{low_id}"),
        "owner",
        json!({ "name": "regulars", "mentionable": true }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(
        (r["name"].as_str(), r["mentionable"].as_bool()),
        (Some("regulars"), Some(true))
    );
    let (s, ordered) = patch(
        &f,
        &roles_uri,
        "owner",
        json!([{ "id": low_id, "position": 2 }]),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{ordered}");
    let pos = |id: &str| {
        ordered
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .unwrap()["position"]
            .as_u64()
    };
    assert_eq!((pos(low_id), pos(mods_id)), (Some(2), Some(1)));
    let (s, _) = del(&f, &format!("{roles_uri}/{low_id}"), "owner").await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, e) = patch(
        &f,
        &format!("{roles_uri}/{low_id}"),
        "owner",
        json!({"name": "ghost"}),
    )
    .await;
    assert_eq!(
        (s, e["code"].as_u64()),
        (StatusCode::NOT_FOUND, Some(10011))
    );

    // Members: list with cursor, modify nick, timeout, kick.
    let (_, page) = get(&f, &format!("/guilds/{gid}/members?limit=2"), "carol").await;
    assert_eq!(page.as_array().unwrap().len(), 2);
    let after = page[1]["user"]["id"].as_str().unwrap();
    let (_, page) = get(
        &f,
        &format!("/guilds/{gid}/members?limit=10&after={after}"),
        "carol",
    )
    .await;
    assert_eq!(page.as_array().unwrap().len(), 1);
    let (s, m) = patch(
        &f,
        &format!("/guilds/{gid}/members/3"),
        "carol",
        json!({ "nick": "Caz" }),
    )
    .await;
    assert_eq!((s, m["nick"].as_str()), (StatusCode::OK, Some("Caz")));
    let (s, _) = patch(
        &f,
        &format!("/guilds/{gid}/members/2"),
        "carol",
        json!({ "nick": "Hax" }),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, m) = patch(
        &f,
        &format!("/guilds/{gid}/members/3"),
        "owner",
        json!({ "communication_disabled_until": "2027-01-15T09:00:00.000+00:00" }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert!(
        m["communication_disabled_until"]
            .as_str()
            .unwrap()
            .starts_with("2027-01-15T09:00:00")
    );
    let (s, _) = patch(
        &f,
        &format!("/guilds/{gid}/members/3"),
        "owner",
        json!({ "communication_disabled_until": "soon" }),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    let (s, _) = del(&f, &format!("/guilds/{gid}/members/1"), "bob").await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = del(&f, &format!("/guilds/{gid}/members/3"), "bob").await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, e) = get(&f, &format!("/guilds/{gid}/members/3"), "owner").await;
    assert_eq!(
        (s, e["code"].as_u64()),
        (StatusCode::NOT_FOUND, Some(10007))
    );
}

#[tokio::test]
async fn bans_flow() {
    let f = fixture();
    let (gid, _) = guild(&f).await;
    let uri = format!("/guilds/{gid}/bans");

    let (s, _) = get(&f, &uri, "bob").await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Reason travels in X-Audit-Log-Reason like on Discord.
    let req = Request::put(format!("/api/v1{uri}/3"))
        .header(header::AUTHORIZATION, "Bot owner")
        .header("x-audit-log-reason", "spamming")
        .body(Body::empty())
        .unwrap();
    let resp = f.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let (s, ban) = get(&f, &format!("{uri}/3"), "owner").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        (ban["reason"].as_str(), ban["user"]["id"].as_str()),
        (Some("spamming"), Some("3"))
    );
    let (_, list) = get(&f, &uri, "owner").await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    // Banned members are gone from the guild.
    let (s, _) = get(&f, &format!("/guilds/{gid}"), "carol").await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Pre-emptive ban with a body, owner and self protection.
    let (s, _) = put(
        &f,
        &format!("{uri}/50"),
        "owner",
        json!({ "delete_message_seconds": 3600 }),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = put(
        &f,
        &format!("{uri}/50"),
        "owner",
        json!({ "delete_message_seconds": 999_999 }),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = put(&f, &format!("{uri}/1"), "owner", json!({})).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    let (s, _) = del(&f, &format!("{uri}/3"), "owner").await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, e) = del(&f, &format!("{uri}/3"), "owner").await;
    assert_eq!(
        (s, e["code"].as_u64()),
        (StatusCode::NOT_FOUND, Some(10026))
    );
    let (_, list) = get(&f, &format!("{uri}?limit=1"), "owner").await;
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn bulk_ban_bans_what_it_can_and_lists_the_rest() {
    let f = fixture();
    let (gid, _) = guild(&f).await;
    let g = GuildId::new(gid.parse().unwrap());
    let uri = format!("/guilds/{gid}/bulk-ban");
    let mut events = f.bus.subscribe(Topic::Guild(g));

    // Plain members may not bulk ban.
    let (s, _) = post(&f, &uri, "bob", json!({ "user_ids": ["3"] })).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // Shape checks.
    let (s, _) = post(&f, &uri, "owner", json!({ "user_ids": [] })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = post(&f, &uri, "owner", json!({ "user_ids": ["x"] })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = post(
        &f,
        &uri,
        "owner",
        json!({ "user_ids": ["2"], "delete_message_seconds": 999_999 }),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Member 2, non-member 50 (pre-emptive) and the owner (1, never bannable).
    let (s, out) = post(
        &f,
        &uri,
        "owner",
        json!({ "user_ids": ["2", "50", "1", "2"], "reason": "raid" }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{out}");
    assert_eq!(out["banned_users"], json!(["2", "50"]));
    assert_eq!(out["failed_users"], json!(["1"]));
    let (_, list) = get(&f, &format!("/guilds/{gid}/bans"), "owner").await;
    assert_eq!(list.as_array().unwrap().len(), 2);
    let (s, _) = get(&f, &format!("/guilds/{gid}"), "bob").await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // One GUILD_BAN_ADD per banned user.
    let mut bans = 0;
    while let Some(ev) = next_event(&mut events).await {
        if matches!(ev, DomainEvent::MemberBanned { .. }) {
            bans += 1;
        }
    }
    assert_eq!(bans, 2);

    // Nobody bannable is an error, not an empty success.
    let (s, _) = post(&f, &uri, "owner", json!({ "user_ids": ["1"] })).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn mutations_publish_domain_events() {
    let f = fixture();
    let (gid, created) = guild(&f).await;
    let g = GuildId::new(gid.parse().unwrap());
    let mut events = f.bus.subscribe(Topic::Guild(g));
    let general = created["channels"][2]["id"].as_str().unwrap().to_owned();

    patch(
        &f,
        &format!("/guilds/{gid}"),
        "owner",
        json!({ "name": "Evented" }),
    )
    .await;
    let ev = next_event(&mut events).await.unwrap();
    let DomainEvent::GuildUpdated { payload, .. } = ev else {
        panic!("expected GuildUpdated, got {ev:?}");
    };
    assert_eq!(payload["name"], "Evented");
    assert!(payload["roles"].is_array());

    let (_, role) = post(
        &f,
        &format!("/guilds/{gid}/roles"),
        "owner",
        json!({ "name": "vip" }),
    )
    .await;
    let ev = next_event(&mut events).await.unwrap();
    let DomainEvent::RoleCreated {
        payload, role: rid, ..
    } = ev
    else {
        panic!("expected RoleCreated, got {ev:?}");
    };
    assert_eq!(payload["role"]["name"], "vip");
    assert_eq!(payload["guild_id"], gid);
    assert_eq!(rid.to_string(), role["id"].as_str().unwrap());

    put(
        &f,
        &format!(
            "/guilds/{gid}/members/2/roles/{}",
            role["id"].as_str().unwrap()
        ),
        "owner",
        json!({}),
    )
    .await;
    let ev = next_event(&mut events).await.unwrap();
    let DomainEvent::MemberUpdated { payload, .. } = ev else {
        panic!("expected MemberUpdated, got {ev:?}");
    };
    assert_eq!(payload["roles"][0], role["id"]);
    assert_eq!(payload["guild_id"], gid);

    patch(
        &f,
        &format!("/channels/{general}"),
        "owner",
        json!({ "topic": "news" }),
    )
    .await;
    let ev = next_event(&mut events).await.unwrap();
    let DomainEvent::ChannelUpdated { payload, .. } = ev else {
        panic!("expected ChannelUpdated, got {ev:?}");
    };
    assert_eq!(payload["topic"], "news");

    let (_, newch) = post(
        &f,
        &format!("/guilds/{gid}/channels"),
        "owner",
        json!({ "name": "fresh" }),
    )
    .await;
    let ev = next_event(&mut events).await.unwrap();
    assert!(matches!(ev, DomainEvent::ChannelCreated { .. }), "{ev:?}");
    del(
        &f,
        &format!("/channels/{}", newch["id"].as_str().unwrap()),
        "owner",
    )
    .await;
    // The viewer set is computed by the producer before the delete (Q159).
    let Some(DomainEvent::ChannelDeleted { viewers, .. }) = next_event(&mut events).await else {
        panic!("expected ChannelDeleted");
    };
    assert!(viewers.contains(&UserId::new(1)));

    put(&f, &format!("/guilds/{gid}/bans/2"), "owner", json!({})).await;
    assert!(matches!(
        next_event(&mut events).await,
        Some(DomainEvent::MemberLeft { .. })
    ));
    assert!(matches!(
        next_event(&mut events).await,
        Some(DomainEvent::MemberBanned { .. })
    ));
    del(&f, &format!("/guilds/{gid}/bans/2"), "owner").await;
    assert!(matches!(
        next_event(&mut events).await,
        Some(DomainEvent::MemberUnbanned { .. })
    ));

    // Failed (forbidden) mutations publish nothing.
    patch(
        &f,
        &format!("/guilds/{gid}"),
        "carol",
        json!({ "name": "Hacked" }),
    )
    .await;
    assert!(next_event(&mut events).await.is_none());

    // Deleting the guild is announced on the platform topic too.
    let mut platform = f.bus.subscribe(Topic::Platform);
    del(&f, &format!("/guilds/{gid}"), "owner").await;
    assert!(matches!(
        next_event(&mut platform).await,
        Some(DomainEvent::GuildDeleted { .. })
    ));
}

#[tokio::test]
async fn openapi_lists_guild_paths() {
    let f = fixture();
    let (s, doc) = call(&f, Method::GET, "/openapi.json", None, None).await;
    assert_eq!(s, StatusCode::OK);
    let paths = doc["paths"].as_object().unwrap();
    for p in [
        "/guilds",
        "/guilds/{guild_id}",
        "/users/@me/guilds",
        "/guilds/{guild_id}/channels",
        "/channels/{channel_id}",
        "/channels/{channel_id}/permissions/{overwrite_id}",
        "/guilds/{guild_id}/roles",
        "/guilds/{guild_id}/roles/{role_id}",
        "/guilds/{guild_id}/members/{user_id}/roles/{role_id}",
        "/guilds/{guild_id}/members",
        "/guilds/{guild_id}/members/{user_id}",
        "/guilds/{guild_id}/bans",
        "/guilds/{guild_id}/bans/{user_id}",
    ] {
        assert!(paths.contains_key(p), "missing {p}");
    }
    assert!(doc["components"]["schemas"]["GuildObject"].is_object());
    // The base document is still there.
    assert!(paths.contains_key("/users/@me") || paths.contains_key("/sr/csrf"));
}

type Logged = (AuditAction, Option<u64>, Option<String>);

#[derive(Default)]
struct Recorder(std::sync::Mutex<Vec<Logged>>);

#[async_trait::async_trait]
impl guilds::backend::AuditSink for Recorder {
    async fn record(
        &self,
        _guild: GuildId,
        _actor: UserId,
        action: AuditAction,
        target_id: Option<u64>,
        _changes: Vec<sr_chat::settings::audit::AuditChange>,
        reason: Option<&str>,
    ) {
        self.0
            .lock()
            .unwrap()
            .push((action, target_id, reason.map(str::to_owned)));
    }
}

#[tokio::test]
async fn guild_mutations_are_audited_and_role_creation_announces_shifted_roles() {
    let rec = Arc::new(Recorder::default());
    let f = fixture_with(Some(rec.clone()));
    let (gid, _) = guild(&f).await;
    let g = GuildId::new(gid.parse().unwrap());
    let mut events = f.bus.subscribe(Topic::Guild(g));

    let (_, first) = post(
        &f,
        &format!("/guilds/{gid}/roles"),
        "owner",
        json!({ "name": "a" }),
    )
    .await;
    assert!(matches!(
        next_event(&mut events).await,
        Some(DomainEvent::RoleCreated { .. })
    ));
    post(
        &f,
        &format!("/guilds/{gid}/roles"),
        "owner",
        json!({ "name": "b" }),
    )
    .await;
    assert!(matches!(
        next_event(&mut events).await,
        Some(DomainEvent::RoleCreated { .. })
    ));
    // The new role pushed "a" up one position, so it is announced too.
    let Some(DomainEvent::RoleUpdated { role, .. }) = next_event(&mut events).await else {
        panic!("expected RoleUpdated for the shifted role");
    };
    assert_eq!(role.get().to_string(), first["id"].as_str().unwrap());

    put(
        &f,
        &format!("/guilds/{gid}/bans/2"),
        "owner",
        json!({ "reason": "spam" }),
    )
    .await;
    let log = rec.0.lock().unwrap().clone();
    let actions: Vec<_> = log.iter().map(|e| e.0).collect();
    assert_eq!(
        actions,
        vec![
            AuditAction::RoleCreate,
            AuditAction::RoleCreate,
            AuditAction::MemberBanAdd
        ]
    );
    assert_eq!(log[2].1, Some(2));
    assert_eq!(log[2].2.as_deref(), Some("spam"));
}

fn png_uri(w: u32, h: u32, seed: u8) -> String {
    let img = image::RgbaImage::from_fn(w, h, |x, y| {
        image::Rgba([(x % 255) as u8, (y % 255) as u8, seed, 255])
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
async fn guild_icon_banner_splash_upload_replace_remove() {
    let f = fixture();
    let (gid, created) = guild(&f).await;
    assert!(
        created["icon"].is_null() && created["banner"].is_null() && created["splash"].is_null()
    );
    let mut events = f
        .bus
        .subscribe(Topic::Guild(GuildId::new(gid.parse().unwrap())));

    let (s, g) = patch(
        &f,
        &format!("/guilds/{gid}"),
        "owner",
        json!({ "icon": png_uri(120, 80, 1), "banner": png_uri(300, 100, 2), "splash": png_uri(64, 64, 3) }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{g}");
    let (icon, banner, splash) = (
        g["icon"].as_str().unwrap().to_owned(),
        g["banner"].as_str().unwrap().to_owned(),
        g["splash"].as_str().unwrap().to_owned(),
    );
    for h in [&icon, &banner, &splash] {
        assert!(sr_storage::media::is_valid_hash(h));
        assert!(
            sr_storage::ObjectStorage::head(&*f.storage, &format!("icons/{gid}/{h}"))
                .await
                .is_ok()
        );
    }
    // Same source, different kinds: separate objects.
    assert_ne!(icon, banner);
    // The icon is a centred square.
    let bytes = sr_storage::ObjectStorage::get(&*f.storage, &format!("icons/{gid}/{icon}"))
        .await
        .unwrap();
    let dims = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .unwrap()
        .into_dimensions()
        .unwrap();
    assert_eq!(dims, (80, 80));
    // Members see the hashes on GET, and GUILD_UPDATE carries them.
    let (_, g) = get(&f, &format!("/guilds/{gid}"), "bob").await;
    assert_eq!(g["icon"], icon.as_str());
    let ev = next_event(&mut events).await.expect("GUILD_UPDATE");
    assert!(format!("{ev:?}").contains(&icon));

    // Replace the icon, remove the banner, leave the splash.
    let (s, g) = patch(
        &f,
        &format!("/guilds/{gid}"),
        "owner",
        json!({ "icon": png_uri(40, 40, 9), "banner": null }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{g}");
    assert_ne!(g["icon"], icon.as_str());
    assert!(g["banner"].is_null());
    assert_eq!(g["splash"], splash.as_str());
    let head = |h: &str| {
        let key = format!("icons/{gid}/{h}");
        let st = f.storage.clone();
        async move { sr_storage::ObjectStorage::head(&*st, &key).await.is_ok() }
    };
    assert!(!head(&icon).await && !head(&banner).await && head(&splash).await);

    // Bad uploads and missing permission change nothing.
    let (s, _) = patch(
        &f,
        &format!("/guilds/{gid}"),
        "owner",
        json!({ "icon": "data:text/html;base64,PGI+" }),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let before = sr_storage::ObjectStorage::list(&*f.storage, "icons/")
        .await
        .unwrap()
        .len();
    let (s, _) = patch(
        &f,
        &format!("/guilds/{gid}"),
        "bob",
        json!({ "icon": png_uri(32, 32, 5) }),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(
        sr_storage::ObjectStorage::list(&*f.storage, "icons/")
            .await
            .unwrap()
            .len(),
        before
    );
}

fn name_user(f: &Fx, id: u64, username: &str, display: Option<&str>) {
    f.repo.store.lock().unwrap().users.insert(
        UserId::new(id),
        sr_chat::repo::UserSummary {
            id: UserId::new(id),
            username: username.into(),
            display_name: display.map(str::to_owned),
            avatar_key: None,
            banner_key: None,
            bot: false,
            primary_guild: None,
        },
    );
}

fn ids_of(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|m| m["user"]["id"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn member_search_role_filter_and_role_members() {
    let f = fixture();
    let (gid, _) = guild(&f).await;
    name_user(&f, 1, "olivia", None);
    name_user(&f, 2, "bobby", Some("Robert"));
    name_user(&f, 3, "carol", None);
    let members = format!("/guilds/{gid}/members");

    let (s, v) = get(&f, &format!("{members}?limit=100&query=ROB"), "carol").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(ids_of(&v), ["2"]);
    // Discord's search endpoint is the same match, query required.
    let (s, v) = get(
        &f,
        &format!("{members}/search?query=oliv&limit=10"),
        "carol",
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(ids_of(&v), ["1"]);
    let (s, _) = get(&f, &format!("{members}/search?query=%20"), "carol").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = get(&f, &format!("{members}/search"), "carol").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = get(&f, &format!("{members}?query={}", "x".repeat(65)), "carol").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // Outsiders still get nothing.
    let (s, _) = get(&f, &format!("{members}?query=a"), "stranger").await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Role filter and the role-members listing.
    let (_, role) = post(
        &f,
        &format!("/guilds/{gid}/roles"),
        "owner",
        json!({ "name": "crew" }),
    )
    .await;
    let rid = role["id"].as_str().unwrap();
    let (s, _) = put(
        &f,
        &format!("{members}/2/roles/{rid}"),
        "owner",
        json!(null),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, v) = get(&f, &format!("{members}?limit=100&role_id={rid}"), "carol").await;
    assert_eq!(ids_of(&v), ["2"]);
    let (s, v) = get(&f, &format!("/guilds/{gid}/roles/{rid}/members"), "carol").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(ids_of(&v), ["2"]);
    let (_, v) = get(&f, &format!("/guilds/{gid}/roles/{gid}/members"), "carol").await;
    assert_eq!(ids_of(&v), ["1", "2", "3"]);
    let (s, _) = get(&f, &format!("/guilds/{gid}/roles/4242/members"), "carol").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = get(&f, &format!("{members}?role_id=nope"), "carol").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn prune_preview_and_execute_are_permission_gated_dispatched_and_audited() {
    let rec = Arc::new(Recorder::default());
    let f = fixture_with(Some(rec.clone()));
    let (gid, _) = guild(&f).await;
    let g = GuildId::new(gid.parse().unwrap());
    // Everyone joined "now"; make Bob and Carol old.
    for m in f.repo.store.lock().unwrap().members.iter_mut() {
        m.joined_at_ms = T0 - 40 * 86_400_000;
    }
    let mut events = f.bus.subscribe(Topic::Guild(g));
    let uri = format!("/guilds/{gid}/prune");

    // Plain members may not even preview.
    let (s, _) = get(&f, &format!("{uri}?days=7"), "bob").await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = post(&f, &uri, "bob", json!({ "days": 7 })).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // Bad input.
    for q in ["days=0", "days=31", "days=7&include_roles=nope"] {
        let (s, _) = get(&f, &format!("{uri}?{q}"), "owner").await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{q}");
    }

    let (s, v) = get(&f, &format!("{uri}?days=7"), "owner").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["pruned"], 2);
    // Preview changes nothing and writes no audit entry.
    assert!(rec.0.lock().unwrap().is_empty());
    let (_, list) = get(&f, &format!("/guilds/{gid}/members?limit=100"), "owner").await;
    assert_eq!(list.as_array().unwrap().len(), 3);

    let (s, v) = post(&f, &uri, "owner", json!({ "days": 7 })).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["pruned"], 2);
    let (_, list) = get(&f, &format!("/guilds/{gid}/members?limit=100"), "owner").await;
    assert_eq!(ids_of(&list), ["1"], "the owner is never pruned");
    // One MemberLeft per removed member, one audit entry for the prune.
    let mut left = Vec::new();
    while let Some(ev) = next_event(&mut events).await {
        if let DomainEvent::MemberLeft { user, .. } = ev {
            left.push(user.get());
        }
    }
    left.sort_unstable();
    assert_eq!(left, [2, 3]);
    let log = rec.0.lock().unwrap().clone();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].0, AuditAction::MemberPrune);
    // Nothing left to do, and an empty prune is not audited.
    let (_, v) = post(&f, &uri, "owner", json!({})).await;
    assert_eq!(v["pruned"], 0);
    assert_eq!(rec.0.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn afk_channel_and_timeout_are_validated_and_returned() {
    let f = fixture();
    let (gid, created) = guild(&f).await;
    let uri = format!("/guilds/{gid}");
    // Defaults: no AFK channel, five minutes.
    assert!(created["afk_channel_id"].is_null());
    assert_eq!(created["afk_timeout"], 300);

    let text = created["channels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == 0)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (s, voice) = post(
        &f,
        &format!("{uri}/channels"),
        "owner",
        json!({ "name": "afk-lounge", "type": 2 }),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{voice}");
    let voice = voice["id"].as_str().unwrap().to_owned();

    // Only a voice channel of this guild qualifies.
    for bad in [text.as_str(), "4242"] {
        let (s, e) = patch(&f, &uri, "owner", json!({ "afk_channel_id": bad })).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}: {e}");
    }
    let (s, _) = patch(&f, &uri, "owner", json!({ "afk_timeout": 1234 })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // MANAGE_GUILD only.
    let (s, _) = patch(&f, &uri, "bob", json!({ "afk_timeout": 60 })).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    let (s, g) = patch(
        &f,
        &uri,
        "owner",
        json!({ "afk_channel_id": voice, "afk_timeout": 900 }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{g}");
    assert_eq!(g["afk_channel_id"], voice.as_str());
    assert_eq!(g["afk_timeout"], 900);
    let (_, g) = get(&f, &uri, "bob").await;
    assert_eq!(
        (g["afk_channel_id"].as_str(), g["afk_timeout"].as_u64()),
        (Some(voice.as_str()), Some(900))
    );

    // An unrelated edit keeps them; null clears the channel.
    let (_, g) = patch(&f, &uri, "owner", json!({ "name": "Still AFK" })).await;
    assert_eq!(g["afk_channel_id"], voice.as_str());
    let (_, g) = patch(&f, &uri, "owner", json!({ "afk_channel_id": null })).await;
    assert!(g["afk_channel_id"].is_null());
    assert_eq!(g["afk_timeout"], 900);
}

#[tokio::test]
async fn create_from_a_builtin_template_in_one_step() {
    let f = fixture();
    let mut platform = f.bus.subscribe(Topic::Platform);
    let (s, g) = post(
        &f,
        "/guilds",
        "owner",
        json!({ "name": "Squad", "template": "gaming" }),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{g}");
    let names: Vec<&str> = g["channels"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect();
    for want in [
        "general",
        "game-chat",
        "clips-and-highlights",
        "Squad 1",
        "Games",
    ] {
        assert!(names.contains(&want), "{want} in {names:?}");
    }
    assert_eq!(g["roles"][0]["id"], g["id"], "@everyone is the first role");
    let sys = g["system_channel_id"].as_str().unwrap();
    let general = g["channels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == sys)
        .unwrap();
    assert_eq!(general["name"], "general");

    // Exactly one GUILD_CREATE domain event.
    let ev = next_event(&mut platform).await.unwrap();
    assert!(matches!(ev, DomainEvent::GuildCreated { .. }), "{ev:?}");
    assert!(next_event(&mut platform).await.is_none(), "only one event");

    // The creator can read it back; an unknown template is refused and creates nothing.
    let id = g["id"].as_str().unwrap();
    assert_eq!(
        get(&f, &format!("/guilds/{id}"), "owner").await.0,
        StatusCode::OK
    );
    let (s, e) = post(
        &f,
        "/guilds",
        "owner",
        json!({ "name": "Nope", "template": "no-such-template" }),
    )
    .await;
    assert!(s.is_client_error(), "{s} {e}");
    let (_, mine) = get(&f, "/users/@me/guilds", "owner").await;
    assert_eq!(mine.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn role_emoji_and_icon_replace_each_other_and_clean_up() {
    let f = fixture();
    let (gid, _) = guild(&f).await;
    let roles_uri = format!("/guilds/{gid}/roles");
    let (s, role) = post(&f, &roles_uri, "owner", json!({ "name": "vip" })).await;
    assert_eq!(s, StatusCode::OK, "{role}");
    let rid = role["id"].as_str().unwrap().to_owned();
    let uri = format!("{roles_uri}/{rid}");

    // An emoji is stored as given; plain text is not an emoji.
    let (s, r) = patch(&f, &uri, "owner", json!({ "unicode_emoji": "🎉" })).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["unicode_emoji"], "🎉");
    let (s, _) = patch(&f, &uri, "owner", json!({ "unicode_emoji": "vip" })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // An image replaces the emoji, is cropped to a square and keyed by the guild.
    let (s, r) = patch(&f, &uri, "owner", json!({ "icon": png_uri(120, 80, 1) })).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert!(r["unicode_emoji"].is_null());
    let icon = r["icon"].as_str().unwrap().to_owned();
    let key = format!("icons/{gid}/{icon}");
    assert!(
        sr_storage::ObjectStorage::head(&*f.storage, &key)
            .await
            .is_ok()
    );

    // An edit that does not mention the icon keeps it.
    let (_, r) = patch(&f, &uri, "owner", json!({ "name": "vip 2" })).await;
    assert_eq!(r["icon"], icon.as_str());

    // An emoji replaces the image and the stored object goes with it.
    let (_, r) = patch(&f, &uri, "owner", json!({ "unicode_emoji": "⭐" })).await;
    assert_eq!(r["unicode_emoji"], "⭐");
    assert!(r["icon"].is_null());
    assert!(
        sr_storage::ObjectStorage::head(&*f.storage, &key)
            .await
            .is_err()
    );
    let (_, r) = patch(&f, &uri, "owner", json!({ "icon": png_uri(40, 40, 2) })).await;
    let icon2 = r["icon"].as_str().unwrap().to_owned();
    assert_ne!(icon2, icon);
    assert!(
        sr_storage::ObjectStorage::head(&*f.storage, &key)
            .await
            .is_err()
    );
    let (s, r) = patch(&f, &uri, "owner", json!({ "icon": null })).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert!(r["icon"].is_null());
    assert!(
        sr_storage::ObjectStorage::head(&*f.storage, &format!("icons/{gid}/{icon2}"))
            .await
            .is_err()
    );

    // Without Manage Roles nothing is stored.
    let (s, _) = patch(&f, &uri, "bob", json!({ "icon": png_uri(40, 40, 3) })).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn text_channels_become_announcement_channels_and_back() {
    let f = fixture();
    let (_gid, created) = guild(&f).await;
    let general = created["channels"][2]["id"].as_str().unwrap().to_owned();
    let uri = format!("/channels/{general}");

    // Members cannot convert; only text and announcement channels swap.
    let (s, _) = patch(&f, &uri, "bob", json!({ "type": 5 })).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, e) = patch(&f, &uri, "owner", json!({ "type": 2 })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    let (s, e) = patch(&f, &uri, "owner", json!({ "type": 4242 })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");

    // The auto-publish flag only sticks on announcement channels.
    let auto = 1u64 << 24;
    let (s, c) = patch(&f, &uri, "owner", json!({ "flags": auto })).await;
    assert_eq!(s, StatusCode::OK, "{c}");
    assert_eq!(c["flags"].as_u64().unwrap() & auto, 0, "a text channel ignores it");
    let (s, c) = patch(&f, &uri, "owner", json!({ "type": 5, "flags": auto })).await;
    assert_eq!(s, StatusCode::OK, "{c}");
    assert_eq!(c["type"], 5);
    assert_eq!(c["flags"].as_u64().unwrap() & auto, auto);
    // Other edits keep the flag; clearing it is explicit.
    let (_, c) = patch(&f, &uri, "owner", json!({ "topic": "news" })).await;
    assert_eq!(c["flags"].as_u64().unwrap() & auto, auto);
    let (_, c) = patch(&f, &uri, "owner", json!({ "flags": 0 })).await;
    assert_eq!(c["flags"].as_u64().unwrap() & auto, 0);
    // Back to text.
    let (s, c) = patch(&f, &uri, "owner", json!({ "type": 0 })).await;
    assert_eq!(s, StatusCode::OK, "{c}");
    assert_eq!(c["type"], 0);
}

#[tokio::test]
async fn editing_a_channel_without_changing_its_type_is_not_a_conversion() {
    let f = fixture();
    let (_gid, created) = guild(&f).await;
    let voice = created["channels"][3]["id"].as_str().unwrap().to_owned();
    // Sending the channel's own type is a no-op, as Discord clients do.
    let (s, c) = patch(&f, &format!("/channels/{voice}"), "owner", json!({ "type": 2, "name": "lounge" })).await;
    assert_eq!(s, StatusCode::OK, "{c}");
    assert_eq!((c["type"].as_u64(), c["name"].as_str()), (Some(2), Some("lounge")));
}
