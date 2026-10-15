//! Search endpoint tests over the in-memory backend and a RAM index.

use super::{ConfirmedUsers, SearchDeps, router};
use crate::routes::messages::{self, FixedEntitlements, MessagesDeps};
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
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sr_chat::guild_rules::plan_new_guild;
use sr_chat::memory::MemoryBackend;
use sr_chat::repo::GuildRepository;
use sr_chat::{Channel, ChannelType, Member};
use sr_core::permissions::{Overwrite, OverwriteKind, Permissions};
use sr_core::{ChannelId, Clock, GuildId, ManualClock, SnowflakeGenerator, UserId};
use sr_events::{DomainEvent, InProcessBus};
use sr_search::{MessageEvent, MessageIndex};
use sr_storage::ObjectStoreStorage;
use std::collections::HashSet;
use std::sync::Arc;
use tower::ServiceExt;

const T0: i64 = 1_800_000_000_000;
const OWNER: u64 = 1;
const BOB: u64 = 2;
const CAROL: u64 = 3; // not a guild member

struct Env {
    app: Router,
    backend: MemoryBackend,
    search: Arc<SearchDeps>,
    clock: Arc<ManualClock>,
    guild: GuildId,
    general: ChannelId,
    other: ChannelId,
}

async fn env(confirmed: &[u64]) -> Env {
    let clock = Arc::new(ManualClock::new(T0));
    let dyn_clock: Arc<dyn Clock> = clock.clone();
    let backend = MemoryBackend::new();
    let gen_ids = SnowflakeGenerator::new(dyn_clock.clone(), 9, 9).unwrap();
    let bp = plan_new_guild(UserId::new(OWNER), "Club", T0, || gen_ids.generate()).unwrap();
    let (guild, general, voice) = (bp.guild.id, bp.channels[2].id, bp.channels[3].id);
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
    let mut settings = ApiSettings::default();
    settings.rate_limit.enabled = false;
    let msgs = MessagesDeps::new(
        backend.clone(),
        Arc::new(backend.clone()),
        Arc::new(ObjectStoreStorage::in_memory()),
        Arc::new(InProcessBus::new()),
        Arc::new(FixedEntitlements(1 << 20)),
        Arc::new(backend.clone()),
        None,
        dyn_clock.clone(),
        1,
        1,
    )
    .unwrap();
    let confirmed: HashSet<UserId> = confirmed.iter().map(|u| UserId::new(*u)).collect();
    let search = Arc::new(
        SearchDeps::new(Arc::new(MessageIndex::in_ram().unwrap()))
            .with_age_gate(Arc::new(ConfirmedUsers(confirmed))),
    );
    let state = AppState::builder()
        .bots(bots)
        .clock(dyn_clock)
        .settings(settings)
        .extension(Arc::new(msgs))
        .extension(search.clone())
        .build();
    let app = build_router(state, router().merge(messages::router()));
    Env {
        app,
        backend,
        search,
        clock,
        guild,
        general,
        other: voice,
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

    /// Post a message as `who` in `ch` and advance the clock so ids order.
    async fn say(&self, ch: ChannelId, who: &str, text: &str) -> String {
        self.clock.advance_ms(10);
        let (s, v) = self
            .call(
                Method::POST,
                &format!("/channels/{ch}/messages"),
                who,
                Some(json!({"content": text})),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{v}");
        v["id"].as_str().unwrap().to_owned()
    }

    /// Feed every stored message to the index, like the indexing job does.
    fn index_all(&self) {
        let events: Vec<MessageEvent> = self
            .backend
            .data()
            .messages
            .iter()
            .map(MessageEvent::from)
            .collect();
        self.search.index.apply_batch(&events).unwrap();
    }

    fn guild_search(&self, query: &str) -> String {
        format!("/guilds/{}/messages/search?{query}", self.guild)
    }

    async fn ids(&self, path: &str, who: &str) -> Vec<String> {
        let (s, v) = self.get(path, who).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        v["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g[0]["id"].as_str().unwrap().to_owned())
            .collect()
    }

    /// A text channel in the guild, optionally hidden from `@everyone`.
    fn add_channel(&self, id: u64, name: &str, hidden: bool, nsfw: bool) -> ChannelId {
        let cid = ChannelId::new(id);
        let mut c = Channel::new(cid, Some(self.guild), ChannelType::Text);
        c.name = Some(name.to_owned());
        c.nsfw = nsfw;
        let mut d = self.backend.data();
        d.channels.insert(cid, c);
        if hidden {
            d.overwrites
                .insert(cid, vec![hide_from_everyone(self.guild)]);
        }
        cid
    }

    fn hide(&self, ch: ChannelId) {
        self.backend
            .data()
            .overwrites
            .insert(ch, vec![hide_from_everyone(self.guild)]);
    }
}

fn hide_from_everyone(guild: GuildId) -> Overwrite {
    Overwrite {
        id: guild.snowflake(),
        kind: OverwriteKind::Role,
        allow: Permissions::empty(),
        deny: Permissions::VIEW_CHANNEL,
    }
}

#[tokio::test]
async fn guild_search_finds_filters_sorts_and_pages() {
    let e = env(&[]).await;
    let a = e.say(e.general, "bob", "the quick brown fox").await;
    let b = e
        .say(
            e.other,
            "owner",
            "a lazy dog and a fox https://example.com/x",
        )
        .await;
    let c = e.say(e.general, "owner", "nothing relevant").await;
    e.index_all();

    // Text search across channels, newest first, with Discord's envelope.
    let (s, v) = e.get(&e.guild_search("content=fox"), "bob").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["total_results"], 2);
    assert_eq!(v["doing_deep_historical_index"], false);
    assert_eq!(v["messages"][0][0]["id"], b);
    assert_eq!(v["messages"][0][0]["hit"], true);
    assert_eq!(v["messages"][1][0]["id"], a);
    assert!(
        v["messages"][0][0]["sr_snippet"]["text"]
            .as_str()
            .unwrap()
            .contains("fox")
    );
    assert_eq!(v["messages"][0][0]["author"]["id"], OWNER.to_string());

    // Oldest first, relevance, paging.
    assert_eq!(
        e.ids(&e.guild_search("content=fox&sort_order=asc"), "bob")
            .await,
        vec![a.clone(), b.clone()]
    );
    assert_eq!(
        e.ids(&e.guild_search("content=fox&sort_by=relevance"), "bob")
            .await
            .len(),
        2
    );
    assert_eq!(
        e.ids(&e.guild_search("content=fox&limit=1&offset=1"), "bob")
            .await,
        vec![a.clone()]
    );
    let (_, paged) = e.get(&e.guild_search("content=fox&limit=1"), "bob").await;
    assert_eq!(paged["total_results"], 2, "total counts every hit");

    // Filters: author, channel, has:link, raw `q` syntax.
    assert_eq!(
        e.ids(
            &e.guild_search(&format!("content=fox&author_id={BOB}")),
            "bob"
        )
        .await,
        vec![a.clone()]
    );
    assert_eq!(
        e.ids(
            &e.guild_search(&format!("content=fox&channel_id={}", e.other)),
            "bob"
        )
        .await,
        vec![b.clone()]
    );
    assert_eq!(
        e.ids(&e.guild_search("has=link"), "bob").await,
        vec![b.clone()]
    );
    assert_eq!(
        e.ids(&e.guild_search(&format!("q=from:{OWNER}")), "bob")
            .await,
        vec![c.clone(), b.clone()]
    );
    assert_eq!(
        e.ids(&e.guild_search("content=%22lazy%20dog%22"), "bob")
            .await,
        vec![b.clone()]
    );
    assert!(
        e.ids(&e.guild_search("content=absent"), "bob")
            .await
            .is_empty()
    );

    // Bad input.
    for q in [
        "",
        "content=%20",
        "has=banana",
        "author_id=bob",
        "content=x&limit=0",
        "content=x&limit=26",
        "content=x&offset=10000",
        "content=x&sort_by=hot",
        "content=x&sort_order=up",
        "q=before:2025-13-40",
    ] {
        let (s, v) = e.get(&e.guild_search(q), "bob").await;
        assert_eq!(
            (s, v["code"].as_u64()),
            (StatusCode::BAD_REQUEST, Some(50035)),
            "{q}"
        );
    }
}

#[tokio::test]
async fn guild_search_enforces_membership_and_channel_permissions() {
    let e = env(&[]).await;
    let secret = e.add_channel(9_001, "staff", true, false);
    let public = e.say(e.general, "owner", "launch plans public").await;
    let private = e.say(secret, "owner", "launch plans secret").await;
    e.index_all();
    let q = e.guild_search("content=launch");

    // Non-members and unknown guilds are refused.
    assert_eq!(e.get(&q, "carol").await.1["code"], 50001);
    assert_eq!(
        e.get("/guilds/123/messages/search?content=x", "owner")
            .await
            .1["code"],
        10004
    );

    // Members only see channels they can read; the administrator sees both.
    assert_eq!(e.ids(&q, "bob").await, vec![public.clone()]);
    assert_eq!(
        e.ids(&q, "owner").await,
        vec![private.clone(), public.clone()]
    );
    // Asking for the hidden channel explicitly does not leak it either.
    assert!(
        e.ids(
            &e.guild_search(&format!("content=launch&channel_id={secret}")),
            "bob"
        )
        .await
        .is_empty()
    );

    // READ_MESSAGE_HISTORY is required as well as VIEW_CHANNEL.
    {
        let mut d = e.backend.data();
        let everyone = d.roles.iter_mut().find(|r| r.is_everyone()).unwrap();
        everyone
            .permissions
            .remove(Permissions::READ_MESSAGE_HISTORY);
    }
    e.search.apply_event(&DomainEvent::RoleUpdated {
        guild: e.guild,
        role: sr_core::RoleId::new(e.guild.get()),
        payload: json!({}),
    });
    assert!(e.ids(&q, "bob").await.is_empty());
    assert_eq!(e.ids(&q, "owner").await.len(), 2);
}

#[tokio::test]
async fn allow_list_is_cached_until_invalidated() {
    let e = env(&[]).await;
    let room = e.add_channel(9_002, "room", false, false);
    let id = e.say(room, "owner", "cache test message").await;
    e.index_all();
    let q = e.guild_search("content=cache");
    assert_eq!(e.ids(&q, "bob").await, vec![id]);

    // The channel is hidden, but the 60 s cache still lists it...
    e.hide(room);
    assert_eq!(e.ids(&q, "bob").await.len(), 1);
    // ...until a permission-relevant event invalidates the guild.
    e.search.apply_event(&DomainEvent::ChannelUpdated {
        guild: e.guild,
        channel: room,
        payload: json!({}),
    });
    assert!(e.ids(&q, "bob").await.is_empty());
}

#[tokio::test]
async fn nsfw_channels_need_a_confirmed_age() {
    let e = env(&[BOB]).await;
    let spicy = e.add_channel(9_003, "spicy", false, true);
    let nsfw_msg = e.say(spicy, "owner", "adult content here").await;
    e.say(e.general, "owner", "adult-free zone").await;
    e.index_all();
    let q = e.guild_search("content=adult");

    // Bob confirmed his age and sees both; the owner did not, so no NSFW.
    let bob_hits = e.ids(&q, "bob").await;
    assert_eq!(bob_hits.len(), 2);
    assert!(bob_hits.contains(&nsfw_msg));
    let owner_hits = e.ids(&q, "owner").await;
    assert_eq!(owner_hits.len(), 1);
    assert!(!owner_hits.contains(&nsfw_msg));

    // The single-channel endpoint applies the same gate.
    let path = format!("/channels/{spicy}/messages/search?content=adult");
    assert_eq!(e.ids(&path, "bob").await, vec![nsfw_msg]);
    assert!(e.ids(&path, "owner").await.is_empty());
}

#[tokio::test]
async fn deleted_messages_disappear() {
    let e = env(&[]).await;
    let keep = e.say(e.general, "bob", "remember the milk").await;
    let gone = e.say(e.general, "bob", "forget the milk").await;
    e.index_all();
    let q = e.guild_search("content=milk");
    assert_eq!(e.ids(&q, "bob").await.len(), 2);

    // Deleted through the API: gone from results even before the index event.
    let (s, _) = e
        .call(
            Method::DELETE,
            &format!("/channels/{}/messages/{gone}", e.general),
            "bob",
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert_eq!(e.ids(&q, "bob").await, vec![keep.clone()]);

    // And once the indexer processes the removal the count drops too.
    e.search
        .index
        .apply(&MessageEvent::Deleted(gone.parse().unwrap()))
        .unwrap();
    let (_, v) = e.get(&q, "bob").await;
    assert_eq!(v["total_results"], 1);
}

#[tokio::test]
async fn channel_search_covers_guild_channels_and_dms() {
    let e = env(&[]).await;
    let first = e.say(e.general, "bob", "pineapple pizza").await;
    e.say(e.other, "bob", "pineapple juice").await;

    // A DM between owner and bob.
    let dm = ChannelId::new(7_001);
    {
        let mut d = e.backend.data();
        d.channels
            .insert(dm, Channel::new(dm, None, ChannelType::Dm));
        d.recipients.insert((dm, UserId::new(OWNER)));
        d.recipients.insert((dm, UserId::new(BOB)));
    }
    let dm_msg = e.say(dm, "owner", "pineapple secrets").await;
    e.index_all();

    // One channel only.
    let path = format!("/channels/{}/messages/search?content=pineapple", e.general);
    assert_eq!(e.ids(&path, "bob").await, vec![first]);
    // DM scope: only participants.
    let path = format!("/channels/{dm}/messages/search?content=pineapple");
    assert_eq!(e.ids(&path, "bob").await, vec![dm_msg.clone()]);
    assert_eq!(e.ids(&path, "owner").await, vec![dm_msg]);
    assert_eq!(e.get(&path, "carol").await.1["code"], 50001);
    // The guild search never reaches DMs.
    assert_eq!(
        e.ids(&e.guild_search("content=pineapple"), "bob")
            .await
            .len(),
        2
    );
    // Unknown channel, hidden channel.
    assert_eq!(
        e.get("/channels/5/messages/search?content=x", "bob")
            .await
            .1["code"],
        10003
    );
    e.hide(e.general);
    let path = format!("/channels/{}/messages/search?content=pineapple", e.general);
    assert_eq!(e.get(&path, "bob").await.1["code"], 50001);
}

#[test]
fn openapi_documents_search_paths() {
    let doc = serde_json::to_value(crate::routes::openapi()).unwrap();
    let paths = doc["paths"].as_object().unwrap();
    assert!(paths.contains_key("/guilds/{guild_id}/messages/search"));
    assert!(paths.contains_key("/channels/{channel_id}/messages/search"));
}
