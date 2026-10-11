//! Endpoint tests over the in-memory repositories with one-shot requests.

use super::{DynRepo, SocialDeps, events, router};
use crate::{
    ApiSettings, AppState,
    auth::{BotIdentity, memory::MemoryBots},
    build_router,
    routes::messages::{FixedEntitlements, MessagesDeps},
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
use sr_chat::social::repo::{ReadStateRepository, SocialRepository};
use sr_core::{ChannelId, Clock, GuildId, ManualClock, SnowflakeGenerator, UserId};
use sr_events::{DomainEvent, EventBus, InProcessBus, Topic};
use sr_storage::ObjectStoreStorage;
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
    bus: Arc<InProcessBus>,
    clock: Arc<ManualClock>,
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

    let messages = MessagesDeps::new(
        backend.clone(),
        Arc::new(backend.clone()),
        storage,
        bus.clone(),
        Arc::new(FixedEntitlements(1 << 20)),
        Arc::new(backend.clone()),
        None,
        dyn_clock.clone(),
        1,
        1,
    )
    .unwrap();
    let social = SocialDeps::new(
        DynRepo(repo.clone() as Arc<dyn SocialRepository>),
        Arc::new(backend.clone()),
        bus.clone(),
        dyn_clock.clone(),
        SocialLimits::default(),
        1,
        3,
    )
    .unwrap();
    let state = AppState::builder()
        .bots(bots)
        .clock(dyn_clock)
        .settings(settings)
        .extension(Arc::new(messages))
        .extension(Arc::new(social))
        .build();
    Env {
        app: build_router(state, router()),
        repo,
        bus,
        clock,
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
                "banner": "banners/a.png",
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
    assert_eq!(v["user_profile"]["banner"], "banners/a.png");
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
