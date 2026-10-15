//! Endpoint tests over the in-memory repositories with one-shot requests.

use super::{DynRepo, SocialDeps, events, router};
use crate::{
    ApiSettings, AppState,
    auth::{BotIdentity, memory::MemoryBots},
    build_router,
    routes::messages::{self, FixedEntitlements, MessagesDeps},
};
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use futures::{StreamExt, stream::BoxStream};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sr_chat::Member;
use sr_chat::guild_rules::plan_new_guild;
use sr_chat::memory::MemoryBackend;
use sr_chat::repo::GuildRepository;
use sr_chat::social::SocialLimits;
use sr_chat::social::memory::MemoryRepo;
use sr_chat::social::profile::Profile;
use sr_chat::social::repo::{ReadStateRepository, RelationshipRepository, SocialRepository};
use sr_core::{ChannelId, Clock, GuildId, ManualClock, SnowflakeGenerator, UserId};
use sr_events::{DomainEvent, EventBus, InProcessBus, Topic};
use sr_storage::{ObjectStorage, ObjectStoreStorage};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

const T0: i64 = 1_800_000_000_000;
const ALICE: u64 = 1; // guild owner
const BOB: u64 = 2; // guild member
const CAROL: u64 = 3; // guild member
const DAVE: u64 = 4; // not in the guild

struct Env {
    app: Router,
    repo: Arc<MemoryRepo>,
    backend: MemoryBackend,
    bus: Arc<InProcessBus>,
    clock: Arc<ManualClock>,
    storage: Arc<ObjectStoreStorage>,
    guild: GuildId,
    ch: ChannelId,
}

fn token(uid: u64) -> &'static str {
    match uid {
        ALICE => "alice",
        BOB => "bob",
        CAROL => "carol",
        _ => "dave",
    }
}

async fn env() -> Env {
    let clock = Arc::new(ManualClock::new(T0));
    let dyn_clock: Arc<dyn Clock> = clock.clone();
    let backend = MemoryBackend::new();
    let bus = Arc::new(InProcessBus::new());
    let storage = Arc::new(ObjectStoreStorage::in_memory());

    let gen_ids = SnowflakeGenerator::new(dyn_clock.clone(), 9, 9).unwrap();
    let bp = plan_new_guild(UserId::new(ALICE), "Club", T0, || gen_ids.generate()).unwrap();
    let (guild, ch) = (bp.guild.id, bp.channels[2].id);
    backend.insert_guild(&bp).await.unwrap();
    for uid in [BOB, CAROL] {
        backend.data().members.push(Member {
            guild_id: guild,
            user_id: UserId::new(uid),
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

    let repo = Arc::new(MemoryRepo::default());
    for uid in [ALICE, BOB, CAROL, DAVE] {
        repo.add_user(UserId::new(uid));
        repo.store.lock().unwrap().profiles.insert(
            UserId::new(uid),
            Profile {
                user: UserId::new(uid),
                ..Profile::default()
            },
        );
    }

    let bots = Arc::new(MemoryBots::new());
    for uid in [ALICE, BOB, CAROL, DAVE] {
        bots.insert(
            token(uid),
            BotIdentity {
                user_id: i64::try_from(uid).unwrap(),
                application_id: 1,
            },
        );
    }
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;

    // The same hook the server installs: sending reopens closed DMs.
    let hooks: Arc<dyn sr_chat::message_hooks::MessageHooks> =
        Arc::new(sr_chat::social::SocialService::new(
            DynRepo(repo.clone() as Arc<dyn SocialRepository>),
            dyn_clock.clone(),
            SnowflakeGenerator::new(dyn_clock.clone(), 1, 4).unwrap(),
            SocialLimits::default(),
        ));
    let social = Arc::new(
        SocialDeps::new(
            DynRepo(repo.clone() as Arc<dyn SocialRepository>),
            Arc::new(backend.clone()),
            bus.clone(),
            dyn_clock.clone(),
            SocialLimits::default(),
            1,
            3,
        )
        .unwrap(),
    );
    let messages = MessagesDeps::new(
        backend.clone(),
        Arc::new(backend.clone()),
        storage.clone(),
        bus.clone(),
        Arc::new(FixedEntitlements(1 << 20)),
        Arc::new(backend.clone()),
        Some(hooks),
        dyn_clock.clone(),
        1,
        1,
    )
    .unwrap()
    .with_dm_announcer(social.clone());
    let state = AppState::builder()
        .bots(bots)
        .clock(dyn_clock)
        .settings(settings)
        .extension(Arc::new(messages))
        .extension(social)
        .extension(Arc::new(crate::routes::media::MediaDeps::new(
            storage.clone(),
        )))
        .build();
    Env {
        app: build_router(state, router().merge(messages::router())),
        repo,
        backend,
        bus,
        clock,
        storage,
        guild,
        ch,
    }
}

impl Env {
    async fn call(
        &self,
        method: Method,
        path: &str,
        who: u64,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/v1{path}"))
            .header("authorization", format!("Bot {}", token(who)));
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

    async fn get(&self, path: &str, who: u64) -> (StatusCode, Value) {
        self.call(Method::GET, path, who, None).await
    }
    async fn put(&self, path: &str, who: u64, body: Option<Value>) -> (StatusCode, Value) {
        self.call(Method::PUT, path, who, body).await
    }
    async fn patch(&self, path: &str, who: u64, body: Value) -> (StatusCode, Value) {
        self.call(Method::PATCH, path, who, Some(body)).await
    }
    async fn post(&self, path: &str, who: u64, body: Value) -> (StatusCode, Value) {
        self.call(Method::POST, path, who, Some(body)).await
    }
    async fn delete(&self, path: &str, who: u64) -> (StatusCode, Value) {
        self.call(Method::DELETE, path, who, None).await
    }

    async fn befriend(&self, a: u64, b: u64) {
        assert_eq!(
            self.put(&format!("/users/@me/relationships/{b}"), a, None)
                .await
                .0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            self.put(&format!("/users/@me/relationships/{a}"), b, None)
                .await
                .0,
            StatusCode::NO_CONTENT
        );
    }

    fn user_events(&self, uid: u64) -> BoxStream<'static, DomainEvent> {
        self.bus.subscribe(Topic::User(UserId::new(uid)))
    }
}

async fn next_event(s: &mut BoxStream<'static, DomainEvent>) -> DomainEvent {
    tokio::time::timeout(Duration::from_secs(1), s.next())
        .await
        .expect("no event")
        .expect("stream ended")
}

fn kinds(v: &Value) -> Vec<(String, i64)> {
    let mut out: Vec<(String, i64)> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["id"].as_str().unwrap().to_owned(),
                r["type"].as_i64().unwrap(),
            )
        })
        .collect();
    out.sort();
    out
}

fn is_empty_list(v: &Value) -> bool {
    v.as_array().is_some_and(Vec::is_empty)
}

#[tokio::test]
async fn friend_request_accept_nickname_and_remove() {
    let e = env().await;
    let (s, _) = e.put("/users/@me/relationships/2", ALICE, None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(
        kinds(&e.get("/users/@me/relationships", ALICE).await.1),
        [("2".into(), 4)]
    );
    assert_eq!(
        kinds(&e.get("/users/@me/relationships", BOB).await.1),
        [("1".into(), 3)]
    );
    // Accepting is the same PUT, with an explicit type.
    let (s, _) = e
        .put("/users/@me/relationships/1", BOB, Some(json!({"type": 1})))
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(
        kinds(&e.get("/users/@me/relationships", ALICE).await.1),
        [("2".into(), 1)]
    );

    let (s, v) = e
        .patch(
            "/users/@me/relationships/2",
            ALICE,
            json!({"nickname": "  Bobby "}),
        )
        .await;
    assert_eq!((s, v["nickname"].as_str()), (StatusCode::OK, Some("Bobby")));
    let (s, v) = e.get("/users/@me/relationships/2", ALICE).await;
    assert_eq!((s, v["type"].as_i64()), (StatusCode::OK, Some(1)));
    // Bob's side carries no nickname.
    assert!(e.get("/users/@me/relationships/1", BOB).await.1["nickname"].is_null());
    let long = "x".repeat(33);
    let (s, v) = e
        .patch(
            "/users/@me/relationships/2",
            ALICE,
            json!({ "nickname": long }),
        )
        .await;
    assert_eq!(
        (s, v["code"].as_i64()),
        (StatusCode::BAD_REQUEST, Some(50035))
    );

    assert_eq!(
        e.delete("/users/@me/relationships/2", ALICE).await.0,
        StatusCode::NO_CONTENT
    );
    assert!(is_empty_list(
        &e.get("/users/@me/relationships", ALICE).await.1
    ));
    assert!(is_empty_list(
        &e.get("/users/@me/relationships", BOB).await.1
    ));
    assert_eq!(
        e.get("/users/@me/relationships/2", ALICE).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn relationship_input_errors() {
    let e = env().await;
    assert_eq!(
        e.put("/users/@me/relationships/1", ALICE, None).await.0,
        StatusCode::BAD_REQUEST,
        "self"
    );
    assert_eq!(
        e.put("/users/@me/relationships/99", ALICE, None).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.put("/users/@me/relationships/nope", ALICE, None).await.0,
        StatusCode::NOT_FOUND
    );
    let (s, v) = e
        .put(
            "/users/@me/relationships/2",
            ALICE,
            Some(json!({"type": 9})),
        )
        .await;
    assert_eq!(
        (s, v["code"].as_i64()),
        (StatusCode::BAD_REQUEST, Some(50035))
    );
    assert_eq!(
        e.delete("/users/@me/relationships/2", ALICE).await.0,
        StatusCode::BAD_REQUEST,
        "nothing to remove"
    );
}

#[tokio::test]
async fn block_and_unblock() {
    let e = env().await;
    e.befriend(ALICE, BOB).await;
    let (s, _) = e
        .put(
            "/users/@me/relationships/2",
            ALICE,
            Some(json!({"type": 2})),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(
        kinds(&e.get("/users/@me/relationships", ALICE).await.1),
        [("2".into(), 2)]
    );
    assert!(is_empty_list(
        &e.get("/users/@me/relationships", BOB).await.1
    ));
    // A blocked user cannot befriend or DM.
    assert_ne!(
        e.put("/users/@me/relationships/1", BOB, None).await.0,
        StatusCode::NO_CONTENT
    );
    let (s, v) = e
        .post("/users/@me/channels", BOB, json!({"recipient_id": "1"}))
        .await;
    assert_eq!(
        (s, v["code"].as_i64()),
        (StatusCode::FORBIDDEN, Some(50007))
    );

    assert_eq!(
        e.delete("/users/@me/relationships/2", ALICE).await.0,
        StatusCode::NO_CONTENT
    );
    assert!(is_empty_list(
        &e.get("/users/@me/relationships", ALICE).await.1
    ));
}

#[tokio::test]
async fn ignore_keeps_friendship() {
    let e = env().await;
    e.befriend(ALICE, BOB).await;
    assert_eq!(
        e.put("/users/@me/ignores/2", ALICE, None).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.put("/users/@me/ignores/2", ALICE, None).await.0,
        StatusCode::NO_CONTENT
    );
    let (_, v) = e.get("/users/@me/ignores", ALICE).await;
    assert_eq!(v.as_array().unwrap().len(), 1);
    assert_eq!(v[0]["id"], "2");
    assert_eq!(
        kinds(&e.get("/users/@me/relationships", ALICE).await.1),
        [("2".into(), 1)]
    );
    assert_eq!(
        e.delete("/users/@me/ignores/2", ALICE).await.0,
        StatusCode::NO_CONTENT
    );
    assert!(is_empty_list(&e.get("/users/@me/ignores", ALICE).await.1));
}

#[tokio::test]
async fn notes_are_private_and_bounded() {
    let e = env().await;
    assert_eq!(
        e.get("/users/@me/notes/2", ALICE).await.0,
        StatusCode::NOT_FOUND
    );
    let (s, _) = e
        .put(
            "/users/@me/notes/2",
            ALICE,
            Some(json!({"note": "met at the lounge"})),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, v) = e.get("/users/@me/notes/2", ALICE).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["note"], "met at the lounge");
    assert_eq!(v["note_user_id"], "2");
    assert_eq!(
        e.get("/users/@me/notes/1", BOB).await.0,
        StatusCode::NOT_FOUND
    );
    let (s, v) = e
        .put(
            "/users/@me/notes/2",
            ALICE,
            Some(json!({"note": "n".repeat(257)})),
        )
        .await;
    assert_eq!(
        (s, v["code"].as_i64()),
        (StatusCode::BAD_REQUEST, Some(50035))
    );
    e.put("/users/@me/notes/2", ALICE, Some(json!({"note": ""})))
        .await;
    assert_eq!(
        e.get("/users/@me/notes/2", ALICE).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        e.put("/users/@me/notes/1", ALICE, Some(json!({"note": "me"})))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn one_to_one_dm_open_list_close_and_event() {
    let e = env().await;
    let mut alice_events = e.user_events(ALICE);
    e.repo
        .store
        .lock()
        .unwrap()
        .shared_guilds
        .insert((UserId::new(ALICE), UserId::new(BOB)));
    let (s, v) = e
        .post("/users/@me/channels", ALICE, json!({"recipient_id": "2"}))
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["type"], 1);
    assert_eq!(v["recipients"].as_array().unwrap().len(), 1);
    assert_eq!(v["recipients"][0]["id"], "2");
    let id = v["id"].as_str().unwrap().to_owned();

    // Created once; the adapter rides CHANNEL_CREATE on the user topic.
    match next_event(&mut alice_events).await {
        DomainEvent::UserScoped { name, payload, .. } => {
            assert_eq!(name, "CHANNEL_CREATE");
            assert_eq!(payload["id"], id.as_str());
            assert_eq!(payload["recipients"][0]["id"], "2");
        }
        other => panic!("unexpected {other:?}"),
    }
    let (_, again) = e
        .post("/users/@me/channels", ALICE, json!({"recipient_id": "2"}))
        .await;
    assert_eq!(again["id"], id.as_str());

    let (_, list) = e.get("/users/@me/channels", ALICE).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    // Bob sees it as a message request (non-friend first DM, Q112).
    let (_, bob) = e.get("/users/@me/channels", BOB).await;
    assert_eq!(bob[0]["is_message_request"], true);
    assert_eq!(
        e.put(
            &format!("/users/@me/channels/{id}/request"),
            BOB,
            Some(json!({"state": "accepted"}))
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert!(e.get("/users/@me/channels", BOB).await.1[0]["is_message_request"].is_null());
    assert_eq!(
        e.put(
            &format!("/users/@me/channels/{id}/request"),
            BOB,
            Some(json!({"state": "pending"}))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );

    assert_eq!(
        e.delete(&format!("/users/@me/channels/{id}"), ALICE)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert!(is_empty_list(&e.get("/users/@me/channels", ALICE).await.1));
    // Outsiders cannot touch it.
    assert_eq!(
        e.delete(&format!("/users/@me/channels/{id}"), DAVE).await.0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn dm_without_shared_server_or_friendship_is_refused() {
    let e = env().await;
    let (s, v) = e
        .post("/users/@me/channels", ALICE, json!({"recipient_id": "4"}))
        .await;
    assert_eq!(
        (s, v["code"].as_i64()),
        (StatusCode::FORBIDDEN, Some(50007))
    );
    let (s, _) = e.post("/users/@me/channels", ALICE, json!({})).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn group_dm_lifecycle() {
    let e = env().await;
    e.befriend(ALICE, BOB).await;
    e.befriend(ALICE, CAROL).await;
    e.befriend(ALICE, DAVE).await;
    let mut carol_events = e.user_events(CAROL);

    let (s, v) = e
        .post(
            "/users/@me/channels",
            ALICE,
            json!({"recipients": ["2", "3"], "name": "crew"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(
        (v["type"].as_i64(), v["owner_id"].as_str()),
        (Some(3), Some("1"))
    );
    assert_eq!(v["recipients"].as_array().unwrap().len(), 2);
    let id = v["id"].as_str().unwrap().to_owned();
    // Carol is told, and sees everyone but herself.
    match next_event(&mut carol_events).await {
        DomainEvent::UserScoped { payload, .. } => {
            let mut ids: Vec<&str> = payload["recipients"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["id"].as_str().unwrap())
                .collect();
            ids.sort_unstable();
            assert_eq!(ids, ["1", "2"]);
        }
        other => panic!("unexpected {other:?}"),
    }

    // Rename (any member), then icon removal.
    let (s, v) = e
        .patch(
            &format!("/users/@me/channels/{id}"),
            BOB,
            json!({"name": "the crew"}),
        )
        .await;
    assert_eq!((s, v["name"].as_str()), (StatusCode::OK, Some("the crew")));
    match next_event(&mut carol_events).await {
        DomainEvent::UserScoped { name, payload, .. } => {
            assert_eq!(name, "CHANNEL_UPDATE");
            assert_eq!(payload["name"], "the crew");
        }
        other => panic!("unexpected {other:?}"),
    }
    let (s, _) = e
        .patch(
            &format!("/users/@me/channels/{id}"),
            BOB,
            json!({"icon": null}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);

    // Add a friend of the adder.
    let (s, _) = e
        .put(&format!("/channels/{id}/recipients/4"), ALICE, None)
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(
        e.get("/users/@me/channels", DAVE).await.1[0]["recipients"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    // Only the owner removes others.
    assert_eq!(
        e.delete(&format!("/channels/{id}/recipients/4"), BOB)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.delete(&format!("/channels/{id}/recipients/4"), ALICE)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert!(is_empty_list(&e.get("/users/@me/channels", DAVE).await.1));

    // Ownership transfer is owner-only.
    assert_eq!(
        e.patch(
            &format!("/users/@me/channels/{id}"),
            BOB,
            json!({"owner_id": "2"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (s, v) = e
        .patch(
            &format!("/users/@me/channels/{id}"),
            ALICE,
            json!({"owner_id": "2"}),
        )
        .await;
    assert_eq!((s, v["owner_id"].as_str()), (StatusCode::OK, Some("2")));

    // The owner leaves; the group survives with the remaining members.
    assert_eq!(
        e.delete(&format!("/users/@me/channels/{id}"), BOB).await.0,
        StatusCode::NO_CONTENT
    );
    let (_, v) = e.get("/users/@me/channels", CAROL).await;
    assert_eq!(v[0]["recipients"].as_array().unwrap().len(), 1);
    assert_ne!(v[0]["owner_id"], "2");
}

#[tokio::test]
async fn adding_to_a_one_to_one_spawns_a_group() {
    let e = env().await;
    e.befriend(ALICE, BOB).await;
    e.befriend(ALICE, CAROL).await;
    let (_, dm) = e
        .post("/users/@me/channels", ALICE, json!({"recipient_id": "2"}))
        .await;
    let id = dm["id"].as_str().unwrap();
    let (s, v) = e
        .put(&format!("/channels/{id}/recipients/3"), ALICE, None)
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["type"], 3);
    assert_ne!(v["id"].as_str(), Some(id));
    assert_eq!(v["recipients"].as_array().unwrap().len(), 2);
    // A group cannot be made from non-friends.
    let (s, _) = e
        .post(
            "/users/@me/channels",
            BOB,
            json!({"recipients": ["1", "4"]}),
        )
        .await;
    assert_ne!(s, StatusCode::OK);
}

#[tokio::test]
async fn ack_moves_forward_and_resets_mentions() {
    let e = env().await;
    let path = |m: u64| format!("/channels/{}/messages/{m}/ack", e.ch);
    e.repo
        .add_mentions(&[UserId::new(BOB)], e.ch, 3)
        .await
        .unwrap();
    assert_eq!(
        e.get("/users/@me/read-states", BOB).await.1["entries"][0]["mention_count"],
        3
    );

    let (s, v) = e.post(&path(500), BOB, json!({})).await;
    assert_eq!((s, v["token"].is_null()), (StatusCode::OK, true));
    let (_, v) = e.get("/users/@me/read-states", BOB).await;
    assert_eq!(v["entries"][0]["id"], e.ch.to_string().as_str());
    assert_eq!(v["entries"][0]["last_message_id"], "500");
    assert_eq!(v["entries"][0]["mention_count"], 0);

    // An older ack never moves the marker back; manual does.
    e.post(&path(400), BOB, json!({})).await;
    assert_eq!(
        e.get("/users/@me/read-states", BOB).await.1["entries"][0]["last_message_id"],
        "500"
    );
    e.post(&path(400), BOB, json!({"manual": true, "mention_count": 2}))
        .await;
    let (_, v) = e.get("/users/@me/read-states", BOB).await;
    assert_eq!(v["entries"][0]["last_message_id"], "400");
    assert_eq!(v["entries"][0]["mention_count"], 2);

    // Per-user state.
    assert!(is_empty_list(
        &e.get("/users/@me/read-states", CAROL).await.1["entries"]
    ));
    // Not a member of the guild: no access. Malformed ids: 404.
    assert_eq!(
        e.post(&path(500), DAVE, json!({})).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.post(
            &format!("/channels/{}/messages/zzz/ack", e.ch),
            BOB,
            json!({})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    // The web client posts without a body.
    let (s, _) = e.call(Method::POST, &path(600), BOB, None).await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn bulk_ack_and_pins_ack() {
    let e = env().await;
    let body = json!({"read_states": [{"channel_id": e.ch.to_string(), "message_id": "77"}]});
    assert_eq!(
        e.post("/read-states/ack-bulk", CAROL, body.clone()).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.get("/users/@me/read-states", CAROL).await.1["entries"][0]["last_message_id"],
        "77"
    );
    assert_eq!(
        e.post("/read-states/ack-bulk", DAVE, body).await.0,
        StatusCode::FORBIDDEN
    );
    let (s, _) = e
        .call(
            Method::POST,
            &format!("/channels/{}/pins/ack", e.ch),
            CAROL,
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert!(
        e.get("/users/@me/read-states", CAROL).await.1["entries"][0]["last_pin_timestamp"]
            .is_string()
    );
}

#[tokio::test]
async fn guild_notification_settings_and_channel_overrides() {
    let e = env().await;
    let g = e.guild;
    let (s, v) = e.get(&format!("/users/@me/guilds/{g}/settings"), BOB).await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["message_notifications"].is_null());
    assert_eq!(v["muted"], false);

    let (s, v) = e
        .patch(
            &format!("/users/@me/guilds/{g}/settings"),
            BOB,
            json!({
                "message_notifications": 1,
                "suppress_everyone": true,
                "muted": true,
                "mute_config": {"selected_time_window": 3600},
                "channel_overrides": {
                    e.ch.to_string(): {"message_notifications": 2, "muted": true}
                }
            }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["message_notifications"], 1);
    assert_eq!(v["suppress_everyone"], true);
    assert_eq!(v["muted"], true);
    let end = v["mute_config"]["end_time"].as_str().unwrap();
    assert_eq!(end, crate::routes::messages::model::iso(T0 + 3_600_000));
    assert_eq!(
        v["channel_overrides"][0]["channel_id"],
        e.ch.to_string().as_str()
    );
    assert_eq!(v["channel_overrides"][0]["message_notifications"], 2);
    assert_eq!(
        v["channel_overrides"][0]["mute_config"]["selected_time_window"],
        -1
    );

    // Settings are per user.
    assert_eq!(
        e.get(&format!("/users/@me/guilds/{g}/settings"), CAROL)
            .await
            .1["muted"],
        false
    );

    // Unmute, inherit the level, and drop the override.
    let (s, v) = e
        .patch(
            &format!("/users/@me/guilds/{g}/settings"),
            BOB,
            json!({
                "message_notifications": null,
                "muted": false,
                "channel_overrides": {e.ch.to_string(): {"message_notifications": null, "muted": false}}
            }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["message_notifications"].is_null());
    assert_eq!(v["muted"], false);
    assert!(is_empty_list(&v["channel_overrides"]));
}

#[tokio::test]
async fn notification_settings_validate_input_and_scope() {
    let e = env().await;
    let g = e.guild;
    for body in [
        json!({"message_notifications": 7}),
        json!({"muted": true, "mute_config": {"selected_time_window": 5}}),
        json!({"channel_overrides": {"nope": {"muted": true}}}),
    ] {
        let (s, _) = e
            .patch(
                &format!("/users/@me/guilds/{g}/settings"),
                BOB,
                body.clone(),
            )
            .await;
        assert!(
            s == StatusCode::BAD_REQUEST || s == StatusCode::NOT_FOUND,
            "{body} -> {s}"
        );
    }
    // A failed request must not have written anything.
    assert_eq!(
        e.get(&format!("/users/@me/guilds/{g}/settings"), BOB)
            .await
            .1["muted"],
        false
    );
    // Only members; unknown guild.
    assert_eq!(
        e.get(&format!("/users/@me/guilds/{g}/settings"), DAVE)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.get("/users/@me/guilds/123456/settings", BOB).await.0,
        StatusCode::NOT_FOUND
    );
    // The DM scope.
    let (s, v) = e
        .patch(
            "/users/@me/guilds/@me/settings",
            BOB,
            json!({"muted": true}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["guild_id"].is_null());
    assert_eq!(v["muted"], true);
    assert_eq!(v["mute_config"]["selected_time_window"], -1);
}

#[tokio::test]
async fn profile_edit_and_view() {
    let e = env().await;
    let (s, v) = e
        .patch(
            "/users/@me",
            ALICE,
            json!({
                "bio": "  hello there ",
                "pronouns": "they/them",
                "accent_color": 0x00ff_8800u32,
                "banner": png_uri(120, 40),
                "custom_status": {"text": "brb", "emoji_name": "coffee"}
            }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["bio"], "hello there");
    assert_eq!(v["custom_status"]["text"], "brb");

    let (s, v) = e.get("/users/1/profile", BOB).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["user_profile"]["bio"], "hello there");
    assert_eq!(v["user_profile"]["pronouns"], "they/them");
    assert_eq!(v["user_profile"]["accent_color"], 0x00ff_8800u32);
    let banner = v["user_profile"]["banner"].as_str().unwrap().to_owned();
    assert!(sr_storage::media::is_valid_hash(&banner), "{banner}");
    assert_eq!(v["user"]["banner"], banner.as_str());
    e.storage
        .head(&format!("avatars/1/{banner}"))
        .await
        .unwrap();
    assert_eq!(v["user"]["id"], "1");
    assert_eq!(
        e.get("/users/@me/profile", ALICE).await.1["user_profile"]["bio"],
        "hello there"
    );

    // Absent fields stay, null clears.
    let (_, v) = e.patch("/users/@me", ALICE, json!({"bio": null})).await;
    assert!(v["bio"].is_null());
    assert_eq!(v["pronouns"], "they/them");
    assert_eq!(v["custom_status"]["text"], "brb");
    let (_, v) = e
        .patch("/users/@me", ALICE, json!({"custom_status": null}))
        .await;
    assert!(v["custom_status"].is_null());

    for bad in [
        json!({"bio": "b".repeat(191)}),
        json!({"pronouns": "p".repeat(41)}),
        json!({"accent_color": 0x0100_0000u32}),
        json!({"locale": "not a tag"}),
        json!({"locale": "en-XA"}),
        json!({"custom_status": {"text": "t".repeat(129)}}),
        json!({"custom_status": {"text": "x", "expires_at": "tomorrow"}}),
    ] {
        let (s, v) = e.patch("/users/@me", ALICE, bad.clone()).await;
        assert_eq!(
            (s, v["code"].as_i64()),
            (StatusCode::BAD_REQUEST, Some(50035)),
            "{bad}"
        );
    }
    assert_eq!(
        e.get("/users/99/profile", BOB).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn presence_preference() {
    let e = env().await;
    let (s, v) = e.get("/users/@me/presence", BOB).await;
    assert_eq!((s, v["status"].as_str()), (StatusCode::OK, Some("online")));
    let (s, v) = e
        .patch(
            "/users/@me/presence",
            BOB,
            json!({"status": "dnd", "duration_seconds": 3600, "custom_status": {"text": "deep work"}}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["status"], "dnd");
    assert_eq!(
        v["status_expires_at"].as_str(),
        Some(crate::routes::messages::model::iso(T0 + 3_600_000).as_str())
    );
    assert_eq!(v["custom_status"]["text"], "deep work");

    // The expiry reverts the status; the custom status stays.
    e.clock.advance_ms(3_601_000);
    let (_, v) = e.get("/users/@me/presence", BOB).await;
    assert_eq!(v["status"], "online");
    assert_eq!(v["custom_status"]["text"], "deep work");

    let (_, v) = e
        .patch("/users/@me/presence", BOB, json!({"status": "invisible"}))
        .await;
    assert_eq!(v["status"], "invisible");
    for bad in [
        json!({"status": "offline"}),
        json!({"status": "away"}),
        json!({"status": "idle", "custom_status": {"emoji_id": "x"}}),
    ] {
        assert_eq!(
            e.patch("/users/@me/presence", BOB, bad).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        e.get("/users/@me/presence", ALICE).await.1["status"],
        "online"
    );
}

#[tokio::test]
async fn unauthenticated_requests_are_refused() {
    let e = env().await;
    let resp = e
        .app
        .clone()
        .oneshot(
            Request::get("/api/v1/users/@me/relationships")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn adapter_maps_every_social_event_to_a_user_scoped_dispatch() {
    let u = UserId::new(1);
    let ch = ChannelId::new(5);
    for (name, with_channel) in [
        ("CHANNEL_CREATE", true),
        ("CHANNEL_DELETE", true),
        ("RELATIONSHIP_ADD", false),
        ("MESSAGE_ACK", true),
    ] {
        let mut ev = events::SocialEvent::new(name, vec![u], json!({"k": 1}));
        if with_channel {
            ev = ev.for_channel(ch);
        }
        match events::to_domain(&ev) {
            Some(DomainEvent::UserScoped {
                users,
                name: n,
                payload,
            }) => {
                assert_eq!((users, n.as_str()), (vec![u], name));
                assert_eq!(payload["k"], 1);
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

// ---- first-message CHANNEL_CREATE (user-scoped, before MESSAGE_CREATE) ----

impl Env {
    /// Open a 1:1 DM as `a` with `b` and mirror it into the chat backend, which
    /// the message routes read (the production repos share one database).
    async fn open_dm_with_chat_channel(&self, a: u64, b: u64) -> String {
        let (s, v) = self
            .post(
                "/users/@me/channels",
                a,
                json!({"recipient_id": b.to_string()}),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{v}");
        let id = v["id"].as_str().unwrap().to_owned();
        let cid = ChannelId::new(id.parse().unwrap());
        let mut d = self.backend.data();
        d.channels.insert(
            cid,
            sr_chat::Channel::new(cid, None, sr_chat::ChannelType::Dm),
        );
        d.recipients.insert((cid, UserId::new(a)));
        d.recipients.insert((cid, UserId::new(b)));
        id
    }

    async fn say(&self, dm: &str, who: u64, text: &str) {
        let (s, v) = self
            .post(
                &format!("/channels/{dm}/messages"),
                who,
                json!({ "content": text }),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{v}");
    }

    fn share_guild(&self, a: u64, b: u64) {
        self.repo
            .store
            .lock()
            .unwrap()
            .shared_guilds
            .insert((UserId::new(a), UserId::new(b)));
    }
}

async fn assert_quiet(s: &mut BoxStream<'static, DomainEvent>) {
    assert!(
        tokio::time::timeout(Duration::from_millis(50), s.next())
            .await
            .is_err(),
        "unexpected extra event"
    );
}

fn assert_channel_create(ev: DomainEvent, dm: &str, other: &str) {
    match ev {
        DomainEvent::UserScoped { name, payload, .. } => {
            assert_eq!(name, "CHANNEL_CREATE");
            assert_eq!(payload["id"], dm);
            // The channel as the recipient sees it: only the other party is listed.
            assert_eq!(payload["recipients"].as_array().unwrap().len(), 1);
            assert_eq!(payload["recipients"][0]["id"], other);
        }
        other => panic!("expected CHANNEL_CREATE, got {other:?}"),
    }
}

#[tokio::test]
async fn first_dm_message_announces_channel_to_recipient_first() {
    let e = env().await;
    e.befriend(ALICE, BOB).await;
    let dm = e.open_dm_with_chat_channel(ALICE, BOB).await;
    let mut bob = e.user_events(BOB);
    let mut alice = e.user_events(ALICE);

    e.say(&dm, ALICE, "hi").await;
    assert_channel_create(next_event(&mut bob).await, &dm, "1");
    assert!(matches!(
        next_event(&mut bob).await,
        DomainEvent::MessageSent { guild: None, .. }
    ));
    assert_quiet(&mut bob).await;
    // The author already knows the channel: only the message reaches her.
    assert!(matches!(
        next_event(&mut alice).await,
        DomainEvent::MessageSent { guild: None, .. }
    ));
    assert_quiet(&mut alice).await;

    // Later messages, in either direction, announce nothing.
    e.say(&dm, ALICE, "again").await;
    e.say(&dm, BOB, "hey").await;
    for _ in 0..2 {
        assert!(matches!(
            next_event(&mut bob).await,
            DomainEvent::MessageSent { .. }
        ));
    }
    assert_quiet(&mut bob).await;
}

#[tokio::test]
async fn reopened_dm_is_announced_again() {
    let e = env().await;
    e.befriend(ALICE, BOB).await;
    let dm = e.open_dm_with_chat_channel(ALICE, BOB).await;
    e.say(&dm, ALICE, "one").await;
    assert_eq!(
        e.delete(&format!("/users/@me/channels/{dm}"), BOB).await.0,
        StatusCode::NO_CONTENT
    );
    let mut bob = e.user_events(BOB);

    e.say(&dm, ALICE, "two").await;
    assert_channel_create(next_event(&mut bob).await, &dm, "1");
    assert!(matches!(
        next_event(&mut bob).await,
        DomainEvent::MessageSent { .. }
    ));
    // Reopened, so the next message is announced to nobody.
    e.say(&dm, ALICE, "three").await;
    assert!(matches!(
        next_event(&mut bob).await,
        DomainEvent::MessageSent { .. }
    ));
    assert_quiet(&mut bob).await;
    assert_eq!(
        e.get("/users/@me/channels", BOB).await.1[0]["id"],
        dm.as_str()
    );
}

#[tokio::test]
async fn pending_message_request_is_not_announced() {
    let e = env().await;
    e.share_guild(ALICE, BOB); // not friends: Bob gets a Message Request
    let dm = e.open_dm_with_chat_channel(ALICE, BOB).await;
    let mut bob = e.user_events(BOB);

    e.say(&dm, ALICE, "hello stranger").await;
    assert!(matches!(
        next_event(&mut bob).await,
        DomainEvent::MessageSent { .. }
    ));
    assert_quiet(&mut bob).await;
}

#[tokio::test]
async fn accepting_a_message_request_announces_the_channel_to_the_recipient() {
    let e = env().await;
    e.share_guild(ALICE, BOB); // not friends: Bob gets a Message Request
    let dm = e.open_dm_with_chat_channel(ALICE, BOB).await;
    let mut bob = e.user_events(BOB);
    let mut alice = e.user_events(ALICE);

    let url = format!("/users/@me/channels/{dm}/request");
    let accept = || e.put(&url, BOB, Some(json!({"state": "accepted"})));
    assert_eq!(accept().await.0, StatusCode::NO_CONTENT);
    assert_channel_create(next_event(&mut bob).await, &dm, "1");
    assert_quiet(&mut bob).await;
    // Only the accepting recipient is told.
    assert_quiet(&mut alice).await;
    // Accepting again changes nothing, so announces nothing.
    assert_eq!(accept().await.0, StatusCode::NO_CONTENT);
    assert_quiet(&mut bob).await;
}

// ---- username add, guild ack, profile extras, live user events ----------------

/// The next `UserScoped` dispatch called `name`, skipping unrelated events.
async fn scoped(s: &mut BoxStream<'static, DomainEvent>, name: &str) -> Value {
    for _ in 0..10 {
        if let DomainEvent::UserScoped {
            name: n, payload, ..
        } = next_event(s).await
            && n == name
        {
            return payload;
        }
    }
    panic!("no {name} dispatch");
}

#[tokio::test]
async fn friend_request_by_username_is_case_insensitive_and_does_not_enumerate() {
    let e = env().await;
    e.repo.add_named_user(UserId::new(ALICE), "alice");
    e.repo.add_named_user(UserId::new(BOB), "Bob_B");
    e.repo.add_named_user(UserId::new(CAROL), "carol");
    e.repo.add_named_user(UserId::new(DAVE), "dave");
    let mut bob = e.user_events(BOB);

    let (s, v) = e
        .post(
            "/users/@me/relationships",
            ALICE,
            json!({"username": "bOb_b", "discriminator": "0"}),
        )
        .await;
    assert_eq!((s, &v), (StatusCode::NO_CONTENT, &Value::Null));
    assert_eq!(
        kinds(&e.get("/users/@me/relationships", ALICE).await.1),
        [("2".into(), 4)]
    );
    let added = scoped(&mut bob, "RELATIONSHIP_ADD").await;
    assert_eq!(
        (added["id"].as_str(), added["type"].as_i64()),
        (Some("1"), Some(3))
    );

    // The target can accept by naming the requester.
    let (s, _) = e
        .post(
            "/users/@me/relationships",
            BOB,
            json!({"username": "ALICE"}),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(
        kinds(&e.get("/users/@me/relationships", ALICE).await.1),
        [("2".into(), 1)]
    );

    // Unknown, blocked-by-them, closed-to-requests and blank all answer identically.
    e.put("/users/@me/relationships/1", DAVE, Some(json!({"type": 2})))
        .await;
    e.repo
        .put_privacy(
            UserId::new(CAROL),
            &sr_chat::social::repo::Privacy {
                friend_requests: sr_chat::social::relationships::FriendRequestSources {
                    everyone: false,
                    friends_of_friends: false,
                    server_members: false,
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let mut answers = Vec::new();
    for name in ["nobody_here", "dave", "carol", "   "] {
        answers.push(
            e.post("/users/@me/relationships", ALICE, json!({"username": name}))
                .await,
        );
    }
    assert_eq!(answers[0].0, StatusCode::BAD_REQUEST, "{answers:?}");
    assert!(answers.iter().all(|a| a == &answers[0]), "{answers:?}");

    // Self and malformed bodies keep their by-id behaviour.
    let (s, _) = e
        .post(
            "/users/@me/relationships",
            ALICE,
            json!({"username": "alice"}),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = e.post("/users/@me/relationships", ALICE, json!({})).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn guild_ack_marks_every_viewable_channel_read() {
    let e = env().await;
    let hidden = ChannelId::new(7_001);
    let empty = ChannelId::new(7_002);
    {
        let mut d = e.backend.data();
        d.channels.get_mut(&e.ch).unwrap().last_message_id = Some(sr_core::MessageId::new(900));
        for (id, last) in [(hidden, Some(901)), (empty, None)] {
            let mut c = sr_chat::Channel::new(id, Some(e.guild), sr_chat::ChannelType::Text);
            c.last_message_id = last.map(sr_core::MessageId::new);
            d.channels.insert(id, c);
        }
        d.overwrites.insert(
            hidden,
            vec![sr_core::permissions::Overwrite {
                id: sr_core::Snowflake::new(BOB),
                kind: sr_core::permissions::OverwriteKind::Member,
                allow: sr_core::Permissions::empty(),
                deny: sr_core::Permissions::VIEW_CHANNEL,
            }],
        );
    }
    let mut bob = e.user_events(BOB);
    e.repo
        .add_mentions(&[UserId::new(BOB)], e.ch, 2)
        .await
        .unwrap();

    let path = format!("/guilds/{}/ack", e.guild);
    let (s, _) = e.call(Method::POST, &path, BOB, None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, v) = e.get("/users/@me/read-states", BOB).await;
    let entries = v["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1, "{v}");
    assert_eq!(entries[0]["id"], e.ch.to_string().as_str());
    assert_eq!(entries[0]["last_message_id"], "900");
    assert_eq!(entries[0]["mention_count"], 0);
    assert_eq!(scoped(&mut bob, "MESSAGE_ACK").await["message_id"], "900");

    // The owner also sees the hidden channel; outsiders and unknown guilds are refused.
    assert_eq!(
        e.post(&path, ALICE, json!({})).await.0,
        StatusCode::NO_CONTENT
    );
    let (_, v) = e.get("/users/@me/read-states", ALICE).await;
    assert_eq!(v["entries"].as_array().unwrap().len(), 2);
    assert_eq!(
        e.post(&path, DAVE, json!({})).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.post("/guilds/12345/ack", BOB, json!({})).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn profile_shows_mutual_friends_guilds_and_friends_since() {
    let e = env().await;
    let g = e.guild;
    e.repo
        .add_member(g, UserId::new(ALICE), Some("Boss"), T0 - 5_000);
    e.repo
        .add_member(g, UserId::new(BOB), Some("Bobby"), T0 - 1_000);
    e.repo
        .add_member(GuildId::new(999), UserId::new(BOB), None, T0);
    e.befriend(ALICE, CAROL).await;
    e.clock.advance_ms(1_000);
    e.befriend(BOB, CAROL).await;
    e.befriend(ALICE, BOB).await;

    let (s, v) = e.get("/users/2/profile", ALICE).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["mutual_friends_count"], 1);
    assert_eq!(v["mutual_friends"][0]["id"], "3");
    let guilds = v["mutual_guilds"].as_array().unwrap();
    assert_eq!(guilds.len(), 1, "only guilds both belong to");
    assert_eq!(guilds[0]["id"], g.to_string().as_str());
    assert_eq!(guilds[0]["nick"], "Bobby");
    assert_eq!(
        guilds[0]["member_since"].as_str(),
        Some(crate::routes::messages::model::iso(T0 - 1_000).as_str())
    );
    assert_eq!(
        v["friends_since"].as_str(),
        Some(crate::routes::messages::model::iso(T0 + 1_000).as_str())
    );
    assert!(v.get("premium_since").is_none() && v.get("premium_type").is_none());

    let (_, v) = e
        .get("/users/2/profile?with_mutual_guilds=false", ALICE)
        .await;
    assert!(v.get("mutual_guilds").is_none());
    assert_eq!(v["mutual_friends_count"], 1);

    // Strangers share nothing; your own profile carries no mutual data.
    let (_, v) = e.get("/users/2/profile", DAVE).await;
    assert_eq!(v["mutual_friends_count"], 0);
    assert_eq!(v["mutual_guilds"].as_array().unwrap().len(), 0);
    assert!(v.get("friends_since").is_none());
    let (_, v) = e.get("/users/@me/profile", ALICE).await;
    assert!(v.get("mutual_friends").is_none() && v.get("mutual_guilds").is_none());
}

#[tokio::test]
async fn profile_edit_fans_user_update_out_to_friends_and_guilds() {
    let e = env().await;
    e.repo.add_member(e.guild, UserId::new(ALICE), None, T0);
    e.repo.add_member(e.guild, UserId::new(BOB), None, T0);
    e.befriend(ALICE, DAVE).await;
    let mut dave = e.user_events(DAVE);
    let mut guild = e.bus.subscribe(Topic::Guild(e.guild));
    let mut alice = e.user_events(ALICE);
    let mut carol = e.user_events(CAROL);

    let (s, _) = e
        .patch("/users/@me", ALICE, json!({"bio": "new bio"}))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(scoped(&mut dave, "USER_UPDATE").await["bio"], "new bio");
    match next_event(&mut guild).await {
        DomainEvent::UserUpdated {
            guild: g,
            user,
            payload,
        } => {
            assert_eq!((g, user), (e.guild, UserId::new(ALICE)));
            assert_eq!(payload["user"]["id"], "1");
        }
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(scoped(&mut alice, "USER_UPDATE").await["bio"], "new bio");
    // Strangers hear nothing.
    assert_quiet(&mut carol).await;
}

#[tokio::test]
async fn presence_changes_are_announced_to_the_gateway() {
    let e = env().await;
    let mut bob = e.user_events(BOB);
    let (s, _) = e
        .patch(
            "/users/@me/presence",
            BOB,
            json!({"status": "invisible", "custom_status": {"text": "ghost", "emoji_name": "boo"}}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let mut seen = None;
    for _ in 0..5 {
        if let DomainEvent::PresenceChanged {
            user,
            status,
            custom_status,
        } = next_event(&mut bob).await
        {
            seen = Some((user, status, custom_status));
            break;
        }
    }
    let (user, status, custom) = seen.expect("PresenceChanged");
    assert_eq!((user, status.as_str()), (UserId::new(BOB), "invisible"));
    let custom = custom.expect("custom status activity");
    assert_eq!(
        (custom["type"].as_i64(), custom["state"].as_str()),
        (Some(4), Some("ghost"))
    );
    assert_eq!(custom["emoji"]["name"], "boo");

    // Clearing the custom status announces an empty one.
    e.patch("/users/@me/presence", BOB, json!({"custom_status": null}))
        .await;
    for _ in 0..5 {
        if let DomainEvent::PresenceChanged { custom_status, .. } = next_event(&mut bob).await {
            assert!(custom_status.is_none());
            return;
        }
    }
    panic!("no second PresenceChanged");
}

#[tokio::test]
async fn group_member_changes_reach_the_right_users() {
    let e = env().await;
    e.befriend(ALICE, BOB).await;
    e.befriend(ALICE, CAROL).await;
    e.befriend(ALICE, DAVE).await;
    let (_, v) = e
        .post(
            "/users/@me/channels",
            ALICE,
            json!({"recipients": ["2", "3"]}),
        )
        .await;
    let id = v["id"].as_str().unwrap().to_owned();
    let mut bob = e.user_events(BOB);
    let mut dave = e.user_events(DAVE);

    // Old members hear about the newcomer; the newcomer gets the channel itself.
    e.put(&format!("/channels/{id}/recipients/4"), ALICE, None)
        .await;
    let add = scoped(&mut bob, "CHANNEL_RECIPIENT_ADD").await;
    assert_eq!(
        (add["channel_id"].as_str(), add["user"]["id"].as_str()),
        (Some(id.as_str()), Some("4"))
    );
    assert_eq!(scoped(&mut dave, "CHANNEL_CREATE").await["id"], id.as_str());
    assert_quiet(&mut dave).await;

    // Kicking tells the removed user (CHANNEL_DELETE) and the rest (RECIPIENT_REMOVE).
    e.delete(&format!("/channels/{id}/recipients/4"), ALICE)
        .await;
    assert_eq!(scoped(&mut dave, "CHANNEL_DELETE").await["id"], id.as_str());
    assert_eq!(
        scoped(&mut bob, "CHANNEL_RECIPIENT_REMOVE").await["user"]["id"],
        "4"
    );
    // Leaving a group does the same for the leaver.
    e.delete(&format!("/users/@me/channels/{id}"), BOB).await;
    assert_eq!(scoped(&mut bob, "CHANNEL_DELETE").await["id"], id.as_str());
}

fn png_uri(w: u32, h: u32) -> String {
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
async fn avatar_upload_replace_and_remove() {
    let e = env().await;
    let mut events = e.bus.subscribe(Topic::User(UserId::new(ALICE)));

    // Upload: cropped to a square, stored under avatars/<user>/<hash>.
    let (s, v) = e
        .patch("/users/@me", ALICE, json!({"avatar": png_uri(300, 200)}))
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let h1 = v["user"]["avatar"].as_str().unwrap().to_owned();
    assert!(sr_storage::media::is_valid_hash(&h1));
    let key1 = format!("avatars/1/{h1}");
    let stored = e.storage.get(&key1).await.unwrap();
    let dims = image::ImageReader::new(std::io::Cursor::new(&stored))
        .with_guessed_format()
        .unwrap()
        .into_dimensions()
        .unwrap();
    assert_eq!(dims, (200, 200));
    // Others see it on the profile and user objects.
    let (_, v) = e.get("/users/1/profile", BOB).await;
    assert_eq!(v["user"]["avatar"], h1.as_str());
    // A USER_UPDATE carrying the hash went to the owner.
    let ev = tokio::time::timeout(Duration::from_secs(1), events.next())
        .await
        .unwrap()
        .unwrap();
    assert!(format!("{ev:?}").contains(&h1));

    // Replace: the old object and its cached variants are deleted.
    e.storage
        .put(&format!("{key1}@64.webp"), bytes::Bytes::from_static(b"v"))
        .await
        .unwrap();
    let (s, v) = e
        .patch("/users/@me", ALICE, json!({"avatar": png_uri(64, 64)}))
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let h2 = v["user"]["avatar"].as_str().unwrap().to_owned();
    assert_ne!(h1, h2);
    assert!(e.storage.head(&key1).await.is_err());
    assert!(e.storage.head(&format!("{key1}@64.webp")).await.is_err());
    assert!(e.storage.head(&format!("avatars/1/{h2}")).await.is_ok());

    // An edit that does not mention the avatar keeps it.
    let (_, v) = e.patch("/users/@me", ALICE, json!({"bio": "x"})).await;
    assert_eq!(v["user"]["avatar"], h2.as_str());

    // Remove: null clears the field and deletes the object.
    let (s, v) = e.patch("/users/@me", ALICE, json!({"avatar": null})).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert!(v["user"]["avatar"].is_null());
    assert!(e.storage.head(&format!("avatars/1/{h2}")).await.is_err());
}

#[tokio::test]
async fn bad_avatar_uploads_are_rejected_without_side_effects() {
    let e = env().await;
    let html = format!(
        "data:image/png;base64,{}",
        data_encoding::BASE64.encode(b"<html><script>alert(1)</script>")
    );
    for (bad, code) in [
        (json!("https://example.com/a.png"), "IMAGE_INVALID"),
        (
            json!("data:image/svg+xml;base64,PHN2Zz48L3N2Zz4="),
            "IMAGE_INVALID",
        ),
        (json!(html), "IMAGE_INVALID"),
        (json!("data:image/png;base64,@@@"), "IMAGE_INVALID"),
    ] {
        let (s, v) = e
            .patch("/users/@me", ALICE, json!({"avatar": bad, "bio": "nope"}))
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{v}");
        assert_eq!(v["errors"]["avatar"]["_errors"][0]["code"], code, "{v}");
    }
    // The failed edits changed nothing (bio untouched) and stored nothing.
    let (_, v) = e.get("/users/@me/profile", ALICE).await;
    assert!(v["user_profile"]["bio"].is_null());
    assert!(e.storage.list("avatars/").await.unwrap().is_empty());
    // Oversized pixel count.
    let big = format!(
        "data:image/png;base64,{}",
        data_encoding::BASE64.encode(&{
            let mut out = Vec::new();
            image::DynamicImage::ImageLuma8(image::GrayImage::new(6000, 6000))
                .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
                .unwrap();
            out
        })
    );
    let (s, v) = e.patch("/users/@me", ALICE, json!({"avatar": big})).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(
        v["errors"]["avatar"]["_errors"][0]["code"],
        "BINARY_TYPE_MAX_SIZE"
    );
}

// ---- user settings, notification defaults, privacy, display name ------------

#[tokio::test]
async fn settings_blob_merges_versions_and_notifies_own_sessions() {
    let e = env().await;
    let mut alice = e.user_events(ALICE);
    let (s, v) = e.get("/users/@me/settings", ALICE).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        (v["version"].as_i64(), &v["settings"]),
        (Some(0), &json!({}))
    );

    let (s, v) = e
        .patch(
            "/users/@me/settings",
            ALICE,
            json!({"settings": {"theme": "ash", "ui": {"compact": true}}}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["version"], 1);
    let payload = scoped(&mut alice, "SR_USER_SETTINGS_UPDATE").await;
    assert_eq!(payload["settings"]["theme"], "ash");

    let (s, v) = e
        .patch(
            "/users/@me/settings",
            ALICE,
            json!({"settings": {"theme": null, "ui": {"zoom": 110}}}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["version"], 2);
    assert_eq!(v["settings"], json!({"ui": {"compact": true, "zoom": 110}}));
    assert_eq!(e.get("/users/@me/settings", ALICE).await.1, v);
    // Per user.
    assert_eq!(e.get("/users/@me/settings", BOB).await.1["version"], 0);

    // Bad shapes and oversize blobs are 400 and change nothing.
    for bad in [
        json!({"settings": [1, 2]}),
        json!({"settings": "x"}),
        json!({"settings": {"blob": "y".repeat(70_000)}}),
    ] {
        let (s, _) = e.patch("/users/@me/settings", ALICE, bad).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }
    assert_eq!(e.get("/users/@me/settings", ALICE).await.1["version"], 2);
}

#[tokio::test]
async fn global_notification_default_applies_below_explicit_choices() {
    let e = env().await;
    let (s, v) = e.get("/users/@me/notification-defaults", BOB).await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["message_notifications"].is_null());

    let (s, v) = e
        .patch(
            "/users/@me/notification-defaults",
            BOB,
            json!({"message_notifications": 1}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["message_notifications"], 1);
    assert_eq!(
        e.get("/users/@me/notification-defaults", BOB).await.1["message_notifications"],
        1
    );
    // Per user, and an empty patch changes nothing.
    assert!(
        e.get("/users/@me/notification-defaults", CAROL).await.1["message_notifications"].is_null()
    );
    let (_, v) = e
        .patch("/users/@me/notification-defaults", BOB, json!({}))
        .await;
    assert_eq!(v["message_notifications"], 1);
    // The resolver sees it.
    let svc = sr_chat::social::SocialService::new(
        DynRepo(e.repo.clone() as Arc<dyn SocialRepository>),
        e.clock.clone() as Arc<dyn Clock>,
        SnowflakeGenerator::new(e.clock.clone() as Arc<dyn Clock>, 1, 5).unwrap(),
        SocialLimits::default(),
    );
    let r = svc
        .resolve_notifications(
            UserId::new(BOB),
            Some(e.guild),
            e.ch,
            None,
            sr_chat::social::notifications::NotificationLevel::All,
        )
        .await
        .unwrap();
    assert_eq!(
        r.level,
        sr_chat::social::notifications::NotificationLevel::OnlyMentions
    );

    for bad in [
        json!({"message_notifications": 3}),
        json!({"message_notifications": -1}),
    ] {
        let (s, _) = e.patch("/users/@me/notification-defaults", BOB, bad).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }
    let (s, v) = e
        .patch(
            "/users/@me/notification-defaults",
            BOB,
            json!({"message_notifications": null}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["message_notifications"].is_null());
}

#[tokio::test]
async fn privacy_settings_round_trip_and_gate_friend_requests() {
    let e = env().await;
    let (s, v) = e.get("/users/@me/privacy", DAVE).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["friend_requests_everyone"], true);
    assert_eq!(v["message_requests"], true);

    let (s, v) = e
        .patch(
            "/users/@me/privacy",
            DAVE,
            json!({"friend_requests_everyone": false, "friend_requests_friends_of_friends": false,
                   "friend_requests_server_members": false, "allow_server_member_dms": false}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["friend_requests_everyone"], false);
    assert_eq!(v["allow_server_member_dms"], false);
    assert_eq!(v["message_requests"], true, "untouched field stays");
    assert_eq!(e.get("/users/@me/privacy", DAVE).await.1, v);
    // Nobody may send Dave a friend request now.
    let (s, _) = e
        .put(&format!("/users/@me/relationships/{DAVE}"), ALICE, None)
        .await;
    assert_ne!(s, StatusCode::NO_CONTENT);
    // Re-open to everyone.
    let (s, _) = e
        .patch(
            "/users/@me/privacy",
            DAVE,
            json!({"friend_requests_everyone": true}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = e
        .put(&format!("/users/@me/relationships/{DAVE}"), ALICE, None)
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
}


#[tokio::test]
async fn safety_settings_round_trip_and_validate() {
    let e = env().await;
    let (_, v) = e.get("/users/@me/privacy", DAVE).await;
    assert_eq!(
        v["sensitive_media"],
        json!({"friends": "show", "others": "blur", "servers": "show"})
    );
    assert_eq!(v["dm_spam_filter"], false);
    assert_eq!(v["server_dm_overrides"], json!({}));

    let (s, v) = e
        .patch(
            "/users/@me/privacy",
            DAVE,
            json!({"sensitive_media": {"others": "block"}, "dm_spam_filter": true,
                   "server_dm_overrides": {"70": false, "71": true}}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["sensitive_media"]["others"], "block");
    assert_eq!(v["sensitive_media"]["friends"], "show", "untouched");
    assert_eq!(v["server_dm_overrides"], json!({"70": false, "71": true}));
    assert_eq!(v["message_requests"], true);

    // null removes one override.
    let (_, v) = e
        .patch(
            "/users/@me/privacy",
            DAVE,
            json!({"server_dm_overrides": {"71": null}}),
        )
        .await;
    assert_eq!(v["server_dm_overrides"], json!({"70": false}));
    assert_eq!(e.get("/users/@me/privacy", DAVE).await.1, v);

    for bad in [
        json!({"sensitive_media": {"friends": "hide"}}),
        json!({"server_dm_overrides": {"abc": true}}),
        json!({"server_dm_overrides": {"0": true}}),
    ] {
        let (s, _) = e.patch("/users/@me/privacy", DAVE, bad).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn notification_keywords_are_cleaned_and_capped() {
    let e = env().await;
    let (s, v) = e.get("/users/@me/notification-keywords", ALICE).await;
    assert_eq!((s, &v["keywords"]), (StatusCode::OK, &json!([])));

    let (s, v) = e
        .put(
            "/users/@me/notification-keywords",
            ALICE,
            Some(json!({"keywords": ["  Deploy  ", "deploy", "red   alert", "", "  "]})),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["keywords"], json!(["Deploy", "red alert"]));
    assert_eq!(e.get("/users/@me/notification-keywords", ALICE).await.1, v);
    // Per user.
    assert_eq!(
        e.get("/users/@me/notification-keywords", DAVE).await.1["keywords"],
        json!([])
    );
    for bad in [
        json!({"keywords": ["!!!"]}),
        json!({"keywords": ["x".repeat(101)]}),
        json!({"keywords": (0..51).map(|i| format!("word{i}")).collect::<Vec<_>>()}),
    ] {
        let (s, _) = e
            .put("/users/@me/notification-keywords", ALICE, Some(bad))
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn display_name_edits_through_global_name() {
    let e = env().await;
    let (s, v) = e
        .patch("/users/@me", ALICE, json!({"global_name": "  Alice A.  "}))
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    // The in-memory chat backend does not read profile fields back into user summaries, so
    // check the stored profile (Postgres reads `accounts.display_name` for `global_name`).
    let stored = |uid: u64| {
        e.repo.store.lock().unwrap().profiles[&UserId::new(uid)]
            .display_name
            .clone()
    };
    assert_eq!(stored(ALICE).as_deref(), Some("Alice A."));
    for bad in ["x".repeat(33), "tab\there".to_owned()] {
        let (s, _) = e
            .patch("/users/@me", ALICE, json!({"global_name": bad}))
            .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }
    let (s, _) = e
        .patch("/users/@me", ALICE, json!({"global_name": null}))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(stored(ALICE), None);
}

#[tokio::test]
async fn profile_decoration_effect_and_theme_edit_and_view() {
    let e = env().await;
    let (s, v) = e
        .patch(
            "/users/@me",
            ALICE,
            json!({
                "profile_effect": "snow",
                "theme_colors": [0x00ff_0000u32, 0x0000_00ffu32]
            }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["profile_effect"], "snow");
    assert_eq!(v["theme_colors"], json!([0x00ff_0000u32, 0x0000_00ffu32]));

    let (_, v) = e.get("/users/1/profile", BOB).await;
    assert_eq!(v["user_profile"]["profile_effect"], "snow");
    assert_eq!(
        v["user_profile"]["theme_colors"],
        json!([0x00ff_0000u32, 0x0000_00ffu32])
    );
    assert!(v["badges"].as_array().unwrap().is_empty());
    assert!(v["member_since"].is_string());

    // Bad slugs and wide colours are refused and change nothing.
    for body in [
        json!({"avatar_decoration": "Not A Slug"}),
        // Well formed but not in the catalogue (no decorations ship; G-48).
        json!({"avatar_decoration": "neon-halo"}),
        json!({"profile_effect": "confetti"}),
        json!({"theme_colors": [0x0100_0000u32, 0]}),
    ] {
        let (s, _) = e.patch("/users/@me", ALICE, body).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }
    // null clears.
    let (_, v) = e
        .patch(
            "/users/@me",
            ALICE,
            json!({"avatar_decoration": null, "profile_effect": null, "theme_colors": null}),
        )
        .await;
    assert!(v["avatar_decoration"].is_null() && v["theme_colors"].is_null());
}

#[tokio::test]
async fn profile_catalogue_lists_effects_without_decorations() {
    let e = env().await;
    let (s, v) = e.get("/sr/profile-catalog", ALICE).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let slugs: Vec<&str> = v["effects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["slug"].as_str().unwrap())
        .collect();
    assert_eq!(slugs, ["sparkle", "aurora", "snow", "embers"]);
    assert_eq!(v["decorations"], json!([]));
}

#[tokio::test]
async fn profile_returns_the_server_profile_only_for_a_shared_server() {
    let e = env().await;
    let g = e.guild;
    e.repo
        .add_member(g, UserId::new(ALICE), Some("Boss"), T0 - 5_000);
    e.repo
        .add_member(g, UserId::new(BOB), Some("Bobby"), T0 - 1_000);
    let (s, v) = e
        .get(&format!("/users/2/profile?guild_id={g}"), ALICE)
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["guild_member_profile"]["nick"], "Bobby");
    assert_eq!(
        v["guild_member_profile"]["guild_id"],
        g.to_string().as_str()
    );
    // Dave is not in the server: nothing about it leaks.
    let (_, v) = e.get(&format!("/users/2/profile?guild_id={g}"), DAVE).await;
    assert!(v.get("guild_member_profile").is_none());
    // Without guild_id there is no server profile either.
    let (_, v) = e.get("/users/2/profile", ALICE).await;
    assert!(v.get("guild_member_profile").is_none());
    let (s, _) = e.get("/users/2/profile?guild_id=nope", ALICE).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}
