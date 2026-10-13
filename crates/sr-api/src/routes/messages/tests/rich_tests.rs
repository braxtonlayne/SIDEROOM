//! Polls, stickers, voice messages, forwards, embed validation and link previews
//! over the in-memory backend and a scripted web (no network).

use super::*;
use crate::routes::messages::rich::JobScheduler;
use crate::routes::messages::{rich_jobs, rich_jobs::UnfurlOutcome};
use sr_chat::rich_repo::StickerRecord;
use sr_core::MessageId;
use sr_unfurl::fetch::{FetchError, HopRequest, HopResponse, HttpTransport, Resolver};
use sr_unfurl::{Policy, SafeFetcher, Unfurler};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;

#[derive(Default)]
pub struct RecordingScheduler {
    pub jobs: Mutex<Vec<(String, Value, Duration)>>,
}

#[async_trait]
impl JobScheduler for RecordingScheduler {
    async fn enqueue(&self, kind: &str, payload: Value, delay: Duration) -> Result<(), String> {
        self.jobs
            .lock()
            .unwrap()
            .push((kind.to_owned(), payload, delay));
        Ok(())
    }
}

impl RecordingScheduler {
    fn of_kind(&self, kind: &str) -> Vec<(Value, Duration)> {
        self.jobs
            .lock()
            .unwrap()
            .iter()
            .filter(|(k, _, _)| k == kind)
            .map(|(_, p, d)| (p.clone(), *d))
            .collect()
    }
}

/// Every host resolves to a public address; pages come from a table.
#[derive(Default)]
pub struct FakeWeb {
    pages: Arc<Mutex<HashMap<String, HopResponse>>>,
}

struct PublicResolver;

#[async_trait]
impl Resolver for PublicResolver {
    async fn resolve(&self, _host: &str, _port: u16) -> Result<Vec<IpAddr>, String> {
        Ok(vec!["93.184.216.34".parse().unwrap()])
    }
}

struct TableTransport(Arc<Mutex<HashMap<String, HopResponse>>>);

#[async_trait]
impl HttpTransport for TableTransport {
    async fn get(&self, req: &HopRequest) -> Result<HopResponse, FetchError> {
        self.0
            .lock()
            .unwrap()
            .get(req.url.as_str())
            .cloned()
            .ok_or_else(|| FetchError::Http("404".into()))
    }
}

impl FakeWeb {
    pub fn unfurler(&self) -> Unfurler {
        Unfurler::new(
            SafeFetcher::new(
                Policy::default(),
                Arc::new(PublicResolver),
                Arc::new(TableTransport(self.pages.clone())),
            ),
            100,
        )
    }

    fn page(&self, url: &str, html: &str) {
        self.pages.lock().unwrap().insert(
            url.to_owned(),
            HopResponse {
                status: 200,
                content_type: Some("text/html".into()),
                body: html.as_bytes().to_vec(),
                ..HopResponse::default()
            },
        );
    }
}

fn poll_body(duration: u32, multi: bool) -> Value {
    json!({
        "poll": {
            "question": {"text": "Best pet?"},
            "answers": [
                {"poll_media": {"text": "Cat", "emoji": {"name": "\u{1F431}"}}},
                {"poll_media": {"text": "Dog"}},
                {"poll_media": {"text": "Fish"}},
            ],
            "duration": duration,
            "allow_multiselect": multi,
        }
    })
}

fn counts(m: &Value) -> Vec<u64> {
    m["poll"]["results"]["answer_counts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["count"].as_u64().unwrap())
        .collect()
}

async fn put(e: &Env, path: &str, who: &str) -> StatusCode {
    e.status(Method::PUT, path, who).await
}

#[tokio::test]
async fn poll_lifecycle_votes_results_events_and_finalise() {
    let e = env().await;
    let mut ev = e.events();
    let (s, m) = e.post("/messages", "bob", poll_body(24, false)).await;
    assert_eq!(s, StatusCode::OK, "{m}");
    let id = id_of(&m);
    assert_eq!(m["poll"]["question"]["text"], "Best pet?");
    assert_eq!(m["poll"]["answers"][1]["answer_id"], 2);
    assert_eq!(m["poll"]["allow_multiselect"], false);
    assert_eq!(m["poll"]["results"]["is_finalized"], false);
    assert_eq!(counts(&m), vec![0, 0, 0]);
    assert!(m["poll"]["expiry"].as_str().unwrap().starts_with("2027-01-16T08:00:00"));
    // The finalise job is queued for the expiry.
    let queued = e.jobs.of_kind("messages.poll_finalize");
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].1, Duration::from_secs(24 * 3600));
    assert_eq!(queued[0].0["message_id"].as_u64().unwrap().to_string(), id);
    let _ = next_event(&mut ev).await; // MESSAGE_CREATE

    let vote = |ans: u32| format!("/polls/{id}/answers/{ans}/voters/@me");
    assert_eq!(put(&e, &vote(1), "owner").await, StatusCode::NO_CONTENT);
    match next_event(&mut ev).await {
        DomainEvent::PollVoteAdded {
            user, answer_id, ..
        } => assert_eq!((user.get(), answer_id), (OWNER, 1)),
        other => panic!("{other:?}"),
    }
    // Voting again is a no-op; single choice moves the vote.
    assert_eq!(put(&e, &vote(1), "owner").await, StatusCode::NO_CONTENT);
    no_event(&mut ev).await;
    assert_eq!(put(&e, &vote(2), "owner").await, StatusCode::NO_CONTENT);
    assert!(matches!(
        next_event(&mut ev).await,
        DomainEvent::PollVoteRemoved { answer_id: 1, .. }
    ));
    assert!(matches!(
        next_event(&mut ev).await,
        DomainEvent::PollVoteAdded { answer_id: 2, .. }
    ));
    assert_eq!(put(&e, &vote(2), "bob").await, StatusCode::NO_CONTENT);
    let _ = next_event(&mut ev).await;

    let (_, owner_view) = e.get(&format!("/messages/{id}"), "owner").await;
    assert_eq!(counts(&owner_view), vec![0, 2, 0]);
    assert_eq!(owner_view["poll"]["results"]["answer_counts"][1]["me_voted"], true);
    assert_eq!(owner_view["poll"]["results"]["answer_counts"][0]["me_voted"], false);

    let (s, voters) = e.get(&format!("/polls/{id}/answers/2"), "bob").await;
    assert_eq!(s, StatusCode::OK);
    let ids: Vec<&str> = voters["users"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["1", "2"]);
    let (_, after) = e.get(&format!("/polls/{id}/answers/2?after=1&limit=1"), "bob").await;
    assert_eq!(after["users"].as_array().unwrap().len(), 1);
    assert_eq!(after["users"][0]["id"], "2");

    // Withdraw.
    assert_eq!(
        e.status(Method::DELETE, &vote(2), "bob").await,
        StatusCode::NO_CONTENT
    );
    assert!(matches!(
        next_event(&mut ev).await,
        DomainEvent::PollVoteRemoved { answer_id: 2, .. }
    ));
    assert_eq!(
        e.status(Method::DELETE, &vote(2), "bob").await,
        StatusCode::NO_CONTENT
    );
    no_event(&mut ev).await;

    // Bad requests.
    let r = e.call(Method::PUT, &vote(9), "bob", None).await;
    assert_eq!((r.0, r.2["code"].as_u64()), (StatusCode::BAD_REQUEST, Some(50035)));
    assert_eq!(put(&e, &format!("/polls/{id}/answers/x/voters/@me"), "bob").await, StatusCode::BAD_REQUEST);
    let plain = id_of(&e.say("bob", "no poll").await);
    let _ = next_event(&mut ev).await;
    assert_eq!(
        put(&e, &format!("/polls/{plain}/answers/1/voters/@me"), "bob").await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(put(&e, &vote(1), "carol").await, StatusCode::FORBIDDEN);
    // Only the author ends a poll early.
    assert_eq!(
        e.post(&format!("/polls/{id}/expire"), "owner", json!({})).await.0,
        StatusCode::FORBIDDEN
    );

    // Expire: results are final, votes refused, MESSAGE_UPDATE announced.
    let (s, done) = e.post(&format!("/polls/{id}/expire"), "bob", json!({})).await;
    assert_eq!(s, StatusCode::OK, "{done}");
    assert_eq!(done["poll"]["results"]["is_finalized"], true);
    assert_eq!(counts(&done), vec![0, 1, 0]);
    match next_event(&mut ev).await {
        DomainEvent::MessageEdited { data, .. } => {
            assert_eq!(data["poll"]["results"]["is_finalized"], true);
        }
        other => panic!("{other:?}"),
    }
    let r = e.call(Method::PUT, &vote(3), "bob", None).await;
    assert_eq!((r.0, r.2["errors"]["poll"].is_object()), (StatusCode::BAD_REQUEST, true));
    assert_eq!(
        e.post(&format!("/polls/{id}/expire"), "bob", json!({})).await.0,
        StatusCode::BAD_REQUEST
    );
    // The scheduled job finds nothing left to do.
    assert!(!rich_jobs::finalize_poll(&e.deps, e.ch, id.parse().unwrap()).await.unwrap());
}

#[tokio::test]
async fn multiselect_poll_expires_by_clock_and_the_sweep_finalises_it() {
    let e = env().await;
    let m = e.post("/messages", "bob", poll_body(1, true)).await.1;
    let id = id_of(&m);
    let vote = |ans: u32| format!("/polls/{id}/answers/{ans}/voters/@me");
    assert_eq!(put(&e, &vote(1), "owner").await, StatusCode::NO_CONTENT);
    assert_eq!(put(&e, &vote(3), "owner").await, StatusCode::NO_CONTENT);
    let (_, v) = e.get(&format!("/messages/{id}"), "owner").await;
    assert_eq!(counts(&v), vec![1, 0, 1]);

    assert_eq!(rich_jobs::sweep_polls(&e.deps).await.unwrap(), 0);
    e.clock.advance_ms(3_600_000);
    // Expired but not finalised yet: votes are already refused.
    assert_eq!(put(&e, &vote(2), "bob").await, StatusCode::BAD_REQUEST);
    let mut ev = e.events();
    assert_eq!(rich_jobs::sweep_polls(&e.deps).await.unwrap(), 1);
    assert_eq!(rich_jobs::sweep_polls(&e.deps).await.unwrap(), 0);
    match next_event(&mut ev).await {
        DomainEvent::MessageEdited { data, .. } => {
            assert_eq!(data["poll"]["results"]["is_finalized"], true);
            assert_eq!(data["poll"]["results"]["answer_counts"][0]["count"], 1);
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn invalid_polls_and_poll_permission() {
    let e = env().await;
    for body in [
        json!({"poll": {"question": {"text": "q"}, "answers": [{"poll_media": {"text": "a"}}]}}),
        poll_body(0, false),
        poll_body(169, false),
        json!({"poll": "yes"}),
    ] {
        let r = e.post("/messages", "bob", body).await;
        assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50035)), "{}", r.1);
    }
    e.revoke_everyone(Permissions::SEND_POLLS);
    let r = e.post("/messages", "bob", poll_body(24, false)).await;
    assert_eq!(r.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn stickers_are_validated_and_rendered() {
    let e = env().await;
    {
        let mut d = e.backend.data();
        let mk = |id, guild, name: &str, available| StickerRecord {
            id,
            guild_id: guild,
            name: name.into(),
            format_type: 1,
            available,
        };
        d.sticker_rows.insert(10, mk(10, Some(e.guild), "wave", true));
        d.sticker_rows.insert(11, mk(11, None, "standard", true));
        d.sticker_rows.insert(12, mk(12, Some(GuildId::new(999)), "foreign", true));
        d.sticker_rows.insert(13, mk(13, Some(e.guild), "off", false));
    }
    let r = e
        .post("/messages", "bob", json!({"sticker_ids": ["10", "11"]}))
        .await;
    assert_eq!(r.0, StatusCode::OK, "{}", r.1);
    assert_eq!(r.1["sticker_items"][0]["name"], "wave");
    assert_eq!(r.1["sticker_items"][1]["id"], "11");
    let (_, again) = e.get(&format!("/messages/{}", id_of(&r.1)), "owner").await;
    assert_eq!(again["sticker_items"].as_array().unwrap().len(), 2);
    for ids in [json!(["12"]), json!(["13"]), json!(["999"]), json!(["10", "10"]), json!(["10", "11", "10", "11"]), json!(["x"])] {
        let r = e.post("/messages", "bob", json!({"sticker_ids": ids})).await;
        assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50035)), "{ids}");
    }
}

async fn upload(e: &Env, who: &str, name: &str, bytes: Vec<u8>) -> Value {
    let (_, v) = e
        .post(
            "/attachments",
            who,
            json!({"files": [{"filename": name, "file_size": bytes.len()}]}),
        )
        .await;
    let slot = v["attachments"][0].clone();
    e.storage
        .put(&slot_key(e.ch, &slot, name), Bytes::from(bytes))
        .await
        .unwrap();
    slot
}

fn ogg() -> Vec<u8> {
    let mut v = b"OggS\0\x02".to_vec();
    v.extend_from_slice(&[0u8; 64]);
    v
}

#[tokio::test]
async fn voice_messages_need_ogg_duration_and_waveform() {
    let e = env().await;
    let wave = data_encoding::BASE64.encode(&[3u8, 9, 27, 81]);
    let slot = upload(&e, "bob", "voice-message.ogg", ogg()).await;
    let att = |dur: Value, w: Value| {
        json!([{
            "id": slot["upload_id"], "uploaded_filename": slot["upload_filename"],
            "duration_secs": dur, "waveform": w,
        }])
    };
    let send = |attachments: Value, content: &str| {
        json!({"flags": 1 << 13, "attachments": attachments, "content": content})
    };
    // Too long, no waveform, bad waveform, and text alongside are all refused.
    for body in [
        send(att(json!(121.0), json!(wave)), ""),
        send(att(json!(5.0), Value::Null), ""),
        send(att(json!(5.0), json!("***")), ""),
        send(att(Value::Null, json!(wave)), ""),
        send(att(json!(5.0), json!(wave)), "hello"),
    ] {
        let r = e.post("/messages", "bob", body).await;
        assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50035)), "{}", r.1);
    }
    let (s, m) = e
        .post("/messages", "bob", send(att(json!(4.5), json!(wave)), ""))
        .await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert_eq!(m["flags"], 1 << 13);
    assert_eq!(m["attachments"][0]["content_type"], "audio/ogg");
    assert_eq!(m["attachments"][0]["duration_secs"], 4.5);
    assert_eq!(m["attachments"][0]["waveform"], wave.as_str());
    let (_, got) = e.get(&format!("/messages/{}", id_of(&m)), "owner").await;
    assert_eq!(got["attachments"][0]["waveform"], wave.as_str());

    // Not ogg: a PNG flagged as a voice message.
    let png = upload(&e, "bob", "x.png", png_with_secret()).await;
    let r = e
        .post(
            "/messages",
            "bob",
            json!({"flags": 1 << 13, "attachments": [{
                "id": png["upload_id"], "uploaded_filename": png["upload_filename"],
                "duration_secs": 2.0, "waveform": wave,
            }]}),
        )
        .await;
    assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50035)), "{}", r.1);
    // A voice message cannot carry embeds.
    let slot2 = upload(&e, "bob", "v2.ogg", ogg()).await;
    let r = e
        .post(
            "/messages",
            "bob",
            json!({"flags": 1 << 13, "embeds": [{"title": "t"}], "attachments": [{
                "id": slot2["upload_id"], "uploaded_filename": slot2["upload_filename"],
                "duration_secs": 2.0, "waveform": wave,
            }]}),
        )
        .await;
    assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50035)), "{}", r.1);
}

#[tokio::test]
async fn embeds_and_allowed_mentions_are_validated() {
    let e = env().await;
    let embeds = |n: usize| json!({"embeds": vec![json!({"title": "t"}); n]});
    assert_eq!(e.post("/messages", "bob", embeds(10)).await.0, StatusCode::OK);
    let r = e.post("/messages", "bob", embeds(11)).await;
    assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50035)));
    let big = json!({"embeds": [
        {"description": "x".repeat(4000)}, {"description": "y".repeat(2001)}
    ]});
    let r = e.post("/messages", "bob", big).await;
    assert_eq!(r.1["errors"]["embeds"]["_errors"][0]["code"], "EMBED_SIZE_EXCEEDS_MAXIMUM");
    let r = e
        .post("/messages", "bob", json!({"embeds": [{"title": "x".repeat(257)}]}))
        .await;
    assert!(r.1["errors"]["embeds"]["0"]["title"].is_object(), "{}", r.1);
    let r = e
        .post("/messages", "bob", json!({"embeds": [{"title": "t", "url": "javascript:alert(1)"}]}))
        .await;
    assert_eq!(r.0, StatusCode::BAD_REQUEST);
    let fields = vec![json!({"name": "n", "value": "v"}); 26];
    let r = e
        .post("/messages", "bob", json!({"embeds": [{"fields": fields}]}))
        .await;
    assert_eq!(r.0, StatusCode::BAD_REQUEST);
    // Types and providers are server-only.
    let (_, m) = e
        .post(
            "/messages",
            "bob",
            json!({"embeds": [{"title": "t", "type": "link", "provider": {"name": "x"}}]}),
        )
        .await;
    assert_eq!(m["embeds"][0]["type"], "rich");
    assert!(m["embeds"][0].get("provider").is_none());

    let r = e
        .post(
            "/messages",
            "bob",
            json!({"content": "x", "allowed_mentions": {"parse": ["users"], "users": ["1"]}}),
        )
        .await;
    assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50035)));
    let r = e
        .post(
            "/messages",
            "bob",
            json!({"content": "x", "allowed_mentions": {"users": ["1", "1"]}}),
        )
        .await;
    assert_eq!(code(&r), (StatusCode::BAD_REQUEST, Some(50035)));
}

#[tokio::test]
async fn forwards_copy_the_source_and_survive_edits() {
    let e = env().await;
    let src = e.say("owner", "original words").await;
    let sid = id_of(&src);
    let (s, fwd) = e
        .post(
            "/messages",
            "bob",
            json!({"message_reference": {
                "type": 1, "message_id": sid, "channel_id": e.ch.to_string()
            }}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{fwd}");
    assert_eq!(fwd["message_reference"]["type"], 1);
    assert_eq!(fwd["message_reference"]["message_id"], sid.as_str());
    assert_eq!(fwd["type"], 0);
    assert_eq!(fwd["content"], "");
    assert_eq!(fwd["message_snapshots"][0]["message"]["content"], "original words");
    assert!(fwd["message_snapshots"][0]["message"]["timestamp"].is_string());

    // Editing or deleting the source does not change the forward.
    let (s, _) = e
        .patch(&format!("/messages/{sid}"), "owner", json!({"content": "changed"}))
        .await;
    assert_eq!(s, StatusCode::OK);
    e.status(Method::DELETE, &format!("/messages/{sid}"), "owner").await;
    let (_, again) = e.get(&format!("/messages/{}", id_of(&fwd)), "bob").await;
    assert_eq!(again["message_snapshots"][0]["message"]["content"], "original words");

    // Forwarding the forward keeps the original copy; a deleted source is gone.
    let (s, f2) = e
        .post(
            "/messages",
            "bob",
            json!({"message_reference": {
                "type": 1, "message_id": id_of(&fwd), "channel_id": e.ch.to_string()
            }}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{f2}");
    assert_eq!(f2["message_snapshots"][0]["message"]["content"], "original words");
    let gone = e
        .post(
            "/messages",
            "bob",
            json!({"message_reference": {"type": 1, "message_id": sid, "channel_id": e.ch.to_string()}}),
        )
        .await;
    assert_eq!(gone.0, StatusCode::NOT_FOUND);

    // A forward carries nothing else, needs the source channel, and the sender must read it.
    let keep = id_of(&fwd);
    let with_text = e
        .post(
            "/messages",
            "bob",
            json!({"content": "hi", "message_reference": {
                "type": 1, "message_id": keep, "channel_id": e.ch.to_string()}}),
        )
        .await;
    assert_eq!(code(&with_text), (StatusCode::BAD_REQUEST, Some(50035)));
    let no_channel = e
        .post(
            "/messages",
            "bob",
            json!({"message_reference": {"type": 1, "message_id": keep}}),
        )
        .await;
    assert_eq!(code(&no_channel), (StatusCode::BAD_REQUEST, Some(50035)));
    e.revoke_everyone(Permissions::READ_MESSAGE_HISTORY);
    let denied = e
        .post(
            "/messages",
            "bob",
            json!({"message_reference": {"type": 1, "message_id": keep, "channel_id": e.ch.to_string()}}),
        )
        .await;
    assert_eq!(denied.0, StatusCode::FORBIDDEN);
}

fn article(title: &str) -> String {
    format!(
        r#"<html><head><meta property="og:title" content="{title}">
        <meta property="og:description" content="About {title}">
        <meta property="og:site_name" content="News"></head><body>x</body></html>"#
    )
}

#[tokio::test]
async fn links_are_previewed_by_a_job_and_announced() {
    let e = env().await;
    e.web.page("https://news.test/a", &article("Alpha"));
    e.web.page("https://news.test/b", &article("Beta"));
    let mut ev = e.events();
    let m = e
        .say("bob", "read https://news.test/a and <https://news.test/b>")
        .await;
    let id = id_of(&m);
    let _ = next_event(&mut ev).await; // MESSAGE_CREATE
    assert!(m["embeds"].as_array().unwrap().is_empty());
    let jobs = e.jobs.of_kind("messages.unfurl");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].0["message_id"].as_u64().unwrap().to_string(), id);

    let out = rich_jobs::run_unfurl(&e.deps, e.ch, id.parse().unwrap()).await.unwrap();
    assert_eq!(out, UnfurlOutcome::Updated);
    match next_event(&mut ev).await {
        DomainEvent::MessageEdited { data, .. } => {
            let embeds = data["embeds"].as_array().unwrap();
            // The <suppressed> link got no embed.
            assert_eq!(embeds.len(), 1);
            assert_eq!(embeds[0]["type"], "link");
            assert_eq!(embeds[0]["title"], "Alpha");
            assert_eq!(embeds[0]["provider"]["name"], "News");
            assert_eq!(embeds[0]["url"], "https://news.test/a");
        }
        other => panic!("{other:?}"),
    }
    let (_, got) = e.get(&format!("/messages/{id}"), "owner").await;
    assert_eq!(got["embeds"][0]["title"], "Alpha");
    assert!(got["edited_timestamp"].is_null());
    // A second pass changes nothing.
    assert_eq!(
        rich_jobs::run_unfurl(&e.deps, e.ch, id.parse().unwrap()).await.unwrap(),
        UnfurlOutcome::Unchanged
    );
    no_event(&mut ev).await;
}

#[tokio::test]
async fn previews_keep_bot_embeds_and_honour_suppression() {
    let e = env().await;
    e.web.page("https://news.test/a", &article("Alpha"));
    let (_, m) = e
        .post(
            "/messages",
            "bob",
            json!({"content": "https://news.test/a", "embeds": [{"title": "mine"}]}),
        )
        .await;
    let id = id_of(&m);
    let mid: MessageId = id.parse().unwrap();
    assert_eq!(rich_jobs::run_unfurl(&e.deps, e.ch, mid).await.unwrap(), UnfurlOutcome::Updated);
    let (_, got) = e.get(&format!("/messages/{id}"), "bob").await;
    assert_eq!(got["embeds"].as_array().unwrap().len(), 2);
    assert_eq!(got["embeds"][0]["title"], "mine");
    assert_eq!(got["embeds"][1]["title"], "Alpha");

    // Suppressing embeds removes the previews but keeps the author's own embed.
    let before = e.jobs.of_kind("messages.unfurl").len();
    let (s, _) = e.patch(&format!("/messages/{id}"), "bob", json!({"flags": 4})).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(e.jobs.of_kind("messages.unfurl").len(), before + 1);
    assert_eq!(rich_jobs::run_unfurl(&e.deps, e.ch, mid).await.unwrap(), UnfurlOutcome::Updated);
    let (_, got) = e.get(&format!("/messages/{id}"), "bob").await;
    assert_eq!(got["embeds"].as_array().unwrap().len(), 1);
    assert_eq!(got["embeds"][0]["title"], "mine");

    // SUPPRESS_EMBEDS on create queues nothing.
    let before = e.jobs.of_kind("messages.unfurl").len();
    e.post("/messages", "bob", json!({"content": "https://news.test/a", "flags": 4})).await;
    assert_eq!(e.jobs.of_kind("messages.unfurl").len(), before);
    // A dead link produces no embed and no error.
    let (_, dead) = e.post("/messages", "bob", json!({"content": "https://gone.test/x"})).await;
    assert_eq!(
        rich_jobs::run_unfurl(&e.deps, e.ch, id_of(&dead).parse().unwrap()).await.unwrap(),
        UnfurlOutcome::Unchanged
    );
}
