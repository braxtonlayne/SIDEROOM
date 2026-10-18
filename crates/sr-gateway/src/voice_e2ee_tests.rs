//! Op 101 E2EE_MLS relay tests (VOX-E2E-004): only channel members send and receive, the
//! first commit per epoch wins, welcomes reach only their recipients, and transitions execute
//! once everyone is ready. The payloads are opaque to the server.

use super::*;
use std::time::Instant;

async fn join(e: &Env, user: u64, channel: ChannelId) -> Client {
    let mut c = client(e, user).await;
    c.voice(1, Some(channel.get()), false, false).await;
    c.event("VOICE_SERVER_UPDATE").await;
    c.drain().await;
    c
}

fn mls(kind: &str, extra: Value) -> Value {
    let mut d = json!({ "type": kind, "channel_id": LOBBY.get().to_string() });
    for (k, v) in extra.as_object().unwrap() {
        d[k] = v.clone();
    }
    json!({ "op": 101, "d": d })
}

/// The `E2EE_MLS` dispatches among `events`.
fn mls_events(events: &[Value]) -> Vec<Value> {
    events
        .iter()
        .filter(|v| v["t"] == "E2EE_MLS")
        .map(|v| v["d"].clone())
        .collect()
}

#[tokio::test]
async fn relay_orders_commits_and_reaches_only_members() {
    let e = env();
    let mut a = join(&e, MEMBER, LOBBY).await;
    let mut b = join(&e, FRIEND, LOBBY).await;
    let mut elsewhere = join(&e, MOD, ROOM_B).await;
    a.drain().await;

    // A key package goes to the other members only.
    b.send(mls("key_package", json!({ "data": "S1AtRlJJRU5E" })))
        .await;
    let got = mls_events(&a.drain().await);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0]["type"], "key_package");
    assert_eq!(got[0]["from"], "2");
    assert_eq!(got[0]["data"], "S1AtRlJJRU5E");
    assert!(mls_events(&b.drain().await).is_empty());
    assert!(mls_events(&elsewhere.drain().await).is_empty());

    // A commits on epoch 0 and welcomes B.
    a.send(mls(
        "commit",
        json!({ "epoch": 0, "data": "Q09NTUlU", "welcome": "V0VMQ09NRQ==", "to": ["2", "4"] }),
    ))
    .await;
    let at_a = mls_events(&a.drain().await);
    assert_eq!(at_a.len(), 1, "{at_a:?}");
    assert_eq!(at_a[0]["type"], "commit");
    assert_eq!(at_a[0]["epoch"], 1);
    let at_b = mls_events(&b.drain().await);
    let kinds: Vec<_> = at_b.iter().map(|d| d["type"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["commit", "welcome"]);
    assert_eq!(at_b[1]["data"], "V0VMQ09NRQ==");
    assert_eq!(at_b[1]["epoch"], 1);
    // MOD was named in `to` but is not in the channel: nothing.
    assert!(mls_events(&elsewhere.drain().await).is_empty());
    assert_eq!(e.gw.e2ee_epoch(LOBBY), 1);

    // B raced on epoch 0: rejected, told the current epoch, nobody else hears of it.
    b.send(mls("commit", json!({ "epoch": 0, "data": "TE9TRVI=" })))
        .await;
    let at_b = mls_events(&b.drain().await);
    assert_eq!(at_b.len(), 1);
    assert_eq!(at_b[0]["type"], "commit_rejected");
    assert_eq!(at_b[0]["epoch"], 1);
    assert!(mls_events(&a.drain().await).is_empty());

    // B is ready for epoch 1: everyone executes.
    b.send(mls("ready", json!({ "epoch": 1 }))).await;
    for c in [&mut a, &mut b] {
        let got = mls_events(&c.drain().await);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0]["type"], "execute");
        assert_eq!(got[0]["epoch"], 1);
    }

    // When the channel empties the ordering state goes with it.
    a.voice(1, None, false, false).await;
    b.voice(1, None, false, false).await;
    a.drain().await;
    b.drain().await;
    assert_eq!(e.gw.e2ee_epoch(LOBBY), 0);
}

#[tokio::test]
async fn non_members_and_bad_payloads_are_ignored() {
    let e = env();
    let mut a = join(&e, MEMBER, LOBBY).await;
    let mut outside = join(&e, MOD, ROOM_B).await;
    a.drain().await;

    // Not in LOBBY.
    outside
        .send(mls("key_package", json!({ "data": "QUJD" })))
        .await;
    outside
        .send(mls("commit", json!({ "epoch": 0, "data": "QUJD" })))
        .await;
    // Not base64, too large, missing epoch, unknown type.
    a.send(mls("proposal", json!({ "data": "<script>" }))).await;
    a.send(mls(
        "proposal",
        json!({ "data": "A".repeat(crate::voice_e2ee::MAX_MLS_CHARS + 1) }),
    ))
    .await;
    a.send(mls("commit", json!({ "data": "QUJD" }))).await;
    a.send(mls("shout", json!({ "data": "QUJD" }))).await;
    assert!(mls_events(&a.drain().await).is_empty());
    assert!(mls_events(&outside.drain().await).is_empty());
    assert_eq!(e.gw.e2ee_epoch(LOBBY), 0);
}

#[tokio::test]
async fn a_lone_committer_executes_at_once_and_leaving_unblocks_a_transition() {
    let e = env();
    let mut a = join(&e, MEMBER, LOBBY).await;
    a.send(mls("commit", json!({ "epoch": 0, "data": "QUJD" })))
        .await;
    let kinds: Vec<_> = mls_events(&a.drain().await)
        .iter()
        .map(|d| d["type"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(kinds, ["commit", "execute"]);

    let mut b = join(&e, FRIEND, LOBBY).await;
    a.drain().await;
    a.send(mls("commit", json!({ "epoch": 1, "data": "QUJD" })))
        .await;
    assert_eq!(mls_events(&b.drain().await)[0]["epoch"], 2);
    // B leaves without saying ready: A no longer waits.
    b.voice(1, None, false, false).await;
    let got = mls_events(&a.drain().await);
    let exec: Vec<_> = got.iter().filter(|d| d["type"] == "execute").collect();
    assert_eq!(exec.len(), 1);
    assert_eq!(exec[0]["epoch"], 2);
}

#[tokio::test]
async fn the_relay_is_off_unless_configured() {
    let e = env_full(true, false);
    let mut a = join(&e, MEMBER, LOBBY).await;
    let mut b = join(&e, FRIEND, LOBBY).await;
    a.drain().await;
    b.send(mls("key_package", json!({ "data": "QUJD" }))).await;
    b.send(mls("commit", json!({ "epoch": 0, "data": "QUJD" })))
        .await;
    assert!(mls_events(&a.drain().await).is_empty());
    assert!(mls_events(&b.drain().await).is_empty());
    assert_eq!(e.gw.e2ee_epoch(LOBBY), 0);
}

mod with_mls {
    //! The relay with real MLS clients on both ends (VOX-E2E-001 + 004): the server hands out
    //! the group id and its external-sender key, relays the members' messages, and proposes
    //! the removal of a member who leaves; the clients' MLS accepts all of it.
    use super::*;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as B64;
    use p256::ecdsa::signature::Signer as _;
    use p256::ecdsa::{Signature, SigningKey};
    use sr_e2ee::mls::{CallClient, Handled, PendingSession};
    use std::collections::HashSet;

    fn mls_client(user: u64) -> CallClient {
        let ik = SigningKey::from_slice(&[user as u8 + 7; 32]).unwrap();
        let public = ik
            .verifying_key()
            .to_encoded_point(false)
            .as_bytes()
            .to_vec();
        let p = PendingSession::generate(user, &public).unwrap();
        let sig: Signature = ik.sign(&p.cert_message());
        p.certify(&sig.to_bytes()).unwrap()
    }

    fn b64(v: &Value) -> Vec<u8> {
        B64.decode(v.as_str().unwrap()).unwrap()
    }

    fn of_type(events: &[Value], t: &str) -> Vec<Value> {
        mls_events(events)
            .into_iter()
            .filter(|d| d["type"] == t)
            .collect()
    }

    #[tokio::test]
    async fn a_call_from_creation_to_an_external_remove() {
        let e = env();
        let mut a = join(&e, MEMBER, LOBBY).await;
        a.send(mls("hello", json!({}))).await;
        let call = of_type(&a.drain().await, "call").remove(0);
        assert_eq!(call["create"], true);
        assert_eq!(call["epoch"], 0);
        let ds = &call["external_sender"];
        let mut ma = mls_client(MEMBER);
        ma.create_group(
            &b64(&call["group_id"]),
            Some((&b64(&ds["key"]), &b64(&ds["identity"]))),
        )
        .unwrap();
        a.send(mls(
            "ready",
            json!({ "epoch": 0, "leaf": ma.own_leaf().unwrap() }),
        ))
        .await;

        // B joins: told not to create, same group id; sends its key package.
        let mut b = join(&e, FRIEND, LOBBY).await;
        a.drain().await;
        b.send(mls("hello", json!({}))).await;
        let call_b = of_type(&b.drain().await, "call").remove(0);
        assert_eq!(call_b["create"], false);
        assert_eq!(call_b["group_id"], call["group_id"]);
        let mut mb = mls_client(FRIEND);
        b.send(mls(
            "key_package",
            json!({ "data": B64.encode(mb.key_package().unwrap()) }),
        ))
        .await;
        let kp = of_type(&a.drain().await, "key_package").remove(0);
        let roster: HashSet<u64> = [MEMBER, FRIEND].into();
        let c = ma.commit(&[b64(&kp["data"])], &roster).unwrap().unwrap();
        a.send(mls(
            "commit",
            json!({
                "epoch": c.based_on,
                "data": B64.encode(&c.commit),
                "welcome": B64.encode(c.welcome.as_ref().unwrap()),
                "to": ["2"],
            }),
        ))
        .await;
        assert_eq!(of_type(&a.drain().await, "commit")[0]["epoch"], 1);
        ma.commit_accepted().unwrap();
        let w = of_type(&b.drain().await, "welcome").remove(0);
        assert_eq!(mb.join(&b64(&w["data"]), &roster).unwrap(), 1);
        b.send(mls(
            "ready",
            json!({ "epoch": 1, "leaf": mb.own_leaf().unwrap() }),
        ))
        .await;
        assert_eq!(of_type(&a.drain().await, "execute")[0]["epoch"], 1);
        b.drain().await;

        // B leaves: the server proposes the removal, A's MLS accepts it and commits it.
        b.voice(1, None, false, false).await;
        let p = of_type(&a.drain().await, "proposal").remove(0);
        assert!(p["from"].is_null());
        let roster: HashSet<u64> = [MEMBER].into();
        assert_eq!(
            ma.handle(&b64(&p["data"]), &roster).unwrap(),
            Handled::Proposal
        );
        let c = ma.commit(&[], &roster).unwrap().unwrap();
        a.send(mls(
            "commit",
            json!({ "epoch": c.based_on, "data": B64.encode(&c.commit) }),
        ))
        .await;
        let got = mls_events(&a.drain().await);
        assert_eq!(got[0]["type"], "commit");
        assert_eq!(got[1]["type"], "execute");
        ma.commit_accepted().unwrap();
        assert_eq!(ma.members().unwrap().len(), 1);
        assert_eq!(e.gw.e2ee_epoch(LOBBY), 2);
    }

    #[tokio::test]
    async fn the_group_restarts_when_its_last_member_leaves() {
        let e = env();
        let mut a = join(&e, MEMBER, LOBBY).await;
        let mut b = join(&e, FRIEND, LOBBY).await;
        a.drain().await;
        a.send(mls("hello", json!({}))).await;
        let first = of_type(&a.drain().await, "call").remove(0);
        assert_eq!(first["create"], true);
        a.send(mls("ready", json!({ "epoch": 0, "leaf": 0 }))).await;
        b.send(mls("hello", json!({}))).await;
        assert_eq!(of_type(&b.drain().await, "call")[0]["create"], false);
        // A (the only one in the group) leaves before adding B: B starts a new group.
        a.voice(1, None, false, false).await;
        let again = of_type(&b.drain().await, "call").remove(0);
        assert_eq!(again["create"], true);
        assert_ne!(again["group_id"], first["group_id"]);
    }
}

// ---- D-263: the capability is declared at join and the handshake starts in time ------------------

fn join_frame(declare: Option<bool>) -> Value {
    let mut d = json!({
        "guild_id": "1", "channel_id": LOBBY.get().to_string(),
        "self_mute": false, "self_deaf": false,
    });
    if let Some(v) = declare {
        d["sr_e2ee"] = json!(v);
    }
    json!({ "op": 4, "d": d })
}

#[tokio::test]
async fn a_join_without_the_e2ee_declaration_is_refused_and_gets_no_token() {
    let e = env();
    for declare in [None, Some(false)] {
        let mut c = client(&e, MEMBER).await;
        c.send_exact(join_frame(declare)).await;
        let err = c.event("SR_VOICE_ERROR").await;
        assert_eq!(err["code"], "e2ee_required", "{err}");
        assert_eq!(err["channel_id"], LOBBY.get().to_string());
        let rest = names(&c.drain().await);
        assert!(
            !rest.contains(&"VOICE_SERVER_UPDATE".to_owned()),
            "{rest:?}"
        );
        assert!(e.gw.voice_state(UserId::new(MEMBER)).is_none());
    }
    assert!(
        e.relay.0.lock().unwrap().is_empty(),
        "the SFU was never told"
    );

    // With the declaration the same join is admitted and gets its token.
    let mut c = client(&e, MEMBER).await;
    c.send_exact(join_frame(Some(true))).await;
    let update = c.event("VOICE_SERVER_UPDATE").await;
    assert!(update["token"].is_string());
    assert!(e.gw.voice_state(UserId::new(MEMBER)).is_some());
}

#[tokio::test]
async fn without_e2ee_nothing_is_required_at_join() {
    let e = env_full(true, false);
    let mut c = client(&e, MEMBER).await;
    c.send_exact(join_frame(None)).await;
    c.event("VOICE_SERVER_UPDATE").await;
    // And there is no hello to wait for.
    e.gw.e2ee_hello_sweep(Instant::now() + Duration::from_secs(60))
        .await;
    assert!(e.gw.voice_state(UserId::new(MEMBER)).is_some());
}

#[tokio::test]
async fn a_member_who_never_says_hello_is_removed_after_ten_seconds() {
    let e = env();
    let mut slow = client(&e, MEMBER).await;
    slow.voice(1, Some(LOBBY.get()), false, false).await;
    slow.event("VOICE_SERVER_UPDATE").await;
    let mut ok = client(&e, FRIEND).await;
    ok.voice(1, Some(LOBBY.get()), false, false).await;
    ok.event("VOICE_SERVER_UPDATE").await;
    ok.send(mls("hello", json!({}))).await;
    ok.event("E2EE_MLS").await;
    slow.drain().await;

    // Not yet due.
    e.gw.e2ee_hello_sweep(Instant::now() + Duration::from_secs(5))
        .await;
    assert!(e.gw.voice_state(UserId::new(MEMBER)).is_some());
    // Overdue: the silent one is removed with the code; the one that said hello stays.
    e.gw.e2ee_hello_sweep(Instant::now() + Duration::from_secs(11))
        .await;
    let err = slow.event("SR_VOICE_ERROR").await;
    assert_eq!(err["code"], "e2ee_required");
    assert!(e.gw.voice_state(UserId::new(MEMBER)).is_none());
    assert!(e.gw.voice_state(UserId::new(FRIEND)).is_some());
    // The other member sees them leave.
    let seen = ok.drain().await;
    assert!(
        seen.iter().any(|v| v["t"] == "VOICE_STATE_UPDATE"
            && v["d"]["user_id"]
                .as_str()
                .and_then(|s| s.parse::<u64>().ok())
                == Some(MEMBER)
            && v["d"]["channel_id"].is_null()),
        "{seen:?}"
    );
}

#[tokio::test]
async fn leaving_before_the_deadline_clears_the_wait() {
    let e = env();
    let mut c = client(&e, MEMBER).await;
    c.voice(1, Some(LOBBY.get()), false, false).await;
    c.event("VOICE_SERVER_UPDATE").await;
    c.voice(1, None, false, false).await;
    c.drain().await;
    // A later join in another channel is judged on its own clock.
    c.voice(1, Some(ROOM_B.get()), false, false).await;
    c.event("VOICE_SERVER_UPDATE").await;
    e.gw.e2ee_hello_sweep(Instant::now() + Duration::from_secs(5))
        .await;
    assert!(e.gw.voice_state(UserId::new(MEMBER)).is_some());
    c.send(json!({ "op": 101, "d": { "type": "hello", "channel_id": ROOM_B.get().to_string() } }))
        .await;
    c.drain().await;
    e.gw.e2ee_hello_sweep(Instant::now() + Duration::from_secs(60))
        .await;
    assert!(e.gw.voice_state(UserId::new(MEMBER)).is_some());
}
