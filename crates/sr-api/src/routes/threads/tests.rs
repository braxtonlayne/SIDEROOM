//! Endpoint tests over the in-memory repository with one-shot requests.

use super::ThreadsDeps;
use crate::routes::messages::{FixedEntitlements, MessagesDeps};
use crate::{
    ApiSettings, AppState,
    auth::{BotIdentity, memory::MemoryBots},
    build_router,
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
use sr_chat::memory::{MemoryBackend, MemoryRepo};
use sr_chat::repo::GuildRepository;
use sr_chat::threads::{ThreadPatch, ThreadService};
use sr_core::{ChannelId, Clock, GuildId, ManualClock, Permissions, SnowflakeGenerator, UserId};
use sr_events::{DomainEvent, EventBus, InProcessBus, Topic};
use sr_storage::ObjectStoreStorage;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

const T0: i64 = 1_800_000_000_000;
const MIN: i64 = 60_000;
const OWNER: u64 = 1;
const BOB: u64 = 2;
const CAROL: u64 = 3;
const DAVE: u64 = 4; // not a guild member

struct Env {
    app: Router,
    repo: MemoryRepo,
    clock: Arc<ManualClock>,
    bus: Arc<InProcessBus>,
    deps: Arc<ThreadsDeps>,
    guild: GuildId,
    ch: ChannelId,
}

fn member(guild: GuildId, user: u64) -> Member {
    Member {
        guild_id: guild,
        user_id: UserId::new(user),
        nick: None,
        role_ids: vec![],
        joined_at_ms: T0,
        communication_disabled_until_ms: None,
        pending: false,
    }
}

async fn env() -> Env {
    let clock = Arc::new(ManualClock::new(T0));
    let dyn_clock: Arc<dyn Clock> = clock.clone();
    let repo = MemoryRepo::default();
    let backend = MemoryBackend::new();
    let bus = Arc::new(InProcessBus::new());
    let gen_ids = SnowflakeGenerator::new(dyn_clock.clone(), 9, 9).unwrap();
    let bp = plan_new_guild(UserId::new(OWNER), "Club", T0, || gen_ids.generate()).unwrap();
    let (guild, ch) = (bp.guild.id, bp.channels[2].id);
    repo.insert_guild(&bp).await.unwrap();
    {
        let mut s = repo.store.lock().unwrap();
        s.members.push(member(guild, BOB));
        s.members.push(member(guild, CAROL));
    }
    let bots = Arc::new(MemoryBots::new());
    for (tok, uid) in [
        ("owner", OWNER),
        ("bob", BOB),
        ("carol", CAROL),
        ("dave", DAVE),
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
    let threads = Arc::new(ThreadsDeps::new(
        ThreadService::new(
            repo.clone(),
            dyn_clock.clone(),
            SnowflakeGenerator::new(dyn_clock.clone(), 9, 10).unwrap(),
        ),
        bus.clone(),
    ));
    let messages = MessagesDeps::new(
        repo.clone(),
        Arc::new(backend.clone()),
        Arc::new(ObjectStoreStorage::in_memory()),
        bus.clone(),
        Arc::new(FixedEntitlements(1 << 20)),
        Arc::new(backend),
        Some(threads.activity_hook()),
        dyn_clock.clone(),
        1,
        1,
    )
    .unwrap();
    let state = AppState::builder()
        .bots(bots)
        .clock(dyn_clock)
        .settings(settings)
        .extension(Arc::new(messages))
        .extension(threads.clone())
        .build();
    let app = build_router(
        state,
        super::router().merge(crate::routes::messages::router()),
    );
    Env {
        app,
        repo,
        clock,
        bus,
        deps: threads,
        guild,
        ch,
    }
}

impl Env {
    async fn call(
        &self,
        method: Method,
        path: &str,
        who: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/v1{path}"))
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
        (parts.status, json)
    }

    async fn get(&self, path: &str, who: &str) -> (StatusCode, Value) {
        self.call(Method::GET, path, who, None).await
    }

    async fn post(&self, path: &str, who: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::POST, path, who, Some(body)).await
    }

    async fn put(&self, path: &str, who: &str) -> StatusCode {
        self.call(Method::PUT, path, who, None).await.0
    }

    async fn delete(&self, path: &str, who: &str) -> StatusCode {
        self.call(Method::DELETE, path, who, None).await.0
    }

    async fn say(&self, channel: &str, who: &str, content: &str) -> (StatusCode, Value) {
        self.post(
            &format!("/channels/{channel}/messages"),
            who,
            json!({ "content": content }),
        )
        .await
    }

    async fn thread_from_message(&self, who: &str, name: &str) -> (String, String) {
        let (s, m) = self.say(&self.ch.to_string(), who, "kickoff").await;
        assert_eq!(s, StatusCode::OK, "{m}");
        let mid = m["id"].as_str().unwrap().to_owned();
        let (s, t) = self
            .post(
                &format!("/channels/{}/messages/{mid}/threads", self.ch),
                who,
                json!({ "name": name }),
            )
            .await;
        assert_eq!(s, StatusCode::CREATED, "{t}");
        (mid, t["id"].as_str().unwrap().to_owned())
    }

    fn events(&self) -> BoxStream<'static, DomainEvent> {
        self.bus.subscribe(Topic::Guild(self.guild))
    }

    fn grant_everyone(&self, p: Permissions) {
        let mut s = self.repo.store.lock().unwrap();
        s.roles
            .iter_mut()
            .find(|r| r.is_everyone())
            .unwrap()
            .permissions |= p;
    }

    async fn active(&self, who: &str) -> Value {
        self.get(&format!("/guilds/{}/threads/active", self.guild), who)
            .await
            .1
    }
}

async fn next_event(s: &mut BoxStream<'static, DomainEvent>) -> DomainEvent {
    tokio::time::timeout(Duration::from_secs(1), s.next())
        .await
        .expect("no event")
        .expect("stream ended")
}

fn code(r: &(StatusCode, Value)) -> (u16, Option<u64>) {
    (r.0.as_u16(), r.1["code"].as_u64())
}

#[tokio::test]
async fn start_from_message_creates_a_thread_with_the_message_id_and_announces_it() {
    let e = env().await;
    let mut ev = e.events();
    let (mid, tid) = e.thread_from_message("bob", "Planning").await;
    assert_eq!(mid, tid);
    let t = e.active("bob").await;
    let th = &t["threads"][0];
    assert_eq!(th["type"], 11);
    assert_eq!(th["name"], "Planning");
    assert_eq!(th["parent_id"], e.ch.to_string());
    assert_eq!(th["owner_id"], BOB.to_string());
    assert_eq!(th["thread_metadata"]["archived"], false);
    assert_eq!(th["thread_metadata"]["auto_archive_duration"], 1440);
    assert_eq!(th["member_count"], 1);
    assert_eq!(th["member"]["user_id"], BOB.to_string());
    assert_eq!(t["members"].as_array().unwrap().len(), 1);

    // MESSAGE_CREATE of the original, THREAD_CREATE, then the two system messages.
    let mut kinds = Vec::new();
    for _ in 0..4 {
        match next_event(&mut ev).await {
            DomainEvent::MessageSent { data, .. } => kinds.push(format!("msg{}", data["type"])),
            DomainEvent::ThreadCreated { payload, .. } => {
                assert_eq!(payload["newly_created"], true);
                kinds.push("thread".into());
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    for want in ["thread", "msg21", "msg18"] {
        assert!(kinds.contains(&want.to_owned()), "{kinds:?}");
    }

    // A message has at most one thread.
    let again = e
        .post(
            &format!("/channels/{}/messages/{mid}/threads", e.ch),
            "carol",
            json!({ "name": "again" }),
        )
        .await;
    assert_eq!(code(&again), (400, Some(160_004)), "{}", again.1);
}

#[tokio::test]
async fn thread_messages_use_the_message_endpoints_and_update_counters() {
    let e = env().await;
    let (_, tid) = e.thread_from_message("bob", "chat").await;
    let mut ev = e.events();
    let (s, m) = e.say(&tid, "carol", "hello thread").await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert_eq!(m["channel_id"], tid);
    // The poster joined the thread; the join is announced.
    let mut saw_members = false;
    for _ in 0..2 {
        match next_event(&mut ev).await {
            DomainEvent::ThreadMembersUpdated { payload, .. } => {
                assert_eq!(payload["member_count"], 2);
                assert_eq!(payload["added_members"][0]["user_id"], CAROL.to_string());
                saw_members = true;
            }
            DomainEvent::ThreadMemberUpdated { .. } | DomainEvent::MessageSent { .. } => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(saw_members);
    let t = e.active("bob").await;
    assert_eq!(t["threads"][0]["message_count"], 1);
    assert_eq!(t["threads"][0]["member_count"], 2);
    let (_, members) = e
        .get(&format!("/channels/{tid}/thread-members"), "bob")
        .await;
    assert_eq!(members.as_array().unwrap().len(), 2);
    // Non-members of the guild cannot post.
    assert_eq!(e.say(&tid, "dave", "hi").await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn join_leave_add_and_remove_members() {
    let e = env().await;
    let (_, tid) = e.thread_from_message("bob", "t").await;
    let mut ev = e.events();
    let member = format!("/channels/{tid}/thread-members");
    assert_eq!(
        e.put(&format!("{member}/@me"), "carol").await,
        StatusCode::NO_CONTENT
    );
    match next_event(&mut ev).await {
        DomainEvent::ThreadMembersUpdated { payload, .. } => {
            assert_eq!(payload["member_count"], 2);
        }
        other => panic!("{other:?}"),
    }
    // Idempotent.
    assert_eq!(
        e.put(&format!("{member}/@me"), "carol").await,
        StatusCode::NO_CONTENT
    );
    // Others can be added by someone who can talk in the thread; strangers cannot.
    assert_eq!(
        e.put(&format!("{member}/{OWNER}"), "bob").await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.put(&format!("{member}/{DAVE}"), "bob").await,
        StatusCode::NOT_FOUND
    );
    let (s, m) = e.get(&format!("{member}/@me"), "carol").await;
    assert_eq!((s, m["user_id"].as_str()), (StatusCode::OK, Some("3")));
    let (s, page) = e.get(&format!("{member}?limit=2"), "bob").await;
    assert_eq!((s, page.as_array().unwrap().len()), (StatusCode::OK, 2));
    let (_, rest) = e.get(&format!("{member}?after=2"), "bob").await;
    assert_eq!(rest.as_array().unwrap().len(), 1);
    // Removing others needs MANAGE_THREADS; leaving is free.
    assert_eq!(
        e.delete(&format!("{member}/{BOB}"), "carol").await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.delete(&format!("{member}/@me"), "carol").await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.delete(&format!("{member}/{BOB}"), "owner").await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        e.get(&format!("{member}/{BOB}"), "owner").await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn private_threads_are_invisible_to_outsiders() {
    let e = env().await;
    e.grant_everyone(Permissions::CREATE_PRIVATE_THREADS);
    let (s, t) = e
        .post(
            &format!("/channels/{}/threads", e.ch),
            "bob",
            json!({ "name": "secret", "type": 12, "invitable": false }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{t}");
    let tid = t["id"].as_str().unwrap().to_owned();
    assert_eq!(t["type"], 12);
    assert_eq!(t["thread_metadata"]["invitable"], false);
    // Carol cannot read, post, list or self-join.
    assert_eq!(e.say(&tid, "carol", "x").await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        e.get(&format!("/channels/{tid}/thread-members"), "carol")
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert!(
        e.active("carol").await["threads"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        e.put(&format!("/channels/{tid}/thread-members/@me"), "carol")
            .await,
        StatusCode::FORBIDDEN
    );
    // The owner of the thread can still add her; then she can post.
    assert_eq!(
        e.put(&format!("/channels/{tid}/thread-members/{CAROL}"), "bob")
            .await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(e.say(&tid, "carol", "hi").await.0, StatusCode::OK);
    // Moderators (the guild owner is an administrator) see it without joining.
    assert_eq!(
        e.active("owner").await["threads"].as_array().unwrap().len(),
        1
    );
    // Without CREATE_PRIVATE_THREADS the type is refused.
    let e2 = env().await;
    let r = e2
        .post(
            &format!("/channels/{}/threads", e2.ch),
            "bob",
            json!({ "name": "secret", "type": 12 }),
        )
        .await;
    assert_eq!(code(&r), (403, Some(50_013)), "{}", r.1);
}

#[tokio::test]
async fn standalone_public_threads_and_validation() {
    let e = env().await;
    let ok = e
        .post(
            &format!("/channels/{}/threads", e.ch),
            "bob",
            json!({ "name": "Loose", "auto_archive_duration": 60, "rate_limit_per_user": 5 }),
        )
        .await;
    assert_eq!(ok.0, StatusCode::CREATED, "{}", ok.1);
    assert_eq!(ok.1["thread_metadata"]["auto_archive_duration"], 60);
    assert_eq!(ok.1["rate_limit_per_user"], 5);
    let bad = e
        .post(
            &format!("/channels/{}/threads", e.ch),
            "bob",
            json!({ "name": "x", "auto_archive_duration": 61 }),
        )
        .await;
    assert_eq!(code(&bad), (400, Some(50_035)));
    let bad = e
        .post(
            &format!("/channels/{}/threads", e.ch),
            "bob",
            json!({ "name": "", "type": 13 }),
        )
        .await;
    assert_eq!(code(&bad), (400, Some(50_035)));
    let none = e
        .post("/channels/999/threads", "bob", json!({ "name": "x" }))
        .await;
    assert_eq!(code(&none), (404, Some(10_003)));
    // A thread cannot hold threads.
    let tid = ok.1["id"].as_str().unwrap();
    let nested = e
        .post(
            &format!("/channels/{tid}/threads"),
            "bob",
            json!({ "name": "deeper" }),
        )
        .await;
    assert_eq!(nested.0, StatusCode::BAD_REQUEST, "{}", nested.1);
}

#[tokio::test]
async fn archived_listing_pages_by_archive_timestamp() {
    let e = env().await;
    let mut ids = Vec::new();
    for n in 0..3 {
        let (_, tid) = e.thread_from_message("bob", &format!("t{n}")).await;
        e.clock.advance_ms(MIN);
        let tid_ch: ChannelId = tid.parse().unwrap();
        e.deps
            .modify(
                UserId::new(BOB),
                tid_ch,
                ThreadPatch {
                    archived: Some(true),
                    ..ThreadPatch::default()
                },
            )
            .await
            .unwrap();
        ids.push(tid);
    }
    let base = format!("/channels/{}/threads/archived/public", e.ch);
    let (s, p1) = e.get(&format!("{base}?limit=2"), "carol").await;
    assert_eq!(s, StatusCode::OK, "{p1}");
    assert_eq!(p1["has_more"], true);
    assert_eq!(p1["threads"][0]["id"], ids[2].as_str());
    let before = p1["threads"][1]["thread_metadata"]["archive_timestamp"]
        .as_str()
        .unwrap()
        .replace('+', "%2B");
    let (_, p2) = e.get(&format!("{base}?before={before}"), "carol").await;
    assert_eq!(p2["has_more"], false);
    assert_eq!(p2["threads"].as_array().unwrap().len(), 1);
    assert_eq!(p2["threads"][0]["id"], ids[0].as_str());
    let bad = e.get(&format!("{base}?before=yesterday"), "carol").await;
    assert_eq!(code(&bad), (400, Some(50_035)));
    // Private archives are for moderators; "joined" is open to everyone.
    let private = format!("/channels/{}/threads/archived/private", e.ch);
    assert_eq!(e.get(&private, "carol").await.0, StatusCode::FORBIDDEN);
    assert_eq!(e.get(&private, "owner").await.0, StatusCode::OK);
    let joined = format!("/channels/{}/users/@me/threads/archived/private", e.ch);
    assert_eq!(e.get(&joined, "carol").await.0, StatusCode::OK);
}

#[tokio::test]
async fn archived_threads_reopen_on_message_and_locked_ones_do_not() {
    let e = env().await;
    let (_, tid) = e.thread_from_message("bob", "t").await;
    let tch: ChannelId = tid.parse().unwrap();
    let patch = |archived: Option<bool>, locked: Option<bool>| ThreadPatch {
        archived,
        locked,
        ..ThreadPatch::default()
    };
    e.deps
        .modify(UserId::new(BOB), tch, patch(Some(true), None))
        .await
        .unwrap();
    let mut ev = e.events();
    assert_eq!(e.say(&tid, "carol", "wake up").await.0, StatusCode::OK);
    let mut reopened = false;
    for _ in 0..4 {
        if let DomainEvent::ThreadUpdated { payload, .. } = next_event(&mut ev).await {
            assert_eq!(payload["thread_metadata"]["archived"], false);
            reopened = true;
            break;
        }
    }
    assert!(reopened);
    // Locked by a moderator: members are refused (160005), moderators are not.
    e.deps
        .modify(UserId::new(OWNER), tch, patch(None, Some(true)))
        .await
        .unwrap();
    let r = e.say(&tid, "carol", "nope").await;
    assert_eq!(code(&r), (400, Some(160_005)), "{}", r.1);
    assert_eq!(e.say(&tid, "owner", "mod").await.0, StatusCode::OK);
}

#[tokio::test]
async fn auto_archive_sweeps_idle_threads_and_announces_them() {
    let e = env().await;
    let (_, tid) = e.thread_from_message("bob", "idle").await;
    let mut ev = e.events();
    e.clock.advance_ms(24 * 60 * MIN + 1);
    assert_eq!(e.deps.auto_archive().await.unwrap(), 1);
    match next_event(&mut ev).await {
        DomainEvent::ThreadUpdated {
            thread, payload, ..
        } => {
            assert_eq!(thread.to_string(), tid);
            assert_eq!(payload["thread_metadata"]["archived"], true);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(e.deps.auto_archive().await.unwrap(), 0);
}

#[tokio::test]
async fn deleting_a_thread_announces_to_its_viewers() {
    let e = env().await;
    let (_, tid) = e.thread_from_message("bob", "doomed").await;
    let tch: ChannelId = tid.parse().unwrap();
    assert!(e.deps.delete(UserId::new(CAROL), tch).await.is_err());
    let mut ev = e.events();
    let obj = e.deps.delete(UserId::new(OWNER), tch).await.unwrap();
    assert_eq!(obj.id, tid);
    match next_event(&mut ev).await {
        DomainEvent::ThreadDeleted {
            payload, viewers, ..
        } => {
            assert_eq!(payload["parent_id"], e.ch.to_string());
            assert!(viewers.contains(&UserId::new(CAROL)));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(e.say(&tid, "bob", "gone").await.0, StatusCode::NOT_FOUND);
}

async fn make_forum(e: &Env, flags: u32) -> (String, Vec<String>) {
    use sr_chat::ChannelType;
    use sr_chat::channel_rules::NewChannel;
    use sr_chat::threads::TagInput;
    let clock: Arc<dyn Clock> = Arc::new(ManualClock::new(T0));
    let chat = sr_chat::ChatService::new(
        e.repo.clone(),
        clock.clone(),
        SnowflakeGenerator::new(clock, 9, 11).unwrap(),
        sr_chat::ChatLimits::default(),
    );
    let tag = |name: &str, moderated: bool| TagInput {
        id: None,
        name: name.into(),
        moderated,
        emoji_id: None,
        emoji_name: None,
    };
    let mut input = NewChannel::new(ChannelType::Forum, "help");
    input.forum.available_tags = Some(vec![tag("question", false), tag("official", true)]);
    input.forum.flags = Some(flags);
    let ch = chat
        .create_channel(UserId::new(OWNER), e.guild, input)
        .await
        .unwrap();
    let tags = ch
        .forum
        .available_tags
        .iter()
        .map(|t| t.id.to_string())
        .collect();
    (ch.id.to_string(), tags)
}

#[tokio::test]
async fn forum_posts_create_thread_and_starter_message_together() {
    let e = env().await;
    let (forum, tags) = make_forum(&e, sr_chat::threads::channel_flags::REQUIRE_TAG).await;
    let mut ev = e.events();
    let (s, post) = e
        .post(
            &format!("/channels/{forum}/threads"),
            "bob",
            json!({
                "name": "How do I start?",
                "applied_tags": [tags[0]],
                "message": { "content": "Where is the manual?" },
            }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{post}");
    assert_eq!(post["type"], 11);
    assert_eq!(post["parent_id"], forum);
    assert_eq!(post["applied_tags"][0], tags[0]);
    assert_eq!(post["message"]["content"], "Where is the manual?");
    assert_eq!(post["message"]["channel_id"], post["id"]);
    assert_eq!(post["message_count"], 1);
    // THREAD_CREATE comes before the starter MESSAGE_CREATE.
    let mut order = Vec::new();
    for _ in 0..2 {
        match next_event(&mut ev).await {
            DomainEvent::ThreadCreated { .. } => order.push("thread"),
            DomainEvent::MessageSent { .. } => order.push("message"),
            DomainEvent::ThreadMemberUpdated { .. } => {}
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(order, vec!["thread", "message"]);

    let url = format!("/channels/{forum}/threads");
    let none = e
        .post(
            &url,
            "bob",
            json!({ "name": "x", "message": { "content": "y" } }),
        )
        .await;
    assert_eq!(code(&none), (400, Some(40_067)), "{}", none.1);
    let moderated = e
        .post(
            &url,
            "bob",
            json!({ "name": "x", "applied_tags": [tags[1]], "message": { "content": "y" } }),
        )
        .await;
    assert_eq!(moderated.0, StatusCode::FORBIDDEN, "{}", moderated.1);
    let unknown = e
        .post(
            &url,
            "bob",
            json!({ "name": "x", "applied_tags": ["12345"], "message": { "content": "y" } }),
        )
        .await;
    assert_eq!(unknown.0, StatusCode::BAD_REQUEST);
    let nomsg = e
        .post(
            &url,
            "bob",
            json!({ "name": "x", "applied_tags": [tags[0]] }),
        )
        .await;
    assert_eq!(code(&nomsg), (400, Some(50_035)));
    // A failed starter message leaves no post behind.
    let empty = e
        .post(
            &url,
            "bob",
            json!({ "name": "ghost", "applied_tags": [tags[0]], "message": { "content": "" } }),
        )
        .await;
    assert_eq!(empty.0, StatusCode::BAD_REQUEST, "{}", empty.1);
    assert_eq!(
        e.active("bob").await["threads"].as_array().unwrap().len(),
        1
    );
    // Plain messages in the forum channel itself are refused.
    assert_eq!(
        e.say(&forum, "bob", "direct").await.0,
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn openapi_documents_the_thread_paths() {
    let doc = serde_json::to_value(super::openapi()).unwrap();
    let paths = doc["paths"].as_object().unwrap();
    for p in [
        "/channels/{channel_id}/messages/{message_id}/threads",
        "/channels/{channel_id}/threads",
        "/channels/{channel_id}/thread-members/{user_id}",
        "/channels/{channel_id}/thread-members",
        "/guilds/{guild_id}/threads/active",
        "/channels/{channel_id}/threads/archived/public",
        "/channels/{channel_id}/threads/archived/private",
        "/channels/{channel_id}/users/@me/threads/archived/private",
    ] {
        assert!(paths.contains_key(p), "missing {p}");
    }
}
