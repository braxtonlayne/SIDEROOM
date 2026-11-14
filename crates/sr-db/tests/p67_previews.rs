//! Postgres-backed tests for the two previews (D-688 the message request, D-689 the thread row): the
//! latest not-deleted message of several channels, read at once. Run with a scratch Postgres:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test p67_previews -- --ignored`

use std::sync::Arc;

use sr_chat::repo::MessageStore;
use sr_chat::social::repo::DmRepository;
use sr_core::{ChannelId, Clock, SnowflakeGenerator, SystemClock, UserId};
use sr_db::PgPool;
use sr_db::chat_repo::PgMessageStore;
use sr_db::social_repo::PgSocialRepo;
use sr_db::testing::TestDb;

type Ids = SnowflakeGenerator<Arc<dyn Clock>>;

fn ids(worker: u8) -> Ids {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    SnowflakeGenerator::new(clock, worker, worker).expect("ids")
}

async fn account(pool: &PgPool, ids: &Ids) -> UserId {
    let person = ids.generate().get() as i64;
    let acct = ids.generate();
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(person)
        .execute(pool)
        .await
        .unwrap();
    let name = format!("u{}", acct.get());
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $2, $3, $4, 'x')",
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

/// One message in `channel`; `kind` 0 is a person's, others are system lines; `deleted` hides it.
struct Msg<'a> {
    channel: ChannelId,
    author: UserId,
    content: &'a str,
    kind: i16,
    deleted: bool,
    files: usize,
}

fn said(channel: ChannelId, author: UserId, content: &str) -> Msg<'_> {
    Msg {
        channel,
        author,
        content,
        kind: 0,
        deleted: false,
        files: 0,
    }
}

async fn insert(pool: &PgPool, ids: &Ids, m: Msg<'_>) -> i64 {
    let id = ids.generate().get() as i64;
    // Snowflake ids carry the time: a short pause keeps the order of two messages in one test.
    tokio::time::sleep(std::time::Duration::from_millis(3)).await;
    sqlx::query(
        "INSERT INTO messages (id, channel_id, author_id, content, type, visibility, deleted_at) \
         VALUES ($1, $2, $3, $4, $5, \
                 CASE WHEN $6 THEN 'user_deleted'::message_visibility ELSE 'visible'::message_visibility END, \
                 CASE WHEN $6 THEN now() END)",
    )
    .bind(id)
    .bind(m.channel.get() as i64)
    .bind(m.author.get() as i64)
    .bind(m.content)
    .bind(m.kind)
    .bind(m.deleted)
    .execute(pool)
    .await
    .unwrap();
    for n in 0..m.files {
        sqlx::query(
            "INSERT INTO attachments (id, message_id, uploader_id, filename, size_bytes, storage_key, state) \
             VALUES ($1, $2, $3, $4, 1, 'att/x', 'ready')",
        )
        .bind(ids.generate().get() as i64)
        .bind(id)
        .bind(m.author.get() as i64)
        .bind(format!("secret-name-{n}.png"))
        .execute(pool)
        .await
        .unwrap();
    }
    id
}

async fn dm(repo: &PgSocialRepo, ids: &Ids, a: UserId, b: UserId) -> ChannelId {
    let ch = ChannelId::new(ids.generate().get());
    repo.insert_dm(ch, a, b).await.unwrap();
    ch
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_pending_request_carries_the_latest_message_as_a_preview() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(11);
    let alice = account(&pool, &test_ids).await;
    let bob = account(&pool, &test_ids).await;
    let repo = PgSocialRepo::new(pool.clone());
    let ch = dm(&repo, &test_ids, alice, bob).await;
    insert(&pool, &test_ids, said(ch, alice, "first")).await;
    let last = insert(&pool, &test_ids, said(ch, alice, "the latest words")).await;
    // The recipient writing back, and a system line after it, are not the sender's latest words.
    insert(&pool, &test_ids, said(ch, bob, "my own reply")).await;
    insert(
        &pool,
        &test_ids,
        Msg {
            kind: 3,
            ..said(ch, alice, "call")
        },
    )
    .await;

    let got = repo.latest_from_others(bob, &[ch]).await.unwrap();
    let g = got.get(&ch).expect("a preview");
    assert_eq!(g.id.get() as i64, last);
    assert_eq!(g.content, "the latest words");
    assert_eq!((g.author_id, g.attachments), (alice, 0));
    // Asked for the other side, the same query reads the other person's message.
    let mine = repo.latest_from_others(alice, &[ch]).await.unwrap();
    assert_eq!(mine.get(&ch).unwrap().content, "my own reply");
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_deleted_message_is_not_previewed() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(12);
    let alice = account(&pool, &test_ids).await;
    let bob = account(&pool, &test_ids).await;
    let carol = account(&pool, &test_ids).await;
    let repo = PgSocialRepo::new(pool.clone());
    let ch = dm(&repo, &test_ids, alice, bob).await;
    let kept = insert(&pool, &test_ids, said(ch, alice, "still here")).await;
    insert(
        &pool,
        &test_ids,
        Msg {
            deleted: true,
            ..said(ch, alice, "taken back")
        },
    )
    .await;
    let got = repo.latest_from_others(bob, &[ch]).await.unwrap();
    assert_eq!(got.get(&ch).unwrap().id.get() as i64, kept);
    // Only a deleted message: no preview at all.
    let other = dm(&repo, &test_ids, alice, carol).await;
    insert(
        &pool,
        &test_ids,
        Msg {
            deleted: true,
            ..said(other, alice, "gone")
        },
    )
    .await;
    let got = repo.latest_from_others(bob, &[ch, other]).await.unwrap();
    assert!(got.contains_key(&ch) && !got.contains_key(&other));
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn an_attachment_only_message_gives_a_count() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(13);
    let alice = account(&pool, &test_ids).await;
    let bob = account(&pool, &test_ids).await;
    let repo = PgSocialRepo::new(pool.clone());
    let ch = dm(&repo, &test_ids, alice, bob).await;
    insert(
        &pool,
        &test_ids,
        Msg {
            files: 3,
            ..said(ch, alice, "")
        },
    )
    .await;
    let got = repo.latest_from_others(bob, &[ch]).await.unwrap();
    let g = got.get(&ch).unwrap();
    assert_eq!((g.content.as_str(), g.attachments), ("", 3));
    db.cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_thread_row_reads_the_latest_not_deleted_message_of_each_thread() {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let test_ids = ids(14);
    let alice = account(&pool, &test_ids).await;
    let bob = account(&pool, &test_ids).await;
    let carol = account(&pool, &test_ids).await;
    let repo = PgSocialRepo::new(pool.clone());
    // Three channels stand in for three threads (the query only knows channel ids).
    let with = dm(&repo, &test_ids, alice, bob).await;
    let fallback = dm(&repo, &test_ids, alice, carol).await;
    let empty = dm(&repo, &test_ids, bob, carol).await;
    insert(&pool, &test_ids, said(with, alice, "one")).await;
    let newest = insert(
        &pool,
        &test_ids,
        Msg {
            files: 1,
            ..said(with, bob, "two")
        },
    )
    .await;
    let before = insert(&pool, &test_ids, said(fallback, alice, "before")).await;
    insert(
        &pool,
        &test_ids,
        Msg {
            deleted: true,
            ..said(fallback, carol, "deleted last")
        },
    )
    .await;

    let store = PgMessageStore::new(pool.clone());
    let got = store.last_messages(&[with, fallback, empty]).await.unwrap();
    // A thread without messages carries none.
    assert!(!got.contains_key(&empty));
    // A deleted last message falls back to the one before.
    assert_eq!(got[&fallback].id.get() as i64, before);
    assert_eq!(got[&fallback].content, "before");
    assert_eq!(got[&with].id.get() as i64, newest);
    assert_eq!((got[&with].author_id, got[&with].attachments), (bob, 1));
    db.cleanup().await.unwrap();
}
