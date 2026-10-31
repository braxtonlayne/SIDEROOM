//! Postgres tests of the bridge against an in-memory Discord. Run with a scratch Postgres that may CREATE DATABASE:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-bridge --test db -- --ignored --test-threads=1`

#[path = "support/mod.rs"]
mod support;

use serde_json::{Value, json};
use sr_bridge::{BridgeError, Direction, TeardownReason, model};
use sr_events::{ChannelId, DomainEvent, GuildId, Topic, UserId};
use sr_features::{BRIDGE_MODULE_ID, PLATFORM_BRIDGE_ID, ScopeRef};
use support::*;

fn sent(g: i64, c: i64, m: i64, author: i64, data: Value) -> DomainEvent {
    DomainEvent::MessageSent {
        guild: Some(GuildId::new(g as u64)),
        channel: ChannelId::new(c as u64),
        message_id: m as u64,
        author: UserId::new(author as u64),
        data,
    }
}

fn msg(content: &str) -> Value {
    json!({"content": content, "type": 0, "attachments": [], "mentions": []})
}

async fn count(w: &World, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(w.db.pool())
        .await
        .unwrap()
}

async fn named_member(w: &World, g: i64, name: &str, avatar: Option<&str>) -> i64 {
    let u = account(w.db.pool()).await;
    member(w.db.pool(), g, u).await;
    sqlx::query("UPDATE accounts SET display_name = $2, avatar_key = $3 WHERE id = $1")
        .bind(u)
        .bind(name)
        .bind(avatar)
        .execute(w.db.pool())
        .await
        .unwrap();
    u
}

// ---- the model and the defaults ----

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_fresh_server_has_the_module_off_and_no_bridge_rows() {
    let w = world().await;
    let owner = account(w.db.pool()).await;
    let (g, _c) = guild(w.db.pool(), owner).await;
    for table in [
        "bridges",
        "bridge_channel_maps",
        "bridge_message_map",
        "bridge_optouts",
        "bridge_outbox",
        "bridge_teardown_residue",
    ] {
        assert_eq!(count(&w, table).await, 0, "{table} starts empty");
    }
    assert!(
        !w.svc.module_on(g).await.unwrap(),
        "bridge.enabled is off by default"
    );
    assert!(
        w.svc.platform_on().await.unwrap(),
        "platform.bridge is on by default"
    );
    assert!(model::get_bridge(w.db.pool(), g).await.unwrap().is_none());
    let status = w.svc.status(g).await.unwrap();
    assert!(!status.module_on && !status.connected && status.maps.is_empty());
    let _ = w.db.cleanup().await;
}

// ---- the token ----

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn only_bot_tokens_are_accepted_and_the_token_is_sealed() {
    let w = world().await;
    let owner = account(w.db.pool()).await;
    let (g, _c) = guild(w.db.pool(), owner).await;
    w.discord.user_token("a-user-token-not-a-bot");
    let err = w
        .svc
        .connect(g, owner, "a-user-token-not-a-bot", None)
        .await
        .unwrap_err();
    assert!(matches!(err, BridgeError::BadToken(_)), "{err:?}");
    assert_eq!(
        count(&w, "bridges").await,
        0,
        "a self-bot token is never stored"
    );
    let err = w.svc.connect(g, owner, "garbage", None).await.unwrap_err();
    assert!(matches!(err, BridgeError::BadToken(_)), "{err:?}");
    let err = w
        .svc
        .connect(g, owner, "has a space", None)
        .await
        .unwrap_err();
    assert!(matches!(err, BridgeError::Invalid(_)));

    let (dg, _) = w.discord.with_bot(TOKEN);
    let b = w
        .svc
        .connect(g, owner, &format!("Bot {TOKEN}"), None)
        .await
        .unwrap();
    assert_eq!(b.discord_guild_id, dg);
    assert_eq!(b.discord_guild_name, "Their server");
    // The column is ciphertext: the token is nowhere in it.
    let enc: Vec<u8> = sqlx::query_scalar("SELECT bot_token_enc FROM bridges WHERE guild_id = $1")
        .bind(g)
        .fetch_one(w.db.pool())
        .await
        .unwrap();
    assert!(!String::from_utf8_lossy(&enc).contains("token-value"));
    assert_eq!(w.svc.open_bot_token(g).await.unwrap(), TOKEN);
    // The bot's own calls carry the token, and the audit log says a bridge was connected.
    assert!(
        w.discord
            .log()
            .iter()
            .any(|s| s.auth.as_deref() == Some(&format!("Bot {TOKEN}")))
    );
    let audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log_entries WHERE guild_id = $1 AND action = 80",
    )
    .bind(g)
    .fetch_one(w.db.pool())
    .await
    .unwrap();
    assert_eq!(audits, 1);
    // Replacing the token of the same bot is fine; a different bot is not.
    w.svc.connect(g, owner, TOKEN, None).await.unwrap();
    w.discord.with_bot("another-bot-token");
    let err = w
        .svc
        .connect(g, owner, "another-bot-token", None)
        .await
        .unwrap_err();
    assert!(matches!(err, BridgeError::Conflict(_)), "{err:?}");
    let _ = w.db.cleanup().await;
}

// ---- maps ----

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn maps_create_webhooks_and_refuse_what_cannot_be_bridged() {
    let w = world().await;
    let b = w.bridged().await;
    assert_eq!(
        w.discord.count("POST", "/channels/"),
        1,
        "one webhook for the sending map"
    );
    let map = w.svc.get_map(b.guild, b.map).await.unwrap();
    assert!(map.discord_webhook_id.is_some());
    // The webhook token is sealed in the row.
    let enc: Vec<u8> =
        sqlx::query_scalar("SELECT webhook_token_enc FROM bridge_channel_maps WHERE id = $1")
            .bind(b.map)
            .fetch_one(w.db.pool())
            .await
            .unwrap();
    assert!(!String::from_utf8_lossy(&enc).contains("whtok"));
    assert!(
        w.notices
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|(c, t)| *c == b.channel && t.contains("bridged"))
    );

    // Already mapped, either side.
    let other = channel(w.db.pool(), b.guild, "other").await;
    let err = w
        .svc
        .create_map(
            b.guild,
            b.owner,
            other,
            b.discord_channels[0],
            Direction::Both,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, BridgeError::Conflict(_)), "{err:?}");
    let err = w
        .svc
        .create_map(
            b.guild,
            b.owner,
            b.channel,
            b.discord_channels[1],
            Direction::Both,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, BridgeError::Conflict(_)), "{err:?}");

    // A direct message is never bridged (end-to-end encrypted conversations cannot be read by the server).
    let dm = dm_channel(w.db.pool()).await;
    let err = w
        .svc
        .create_map(b.guild, b.owner, dm, b.discord_channels[1], Direction::Both)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, BridgeError::Invalid(m) if m.contains("end-to-end")),
        "{err:?}"
    );
    // A voice channel is not text.
    let voice = channel_of(w.db.pool(), b.guild, "talk", 2).await;
    assert!(
        w.svc
            .create_map(
                b.guild,
                b.owner,
                voice,
                b.discord_channels[1],
                Direction::Both
            )
            .await
            .is_err()
    );
    // A channel of another server.
    let other_owner = account(w.db.pool()).await;
    let (_g2, foreign) = guild(w.db.pool(), other_owner).await;
    let err = w
        .svc
        .create_map(
            b.guild,
            b.owner,
            foreign,
            b.discord_channels[1],
            Direction::Both,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, BridgeError::NotFound(_)), "{err:?}");
    // A Discord channel the bot cannot see.
    let err = w
        .svc
        .create_map(b.guild, b.owner, other, 424_242, Direction::Both)
        .await
        .unwrap_err();
    assert!(matches!(err, BridgeError::Invalid(_)), "{err:?}");

    // A receive-only map has no webhook; switching it to send creates one, back deletes it.
    let m2 = w
        .svc
        .create_map(
            b.guild,
            b.owner,
            other,
            b.discord_channels[1],
            Direction::FromDiscord,
        )
        .await
        .unwrap();
    assert!(m2.discord_webhook_id.is_none());
    let before = w.discord.webhook_ids().len();
    let m2 = w
        .svc
        .update_map(b.guild, m2.id, Direction::Both)
        .await
        .unwrap();
    assert!(m2.discord_webhook_id.is_some());
    assert_eq!(w.discord.webhook_ids().len(), before + 1);
    let m2 = w
        .svc
        .update_map(b.guild, m2.id, Direction::FromDiscord)
        .await
        .unwrap();
    assert!(m2.discord_webhook_id.is_none());
    assert_eq!(w.discord.webhook_ids().len(), before);

    // Removing a map deletes its webhook and tells the channel.
    w.svc.delete_map(b.guild, m2.id).await.unwrap();
    w.svc.delete_map(b.guild, b.map).await.unwrap();
    assert!(w.discord.webhook_ids().is_empty());
    assert!(
        w.notices
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|(c, t)| *c == b.channel && t.contains("no longer bridged"))
    );
    // Another server's map id is not found.
    assert!(matches!(
        w.svc.get_map(b.guild + 1, b.map).await,
        Err(BridgeError::NotFound(_))
    ));
    let _ = w.db.cleanup().await;
}

// ---- outbound ----

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn messages_are_posted_through_the_webhook_with_name_avatar_and_markdown() {
    let w = world().await;
    let b = w.bridged().await;
    let ann = named_member(&w, b.guild, "Ann Lee", Some("abc123")).await;
    let bob = named_member(&w, b.guild, "Bob", None).await;
    let mut data = msg("");
    data["content"] = json!(format!(
        "hi <@{bob}> in <#{}> @everyone <:wave:123>",
        b.channel
    ));
    data["mentions"] = json!([{"id": bob.to_string(), "username": "bob", "global_name": "Bob"}]);
    data["attachments"] = json!([{"filename": "cat.png", "url": "/cdn/attachments/1/2/cat.png"}]);
    data["embeds"] = json!([{"title": "dropped"}]);
    w.svc
        .handle_event(&sent(b.guild, b.channel, 1001, ann, data))
        .await;
    assert_eq!(count(&w, "bridge_outbox").await, 1);
    assert_eq!(w.svc.drain_outbox().await.unwrap(), 1);
    let posts = w.discord.posted();
    assert_eq!(posts.len(), 1);
    let p = &posts[0];
    assert_eq!(p["username"], "Ann Lee");
    assert_eq!(p["allowed_mentions"]["parse"], json!([]));
    let avatar = p["avatar_url"].as_str().unwrap();
    assert!(avatar.starts_with("https://sr.test/cdn/avatars/") && avatar.contains("abc123"));
    assert!(
        avatar.contains("ex=86400"),
        "a signed link with a 24 hour life: {avatar}"
    );
    let content = p["content"].as_str().unwrap();
    assert!(content.contains("@Bob"), "{content}");
    assert!(content.contains("#general"), "{content}");
    assert!(content.contains(":wave:") && !content.contains("<:wave"));
    assert!(
        !content.contains("@everyone"),
        "broadcast mentions are defused: {content}"
    );
    assert!(
        content.contains("cat.png: https://sr.test/cdn/attachments/1/2/cat.png?ex=86400"),
        "{content}"
    );
    assert!(p.get("embeds").is_none(), "embeds are dropped");
    // The message map links both ids; the queue is empty.
    let mm = model::message_map_for(w.db.pool(), 1001)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(mm.direction, sr_bridge::MessageMapDirection::Out);
    assert_eq!(mm.author_id, Some(ann));
    assert_eq!(count(&w, "bridge_outbox").await, 0);
    // Posting twice for the same event does nothing more.
    w.svc
        .handle_event(&sent(b.guild, b.channel, 1001, ann, msg("again")))
        .await;
    assert_eq!(count(&w, "bridge_outbox").await, 0);
    let _ = w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn edits_and_deletes_follow_the_message() {
    let w = world().await;
    let b = w.bridged().await;
    let ann = named_member(&w, b.guild, "Ann", None).await;
    let (g, c) = (
        GuildId::new(b.guild as u64),
        ChannelId::new(b.channel as u64),
    );
    let edit = |m: u64, content: &str| {
        let mut d = msg(content);
        d["author"] = json!({"id": ann.to_string()});
        DomainEvent::MessageEdited {
            guild: Some(g),
            channel: c,
            message_id: m,
            data: d,
        }
    };
    w.svc
        .handle_event(&sent(b.guild, b.channel, 2001, ann, msg("first")))
        .await;
    w.svc.drain_outbox().await.unwrap();
    w.svc.handle_event(&edit(2001, "second")).await;
    w.svc.drain_outbox().await.unwrap();
    {
        let s = w.discord.state.lock().unwrap();
        assert_eq!(s.messages.values().next().unwrap()["content"], "second");
    }
    assert_eq!(w.discord.count("PATCH", "/webhooks/"), 1);
    w.svc
        .handle_event(&DomainEvent::MessageDeleted {
            guild: Some(g),
            channel: c,
            message_id: 2001,
        })
        .await;
    w.svc.drain_outbox().await.unwrap();
    assert!(w.discord.state.lock().unwrap().messages.is_empty());
    assert_eq!(count(&w, "bridge_message_map").await, 0);

    // An edit that arrives before the post is sent folds into it; a delete cancels it.
    w.svc
        .handle_event(&sent(b.guild, b.channel, 2002, ann, msg("draft")))
        .await;
    w.svc.handle_event(&edit(2002, "final")).await;
    assert_eq!(count(&w, "bridge_outbox").await, 1);
    w.svc
        .handle_event(&sent(b.guild, b.channel, 2003, ann, msg("never")))
        .await;
    w.svc
        .handle_event(&DomainEvent::MessageDeleted {
            guild: Some(g),
            channel: c,
            message_id: 2003,
        })
        .await;
    w.svc.drain_outbox().await.unwrap();
    let posts = w.discord.posted();
    assert_eq!(posts.last().unwrap()["content"], "final");
    assert!(!posts.iter().any(|p| p["content"] == "never"));
    let _ = w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn opt_outs_the_loop_guard_and_directions_keep_messages_home() {
    let w = world().await;
    let b = w.bridged().await;
    let ann = named_member(&w, b.guild, "Ann", None).await;
    // Opted out: nothing is queued, and the opt-out is counted for the owner.
    w.svc.set_optout(ann, b.channel, true).await.unwrap();
    w.svc
        .handle_event(&sent(b.guild, b.channel, 3001, ann, msg("private")))
        .await;
    assert_eq!(count(&w, "bridge_outbox").await, 0);
    assert_eq!(w.svc.status(b.guild).await.unwrap().maps[0].optouts, 1);
    let pc = w.svc.public_channels(b.guild, ann).await.unwrap();
    assert_eq!(pc.len(), 1);
    assert!(pc[0].opted_out);
    // Taking it back mirrors again.
    w.svc.set_optout(ann, b.channel, false).await.unwrap();
    w.svc
        .handle_event(&sent(b.guild, b.channel, 3002, ann, msg("public")))
        .await;
    assert_eq!(count(&w, "bridge_outbox").await, 1);
    // Opting out drops what was still waiting.
    w.svc.set_optout(ann, b.channel, true).await.unwrap();
    assert_eq!(count(&w, "bridge_outbox").await, 0);
    // An unmapped channel has no opt-out.
    let plain = channel(w.db.pool(), b.guild, "plain").await;
    assert!(matches!(
        w.svc.set_optout(ann, plain, true).await,
        Err(BridgeError::NotFound(_))
    ));

    // A message that came from Discord is never sent back, and system messages are not mirrored.
    let bob = named_member(&w, b.guild, "Bob", None).await;
    let mut from_discord = msg("hello from discord");
    from_discord["sr_bridge"] = json!({"platform": "discord", "author_name": "x"});
    w.svc
        .handle_event(&sent(b.guild, b.channel, 3003, bob, from_discord))
        .await;
    let mut join = msg("welcome");
    join["type"] = json!(7);
    w.svc
        .handle_event(&sent(b.guild, b.channel, 3004, bob, join))
        .await;
    assert_eq!(count(&w, "bridge_outbox").await, 0);

    // Receive-only and unmapped channels send nothing.
    let ro = channel(w.db.pool(), b.guild, "ro").await;
    w.svc
        .create_map(
            b.guild,
            b.owner,
            ro,
            b.discord_channels[1],
            Direction::FromDiscord,
        )
        .await
        .unwrap();
    w.svc
        .handle_event(&sent(b.guild, ro, 3005, bob, msg("only here")))
        .await;
    w.svc
        .handle_event(&sent(b.guild, plain, 3006, bob, msg("not mapped")))
        .await;
    assert_eq!(count(&w, "bridge_outbox").await, 0);

    // The module off means nothing is mirrored even if rows linger.
    w.features
        .set_enabled(BRIDGE_MODULE_ID, &guild_scope(b.guild), Some(false))
        .await
        .unwrap();
    w.svc
        .handle_event(&sent(b.guild, b.channel, 3007, bob, msg("module off")))
        .await;
    assert_eq!(count(&w, "bridge_outbox").await, 0);
    let _ = w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_queue_honours_rate_limit_headers_and_a_429() {
    let w = world().await;
    let b = w.bridged().await;
    let ann = named_member(&w, b.guild, "Ann", None).await;
    // Discord says the bucket is empty for 0.4 s after each post: the second post waits for it.
    w.discord.state.lock().unwrap().exhaust_bucket_secs = Some(0.4);
    w.svc
        .handle_event(&sent(b.guild, b.channel, 4001, ann, msg("one")))
        .await;
    w.svc
        .handle_event(&sent(b.guild, b.channel, 4002, ann, msg("two")))
        .await;
    assert_eq!(w.svc.run_outbox_once().await.unwrap(), 1);
    assert_eq!(w.discord.posted().len(), 1);
    // Blocked: the pass sends nothing, and does not even try.
    assert_eq!(w.svc.run_outbox_once().await.unwrap(), 0);
    assert_eq!(w.discord.posted().len(), 1, "the empty bucket is respected");
    tokio::time::sleep(std::time::Duration::from_millis(450)).await;
    assert_eq!(w.svc.run_outbox_once().await.unwrap(), 1);
    assert_eq!(w.discord.posted().len(), 2);
    let order: Vec<String> = w
        .discord
        .posted()
        .iter()
        .map(|p| p["content"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(order, vec!["one", "two"], "order is kept");

    // A 429 reschedules without spending an attempt, then the post goes through.
    w.discord.state.lock().unwrap().exhaust_bucket_secs = None;
    w.discord.state.lock().unwrap().rate_limit_next = 1;
    // Let the empty bucket of the last post refill first.
    tokio::time::sleep(std::time::Duration::from_millis(450)).await;
    w.svc
        .handle_event(&sent(b.guild, b.channel, 4003, ann, msg("three")))
        .await;
    assert_eq!(w.svc.run_outbox_once().await.unwrap(), 0);
    let attempts: i32 = sqlx::query_scalar("SELECT attempts FROM bridge_outbox")
        .fetch_one(w.db.pool())
        .await
        .unwrap();
    assert_eq!(attempts, 0, "a 429 is not a failure");
    assert_eq!(
        w.svc.run_outbox_once().await.unwrap(),
        0,
        "still waiting out retry_after"
    );
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    assert_eq!(w.svc.run_outbox_once().await.unwrap(), 1);
    assert_eq!(w.discord.posted().last().unwrap()["content"], "three");
    let _ = w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn failures_retry_with_backoff_then_become_dead_letters() {
    let w = world().await;
    let b = w.bridged().await;
    let ann = named_member(&w, b.guild, "Ann", None).await;
    w.discord.state.lock().unwrap().fail_next = 100;
    w.svc
        .handle_event(&sent(b.guild, b.channel, 5001, ann, msg("doomed")))
        .await;
    for _ in 0..10 {
        w.svc.run_outbox_once().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(8)).await;
    }
    assert_eq!(
        w.svc.dead_letter_count().await.unwrap(),
        1,
        "dead after max attempts (3)"
    );
    assert_eq!(w.svc.queued_count().await.unwrap(), 0);
    let status = w.svc.status(b.guild).await.unwrap();
    assert_eq!(status.dead_letters, 1);
    assert_eq!(status.maps[0].dead, 1);
    let err: Option<String> =
        sqlx::query_scalar("SELECT last_error FROM bridge_outbox WHERE state = 'dead'")
            .fetch_one(w.db.pool())
            .await
            .unwrap();
    assert!(
        !err.unwrap_or_default().contains("whtok"),
        "no token in a stored error"
    );
    // The next message still goes (the dead letter does not block the map).
    w.discord.state.lock().unwrap().fail_next = 0;
    w.svc
        .handle_event(&sent(b.guild, b.channel, 5002, ann, msg("fine")))
        .await;
    assert_eq!(w.svc.drain_outbox().await.unwrap(), 1);
    assert_eq!(w.discord.posted().last().unwrap()["content"], "fine");
    let _ = w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_test_post_reaches_discord_at_once() {
    let w = world().await;
    let b = w.bridged().await;
    w.svc.test_post(b.guild, b.map).await.unwrap();
    let posts = w.discord.posted();
    assert_eq!(posts.len(), 1);
    assert!(
        posts[0]["content"]
            .as_str()
            .unwrap()
            .contains("Hello from SIDE ROOM")
    );
    let _ = w.db.cleanup().await;
}

// ---- teardown ----

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn teardown_deletes_webhooks_rows_and_token_and_leaves_one_audit_row() {
    let w = world().await;
    let b = w.bridged().await;
    let second = channel(w.db.pool(), b.guild, "second").await;
    w.svc
        .create_map(
            b.guild,
            b.owner,
            second,
            b.discord_channels[1],
            Direction::Both,
        )
        .await
        .unwrap();
    let ann = named_member(&w, b.guild, "Ann", None).await;
    w.svc.set_optout(ann, b.channel, true).await.unwrap();
    w.svc
        .handle_event(&sent(b.guild, second, 6001, ann, msg("posted")))
        .await;
    w.svc.drain_outbox().await.unwrap();
    w.svc
        .handle_event(&sent(b.guild, second, 6002, ann, msg("queued")))
        .await;
    assert_eq!(w.discord.webhook_ids().len(), 2);
    w.notices.0.lock().unwrap().clear();

    // Off, as the toggle handler does it.
    w.features
        .set_enabled(BRIDGE_MODULE_ID, &guild_scope(b.guild), Some(false))
        .await
        .unwrap();
    let report = w
        .svc
        .teardown(b.guild, b.owner, TeardownReason::Owner)
        .await
        .unwrap();
    assert!(report.was_connected);
    assert_eq!(report.maps_removed, 2);
    assert_eq!(report.webhooks_deleted, 2);
    assert!(report.leftovers.is_empty());

    assert!(
        w.discord.webhook_ids().is_empty(),
        "the webhooks are gone from Discord"
    );
    assert_eq!(w.discord.count("DELETE", "/webhooks/"), 2);
    for table in [
        "bridges",
        "bridge_channel_maps",
        "bridge_message_map",
        "bridge_optouts",
        "bridge_outbox",
    ] {
        assert_eq!(
            count(&w, table).await,
            0,
            "{table} is empty after the teardown"
        );
    }
    assert!(
        matches!(
            w.svc.open_bot_token(b.guild).await,
            Err(BridgeError::NotFound(_))
        ),
        "the token is wiped"
    );
    // One audit row with the count of maps removed.
    let rows: Vec<(i16, Value)> = sqlx::query_as(
        "SELECT action, options FROM audit_log_entries WHERE guild_id = $1 AND action = 82",
    )
    .bind(b.guild)
    .fetch_all(w.db.pool())
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1["count"], 2);
    // A system message in each formerly mapped channel.
    let notes = w.notices.0.lock().unwrap().clone();
    assert!(
        notes
            .iter()
            .any(|(c, t)| *c == b.channel && t == "The Discord bridge was switched off")
    );
    assert!(
        notes
            .iter()
            .any(|(c, t)| *c == second && t == "The Discord bridge was switched off")
    );
    // Messages already mirrored stay where they are on Discord: only the webhooks were deleted.
    assert_eq!(w.discord.count("DELETE", "/webhooks/"), 2);
    assert_eq!(w.discord.state.lock().unwrap().messages.len(), 1);
    // A post here after the teardown reaches nothing.
    w.svc
        .handle_event(&sent(b.guild, second, 6003, ann, msg("after")))
        .await;
    assert_eq!(count(&w, "bridge_outbox").await, 0);
    assert_eq!(w.discord.posted().len(), 1);
    // Idempotent.
    let again = w
        .svc
        .teardown(b.guild, b.owner, TeardownReason::Owner)
        .await
        .unwrap();
    assert!(!again.was_connected && again.maps_removed == 0);

    // Re-enabling starts empty: paste the token again, map afresh.
    w.features
        .set_enabled(BRIDGE_MODULE_ID, &guild_scope(b.guild), Some(true))
        .await
        .unwrap();
    assert!(w.svc.status(b.guild).await.unwrap().maps.is_empty());
    w.svc.connect(b.guild, b.owner, TOKEN, None).await.unwrap();
    assert!(w.svc.status(b.guild).await.unwrap().maps.is_empty());
    let _ = w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_webhook_that_cannot_be_deleted_is_tried_three_times_and_remembered_by_id() {
    let w = world().await;
    let b = w.bridged().await;
    let stuck = w
        .svc
        .get_map(b.guild, b.map)
        .await
        .unwrap()
        .discord_webhook_id
        .unwrap();
    w.discord.state.lock().unwrap().undeletable.insert(stuck);
    let report = w
        .svc
        .teardown(b.guild, b.owner, TeardownReason::Owner)
        .await
        .unwrap();
    assert_eq!(report.webhooks_deleted, 0);
    assert_eq!(report.leftovers.len(), 1);
    assert_eq!(report.leftovers[0].webhook_id, stuck);
    assert_eq!(
        report.leftovers[0].discord_channel_id,
        b.discord_channels[0]
    );
    assert_eq!(
        w.discord.count("DELETE", &format!("/webhooks/{stuck}")),
        3,
        "three attempts"
    );
    // Everything local is gone regardless, and the owner can read the id.
    assert_eq!(count(&w, "bridges").await, 0);
    let left = w.svc.leftovers(b.guild).await.unwrap();
    assert_eq!(left, report.leftovers);
    // Connecting a bot again clears the list.
    w.svc.connect(b.guild, b.owner, TOKEN, None).await.unwrap();
    assert!(w.svc.leftovers(b.guild).await.unwrap().is_empty());
    let _ = w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_platform_kill_tears_down_every_server_and_the_module_stays_off() {
    let w = world().await;
    let a = w.bridged().await;
    // A second server with its own bot, and a third with the module on but no token yet.
    let owner2 = account(w.db.pool()).await;
    let (g2, c2) = guild(w.db.pool(), owner2).await;
    let (_, dch2) = w.discord.with_bot("second-bot-token");
    w.features
        .set_enabled(BRIDGE_MODULE_ID, &guild_scope(g2), Some(true))
        .await
        .unwrap();
    w.svc
        .connect(g2, owner2, "second-bot-token", None)
        .await
        .unwrap();
    w.svc
        .create_map(g2, owner2, c2, dch2[0], Direction::Both)
        .await
        .unwrap();
    let g3_owner = account(w.db.pool()).await;
    let (g3, _) = guild(w.db.pool(), g3_owner).await;
    w.features
        .set_enabled(BRIDGE_MODULE_ID, &guild_scope(g3), Some(true))
        .await
        .unwrap();

    // The operator's kill: the event reaches the bridge, which tears everything down.
    w.features
        .set_enabled(PLATFORM_BRIDGE_ID, &ScopeRef::Platform, Some(false))
        .await
        .unwrap();
    assert!(!w.svc.platform_on().await.unwrap());
    assert!(!w.svc.module_on(a.guild).await.unwrap());
    w.svc
        .handle_event(&DomainEvent::FeatureSettingsChanged {
            feature_id: PLATFORM_BRIDGE_ID.into(),
            scope: Topic::Platform,
        })
        .await;
    assert_eq!(count(&w, "bridges").await, 0);
    assert_eq!(count(&w, "bridge_channel_maps").await, 0);
    assert!(w.discord.webhook_ids().is_empty());
    for g in [a.guild, g2] {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_log_entries WHERE guild_id = $1 AND action = 82",
        )
        .bind(g)
        .fetch_one(w.db.pool())
        .await
        .unwrap();
        assert_eq!(n, 1, "logged per server");
    }
    // Nothing is mirrored while it is killed.
    let ann = named_member(&w, a.guild, "Ann", None).await;
    w.svc
        .handle_event(&sent(a.guild, a.channel, 7001, ann, msg("x")))
        .await;
    assert_eq!(count(&w, "bridge_outbox").await, 0);

    // The feature returns: every server that had chosen the module on has it off still.
    w.features
        .set_enabled(PLATFORM_BRIDGE_ID, &ScopeRef::Platform, Some(true))
        .await
        .unwrap();
    for g in [a.guild, g2, g3] {
        assert!(
            !w.svc.module_on(g).await.unwrap(),
            "the module stays off after the kill"
        );
    }
    let _ = w.db.cleanup().await;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn switching_the_module_off_by_any_path_tears_down_through_the_event_backstop() {
    let w = world().await;
    let b = w.bridged().await;
    w.features
        .set_enabled(BRIDGE_MODULE_ID, &guild_scope(b.guild), Some(false))
        .await
        .unwrap();
    w.svc
        .handle_event(&DomainEvent::FeatureSettingsChanged {
            feature_id: BRIDGE_MODULE_ID.into(),
            scope: Topic::Guild(GuildId::new(b.guild as u64)),
        })
        .await;
    assert_eq!(count(&w, "bridges").await, 0);
    assert!(w.discord.webhook_ids().is_empty());
    // Switching it on does not tear anything down.
    let b2 = w.bridged_with("second-token").await;
    w.svc
        .handle_event(&DomainEvent::FeatureSettingsChanged {
            feature_id: BRIDGE_MODULE_ID.into(),
            scope: Topic::Guild(GuildId::new(b2.guild as u64)),
        })
        .await;
    assert_eq!(count(&w, "bridges").await, 1);
    // Deleting a mapped channel removes its map and its webhook.
    w.svc
        .handle_event(&DomainEvent::ChannelDeleted {
            guild: GuildId::new(b2.guild as u64),
            channel: ChannelId::new(b2.channel as u64),
            payload: json!({}),
            viewers: vec![],
        })
        .await;
    assert_eq!(count(&w, "bridge_channel_maps").await, 0);
    assert!(w.discord.webhook_ids().is_empty());
    let _ = w.db.cleanup().await;
}

// ---- data requests ----

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_members_export_lists_ids_and_erasure_deletes_the_discord_copies() {
    let w = world().await;
    let b = w.bridged().await;
    let ann = named_member(&w, b.guild, "Ann", None).await;
    let bob = named_member(&w, b.guild, "Bob", None).await;
    for (m, who) in [(8001, ann), (8002, ann), (8003, bob)] {
        w.svc
            .handle_event(&sent(b.guild, b.channel, m, who, msg("hello")))
            .await;
    }
    w.svc.drain_outbox().await.unwrap();
    w.svc.set_optout(ann, b.channel, true).await.unwrap();
    let export = w.svc.export_for(ann).await.unwrap();
    assert_eq!(export.message_ids, vec![8001, 8002]);
    assert_eq!(export.optout_channels, vec![b.channel]);
    assert_eq!(w.svc.erase_for(ann).await.unwrap(), 2);
    assert_eq!(
        w.discord.state.lock().unwrap().messages.len(),
        1,
        "Bob's copy stays"
    );
    let after = w.svc.export_for(ann).await.unwrap();
    assert!(after.message_ids.is_empty() && after.optout_channels.is_empty());
    let _ = w.db.cleanup().await;
}

// ---- secrets stay secret ----

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn no_report_or_status_carries_a_token() {
    let w = world().await;
    let b = w.bridged().await;
    let status = serde_json::to_string(&w.svc.status(b.guild).await.unwrap()).unwrap();
    assert!(!status.contains(TOKEN) && !status.contains("whtok") && !status.contains("token_enc"));
    let ids = w.discord.webhook_ids();
    let wh_token = w.discord.state.lock().unwrap().webhooks[&ids[0]].0.clone();
    let report = w
        .svc
        .teardown(b.guild, b.owner, TeardownReason::Owner)
        .await
        .unwrap();
    let dump = format!("{report:?}");
    assert!(!dump.contains(&wh_token) && !dump.contains(TOKEN));
    let _ = w.db.cleanup().await;
}
