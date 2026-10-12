//! Postgres-backed thread and forum tests. Run with a scratch database:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test thread_repo -- --ignored --test-threads=1`

use std::sync::Arc;

use sr_chat::channel_rules::NewChannel;
use sr_chat::domain::{ChannelType, MessageType};
use sr_chat::error::ChatError;
use sr_chat::threads::model::{TagInput, channel_flags};
use sr_chat::threads::repo::ArchivedFilter;
use sr_chat::threads::{ForumPost, StartThread, ThreadPatch, ThreadService};
use sr_chat::{ChatLimits, ChatService, SendMessage};
use sr_core::{ChannelId, Clock, GuildId, ManualClock, SnowflakeGenerator, SystemClock, UserId};
use sr_db::chat_repo::PgChatRepo;
use sr_db::{DbConfig, PgPool};

const MIN: i64 = 60_000;

async fn pool() -> PgPool {
    let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
    let pool = sr_db::connect(&DbConfig::new(url)).await.expect("connect");
    sr_db::migrate(&pool).await.expect("migrate");
    pool
}

struct Fx {
    pool: PgPool,
    chat: ChatService<PgChatRepo>,
    threads: ThreadService<PgChatRepo>,
    clock: Arc<ManualClock>,
    owner: UserId,
    bob: UserId,
    carol: UserId,
    guild: GuildId,
    general: ChannelId,
}

/// Insert a person + account directly (accounts are owned by the identity crates).
async fn account(pool: &PgPool, ids: &SnowflakeGenerator<Arc<dyn Clock>>) -> UserId {
    let person = ids.generate().get() as i64;
    let acct = ids.generate();
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(person)
        .execute(pool)
        .await
        .unwrap();
    let name = format!("t{}", acct.get());
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

async fn join(pool: &PgPool, guild: GuildId, user: UserId) {
    sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
        .bind(guild.get() as i64)
        .bind(user.get() as i64)
        .execute(pool)
        .await
        .unwrap();
}

async fn fx() -> Fx {
    let pool = pool().await;
    let sys: Arc<dyn Clock> = Arc::new(SystemClock);
    let test_ids = SnowflakeGenerator::new(sys.clone(), 6, 6).unwrap();
    let now = sys.now_unix_ms();
    let clock = Arc::new(ManualClock::new(now));
    let dyn_clock: Arc<dyn Clock> = clock.clone();
    let chat = ChatService::new(
        PgChatRepo::new(pool.clone()),
        dyn_clock.clone(),
        SnowflakeGenerator::new(sys.clone(), 6, 7).unwrap(),
        ChatLimits::default(),
    );
    let threads = ThreadService::new(
        PgChatRepo::new(pool.clone()),
        dyn_clock,
        SnowflakeGenerator::new(sys, 6, 8).unwrap(),
    );
    let owner = account(&pool, &test_ids).await;
    let bob = account(&pool, &test_ids).await;
    let carol = account(&pool, &test_ids).await;
    let bp = chat.create_guild(owner, "Threads").await.unwrap();
    join(&pool, bp.guild.id, bob).await;
    join(&pool, bp.guild.id, carol).await;
    Fx {
        pool,
        chat,
        threads,
        clock,
        owner,
        bob,
        carol,
        guild: bp.guild.id,
        general: bp.channels[2].id,
    }
}

fn say(s: &str) -> SendMessage {
    SendMessage {
        content: s.into(),
        ..Default::default()
    }
}

fn start(name: &str) -> StartThread {
    StartThread {
        name: name.into(),
        ..StartThread::default()
    }
}

async fn count(pool: &PgPool, sql: &'static str, id: ChannelId) -> i64 {
    sqlx::query_scalar(sql)
        .bind(id.get() as i64)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn thread_lifecycle_round_trips_through_postgres() {
    let f = fx().await;
    let m = f
        .chat
        .send_message(f.bob, f.general, say("plan"))
        .await
        .unwrap();
    let out = f
        .threads
        .start_from_message(f.carol, f.general, m.id, start("Planning"))
        .await
        .unwrap();
    let tid = out.view.channel.id;
    assert_eq!(tid.get(), m.id.get());
    let info = out.view.channel.thread.clone().unwrap();
    assert_eq!(info.member_count, 1);
    assert!(!info.archived);
    assert_eq!(info.auto_archive_duration, 1440);
    assert!(
        out.messages
            .iter()
            .any(|m| m.kind == MessageType::ThreadCreated && m.channel_id == f.general)
    );
    assert!(
        out.messages
            .iter()
            .any(|m| m.kind == MessageType::ThreadStarterMessage && m.channel_id == tid)
    );
    assert!(matches!(
        f.threads
            .start_from_message(f.bob, f.general, m.id, start("again"))
            .await,
        Err(ChatError::ThreadAlreadyCreated)
    ));

    // Join, post, counters and membership.
    f.threads.add_member(f.bob, tid, f.bob).await.unwrap();
    f.clock.advance_ms(MIN);
    let a = f.chat.send_message(f.bob, tid, say("one")).await.unwrap();
    let act = f.threads.record_message(&a).await.unwrap().unwrap();
    assert!(act.joined.is_none(), "already a member");
    let b = f.chat.send_message(f.owner, tid, say("two")).await.unwrap();
    let act = f.threads.record_message(&b).await.unwrap().unwrap();
    assert!(act.joined.is_some());
    let t = act.thread.thread.unwrap();
    assert_eq!((t.message_count, t.member_count), (2, 3));
    assert!(f.threads.record_message(&m).await.unwrap().is_none());

    // Deleting a message lowers the count; the total stays.
    f.chat.delete_message(f.bob, tid, a.id).await.unwrap();
    let v = f.threads.view(f.bob, tid).await.unwrap();
    let t = v.channel.thread.unwrap();
    assert_eq!((t.message_count, t.total_message_sent), (1, 2));
    assert_eq!(v.channel.last_message_id, Some(b.id));

    // Deleting the starter message keeps the thread.
    f.chat.delete_message(f.bob, f.general, m.id).await.unwrap();
    assert!(f.threads.view(f.carol, tid).await.is_ok());

    // Not a guild channel.
    let listed = f.chat.list_channels(f.owner, f.guild).await.unwrap();
    assert!(listed.iter().all(|c| !c.channel.kind.is_thread()));

    // Archive, list, unarchive by message.
    f.threads
        .modify(
            f.carol,
            tid,
            ThreadPatch {
                archived: Some(true),
                ..ThreadPatch::default()
            },
        )
        .await
        .unwrap();
    assert!(
        f.threads
            .list_active(f.bob, f.guild)
            .await
            .unwrap()
            .threads
            .is_empty()
    );
    let arch = f
        .threads
        .list_archived(f.bob, f.general, ArchivedFilter::Public, None, 10)
        .await
        .unwrap();
    assert_eq!(arch.threads.len(), 1);
    assert_eq!(arch.members.len(), 1);
    f.clock.advance_ms(MIN);
    let c = f.chat.send_message(f.bob, tid, say("back")).await.unwrap();
    let act = f.threads.record_message(&c).await.unwrap().unwrap();
    assert!(act.unarchived);
    assert!(!act.thread.thread.unwrap().archived);

    // Leaving drops the membership.
    let left = f.threads.remove_member(f.bob, tid, f.bob).await.unwrap();
    assert!(left.changed);
    assert_eq!(left.thread.thread.unwrap().member_count, 2);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn private_threads_hide_from_non_members_in_postgres() {
    let f = fx().await;
    sqlx::query("UPDATE roles SET permissions = permissions | $2 WHERE id = $1")
        .bind(f.guild.get() as i64)
        .bind(sr_core::permissions::Permissions::CREATE_PRIVATE_THREADS.bits() as i64)
        .execute(&f.pool)
        .await
        .unwrap();
    let mut input = start("secret");
    input.kind = Some(ChannelType::PrivateThread);
    input.invitable = Some(true);
    let tid = f
        .threads
        .start_standalone(f.bob, f.general, input)
        .await
        .unwrap()
        .view
        .channel
        .id;
    assert!(matches!(
        f.threads.view(f.carol, tid).await,
        Err(ChatError::MissingAccess)
    ));
    assert!(matches!(
        f.chat.send_message(f.carol, tid, say("x")).await,
        Err(ChatError::MissingAccess)
    ));
    assert!(
        f.threads
            .list_active(f.carol, f.guild)
            .await
            .unwrap()
            .threads
            .is_empty()
    );
    f.threads.add_member(f.bob, tid, f.carol).await.unwrap();
    assert!(f.chat.send_message(f.carol, tid, say("hi")).await.is_ok());
    assert_eq!(
        f.threads
            .list_active(f.carol, f.guild)
            .await
            .unwrap()
            .threads
            .len(),
        1
    );
    let viewers = f
        .threads
        .viewers(&f.threads.view(f.bob, tid).await.unwrap().channel)
        .await
        .unwrap();
    assert!(viewers.contains(&f.owner) && viewers.contains(&f.bob) && viewers.contains(&f.carol));
    f.threads
        .remove_member(f.owner, tid, f.carol)
        .await
        .unwrap();
    assert!(f.threads.view(f.carol, tid).await.is_err());
    // Archived private threads: the joined listing and the moderator listing.
    f.threads
        .modify(
            f.bob,
            tid,
            ThreadPatch {
                archived: Some(true),
                ..ThreadPatch::default()
            },
        )
        .await
        .unwrap();
    let joined = f
        .threads
        .list_archived(
            f.bob,
            f.general,
            ArchivedFilter::JoinedPrivate(f.bob),
            None,
            10,
        )
        .await
        .unwrap();
    assert_eq!(joined.threads.len(), 1);
    let none = f
        .threads
        .list_archived(
            f.carol,
            f.general,
            ArchivedFilter::JoinedPrivate(f.carol),
            None,
            10,
        )
        .await
        .unwrap();
    assert!(none.threads.is_empty());
    assert!(
        f.threads
            .list_archived(f.owner, f.general, ArchivedFilter::Private, None, 10)
            .await
            .unwrap()
            .threads
            .len()
            == 1
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn auto_archive_job_query_and_pinned_exemption() {
    let f = fx().await;
    let m = f
        .chat
        .send_message(f.bob, f.general, say("a"))
        .await
        .unwrap();
    let mut input = start("short");
    input.auto_archive_duration = Some(60);
    let tid = f
        .threads
        .start_from_message(f.bob, f.general, m.id, input)
        .await
        .unwrap()
        .view
        .channel
        .id;
    f.clock.advance_ms(59 * MIN);
    assert!(
        !f.threads
            .archive_idle(1000)
            .await
            .unwrap()
            .iter()
            .any(|c| c.id == tid)
    );
    // Activity restarts the clock.
    let msg = f.chat.send_message(f.bob, tid, say("hi")).await.unwrap();
    f.threads.record_message(&msg).await.unwrap();
    f.clock.advance_ms(30 * MIN);
    assert!(
        !f.threads
            .archive_idle(1000)
            .await
            .unwrap()
            .iter()
            .any(|c| c.id == tid)
    );
    f.clock.advance_ms(31 * MIN);
    let done = f.threads.archive_idle(1000).await.unwrap();
    let c = done.iter().find(|c| c.id == tid).expect("archived");
    assert!(c.thread.as_ref().unwrap().archived);
    // Idempotent.
    assert!(
        !f.threads
            .archive_idle(1000)
            .await
            .unwrap()
            .iter()
            .any(|c| c.id == tid)
    );
}

fn tag(name: &str, moderated: bool) -> TagInput {
    TagInput {
        id: None,
        name: name.into(),
        moderated,
        emoji_id: None,
        emoji_name: Some("x".into()),
    }
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn forum_posts_tags_pins_and_cascade() {
    let f = fx().await;
    let mut input = NewChannel::new(ChannelType::Forum, "help");
    input.topic = Some("Read the rules first".into());
    input.forum.available_tags = Some(vec![tag("question", false), tag("official", true)]);
    input.forum.flags = Some(channel_flags::REQUIRE_TAG);
    input.forum.default_sort_order = Some(Some(1));
    input.forum.default_forum_layout = Some(2);
    input.forum.default_thread_rate_limit = Some(5);
    let forum = f
        .chat
        .create_channel(f.owner, f.guild, input)
        .await
        .unwrap();
    let fid = forum.id;
    let stored = f.chat.get_channel_view(f.owner, fid).await.unwrap().channel;
    assert_eq!(stored.forum.available_tags.len(), 2);
    assert_eq!(stored.forum.default_sort_order, Some(1));
    assert_eq!(stored.forum.default_forum_layout, Some(2));
    assert_eq!(
        stored.flags & channel_flags::REQUIRE_TAG,
        channel_flags::REQUIRE_TAG
    );
    let tags: Vec<u64> = stored.forum.available_tags.iter().map(|t| t.id).collect();

    let post = |tags: Vec<u64>| ForumPost {
        name: "How?".into(),
        applied_tags: tags,
        ..ForumPost::default()
    };
    assert!(matches!(
        f.threads.start_forum_post(f.bob, fid, post(vec![])).await,
        Err(ChatError::TagRequired)
    ));
    assert!(matches!(
        f.threads
            .start_forum_post(f.bob, fid, post(vec![tags[1]]))
            .await,
        Err(ChatError::MissingPermissions)
    ));
    let v = f
        .threads
        .start_forum_post(f.bob, fid, post(vec![tags[0]]))
        .await
        .unwrap();
    let pid = v.channel.id;
    assert_eq!(
        v.channel.thread.as_ref().unwrap().applied_tags,
        vec![tags[0]]
    );
    assert_eq!(v.channel.rate_limit_per_user, 5);
    let first = f
        .chat
        .send_message(f.bob, pid, say("the question"))
        .await
        .unwrap();
    f.threads.record_message(&first).await.unwrap();

    // Pin it (moderator), then the forum drops the tag from its list.
    let pinned = f
        .threads
        .modify(
            f.owner,
            pid,
            ThreadPatch {
                flags: Some(channel_flags::PINNED),
                ..ThreadPatch::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        pinned.channel.flags & channel_flags::PINNED,
        channel_flags::PINNED
    );
    let mut patch = sr_chat::channel_rules::ChannelPatch::default();
    patch.forum.available_tags = Some(vec![TagInput {
        id: Some(tags[1]),
        name: "official".into(),
        moderated: true,
        emoji_id: None,
        emoji_name: None,
    }]);
    f.chat
        .edit_channel(
            f.owner,
            fid,
            patch,
            sr_chat::admin_rules::ChannelMove::default(),
        )
        .await
        .unwrap();
    let after = f.threads.view(f.bob, pid).await.unwrap().channel;
    assert!(
        after.thread.unwrap().applied_tags.is_empty(),
        "stale tag removed"
    );

    // Pinned posts never auto-archive.
    f.clock.advance_ms(30 * 24 * 60 * MIN);
    assert!(
        !f.threads
            .archive_idle(1000)
            .await
            .unwrap()
            .iter()
            .any(|c| c.id == pid)
    );

    // Deleting the forum deletes its posts, their members and their messages.
    assert_eq!(
        count(
            &f.pool,
            "SELECT count(*) FROM thread_members WHERE channel_id = $1",
            pid
        )
        .await,
        1
    );
    f.chat.delete_channel(f.owner, fid).await.unwrap();
    assert_eq!(
        count(&f.pool, "SELECT count(*) FROM channels WHERE id = $1", pid).await,
        0
    );
    assert_eq!(
        count(
            &f.pool,
            "SELECT count(*) FROM thread_metadata WHERE channel_id = $1",
            pid
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &f.pool,
            "SELECT count(*) FROM thread_members WHERE channel_id = $1",
            pid
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &f.pool,
            "SELECT count(*) FROM messages WHERE channel_id = $1",
            pid
        )
        .await,
        0
    );
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn deleting_a_text_channel_deletes_its_threads() {
    let f = fx().await;
    let created = f
        .chat
        .create_channel(
            f.owner,
            f.guild,
            NewChannel::new(ChannelType::Text, "scratch"),
        )
        .await
        .unwrap();
    let m = f
        .chat
        .send_message(f.owner, created.id, say("x"))
        .await
        .unwrap();
    let tid = f
        .threads
        .start_from_message(f.owner, created.id, m.id, start("t"))
        .await
        .unwrap()
        .view
        .channel
        .id;
    f.chat.delete_channel(f.owner, created.id).await.unwrap();
    assert_eq!(
        count(&f.pool, "SELECT count(*) FROM channels WHERE id = $1", tid).await,
        0
    );
}
