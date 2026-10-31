//! Postgres tests for the rich message store: polls, previews, stickers, forward
//! snapshots and voice-message attachment metadata.
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test rich_store -- --ignored --test-threads=1`

use std::sync::Arc;

use sr_chat::message::{Embed, ForwardSnapshot, Poll, PollAnswer, PollMedia};
use sr_chat::repo::{Attachment, MessageRepository, MessageStore, UploadState};
use sr_chat::rich_repo::{RetractResult, RichStore, VoteResult};
use sr_chat::{ChatLimits, ChatService, ForwardRequest, SendMessage};
use sr_core::{AttachmentId, Clock, SnowflakeGenerator, SystemClock, UserId};
use sr_db::chat_repo::{PgChatRepo, PgMessageStore};
use sr_db::rich_repo::PgRichStore;
use sr_db::{DbConfig, PgPool};

async fn pool() -> PgPool {
    let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
    let pool = sr_db::connect(&DbConfig::new(url)).await.expect("connect");
    sr_db::migrate(&pool).await.expect("migrate");
    pool
}

async fn account(pool: &PgPool, ids: &SnowflakeGenerator<Arc<dyn Clock>>) -> UserId {
    let person = ids.generate().get() as i64;
    let acct = ids.generate();
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(person)
        .execute(pool)
        .await
        .unwrap();
    let name = format!("r{}", acct.get());
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) \
         VALUES ($1, $2, $3, $4, 'x')",
    )
    .bind(acct.get() as i64)
    .bind(person)
    .bind(&name[..name.len().min(32)])
    .bind(format!("{name}@example.test"))
    .execute(pool)
    .await
    .unwrap();
    UserId::new(acct.get())
}

fn poll(multi: bool, expiry_ms: i64) -> Poll {
    let ans = |id, t: &str| PollAnswer {
        answer_id: id,
        poll_media: PollMedia {
            text: Some(t.into()),
            emoji: None,
        },
    };
    Poll {
        question: PollMedia {
            text: Some("q".into()),
            emoji: None,
        },
        answers: vec![ans(1, "a"), ans(2, "b")],
        allow_multiselect: multi,
        duration_hours: Some(1),
        layout_type: Some(1),
        expiry_ms: Some(expiry_ms),
        finalized: false,
        anonymous_counts: Default::default(),
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn polls_votes_finalise_and_embeds() {
    let pool = pool().await;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids = SnowflakeGenerator::new(clock.clone(), 11, 11).expect("ids");
    let svc = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock.clone(),
        SnowflakeGenerator::new(clock.clone(), 12, 12).expect("ids"),
        ChatLimits::default(),
    );
    let rich = PgRichStore::new(pool.clone());
    let owner = account(&pool, &ids).await;
    let other = account(&pool, &ids).await;
    let bp = svc.create_guild(owner, "Poll Club").await.unwrap();
    let general = bp.channels[2].id;
    let now = clock.now_unix_ms();

    let single = svc
        .send_message(
            owner,
            general,
            SendMessage {
                poll: Some(poll(false, now + 3_600_000)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let repo = PgChatRepo::new(pool.clone());
    // The stored poll round-trips with its expiry.
    let back = repo.get_message(general, single.id).await.unwrap().unwrap();
    assert_eq!(back.poll.as_ref().unwrap().expiry_ms, Some(now + 3_600_000));

    // Single choice: the second vote moves the first.
    assert_eq!(
        rich.cast_vote(single.id, 1, other, now).await.unwrap(),
        VoteResult::Added { replaced: vec![] }
    );
    assert_eq!(
        rich.cast_vote(single.id, 1, other, now).await.unwrap(),
        VoteResult::AlreadyVoted
    );
    assert_eq!(
        rich.cast_vote(single.id, 2, other, now).await.unwrap(),
        VoteResult::Added { replaced: vec![1] }
    );
    assert_eq!(
        rich.cast_vote(single.id, 9, other, now).await.unwrap(),
        VoteResult::UnknownAnswer
    );
    rich.cast_vote(single.id, 2, owner, now).await.unwrap();
    let t = rich
        .poll_tallies(&[single.id], Some(other))
        .await
        .unwrap()
        .remove(&single.id)
        .unwrap();
    assert_eq!((t.count(1), t.count(2)), (0, 2));
    assert!(t.mine.contains(&2) && !t.mine.contains(&1));
    let voters = rich.poll_voters(single.id, 2, None, 10).await.unwrap();
    assert_eq!(voters.len(), 2);
    let after = rich
        .poll_voters(single.id, 2, Some(voters[0]), 10)
        .await
        .unwrap();
    assert_eq!(after, vec![voters[1]]);
    assert_eq!(
        rich.retract_vote(single.id, 2, other, now).await.unwrap(),
        RetractResult::Removed
    );
    assert_eq!(
        rich.retract_vote(single.id, 2, other, now).await.unwrap(),
        RetractResult::NotVoted
    );

    // Not due yet; early expiry then finalise exactly once.
    assert!(
        rich.due_polls(now, 10)
            .await
            .unwrap()
            .iter()
            .all(|(_, m)| *m != single.id)
    );
    assert!(!rich.finalize_poll(single.id, now).await.unwrap());
    assert!(rich.expire_poll_now(single.id, now).await.unwrap());
    assert!(!rich.expire_poll_now(single.id, now).await.unwrap());
    assert!(
        rich.due_polls(now + 1, 10)
            .await
            .unwrap()
            .contains(&(general, single.id))
    );
    assert_eq!(
        rich.cast_vote(single.id, 1, other, now + 1).await.unwrap(),
        VoteResult::Closed
    );
    assert!(rich.finalize_poll(single.id, now + 1).await.unwrap());
    assert!(!rich.finalize_poll(single.id, now + 1).await.unwrap());
    assert!(
        rich.due_polls(now + 1, 10)
            .await
            .unwrap()
            .iter()
            .all(|(_, m)| *m != single.id)
    );
    let done = repo.get_message(general, single.id).await.unwrap().unwrap();
    assert!(done.poll.unwrap().finalized);
    assert_eq!(
        rich.retract_vote(single.id, 2, owner, now + 1)
            .await
            .unwrap(),
        RetractResult::Closed
    );

    // Multi choice keeps both.
    let multi = svc
        .send_message(
            owner,
            general,
            SendMessage {
                poll: Some(poll(true, now + 3_600_000)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    rich.cast_vote(multi.id, 1, owner, now).await.unwrap();
    assert_eq!(
        rich.cast_vote(multi.id, 2, owner, now).await.unwrap(),
        VoteResult::Added { replaced: vec![] }
    );
    // A message without a poll.
    let plain = svc
        .send_message(
            owner,
            general,
            SendMessage {
                content: "hi".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        rich.cast_vote(plain.id, 1, owner, now).await.unwrap(),
        VoteResult::NoPoll
    );

    // An expired poll is found by the sweep query.
    let old = svc
        .send_message(
            owner,
            general,
            SendMessage {
                poll: Some(poll(false, now - 1000)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(
        rich.due_polls(now, 100)
            .await
            .unwrap()
            .contains(&(general, old.id))
    );
    assert!(rich.finalize_poll(old.id, now).await.unwrap());

    // Embeds replace the stored list.
    rich.set_embeds(
        plain.id,
        &[Embed {
            title: Some("t".into()),
            kind: Some("link".into()),
            ..Embed::default()
        }],
    )
    .await
    .unwrap();
    let p = repo.get_message(general, plain.id).await.unwrap().unwrap();
    assert_eq!(p.embeds.len(), 1);
    assert!(p.embeds[0].is_unfurled());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn stickers_forwards_and_voice_metadata_round_trip() {
    let pool = pool().await;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids = SnowflakeGenerator::new(clock.clone(), 13, 13).expect("ids");
    let svc = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock.clone(),
        SnowflakeGenerator::new(clock.clone(), 14, 14).expect("ids"),
        ChatLimits::default(),
    );
    let rich = PgRichStore::new(pool.clone());
    let store = PgMessageStore::new(pool.clone());
    let owner = account(&pool, &ids).await;
    let bp = svc.create_guild(owner, "Rich Club").await.unwrap();
    let guild = bp.guild.id;
    let general = bp.channels[2].id;

    // Guild sticker and a standard (guildless) one.
    let (gs, ss) = (ids.generate().get(), ids.generate().get());
    for (id, g) in [(gs, Some(guild.get() as i64)), (ss, None)] {
        sqlx::query(
            "INSERT INTO stickers (id, guild_id, name, tags, format_type, creator_id, media_key) \
             VALUES ($1, $2, 'wave', 'hi', 1, $3, 'k')",
        )
        .bind(id as i64)
        .bind(g)
        .bind(owner.get() as i64)
        .execute(&pool)
        .await
        .unwrap();
    }
    let found = rich.stickers(&[gs, ss, 1]).await.unwrap();
    assert_eq!(found.len(), 2);
    assert!(found.iter().any(|s| s.id == ss && s.guild_id.is_none()));
    assert!(
        found
            .iter()
            .any(|s| s.id == gs && s.guild_id == Some(guild))
    );
    let with_stickers = svc
        .send_message(
            owner,
            general,
            SendMessage {
                sticker_ids: vec![gs, ss],
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let repo = PgChatRepo::new(pool.clone());
    let back = repo
        .get_message(general, with_stickers.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(back.sticker_ids, vec![gs, ss]);

    // Voice metadata on an attachment.
    let att_id = AttachmentId::new(ids.generate().get());
    let mut att = Attachment {
        id: att_id,
        channel_id: general,
        uploader: owner,
        filename: "voice-message.ogg".into(),
        size: 10,
        key: format!("chat/{general}/{att_id}/voice-message.ogg"),
        declared_content_type: None,
        content_type: Some("audio/ogg".into()),
        description: None,
        spoiler: false,
        state: UploadState::Ready,
        message_id: None,
        expires_at_ms: None,
        created_at_ms: clock.now_unix_ms(),
        duration_ms: None,
        waveform: None,
        width: None,
        height: None,
        blurhash: None,
    };
    store.insert_attachment(&att).await.unwrap();
    att.duration_ms = Some(4500);
    att.waveform = Some("AQID".into());
    store.update_attachment(&att).await.unwrap();
    let got = store.get_attachments(&[att_id]).await.unwrap().remove(0);
    assert_eq!(got.duration_ms, Some(4500));
    assert_eq!(got.waveform.as_deref(), Some("AQID"));

    // A forward keeps its snapshot through the database.
    let src = svc
        .send_message(
            owner,
            general,
            SendMessage {
                content: "source".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let snapshot = ForwardSnapshot {
        source_guild_id: Some(guild.get()),
        content: "source".into(),
        timestamp_ms: 1234,
        ..ForwardSnapshot::default()
    };
    let fwd = svc
        .send_message(
            owner,
            general,
            SendMessage {
                forward: Some(ForwardRequest {
                    source_message: src.id,
                    source_channel: general,
                    snapshot: snapshot.clone(),
                }),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let back = repo.get_message(general, fwd.id).await.unwrap().unwrap();
    let r = back.reference.unwrap();
    assert_eq!(r.message_id, src.id);
    assert_eq!(r.forward, Some(snapshot));
    // A plain reply has no snapshot.
    let reply = svc
        .send_message(
            owner,
            general,
            SendMessage {
                content: "re".into(),
                reply_to: Some(src.id),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let back = repo.get_message(general, reply.id).await.unwrap().unwrap();
    assert!(back.reference.unwrap().forward.is_none());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn webhook_author_overrides_round_trip() {
    use sr_chat::rich_repo::WebhookAuthor;
    let pool = pool().await;
    let store = PgRichStore::new(pool);
    let (a, b, none) = (
        sr_core::MessageId::new(900_001),
        sr_core::MessageId::new(900_002),
        sr_core::MessageId::new(900_003),
    );
    let first = WebhookAuthor {
        webhook_id: 77,
        username: "Deploys".into(),
        avatar_url: None,
    };
    store.set_webhook_author(a, &first).await.unwrap();
    store
        .set_webhook_author(
            b,
            &WebhookAuthor {
                webhook_id: 77,
                username: "Build Bot".into(),
                avatar_url: Some("https://img.example/a.png".into()),
            },
        )
        .await
        .unwrap();
    let got = store.webhook_authors(&[a, b, none]).await.unwrap();
    assert_eq!(got.len(), 2, "messages without an override are omitted");
    assert_eq!(got[&a], first);
    assert_eq!(
        got[&b].avatar_url.as_deref(),
        Some("https://img.example/a.png")
    );

    // A second write for the same message replaces the first.
    store
        .set_webhook_author(
            a,
            &WebhookAuthor {
                webhook_id: 77,
                username: "Renamed".into(),
                avatar_url: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        store.webhook_authors(&[a]).await.unwrap()[&a].username,
        "Renamed"
    );
    assert!(store.webhook_authors(&[]).await.unwrap().is_empty());
}

/// BRG-GW-001: the Discord author, avatar and attachment links of a bridged message round-trip, a second write
/// replaces the first, and messages that were not bridged are omitted.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn bridge_author_round_trips() {
    use sr_chat::rich_repo::{BridgeAuthor, BridgeLink};
    let pool = pool().await;
    let store = PgRichStore::new(pool);
    let (a, none) = (
        sr_core::MessageId::new(900_101),
        sr_core::MessageId::new(900_102),
    );
    let first = BridgeAuthor {
        platform: "discord".into(),
        author_name: "Dana".into(),
        author_avatar: Some("https://cdn.discordapp.com/avatars/7/abc.png".into()),
        discord_user_id: Some("7".into()),
        links: vec![BridgeLink {
            filename: "cat.png".into(),
            url: "https://cdn.discordapp.com/attachments/1/2/cat.png?ex=65f00000".into(),
            expires_at_ms: Some(1_710_000_000_000),
        }],
    };
    store.set_bridge_author(a, &first).await.unwrap();
    let got = store.bridge_authors(&[a, none]).await.unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[&a], first);
    let second = BridgeAuthor {
        author_name: "Dana (edited)".into(),
        links: Vec::new(),
        ..first
    };
    store.set_bridge_author(a, &second).await.unwrap();
    assert_eq!(store.bridge_authors(&[a]).await.unwrap()[&a], second);
    assert!(store.bridge_authors(&[]).await.unwrap().is_empty());
}

/// D-237: `decorate` asks which ids are application emoji and which application a bot user runs.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn application_emoji_owners_and_bot_applications() {
    let pool = pool().await;
    let ids = SnowflakeGenerator::new(Arc::new(SystemClock) as Arc<dyn Clock>, 9, 9).unwrap();
    let owner = account(&pool, &ids).await;
    let bot = account(&pool, &ids).await;
    let human = account(&pool, &ids).await;
    let app = ids.generate().get() as i64;
    let emoji = ids.generate().get() as i64;
    sqlx::query(
        "INSERT INTO applications (id, owner_id, bot_user_id, name, client_secret_hash) \
         VALUES ($1, $2, $3, 'Helper', decode(repeat('ab', 32), 'hex'))",
    )
    .bind(app)
    .bind(owner.get() as i64)
    .bind(bot.get() as i64)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO application_emojis (id, application_id, name, media_key) \
         VALUES ($1, $2, 'wave', 'emoji/app/x.png')",
    )
    .bind(emoji)
    .bind(app)
    .execute(&pool)
    .await
    .unwrap();

    let store = PgRichStore::new(pool);
    let owners = store
        .app_emoji_owners(&[emoji as u64, 12_345])
        .await
        .unwrap();
    assert_eq!(owners.len(), 1);
    assert_eq!(owners[&(emoji as u64)], app);
    assert!(store.app_emoji_owners(&[]).await.unwrap().is_empty());

    let bots = store.bot_applications(&[bot, human]).await.unwrap();
    assert_eq!(bots.len(), 1, "a person is not a bot");
    assert_eq!(bots[&bot], app);
}
