//! Op 8 `REQUEST_GUILD_MEMBERS` (D-242, H-52) through the in-process gateway.

use std::time::Duration;

use serde_json::{Value, json};
use sr_core::{GuildId, UserId};

use crate::members_request::{DEFAULT_QUERY_LIMIT, MAX_NONCE_BYTES, MAX_USER_IDS, parse};
use crate::test_support::{TestClient, TestWorld};
use crate::{GatewayConfig, MemberRequest, close, intents};

const G: u64 = 1;
const MEMBERS: u64 = intents::GUILD_MEMBERS;
const BOTH: u64 = intents::GUILD_MEMBERS | intents::GUILD_PRESENCES;

fn world(roster: u64) -> TestWorld {
    let w = TestWorld::new(GatewayConfig::default());
    w.add_roster(GuildId::new(G), 1000, roster);
    w
}

/// Identify a bot (`bot:<id>:<privileged bits it may use>`) that is installed in guild 1.
async fn bot(w: &TestWorld, allowed: u64, wanted: u64) -> TestClient {
    w.add_member(UserId::new(7), GuildId::new(G));
    let mut c = w.connect();
    c.identify(&format!("bot:7:{allowed}"), wanted, 1).await;
    c
}

fn op8(d: Value) -> Value {
    json!({"op": 8, "d": d})
}

#[test]
fn payloads_follow_discords_shape() {
    let ok = |d: Value| parse(&d).unwrap();
    let p = ok(json!({"guild_id": "1", "query": "", "limit": 0, "nonce": "n"}));
    assert_eq!(
        p.request,
        MemberRequest::Search {
            query: String::new(),
            limit: 0
        }
    );
    assert_eq!(p.nonce.as_deref(), Some("n"));
    // A non-empty query never means "everyone".
    let p = ok(json!({"guild_id": 1, "query": "al"}));
    assert_eq!(
        p.request,
        MemberRequest::Search {
            query: "al".into(),
            limit: DEFAULT_QUERY_LIMIT
        }
    );
    let p = ok(json!({"guild_id": "1", "user_ids": ["5", 6, "5", "x"], "presences": true}));
    assert_eq!(
        p.request,
        MemberRequest::Ids(vec![UserId::new(5), UserId::new(6)])
    );
    assert_eq!(p.bad_ids, vec!["x".to_owned()]);
    assert!(p.presences);
    assert!(matches!(
        ok(json!({"guild_id": "1", "user_ids": "9"})).request,
        MemberRequest::Ids(v) if v == vec![UserId::new(9)]
    ));
    let long = "n".repeat(MAX_NONCE_BYTES + 1);
    let many: Vec<String> = (1..=MAX_USER_IDS as u64 + 1)
        .map(|i| i.to_string())
        .collect();
    for d in [
        json!("nope"),
        json!({"query": ""}),
        json!({"guild_id": "x", "query": ""}),
        json!({"guild_id": "1"}),
        json!({"guild_id": "1", "query": "a", "user_ids": ["2"]}),
        json!({"guild_id": "1", "query": "", "nonce": long}),
        json!({"guild_id": "1", "query": "", "limit": -1}),
        json!({"guild_id": "1", "query": 5}),
        json!({"guild_id": "1", "query": "", "presences": "yes"}),
        json!({"guild_id": "1", "user_ids": many}),
    ] {
        assert_eq!(parse(&d).unwrap_err(), close::DECODE_ERROR, "{d}");
    }
}

#[tokio::test]
async fn query_and_ids_are_answered_in_one_chunk_with_the_nonce() {
    let w = world(30);
    let mut c = bot(&w, 0, intents::GUILDS).await;
    c.send(op8(
        json!({"guild_id": "1", "query": "member101", "limit": 5, "nonce": "q1"}),
    ))
    .await;
    let d = c.await_dispatch("GUILD_MEMBERS_CHUNK").await["d"].clone();
    let names: Vec<_> = d["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["user"]["username"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        names,
        [
            "member1010",
            "member1011",
            "member1012",
            "member1013",
            "member1014"
        ]
    );
    assert_eq!(d["chunk_index"], 0);
    assert_eq!(d["chunk_count"], 1);
    assert_eq!(
        (d["guild_id"].as_str(), d["nonce"].as_str()),
        (Some("1"), Some("q1"))
    );
    assert!(d.get("not_found").is_none() && d.get("presences").is_none());

    c.send(op8(
        json!({"guild_id": "1", "user_ids": ["1003", "42", "1004"], "nonce": "i1"}),
    ))
    .await;
    let d = c.await_dispatch("GUILD_MEMBERS_CHUNK").await["d"].clone();
    assert_eq!(d["members"].as_array().unwrap().len(), 2);
    assert_eq!(d["not_found"], json!(["42"]));
    assert_eq!(d["nonce"], "i1");

    // No match is still one (empty) chunk, so a waiting client always hears back.
    c.send(op8(json!({"guild_id": "1", "query": "zzz"}))).await;
    let d = c.await_dispatch("GUILD_MEMBERS_CHUNK").await["d"].clone();
    assert_eq!(d["members"].as_array().unwrap().len(), 0);
    assert_eq!(d["chunk_count"], 1);
    assert!(d.get("nonce").is_none());
}

#[tokio::test]
async fn the_whole_list_needs_the_members_intent_and_chunks_at_1000() {
    let w = world(1001);
    let mut c = bot(&w, MEMBERS, intents::GUILDS | MEMBERS).await;
    c.send(op8(
        json!({"guild_id": "1", "query": "", "limit": 0, "nonce": "all"}),
    ))
    .await;
    let first = c.await_dispatch("GUILD_MEMBERS_CHUNK").await["d"].clone();
    let second = c.await_dispatch("GUILD_MEMBERS_CHUNK").await["d"].clone();
    for (d, index, len) in [(&first, 0, 1000), (&second, 1, 1)] {
        assert_eq!(d["chunk_index"], index);
        assert_eq!(d["chunk_count"], 2);
        assert_eq!(d["members"].as_array().unwrap().len(), len);
        assert_eq!(d["nonce"], "all");
    }

    // The application may use the intent but this session did not declare it: 4014.
    let w = world(3);
    let mut c = bot(&w, MEMBERS, intents::GUILDS).await;
    c.send(op8(json!({"guild_id": "1", "query": "", "limit": 0})))
        .await;
    assert_eq!(c.recv_close().await, close::DISALLOWED_INTENTS);
    // A prefix query does not need it.
    let mut c = bot(&w, 0, intents::GUILDS).await;
    c.send(op8(
        json!({"guild_id": "1", "query": "member100", "limit": 10}),
    ))
    .await;
    c.await_dispatch("GUILD_MEMBERS_CHUNK").await;
}

#[tokio::test]
async fn user_sessions_search_but_never_get_the_whole_list() {
    let w = world(5);
    w.add_member(UserId::new(2), GuildId::new(G));
    let mut c = w.connect_users(&[2], 0).await.remove(0);
    c.send(op8(
        json!({"guild_id": "1", "query": "member100", "limit": 2, "nonce": "u"}),
    ))
    .await;
    let d = c.await_dispatch("GUILD_MEMBERS_CHUNK").await["d"].clone();
    assert_eq!(d["members"].as_array().unwrap().len(), 2);
    c.send(op8(json!({"guild_id": "1", "user_ids": ["1001"]})))
        .await;
    c.await_dispatch("GUILD_MEMBERS_CHUNK").await;
    c.send(op8(json!({"guild_id": "1", "query": "", "limit": 0})))
        .await;
    assert_eq!(c.recv_close().await, close::DISALLOWED_INTENTS);
}

#[tokio::test]
async fn presences_need_their_intent_and_list_online_members() {
    let w = world(0);
    // Two roster members with sessions: 2000 online, 2001 invisible.
    w.add_roster(GuildId::new(G), 2000, 2);
    for u in [2000, 2001] {
        w.add_member(UserId::new(u), GuildId::new(G));
    }
    let mut online = w.connect_users(&[2000], intents::GUILDS).await.remove(0);
    let mut hidden = w.connect_users(&[2001], intents::GUILDS).await.remove(0);
    hidden
        .send(json!({"op": 3, "d": {"status": "invisible", "activities": []}}))
        .await;
    online.drain().await;
    hidden.drain().await;

    let mut c = bot(&w, BOTH, intents::GUILDS | BOTH).await;
    c.send(op8(
        json!({"guild_id": "1", "query": "member200", "limit": 10, "presences": true}),
    ))
    .await;
    let d = c.await_dispatch("GUILD_MEMBERS_CHUNK").await["d"].clone();
    assert_eq!(d["members"].as_array().unwrap().len(), 2);
    let presences = d["presences"].as_array().unwrap();
    assert_eq!(presences.len(), 1, "{presences:?}");
    assert_eq!(presences[0]["user"]["id"], "2000");
    assert_eq!(presences[0]["status"], "online");

    // Without GUILD_PRESENCES in the session's intents the request is refused.
    let mut c = bot(&w, MEMBERS, intents::GUILDS | MEMBERS).await;
    c.send(op8(
        json!({"guild_id": "1", "user_ids": ["2000"], "presences": true}),
    ))
    .await;
    assert_eq!(c.recv_close().await, close::DISALLOWED_INTENTS);
}

#[tokio::test]
async fn malformed_requests_close_with_4002_and_strangers_are_ignored() {
    let w = world(3);
    let mut c = bot(&w, 0, intents::GUILDS).await;
    c.send(op8(
        json!({"guild_id": "1", "query": "a", "user_ids": ["1"]}),
    ))
    .await;
    assert_eq!(c.recv_close().await, close::DECODE_ERROR);

    // A guild the session is not in: nothing happens, the connection stays.
    let mut c = bot(&w, 0, intents::GUILDS).await;
    c.send(op8(json!({"guild_id": "9", "query": "m", "limit": 1})))
        .await;
    assert!(
        c.try_dispatch("GUILD_MEMBERS_CHUNK", Duration::from_millis(150))
            .await
            .is_none()
    );
    // Before IDENTIFY op 8 is refused like any op.
    let mut c = w.connect();
    c.recv().await;
    c.send(op8(json!({"guild_id": "1", "query": "m", "limit": 1})))
        .await;
    assert_eq!(c.recv_close().await, close::NOT_AUTHENTICATED);
}
