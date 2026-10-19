//! Postgres-backed message store tests (reactions, nonces, pins, attachments).
//! Run with a scratch database:
//! `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test message_store -- --ignored`

use std::sync::Arc;

use sr_chat::repo::{EmojiKey, MessageStore, Page};
use sr_chat::{ChatLimits, ChatService, SendMessage};
use sr_core::{Clock, SnowflakeGenerator, SystemClock, UserId};
use sr_db::chat_repo::{PgChatRepo, PgMessageStore};
use sr_db::{DbConfig, PgPool};

async fn pool() -> PgPool {
    let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
    let pool = sr_db::connect(&DbConfig::new(url)).await.expect("connect");
    sr_db::migrate(&pool).await.expect("migrate");
    pool
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
    let name = format!("m{}", acct.get());
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

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn reactions_nonces_and_pins_round_trip() {
    let pool = pool().await;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids = SnowflakeGenerator::new(clock.clone(), 7, 7).expect("ids");
    let svc = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock.clone(),
        SnowflakeGenerator::new(clock.clone(), 8, 8).expect("ids"),
        ChatLimits::default(),
    );
    let store = PgMessageStore::new(pool.clone());
    let owner = account(&pool, &ids).await;
    let bp = svc.create_guild(owner, "Store Club").await.unwrap();
    let general = bp.channels[2].id;
    let m = svc
        .send_message(
            owner,
            general,
            SendMessage {
                content: "react to me".into(),
                nonce: Some("abc".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    store
        .put_nonce(general, owner, "abc", m.id, 0)
        .await
        .unwrap();
    let since = clock.now_unix_ms() - 60_000;
    assert_eq!(
        store
            .find_nonce(general, owner, "abc", since)
            .await
            .unwrap(),
        Some(m.id)
    );
    assert_eq!(store.nonce_of(m.id).await.unwrap().as_deref(), Some("abc"));

    let thumbs = EmojiKey::parse("\u{1F44D}").unwrap();
    assert!(
        store
            .add_reaction(general, m.id, owner, &thumbs, clock.now_unix_ms())
            .await
            .unwrap()
    );
    let rows = store.reactions_for(&[m.id], owner).await.unwrap();
    assert_eq!(rows[&m.id][0].count, 1);
    assert!(rows[&m.id][0].me);
    assert!(store.remove_reaction(m.id, owner, &thumbs).await.unwrap());

    let page = store
        .fetch(
            general,
            Page {
                before: None,
                after: None,
                limit: 10,
                ascending: false,
            },
        )
        .await
        .unwrap();
    assert!(page.iter().any(|x| x.id == m.id));
}

/// The EXIF/metadata strip rewrites an uploaded image to fewer bytes, then marks the upload ready.
/// The database must keep the new size, not the size originally declared (found by the live-S3
/// driver: `update_attachment` used to leave `size_bytes` untouched).
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn update_attachment_persists_the_new_size() {
    use sr_chat::repo::{Attachment, UploadState};
    use sr_core::AttachmentId;

    let pool = pool().await;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids = SnowflakeGenerator::new(clock.clone(), 9, 9).expect("ids");
    let svc = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock.clone(),
        SnowflakeGenerator::new(clock.clone(), 10, 10).expect("ids"),
        ChatLimits::default(),
    );
    let store = PgMessageStore::new(pool.clone());
    let owner = account(&pool, &ids).await;
    let bp = svc.create_guild(owner, "Size Club").await.unwrap();
    let general = bp.channels[2].id;

    let mut att = Attachment {
        id: AttachmentId::new(ids.generate().get()),
        channel_id: general,
        uploader: owner,
        filename: "photo.jpg".into(),
        size: 1000,
        key: format!("chat/{general}/size-test/photo.jpg"),
        declared_content_type: Some("image/jpeg".into()),
        content_type: None,
        description: None,
        spoiler: false,
        state: UploadState::Pending,
        message_id: None,
        expires_at_ms: None,
        created_at_ms: clock.now_unix_ms(),
        duration_ms: None,
        waveform: None,
        width: Some(640),
        height: Some(480),
        blurhash: Some("LEHV6nWB2yk8pyo0adR*.7kCMdnj".into()),
    };
    store.insert_attachment(&att).await.unwrap();

    att.size = 400;
    att.content_type = Some("image/jpeg".into());
    att.state = UploadState::Ready;
    store.update_attachment(&att).await.unwrap();

    let got = store.get_attachments(&[att.id]).await.unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].size, 400, "the stripped size must be stored");
    assert_eq!(got[0].content_type.as_deref(), Some("image/jpeg"));
    assert_eq!(got[0].state, UploadState::Ready);
    // The client-declared picture size and BlurHash survive the update.
    assert_eq!((got[0].width, got[0].height), (Some(640), Some(480)));
    assert_eq!(
        got[0].blurhash.as_deref(),
        Some("LEHV6nWB2yk8pyo0adR*.7kCMdnj")
    );

    // Multipart upload record of a large slot (LNG-CMP-007): set, read, replaced, cleared, and
    // listed for the abandoned-upload sweep.
    assert_eq!(store.attachment_multipart(att.id).await.unwrap(), None);
    store
        .set_attachment_multipart(att.id, "mp-1", 64 << 20)
        .await
        .unwrap();
    store
        .set_attachment_multipart(att.id, "mp-2", 64 << 20)
        .await
        .unwrap();
    assert_eq!(
        store.attachment_multipart(att.id).await.unwrap(),
        Some(("mp-2".to_owned(), 64 << 20))
    );
    let listed = sr_db::chat_repo::attachment_multipart_ids(&pool, &[att.id])
        .await
        .unwrap();
    assert_eq!(listed, vec![(att.id, "mp-2".to_owned())]);
    store.clear_attachment_multipart(att.id).await.unwrap();
    assert_eq!(store.attachment_multipart(att.id).await.unwrap(), None);
}
