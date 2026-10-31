//! Postgres tests of the inbound half (BRG-GW-001): the dispatch handler (loop guard, replies, edits, deletes,
//! reactions, the abuse limit, attachment links) and the shard manager against an in-memory gateway.
//!
//! Run with a scratch Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-bridge --test inbound -- --ignored --test-threads=1`

#[path = "support/mod.rs"]
mod support;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use sr_bridge::BridgeResult;
use sr_bridge::inbound::{
    ChatSink, Inbound, InboundConfig, InboundHost, InboundManager, InboundPost, Outcome, Why,
    convert::InLink,
};
use sr_bridge::model::{Bridge, get_bridge};
use sr_sdk::gateway::{Dispatch, GatewayConnection, GatewayConnector, Incoming};
use support::*;
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------------------------- the chat side

#[derive(Default)]
struct Log {
    posts: Vec<(i64, InboundPost)>,
    edits: Vec<(i64, Option<String>, Option<Vec<InLink>>)>,
    deletes: Vec<i64>,
    reactions: Vec<(i64, String, bool)>,
    typing: Vec<(i64, i64)>,
}

#[derive(Default)]
struct MemSink {
    log: Mutex<Log>,
    next: Mutex<i64>,
}

#[async_trait]
impl ChatSink for MemSink {
    async fn post(&self, post: InboundPost) -> BridgeResult<i64> {
        let id = {
            let mut n = self.next.lock().unwrap();
            *n += 1;
            9_000_000 + *n
        };
        self.log.lock().unwrap().posts.push((id, post));
        Ok(id)
    }
    async fn edit(
        &self,
        _channel_id: i64,
        message_id: i64,
        content: Option<String>,
        links: Option<Vec<InLink>>,
    ) -> BridgeResult<()> {
        self.log
            .lock()
            .unwrap()
            .edits
            .push((message_id, content, links));
        Ok(())
    }
    async fn delete(&self, _channel_id: i64, message_id: i64) -> BridgeResult<()> {
        self.log.lock().unwrap().deletes.push(message_id);
        Ok(())
    }
    async fn typing(&self, channel_id: i64, user: i64) -> BridgeResult<()> {
        self.log.lock().unwrap().typing.push((channel_id, user));
        Ok(())
    }
    async fn react(
        &self,
        _channel_id: i64,
        message_id: i64,
        emoji: &str,
        add: bool,
    ) -> BridgeResult<()> {
        self.log
            .lock()
            .unwrap()
            .reactions
            .push((message_id, emoji.to_owned(), add));
        Ok(())
    }
}

impl MemSink {
    fn posts(&self) -> Vec<(i64, InboundPost)> {
        self.log.lock().unwrap().posts.clone()
    }
}

// ---------------------------------------------------------------------------------------------- fixtures

struct Fx {
    db: TestDb,
    guild: i64,
    owner: i64,
    sr_channel: i64,
    map_id: i64,
    /// The Discord channel of the map.
    dch: i64,
    webhook: i64,
    sink: Arc<MemSink>,
    inbound: Arc<Inbound>,
}

const BOT: i64 = 5_000;

async fn bridge_for(
    pool: &PgPool,
    owner: i64,
    guild: i64,
    sr_channel: i64,
    dir: &str,
) -> (i64, i64, i64) {
    sqlx::query(
        "INSERT INTO bridges (guild_id, discord_guild_id, bot_user_id, bot_token_enc, status, created_by) \
         VALUES ($1, $2, $3, '\\x00'::bytea, 'connecting', $4)",
    )
    .bind(guild)
    .bind(uid())
    .bind(BOT)
    .bind(owner)
    .execute(pool)
    .await
    .unwrap();
    let (map, dch, webhook) = (uid(), uid(), uid());
    sqlx::query(
        "INSERT INTO bridge_channel_maps (id, guild_id, channel_id, discord_channel_id, discord_webhook_id, \
         webhook_token_enc, direction, created_by) VALUES ($1, $2, $3, $4, $5, '\\x00'::bytea, $6, $7)",
    )
    .bind(map)
    .bind(guild)
    .bind(sr_channel)
    .bind(dch)
    .bind(webhook)
    .bind(dir)
    .bind(owner)
    .execute(pool)
    .await
    .unwrap();
    (map, dch, webhook)
}

async fn fx_with(dir: &str, per_minute: u32) -> Fx {
    let db = TestDb::new().await.unwrap();
    let owner = account(db.pool()).await;
    let (guild, sr_channel) = guild(db.pool(), owner).await;
    let (map_id, dch, webhook) = bridge_for(db.pool(), owner, guild, sr_channel, dir).await;
    let sink = Arc::new(MemSink::default());
    let mut cfg = InboundConfig::defaults();
    cfg.messages_per_minute = Arc::new(move || per_minute);
    let inbound = Arc::new(Inbound::new(db.pool().clone(), sink.clone(), cfg));
    Fx {
        db,
        guild,
        owner,
        sr_channel,
        map_id,
        dch,
        webhook,
        sink,
        inbound,
    }
}

async fn fx() -> Fx {
    fx_with("both", 60).await
}

impl Fx {
    async fn bridge(&self) -> Bridge {
        get_bridge(self.db.pool(), self.guild)
            .await
            .unwrap()
            .unwrap()
    }

    async fn send(&self, name: &str, data: Value) -> Outcome {
        let b = self.bridge().await;
        self.inbound
            .handle(
                &b,
                &Dispatch {
                    shard: 0,
                    name: name.to_owned(),
                    data,
                },
            )
            .await
            .unwrap()
    }

    fn msg(&self, id: i64, author: i64, content: &str) -> Value {
        json!({"id": id.to_string(), "channel_id": self.dch.to_string(), "type": 0,
               "content": content,
               "author": {"id": author.to_string(), "username": "dana", "global_name": "Dana", "avatar": "av1"}})
    }
}

// ---------------------------------------------------------------------------------------------- handler tests

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_discord_message_is_shown_by_the_bridge_author_with_the_discord_user_and_links() {
    let f = fx().await;
    let mut m = f.msg(1001, 77, "hello <@88> @everyone <:pog:5>");
    m["mentions"] = json!([{"id": "88", "username": "eve", "global_name": "Eve"}]);
    m["attachments"] = json!([
        {"filename": "cat.png", "url": "https://cdn.discordapp.com/attachments/1/2/cat.png?ex=65f00000&is=1&hm=2"}
    ]);
    let Outcome::Posted(sr) = f.send("MESSAGE_CREATE", m).await else {
        panic!("posted");
    };
    let posts = f.sink.posts();
    assert_eq!(posts.len(), 1);
    let (id, p) = &posts[0];
    assert_eq!(*id, sr);
    assert_eq!((p.guild_id, p.channel_id), (f.guild, f.sr_channel));
    assert_eq!(
        p.as_user, f.owner,
        "stored under the account that connected the bot"
    );
    assert_eq!(p.author.name, "Dana");
    assert_eq!(p.author.discord_user_id, 77);
    assert_eq!(p.author.platform, "discord");
    assert!(p.content.contains("@\u{200b}Eve") && p.content.contains("@\u{200b}everyone"));
    assert!(!p.content.contains("<@") && p.content.contains(":pog:"));
    assert_eq!(p.author.links.len(), 1);
    assert_eq!(p.author.links[0].expires_at_ms, 0x65f0_0000_i64 * 1000);
    // The pair is remembered, so edits and replies find it.
    let dm: i64 = sqlx::query_scalar(
        "SELECT discord_message_id FROM bridge_message_map WHERE sr_message_id = $1 AND direction = 'in'",
    )
    .bind(sr)
    .fetch_one(f.db.pool())
    .await
    .unwrap();
    assert_eq!(dm, 1001);
    let _ = f.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn what_the_bridge_wrote_never_comes_back_as_a_new_message() {
    let f = fx().await;
    // The bridge's own bot.
    assert_eq!(
        f.send("MESSAGE_CREATE", f.msg(1, BOT, "x")).await,
        Outcome::Ignored(Why::OwnBot)
    );
    // A post through one of the bridge's own webhooks (our message echoed back).
    let mut echo = f.msg(2, 4242, "mirrored");
    echo["webhook_id"] = json!(f.webhook.to_string());
    assert_eq!(
        f.send("MESSAGE_CREATE", echo).await,
        Outcome::Ignored(Why::OwnWebhook)
    );
    // A message of ours that is already in the message map (direction out).
    sqlx::query("INSERT INTO bridge_message_map (sr_message_id, discord_message_id, map_id, direction) VALUES (77, 3, $1, 'out')")
        .bind(f.map_id)
        .execute(f.db.pool())
        .await
        .unwrap();
    assert_eq!(
        f.send("MESSAGE_CREATE", f.msg(3, 9, "known")).await,
        Outcome::Ignored(Why::Known)
    );
    // A foreign webhook (another integration) is a normal message.
    let mut other = f.msg(4, 4243, "from another webhook");
    other["webhook_id"] = json!(uid().to_string());
    assert!(matches!(
        f.send("MESSAGE_CREATE", other).await,
        Outcome::Posted(_)
    ));
    // A repeated dispatch is not shown twice.
    assert_eq!(
        f.send("MESSAGE_CREATE", f.msg(4, 4243, "from another webhook"))
            .await,
        Outcome::Ignored(Why::Known)
    );
    assert_eq!(f.sink.posts().len(), 1);
    let _ = f.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn only_mapped_channels_that_bring_messages_in_are_shown() {
    let f = fx_with("to_discord", 60).await;
    assert_eq!(
        f.send("MESSAGE_CREATE", f.msg(1, 9, "x")).await,
        Outcome::Ignored(Why::Unmapped),
        "a to_discord map does not receive"
    );
    let mut elsewhere = f.msg(2, 9, "x");
    elsewhere["channel_id"] = json!(uid().to_string());
    assert_eq!(
        f.send("MESSAGE_CREATE", elsewhere).await,
        Outcome::Ignored(Why::Unmapped)
    );
    assert!(f.sink.posts().is_empty());
    let _ = f.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_reply_is_resolved_through_the_message_map_in_both_directions() {
    let f = fx().await;
    let Outcome::Posted(first) = f.send("MESSAGE_CREATE", f.msg(10, 9, "first")).await else {
        panic!()
    };
    let mut reply = f.msg(11, 8, "second");
    reply["type"] = json!(19);
    reply["message_reference"] = json!({"message_id": "10", "channel_id": f.dch.to_string()});
    let Outcome::Posted(_) = f.send("MESSAGE_CREATE", reply).await else {
        panic!()
    };
    // A reply to a message we mirrored out (the webhook's Discord id is in the map).
    sqlx::query("INSERT INTO bridge_message_map (sr_message_id, discord_message_id, map_id, webhook_id, direction) VALUES (555, 12, $1, $2, 'out')")
        .bind(f.map_id)
        .bind(f.webhook)
        .execute(f.db.pool())
        .await
        .unwrap();
    let mut to_ours = f.msg(13, 8, "third");
    to_ours["type"] = json!(19);
    to_ours["message_reference"] = json!({"message_id": "12"});
    f.send("MESSAGE_CREATE", to_ours).await;
    // A reply to something the bridge never saw is just a message.
    let mut lost = f.msg(14, 8, "fourth");
    lost["type"] = json!(19);
    lost["message_reference"] = json!({"message_id": "999999"});
    f.send("MESSAGE_CREATE", lost).await;
    let replies: Vec<_> = f
        .sink
        .posts()
        .into_iter()
        .map(|(_, p)| p.reply_to)
        .collect();
    assert_eq!(replies, [None, Some(first), Some(555), None]);
    let _ = f.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn edits_and_deletes_follow_messages_that_came_in_and_leave_ours_alone() {
    let f = fx().await;
    let Outcome::Posted(sr) = f.send("MESSAGE_CREATE", f.msg(20, 9, "v1")).await else {
        panic!()
    };
    // An edit.
    let mut upd = f.msg(20, 9, "v2");
    upd.as_object_mut().unwrap().remove("author");
    assert_eq!(f.send("MESSAGE_UPDATE", upd).await, Outcome::Edited(sr));
    // A partial update (an embed resolved) changes nothing.
    assert_eq!(
        f.send(
            "MESSAGE_UPDATE",
            json!({"id": "20", "channel_id": f.dch.to_string(), "embeds": []})
        )
        .await,
        Outcome::Ignored(Why::NothingToChange)
    );
    assert_eq!(
        f.sink.log.lock().unwrap().edits,
        [(sr, Some("v2".to_owned()), None)]
    );
    // One of ours edited or deleted on Discord is not touched here.
    sqlx::query("INSERT INTO bridge_message_map (sr_message_id, discord_message_id, map_id, direction) VALUES (600, 21, $1, 'out')")
        .bind(f.map_id)
        .execute(f.db.pool())
        .await
        .unwrap();
    assert_eq!(
        f.send("MESSAGE_UPDATE", f.msg(21, 9, "changed")).await,
        Outcome::Ignored(Why::NotBridged)
    );
    assert_eq!(
        f.send(
            "MESSAGE_DELETE",
            json!({"id": "21", "channel_id": f.dch.to_string()})
        )
        .await,
        Outcome::Ignored(Why::NotBridged)
    );
    // A delete (single and bulk) removes the message and its map row.
    let Outcome::Posted(sr2) = f.send("MESSAGE_CREATE", f.msg(22, 9, "two")).await else {
        panic!()
    };
    assert_eq!(
        f.send(
            "MESSAGE_DELETE_BULK",
            json!({"ids": ["20", "22"], "channel_id": f.dch.to_string()})
        )
        .await,
        Outcome::Deleted(2)
    );
    assert_eq!(f.sink.log.lock().unwrap().deletes, [sr, sr2]);
    let left: i64 =
        sqlx::query_scalar("SELECT count(*) FROM bridge_message_map WHERE direction = 'in'")
            .fetch_one(f.db.pool())
            .await
            .unwrap();
    assert_eq!(left, 0);
    let _ = f.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_reaction_is_added_with_the_first_discord_user_and_removed_with_the_last() {
    let f = fx().await;
    let Outcome::Posted(sr) = f.send("MESSAGE_CREATE", f.msg(30, 9, "react to me")).await else {
        panic!()
    };
    let react = |name: &str, user: i64, emoji: Value| {
        let d = json!({"channel_id": f.dch.to_string(), "message_id": "30", "user_id": user.to_string(), "emoji": emoji});
        (name.to_owned(), d)
    };
    let thumbs = json!({"id": null, "name": "\u{1F44D}"});
    for (n, d) in [
        react("MESSAGE_REACTION_ADD", 1, thumbs.clone()),
        react("MESSAGE_REACTION_ADD", 2, thumbs.clone()),
        react("MESSAGE_REACTION_ADD", 2, thumbs.clone()), // repeated
        react("MESSAGE_REACTION_REMOVE", 1, thumbs.clone()),
    ] {
        f.send(&n, d).await;
    }
    assert_eq!(
        f.sink.log.lock().unwrap().reactions,
        [(sr, "\u{1F44D}".to_owned(), true)],
        "added once; user 2 is still there"
    );
    let (n, d) = react("MESSAGE_REACTION_REMOVE", 2, thumbs);
    f.send(&n, d).await;
    assert_eq!(
        f.sink.log.lock().unwrap().reactions.last().unwrap(),
        &(sr, "\u{1F44D}".to_owned(), false)
    );
    // A custom emoji is not mirrored.
    let (n, d) = react(
        "MESSAGE_REACTION_ADD",
        3,
        json!({"id": "99", "name": "pog"}),
    );
    assert_eq!(f.send(&n, d).await, Outcome::Ignored(Why::CustomEmoji));
    // The bridge's own bot reacting is not echoed.
    let (n, d) = react(
        "MESSAGE_REACTION_ADD",
        BOT,
        json!({"id": null, "name": "\u{2705}"}),
    );
    assert_eq!(f.send(&n, d).await, Outcome::Ignored(Why::OwnBot));
    assert_eq!(f.sink.log.lock().unwrap().reactions.len(), 2);
    let _ = f.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_bridge_over_its_message_figure_drops_the_rest() {
    let f = fx_with("both", 2).await;
    let mut out = Vec::new();
    for i in 0..4 {
        out.push(f.send("MESSAGE_CREATE", f.msg(100 + i, 9, "spam")).await);
    }
    assert!(matches!(out[0], Outcome::Posted(_)) && matches!(out[1], Outcome::Posted(_)));
    assert_eq!(out[2], Outcome::Ignored(Why::RateLimited));
    assert_eq!(out[3], Outcome::Ignored(Why::RateLimited));
    assert_eq!(f.sink.posts().len(), 2);
    assert_eq!(f.inbound.dropped(f.guild), 2);
    let _ = f.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_teardown_deleting_the_maps_takes_the_reaction_and_message_rows_with_it() {
    let f = fx().await;
    f.send("MESSAGE_CREATE", f.msg(40, 9, "x")).await;
    f.send(
        "MESSAGE_REACTION_ADD",
        json!({"channel_id": f.dch.to_string(), "message_id": "40", "user_id": "1", "emoji": {"id": null, "name": "\u{1F44D}"}}),
    )
    .await;
    sqlx::query("DELETE FROM bridge_channel_maps WHERE guild_id = $1")
        .bind(f.guild)
        .execute(f.db.pool())
        .await
        .unwrap();
    for t in ["bridge_message_map", "bridge_reactions"] {
        let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {t}")))
            .fetch_one(f.db.pool())
            .await
            .unwrap();
        assert_eq!(n, 0, "{t}");
    }
    let _ = f.db.cleanup().await;
}

// ---------------------------------------------------------------------------------------------- the manager

struct Conn {
    rx: mpsc::UnboundedReceiver<Incoming>,
    sent: Arc<Mutex<Vec<Value>>>,
}

#[async_trait]
impl GatewayConnection for Conn {
    async fn recv(&mut self) -> sr_sdk::Result<Incoming> {
        match self.rx.recv().await {
            Some(i) => Ok(i),
            None => std::future::pending().await,
        }
    }
    async fn send(&mut self, text: String) -> sr_sdk::Result<()> {
        if let Ok(v) = serde_json::from_str(&text) {
            self.sent.lock().unwrap().push(v);
        }
        Ok(())
    }
}

#[derive(Default)]
struct FakeGateway {
    pending: Mutex<VecDeque<Conn>>,
    urls: Mutex<Vec<String>>,
}

impl FakeGateway {
    /// Queue one connection; the sender is the Discord side of it.
    fn queue(&self) -> mpsc::UnboundedSender<Incoming> {
        self.queue_logged().0
    }

    /// As [`Self::queue`], plus the frames the client sent on this connection.
    fn queue_logged(&self) -> (mpsc::UnboundedSender<Incoming>, Arc<Mutex<Vec<Value>>>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let sent = Arc::new(Mutex::new(Vec::new()));
        self.pending.lock().unwrap().push_back(Conn {
            rx,
            sent: sent.clone(),
        });
        (tx, sent)
    }
}

#[async_trait]
impl GatewayConnector for FakeGateway {
    async fn connect(&self, url: &str) -> sr_sdk::Result<Box<dyn GatewayConnection>> {
        self.urls.lock().unwrap().push(url.to_owned());
        match self.pending.lock().unwrap().pop_front() {
            Some(c) => Ok(Box::new(c)),
            None => Err(sr_sdk::Error::Invalid("no connection queued".into())),
        }
    }
}

fn frame(v: Value) -> Incoming {
    Incoming::Text(v.to_string())
}

fn hello() -> Incoming {
    frame(json!({"op": 10, "d": {"heartbeat_interval": 40000}}))
}

fn ready() -> Incoming {
    frame(json!({"op": 0, "s": 1, "t": "READY", "d": {
        "session_id": "s1", "resume_gateway_url": "wss://gateway-us-east1-b.discord.gg",
        "user": {"id": BOT.to_string(), "bot": true}, "application": {"id": "5", "flags": 0}, "guilds": []}}))
}

fn dispatch(seq: u64, name: &str, d: Value) -> Incoming {
    frame(json!({"op": 0, "s": seq, "t": name, "d": d}))
}

struct Host {
    off: Mutex<Vec<i64>>,
}

#[async_trait]
impl InboundHost for Host {
    async fn bot_token(&self, _guild: i64) -> BridgeResult<String> {
        Ok("fake-token".to_owned())
    }
    async fn active(&self, guild: i64) -> BridgeResult<bool> {
        Ok(!self.off.lock().unwrap().contains(&guild))
    }
}

async fn until<F, Fut>(what: &str, mut f: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..100 {
        if f().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for {what}");
}

async fn status(pool: &PgPool, guild: i64) -> String {
    sqlx::query_scalar("SELECT status FROM bridges WHERE guild_id = $1")
        .bind(guild)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn manager(f: &Fx, gw: Arc<FakeGateway>, host: Arc<Host>, max: usize) -> Arc<InboundManager> {
    let mut cfg = InboundConfig::defaults();
    cfg.max_bridges = Arc::new(move || max);
    cfg.gateway_url = Some("ws://127.0.0.1:8091/gateway".to_owned());
    let inbound = Arc::new(Inbound::new(f.db.pool().clone(), f.sink.clone(), cfg));
    InboundManager::new(f.db.pool().clone(), host, inbound, gw)
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_shard_runs_per_bridge_connects_and_brings_messages_in() {
    let f = fx().await;
    let gw = Arc::new(FakeGateway::default());
    let discord = gw.queue();
    let host = Arc::new(Host {
        off: Mutex::new(vec![]),
    });
    let m = manager(&f, gw.clone(), host.clone(), 10);
    let r = m.reconcile().await.unwrap();
    assert_eq!(r.started, [f.guild]);
    assert_eq!(m.running(), [f.guild]);
    // Reconciling again starts nothing new: one shard per bridge.
    assert!(m.reconcile().await.unwrap().started.is_empty());
    discord.send(hello()).unwrap();
    discord.send(ready()).unwrap();
    discord
        .send(dispatch(
            2,
            "MESSAGE_CREATE",
            f.msg(500, 9, "over the gateway"),
        ))
        .unwrap();
    until("the message", || async { f.sink.posts().len() == 1 }).await;
    until("connected", || async {
        status(f.db.pool(), f.guild).await == "connected"
    })
    .await;
    assert_eq!(f.sink.posts()[0].1.content, "over the gateway");
    assert_eq!(
        gw.urls.lock().unwrap()[0],
        "ws://127.0.0.1:8091/gateway",
        "the configured (fake) gateway is used"
    );
    // The module is switched off: the next pass stops the shard.
    host.off.lock().unwrap().push(f.guild);
    let r = m.reconcile().await.unwrap();
    assert_eq!(r.stopped, [f.guild]);
    assert!(m.running().is_empty());
    m.stop_all();
    let _ = f.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn at_most_max_bridges_shards_run() {
    let f = fx().await;
    // A second bridge on another server.
    let owner2 = account(f.db.pool()).await;
    let (g2, c2) = guild(f.db.pool(), owner2).await;
    bridge_for(f.db.pool(), owner2, g2, c2, "both").await;
    let gw = Arc::new(FakeGateway::default());
    let _a = gw.queue();
    let _b = gw.queue();
    let host = Arc::new(Host {
        off: Mutex::new(vec![]),
    });
    let m = manager(&f, gw, host, 1);
    let r = m.reconcile().await.unwrap();
    assert_eq!(r.started.len(), 1);
    assert_eq!(r.over_limit.len(), 1);
    assert_eq!(m.running().len(), 1, "the oldest bridge got the only slot");
    assert_eq!(m.running(), r.started);
    m.stop_all();
    let _ = f.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_refused_token_marks_the_bridge_and_a_refused_intent_is_not_retried_at_once() {
    // 4004: authentication failed.
    let f = fx().await;
    let gw = Arc::new(FakeGateway::default());
    let discord = gw.queue();
    let host = Arc::new(Host {
        off: Mutex::new(vec![]),
    });
    let m = manager(&f, gw.clone(), host.clone(), 10);
    m.reconcile().await.unwrap();
    discord.send(hello()).unwrap();
    discord.send(Incoming::Close(Some(4004))).unwrap();
    until("token_invalid", || async {
        status(f.db.pool(), f.guild).await == "token_invalid"
    })
    .await;
    // A bridge with a refused token is not started again.
    assert!(m.reconcile().await.unwrap().started.is_empty());
    assert!(m.running().is_empty());
    // 4014: disallowed intents (MESSAGE_CONTENT not enabled in the developer portal).
    sqlx::query("UPDATE bridges SET status = 'connecting' WHERE guild_id = $1")
        .bind(f.guild)
        .execute(f.db.pool())
        .await
        .unwrap();
    let discord = gw.queue();
    assert_eq!(m.reconcile().await.unwrap().started, [f.guild]);
    discord.send(hello()).unwrap();
    discord.send(Incoming::Close(Some(4014))).unwrap();
    until("disconnected", || async {
        status(f.db.pool(), f.guild).await == "disconnected"
    })
    .await;
    // The first refusal steps down to the plain intents (see the intent ladder test); a refusal there, with
    // nothing left to drop, is what leaves the bridge alone for a while.
    let discord = gw.queue();
    assert_eq!(m.reconcile().await.unwrap().started, [f.guild]);
    discord.send(hello()).unwrap();
    discord.send(Incoming::Close(Some(4014))).unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let again = m.reconcile().await.unwrap();
    assert!(
        again.started.is_empty(),
        "left alone for a while after a fatal close"
    );
    m.stop_all();
    let _ = f.db.cleanup().await;
}

// ---------------------------------------------------------------------------------------------- members

async fn members(pool: &PgPool, guild: i64) -> Vec<(i64, String, Option<String>, String)> {
    sqlx::query_as(
        "SELECT discord_user_id, display_name, role_name, presence FROM bridge_members \
         WHERE guild_id = $1 ORDER BY discord_user_id",
    )
    .bind(guild)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn discord_members_are_synced_into_the_bridged_member_list_and_are_not_accounts() {
    let f = fx().await;
    let dg = f.bridge().await.discord_guild_id;
    let g = dg.to_string();
    let user = |id: i64, name: &str, roles: Value| json!({"user": {"id": id.to_string(), "username": name}, "roles": roles});
    // Roles arrive with GUILD_CREATE.
    f.send(
        "GUILD_CREATE",
        json!({"id": g, "roles": [
            {"id": g, "name": "@everyone", "color": 0, "position": 0},
            {"id": "50", "name": "Mod", "color": 16711680, "position": 5}]}),
    )
    .await;
    // A two-chunk sync.
    let chunk = |i: i64, ms: Vec<Value>| json!({"guild_id": g, "chunk_index": i, "chunk_count": 2, "members": ms});
    f.send(
        "GUILD_MEMBERS_CHUNK",
        chunk(
            0,
            vec![user(1, "ann", json!(["50"])), user(2, "ben", json!([]))],
        ),
    )
    .await;
    f.send(
        "GUILD_MEMBERS_CHUNK",
        chunk(1, vec![user(3, "cy", json!([]))]),
    )
    .await;
    let got = members(f.db.pool(), f.guild).await;
    assert_eq!(got.len(), 3);
    assert_eq!(
        got[0],
        (1, "ann".into(), Some("Mod".into()), "offline".into())
    );
    assert_eq!(got[1].2, None, "no role, no role name");
    // A member who left while the bridge was down is dropped by the next full sync.
    f.send(
        "GUILD_MEMBERS_CHUNK",
        json!({"guild_id": g, "chunk_index": 0, "chunk_count": 1,
               "members": [user(1, "ann", json!(["50"]))]}),
    )
    .await;
    assert_eq!(members(f.db.pool(), f.guild).await.len(), 1);
    // Live events.
    f.send(
        "GUILD_MEMBER_ADD",
        json!({"guild_id": g, "nick": "Benny", "user": {"id": "2", "username": "ben"}, "roles": []}),
    )
    .await;
    f.send(
        "GUILD_MEMBER_UPDATE",
        json!({"guild_id": g, "user": {"id": "1", "username": "ann"}, "nick": "Annie", "roles": ["50"]}),
    )
    .await;
    f.send(
        "PRESENCE_UPDATE",
        json!({"guild_id": g, "user": {"id": "1"}, "status": "dnd"}),
    )
    .await;
    f.send(
        "GUILD_ROLE_UPDATE",
        json!({"guild_id": g, "role": {"id": "50", "name": "Admin", "color": 255, "position": 5}}),
    )
    .await;
    let got = members(f.db.pool(), f.guild).await;
    assert_eq!(
        got[0],
        (1, "Annie".into(), Some("Admin".into()), "dnd".into())
    );
    assert_eq!(got[1].1, "Benny");
    f.send(
        "GUILD_MEMBER_REMOVE",
        json!({"guild_id": g, "user": {"id": "2"}}),
    )
    .await;
    assert_eq!(members(f.db.pool(), f.guild).await.len(), 1);
    // Another Discord server the bot is in changes nothing.
    assert_eq!(
        f.send(
            "GUILD_MEMBER_ADD",
            json!({"guild_id": "999", "user": {"id": "8", "username": "zed"}})
        )
        .await,
        Outcome::Ignored(Why::OtherServer)
    );
    // They are not accounts: nothing was added to accounts or guild_members.
    let accounts: i64 = sqlx::query_scalar("SELECT count(*) FROM accounts")
        .fetch_one(f.db.pool())
        .await
        .unwrap();
    let gm: i64 = sqlx::query_scalar("SELECT count(*) FROM guild_members WHERE guild_id = $1")
        .bind(f.guild)
        .fetch_one(f.db.pool())
        .await
        .unwrap();
    assert_eq!((accounts, gm), (1, 1), "only the owner exists");
    // The list the route reads is these rows, found by the mapped channel.
    let via_channel: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM bridge_members m JOIN bridge_channel_maps c ON c.guild_id = m.guild_id \
         WHERE c.channel_id = $1",
    )
    .bind(f.sr_channel)
    .fetch_one(f.db.pool())
    .await
    .unwrap();
    assert_eq!(via_channel, 1);
    let _ = f.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_discord_member_typing_shows_in_the_mapped_channel_only() {
    let f = fx().await;
    let ev = |ch: i64, u: i64| json!({"channel_id": ch.to_string(), "user_id": u.to_string(), "timestamp": 1});
    assert_eq!(f.send("TYPING_START", ev(f.dch, 77)).await, Outcome::Typing);
    assert_eq!(
        f.send("TYPING_START", ev(f.dch, BOT)).await,
        Outcome::Ignored(Why::OwnBot)
    );
    assert_eq!(
        f.send("TYPING_START", ev(uid(), 77)).await,
        Outcome::Ignored(Why::Unmapped)
    );
    assert_eq!(f.sink.log.lock().unwrap().typing, [(f.sr_channel, 77)]);
    let _ = f.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_member_intent_is_asked_for_first_and_dropped_when_discord_refuses_it() {
    use sr_bridge::inbound::INBOUND_INTENTS;
    let f = fx().await;
    let gw = Arc::new(FakeGateway::default());
    let (discord, sent) = gw.queue_logged();
    let host = Arc::new(Host {
        off: Mutex::new(vec![]),
    });
    let m = manager(&f, gw.clone(), host, 10);
    m.reconcile().await.unwrap();
    discord.send(hello()).unwrap();
    until("identify", || async { !sent.lock().unwrap().is_empty() }).await;
    let identify = sent.lock().unwrap()[0].clone();
    let asked = identify["d"]["intents"].as_u64().unwrap();
    assert_eq!(asked & (1 << 1), 1 << 1, "GUILD_MEMBERS is asked for");
    assert_eq!(asked & (1 << 8), 0, "presence is off unless configured");
    assert_eq!(asked & INBOUND_INTENTS, INBOUND_INTENTS);
    // READY: the whole member list is requested.
    discord.send(ready()).unwrap();
    until("request guild members", || async {
        sent.lock().unwrap().iter().any(|v| v["op"] == 8)
    })
    .await;
    let req = sent
        .lock()
        .unwrap()
        .iter()
        .find(|v| v["op"] == 8)
        .unwrap()
        .clone();
    assert_eq!(
        req["d"]["guild_id"],
        f.bridge().await.discord_guild_id.to_string()
    );
    assert_eq!(req["d"]["limit"], 0);
    // Discord refuses the privileged intent: the next try goes without it, and keeps working.
    discord.send(Incoming::Close(Some(4014))).unwrap();
    until("disconnected", || async {
        status(f.db.pool(), f.guild).await == "disconnected"
    })
    .await;
    let (discord2, sent2) = gw.queue_logged();
    assert_eq!(
        m.reconcile().await.unwrap().started,
        [f.guild],
        "no cool-down for a step down"
    );
    discord2.send(hello()).unwrap();
    until("second identify", || async {
        !sent2.lock().unwrap().is_empty()
    })
    .await;
    let asked2 = sent2.lock().unwrap()[0]["d"]["intents"].as_u64().unwrap();
    assert_eq!(asked2 & (1 << 1), 0, "GUILD_MEMBERS dropped");
    assert_eq!(asked2, INBOUND_INTENTS);
    discord2.send(ready()).unwrap();
    until("connected again", || async {
        status(f.db.pool(), f.guild).await == "connected"
    })
    .await;
    assert!(
        sent2.lock().unwrap().iter().all(|v| v["op"] != 8),
        "no member request without the intent"
    );
    m.stop_all();
    let _ = f.db.cleanup().await;
}
