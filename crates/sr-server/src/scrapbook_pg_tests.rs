//! The scrapbook vault against real Postgres (PRF-SCR-001; run with `SR_TEST_DATABASE_URL`, `--include-ignored`,
//! `--test-threads=1`): the module's trigger through the request job, the service account's confinement to the
//! server's own shelf and its `upload` grant, the delete cascade, **a bucket that refuses the delete being
//! visible to the managers and retried**, the kill switch, erasure and export.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use serde_json::json;
use sqlx::PgPool;
use sr_chat::repo::MessageStore;
use sr_chat::{ChatLimits, ChatService};
use sr_core::{ChannelId, Clock, GuildId, SnowflakeGenerator, SystemClock, UserId};
use sr_db::chat_repo::{PgChatRepo, PgMessageStore};
use sr_db::mounts_repo::PgMountRepo;
use sr_db::scrapbook_repo::PgScrapbook;
use sr_db::sealed::SecretBox;
use sr_db::testing::TestDb;
use sr_events::{DomainEvent, EventBus, InProcessBus};
use sr_features::actions::{AttachmentInfo, ChannelInfo, MessageInfo};
use sr_features::modules::scrapbook::{SCRAPBOOK_ID, Scrapbook, request_key};
use sr_features::state::{InMemoryState, StateEntry};
use sr_features::testing::FakeActions;
use sr_features::{Feature, FeatureCtx, FeatureRegistry, FeatureService, InMemorySettingsStore, ScopeRef};
use sr_storage::mount_storage::MountStorages;
use sr_storage::mounts::{
    INSTANCE_MOUNT_ID, MountInput, MountOwner, MountRepo, NewMount, Provider, Secret,
};
use sr_storage::scopes::Grant;
use sr_storage::scrapbook::{
    Archiver, DeleteScope, Refusal, ScrapbookBuckets, ScrapbookStore, Target,
};
use sr_storage::storage::{MultipartSession, ObjectInfo, ObjectStream, PresignMethod};
use sr_storage::{ObjectStorage, ObjectStoreStorage};

use crate::lag_bus::LagTapBus;
use crate::scrapbook_jobs::{PgScrapbookBuckets, Worker, spawn_cascade_consumer};

const TRIGGER: &str = "\u{1f4cc}";

/// An in-memory bucket whose deletes can be made to fail.
struct Flaky {
    inner: ObjectStoreStorage,
    fail_delete: AtomicBool,
}

impl Flaky {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: ObjectStoreStorage::in_memory(),
            fail_delete: AtomicBool::new(false),
        })
    }
}

#[async_trait]
impl ObjectStorage for Flaky {
    async fn put(&self, key: &str, data: Bytes) -> sr_storage::Result<()> {
        self.inner.put(key, data).await
    }
    async fn get(&self, key: &str) -> sr_storage::Result<Bytes> {
        self.inner.get(key).await
    }
    async fn get_range(&self, key: &str, s: u64, e: u64) -> sr_storage::Result<Bytes> {
        self.inner.get_range(key, s, e).await
    }
    async fn get_stream(
        &self,
        key: &str,
        range: Option<std::ops::Range<u64>>,
    ) -> sr_storage::Result<ObjectStream> {
        self.inner.get_stream(key, range).await
    }
    async fn head(&self, key: &str) -> sr_storage::Result<ObjectInfo> {
        self.inner.head(key).await
    }
    async fn delete(&self, key: &str) -> sr_storage::Result<()> {
        if self.fail_delete.load(Ordering::SeqCst) {
            return Err(sr_storage::StorageError::Backend(
                "403 AccessDenied: the key was revoked".into(),
            ));
        }
        self.inner.delete(key).await
    }
    async fn list(&self, prefix: &str) -> sr_storage::Result<Vec<ObjectInfo>> {
        self.inner.list(prefix).await
    }
    async fn presign(
        &self,
        m: PresignMethod,
        key: &str,
        ttl: Duration,
    ) -> sr_storage::Result<String> {
        self.inner.presign(m, key, ttl).await
    }
    async fn multipart_create(&self, key: &str) -> sr_storage::Result<Box<dyn MultipartSession>> {
        self.inner.multipart_create(key).await
    }
}

struct TestBuckets(Arc<Flaky>);

#[async_trait]
impl ScrapbookBuckets for TestBuckets {
    async fn writer(&self, _t: &Target) -> Result<Arc<dyn ObjectStorage>, Refusal> {
        Ok(self.0.clone())
    }
    async fn remover(&self, _m: i64) -> Result<Arc<dyn ObjectStorage>, String> {
        Ok(self.0.clone())
    }
}

async fn account(pool: &PgPool, id: i64, name: &str) {
    sqlx::query("INSERT INTO persons (id) VALUES ($1)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO accounts (id, person_id, username, email, password_hash) VALUES ($1, $1, $2, $3, 'x')",
    )
    .bind(id)
    .bind(name)
    .bind(format!("{name}@example.test"))
    .execute(pool)
    .await
    .unwrap();
}

struct Fx {
    _db: TestDb,
    pool: PgPool,
    owner: i64,
    member: i64,
    guild: i64,
    channel: i64,
    mount: i64,
    shelf: Arc<Flaky>,
    source: Arc<Flaky>,
    store: Arc<PgScrapbook>,
    archiver: Arc<Archiver>,
    worker: Arc<Worker>,
    features: FeatureService,
    ids: SnowflakeGenerator<Arc<dyn Clock>>,
}

async fn guild_mount(pool: &PgPool, id: i64, guild: i64, label: &str) -> sr_storage::mounts::Mount {
    let repo = PgMountRepo::new(pool.clone());
    let sb = SecretBox::new([9; 32]);
    repo.create(
        NewMount::seal(
            &sb,
            id,
            MountInput {
                owner: MountOwner::Guild(guild),
                label: label.into(),
                provider: Provider::Garage,
                endpoint: "http://127.0.0.1:3900".into(),
                region: "garage".into(),
                bucket: "shelf".into(),
                prefix: String::new(),
                path_style: true,
                access_key_id: "GK0123456789abcd".into(),
            },
            &Secret::new("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"),
        )
        .unwrap(),
        Some(5),
    )
    .await
    .unwrap()
}

async fn fx() -> Fx {
    let db = TestDb::new().await.unwrap();
    let pool = db.pool().clone();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let ids = SnowflakeGenerator::new(clock.clone(), 30, 30).unwrap();
    let owner = ids.generate().get() as i64;
    account(&pool, owner, "owner").await;
    let member = ids.generate().get() as i64;
    account(&pool, member, "ada").await;

    let svc = ChatService::new(
        PgChatRepo::new(pool.clone()),
        clock.clone(),
        SnowflakeGenerator::new(clock.clone(), 31, 31).unwrap(),
        ChatLimits::default(),
    );
    let bp = svc
        .create_guild(UserId::new(owner as u64), "Club")
        .await
        .unwrap();
    let guild = bp.guild.id.get() as i64;
    let channel = bp
        .channels
        .iter()
        .find(|c| c.name.as_deref() == Some("general"))
        .unwrap()
        .id
        .get() as i64;
    sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
        .bind(guild)
        .bind(member)
        .execute(&pool)
        .await
        .unwrap();
    let mount = guild_mount(&pool, 7001, guild, "Club shelf").await.id;

    let shelf = Flaky::new();
    let source = Flaky::new();
    let store = Arc::new(PgScrapbook::new(pool.clone()));
    let next = Arc::new(std::sync::atomic::AtomicI64::new(9_000_000));
    let ids_fn: Arc<dyn Fn() -> i64 + Send + Sync> = {
        let next = next.clone();
        Arc::new(move || next.fetch_add(1, Ordering::SeqCst))
    };
    let archiver = Arc::new(Archiver::new(
        store.clone(),
        Arc::new(TestBuckets(shelf.clone())),
        source.clone(),
        ids_fn,
    ));
    store
        .set_target(&Target {
            guild_id: guild,
            mount_id: mount,
            granted_by: owner,
        })
        .await
        .unwrap();

    // The module registry with the operator's opt-in and the server's own switch.
    let mut reg = FeatureRegistry::new();
    reg.register(Scrapbook).unwrap();
    let features = FeatureService::new(
        Arc::new(reg),
        Arc::new(InMemorySettingsStore::new()),
        None,
    );
    features
        .set_enabled(SCRAPBOOK_ID, &ScopeRef::Platform, Some(true))
        .await
        .unwrap();
    features
        .set_enabled(
            SCRAPBOOK_ID,
            &ScopeRef::Guild {
                guild: GuildId::new(guild as u64),
            },
            Some(true),
        )
        .await
        .unwrap();
    let bus: Arc<dyn EventBus> = Arc::new(InProcessBus::new());
    let ctx = FeatureCtx {
        bus,
        settings: features.clone(),
        clock: clock.clone(),
        actions: Arc::new(FakeActions::new()),
        state: Arc::new(InMemoryState::new()),
    };
    let messages: Arc<dyn MessageStore> = Arc::new(PgMessageStore::new(pool.clone()));
    let worker = Arc::new(Worker {
        archiver: archiver.clone(),
        chat: Arc::new(PgChatRepo::new(pool.clone())),
        messages,
        ctx,
        pool: pool.clone(),
        clock,
    });
    Fx {
        _db: db,
        pool,
        owner,
        member,
        guild,
        channel,
        mount,
        shelf,
        source,
        store,
        archiver,
        worker,
        features,
        ids: SnowflakeGenerator::new(Arc::new(SystemClock) as Arc<dyn Clock>, 26, 26).unwrap(),
    }
}

/// A message with one attachment whose bytes sit in the instance bucket.
async fn message_with_file(f: &Fx, content: &str, filename: &str, bytes: usize) -> i64 {
    // A real snowflake, so the entry's date is plausible. The attachment takes the next number.
    let id = f.ids.generate().get() as i64;
    let att = f.ids.generate().get() as i64;
    sqlx::query(
        "INSERT INTO messages (id, channel_id, guild_id, author_id, content) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(f.channel)
    .bind(f.guild)
    .bind(f.member)
    .bind(content)
    .execute(&f.pool)
    .await
    .unwrap();
    let key = format!("attachments/{id}/{filename}");
    f.source
        .put(&key, Bytes::from(vec![5u8; bytes]))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO attachments (id, message_id, uploader_id, filename, content_type, size_bytes, storage_key, state) \
         VALUES ($1, $2, $3, $4, 'image/png', $5, $6, 'ready')",
    )
    .bind(att)
    .bind(id)
    .bind(f.member)
    .bind(filename)
    .bind(bytes as i64)
    .bind(&key)
    .execute(&f.pool)
    .await
    .unwrap();
    id
}

/// What the module's trigger leaves behind, as the job finds it.
async fn request(f: &Fx, message: i64) {
    f.worker
        .ctx
        .state
        .put(
            StateEntry::new(
                SCRAPBOOK_ID,
                GuildId::new(f.guild as u64),
                request_key(message as u64),
                json!({"channel": f.channel, "via": "reaction", "attempts": 0}),
            )
            .due(0),
        )
        .await
        .unwrap();
}

async fn entry_exists(f: &Fx, message: i64) -> bool {
    f.store
        .by_message(f.guild, message)
        .await
        .unwrap()
        .is_some()
}

async fn wait_until<F, Fut>(what: &str, mut f: F)
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

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_reaction_request_becomes_an_entry_with_its_copy() {
    let f = fx().await;
    let m = message_with_file(&f, "frame this", "cat.png", 20).await;
    request(&f, m).await;
    assert_eq!(f.worker.run_requests().await.unwrap(), 1);
    let x = f.store.by_message(f.guild, m).await.unwrap().unwrap();
    assert_eq!(x.entry.content, "frame this");
    assert_eq!(x.entry.author_id, f.member);
    assert_eq!(x.entry.author_name, "ada");
    assert_eq!(x.objects.len(), 1);
    assert!(x.objects[0].key.starts_with("scrapbook/"), "{}", x.objects[0].key);
    assert!(x.objects[0].key.contains(&format!("/{m}/0-cat.png")));
    assert_eq!(f.shelf.get(&x.objects[0].key).await.unwrap().len(), 20);
    // The request row is gone, and a second run changes nothing.
    assert!(
        f.worker
            .ctx
            .state
            .list(SCRAPBOOK_ID, GuildId::new(f.guild as u64), "req:")
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(f.worker.run_requests().await.unwrap(), 0);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_module_trigger_leaves_a_request_only_at_the_threshold() {
    // The module itself, over a fake platform: threshold and trigger emoji decide.
    let f = fx().await;
    let actions = Arc::new(FakeActions::new());
    let ctx = FeatureCtx {
        actions: actions.clone(),
        ..f.worker.ctx.clone()
    };
    f.features
        .set_settings(
            SCRAPBOOK_ID,
            &ScopeRef::Guild {
                guild: GuildId::new(f.guild as u64),
            },
            json!({"threshold": 2}).as_object().unwrap().clone(),
        )
        .await
        .unwrap();
    let g = GuildId::new(f.guild as u64);
    let ch = ChannelId::new(f.channel as u64);
    actions.put_user(3, "Ada");
    actions.put_channel(ChannelInfo {
        id: ch,
        guild: Some(g),
        kind: 0,
        name: "general".into(),
        nsfw: false,
        parent: None,
        rate_limit_per_user: 0,
        everyone_can_view: true,
    });
    actions.put_message(MessageInfo {
        id: 77,
        channel: ch,
        guild: Some(g),
        author: UserId::new(3),
        content: "x".into(),
        embeds: vec![],
        attachments: vec![AttachmentInfo::default()],
    });
    let ev = |user: u64| DomainEvent::ReactionAdded {
        guild: Some(g),
        channel: ch,
        message_id: 77,
        user: UserId::new(user),
        emoji: json!({"id": null, "name": TRIGGER}),
    };
    actions.react(77, TRIGGER, 1);
    Scrapbook.on_event(&ev(1), &ctx).await.unwrap();
    assert!(ctx.state.list(SCRAPBOOK_ID, g, "req:").await.unwrap().is_empty());
    actions.react(77, TRIGGER, 2);
    Scrapbook.on_event(&ev(2), &ctx).await.unwrap();
    assert_eq!(ctx.state.list(SCRAPBOOK_ID, g, "req:").await.unwrap().len(), 1);
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_request_is_dropped_when_the_module_is_off_the_message_is_gone_or_there_is_no_shelf() {
    let f = fx().await;
    let g = GuildId::new(f.guild as u64);
    // Module off in this server (the kill switch of the server): nothing is archived.
    let m = message_with_file(&f, "a", "a.png", 5).await;
    request(&f, m).await;
    f.features
        .set_enabled(SCRAPBOOK_ID, &ScopeRef::Guild { guild: g }, Some(false))
        .await
        .unwrap();
    assert_eq!(f.worker.run_requests().await.unwrap(), 0);
    assert!(!entry_exists(&f, m).await);
    assert!(f.shelf.list("").await.unwrap().is_empty());
    assert!(f.worker.ctx.state.list(SCRAPBOOK_ID, g, "req:").await.unwrap().is_empty());
    f.features
        .set_enabled(SCRAPBOOK_ID, &ScopeRef::Guild { guild: g }, Some(true))
        .await
        .unwrap();

    // The operator withdraws the module (platform kill switch): same.
    request(&f, m).await;
    f.features
        .set_enabled(SCRAPBOOK_ID, &ScopeRef::Platform, Some(false))
        .await
        .unwrap();
    assert_eq!(f.worker.run_requests().await.unwrap(), 0);
    assert!(!entry_exists(&f, m).await);
    f.features
        .set_enabled(SCRAPBOOK_ID, &ScopeRef::Platform, Some(true))
        .await
        .unwrap();

    // A message that was deleted before the job ran.
    sqlx::query("UPDATE messages SET visibility = 'user_deleted', deleted_at = now(), purge_after = now() + interval '30 days' WHERE id = $1")
        .bind(m)
        .execute(&f.pool)
        .await
        .unwrap();
    request(&f, m).await;
    assert_eq!(f.worker.run_requests().await.unwrap(), 0);
    assert!(!entry_exists(&f, m).await);

    // No shelf chosen.
    let m2 = message_with_file(&f, "b", "b.png", 5).await;
    f.store.clear_target(f.guild).await.unwrap();
    request(&f, m2).await;
    assert_eq!(f.worker.run_requests().await.unwrap(), 0);
    assert!(!entry_exists(&f, m2).await);
    assert!(f.worker.ctx.state.list(SCRAPBOOK_ID, g, "req:").await.unwrap().is_empty());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_oversize_file_is_skipped_and_the_text_is_kept() {
    let f = fx().await;
    let g = GuildId::new(f.guild as u64);
    f.features
        .set_settings(
            SCRAPBOOK_ID,
            &ScopeRef::Platform,
            json!({"max_object_mib": 1}).as_object().unwrap().clone(),
        )
        .await
        .unwrap();
    let m = message_with_file(&f, "big one", "big.png", 2 * 1024 * 1024).await;
    request(&f, m).await;
    assert_eq!(f.worker.run_requests().await.unwrap(), 1);
    let x = f.store.by_message(f.guild, m).await.unwrap().unwrap();
    assert_eq!(x.objects.len(), 0);
    assert_eq!(x.entry.skipped, 1);
    assert_eq!(x.entry.content, "big one");
    assert!(f.shelf.list("").await.unwrap().is_empty());
    let _ = g;
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn deleting_the_message_removes_the_entry_and_the_copy_through_the_bus() {
    let f = fx().await;
    let m = message_with_file(&f, "gone soon", "g.png", 12).await;
    request(&f, m).await;
    f.worker.run_requests().await.unwrap();
    assert_eq!(f.shelf.list("").await.unwrap().len(), 1);

    let bus = Arc::new(LagTapBus::new(Arc::new(InProcessBus::new())));
    let consumer = spawn_cascade_consumer(bus.tap(), f.archiver.clone(), Arc::new(SystemClock));
    bus.publish(
        sr_events::Topic::Channel(ChannelId::new(f.channel as u64)),
        DomainEvent::MessageDeleted {
            guild: Some(GuildId::new(f.guild as u64)),
            channel: ChannelId::new(f.channel as u64),
            message_id: m as u64,
        },
    )
    .await
    .unwrap();
    let pool = f.pool.clone();
    wait_until("the entry to go", || {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM scrapbook_entries")
                .fetch_one(&pool)
                .await
                .unwrap()
                == 0
        }
    })
    .await;
    let shelf = f.shelf.clone();
    wait_until("the copy to go", || {
        let shelf = shelf.clone();
        async move { shelf.list("").await.unwrap().is_empty() }
    })
    .await;
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM scrapbook_removals")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    assert_eq!(rows, 0, "a clean cascade leaves no removal row behind");

    // The cascade does not depend on the module: switch it off and a second message still cleans up.
    let m2 = message_with_file(&f, "again", "h.png", 12).await;
    request(&f, m2).await;
    f.worker.run_requests().await.unwrap();
    f.features
        .set_enabled(SCRAPBOOK_ID, &ScopeRef::Platform, Some(false))
        .await
        .unwrap();
    bus.publish(
        sr_events::Topic::Channel(ChannelId::new(f.channel as u64)),
        DomainEvent::MessagesBulkDeleted {
            guild: Some(GuildId::new(f.guild as u64)),
            channel: ChannelId::new(f.channel as u64),
            message_ids: vec![m2 as u64],
        },
    )
    .await
    .unwrap();
    let store = f.store.clone();
    let guild = f.guild;
    wait_until("the second entry to go", || {
        let store = store.clone();
        async move { store.by_message(guild, m2).await.unwrap().is_none() }
    })
    .await;
    consumer.abort();
}

/// The roadmap's risk, end to end: the third-party bucket refuses the delete.
#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_bucket_failure_in_the_cascade_is_visible_to_managers_and_retried_until_it_works() {
    let f = fx().await;
    let m = message_with_file(&f, "keep me", "k.png", 9).await;
    request(&f, m).await;
    f.worker.run_requests().await.unwrap();
    let key = f.store.by_message(f.guild, m).await.unwrap().unwrap().objects[0]
        .key
        .clone();

    f.shelf.fail_delete.store(true, Ordering::SeqCst);
    let report = f
        .archiver
        .cascade(DeleteScope::Message(m), 1_000)
        .await
        .unwrap();
    assert_eq!((report.entries, report.removed, report.failed), (1, 0, 1));
    assert!(!entry_exists(&f, m).await, "the gallery no longer shows it");
    assert!(f.shelf.head(&key).await.is_ok(), "the copy is still in the bucket");

    // What `GET /guilds/{id}/scrapbook/removals` serves to the managers.
    let failed = f.store.failed_removals(f.guild).await.unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].key, key);
    assert_eq!(failed[0].mount_id, f.mount);
    assert_eq!(failed[0].message_id, Some(m));
    assert_eq!(failed[0].attempts, 1);
    assert!(failed[0].last_error.contains("AccessDenied"), "{}", failed[0].last_error);
    // Another server sees nothing of it.
    assert!(f.store.failed_removals(f.guild + 1).await.unwrap().is_empty());

    // Not due, then due and still failing: the count grows, the row stays.
    assert_eq!(f.archiver.drain(1_000, 10).await.unwrap(), (0, 0));
    let due = failed[0].next_attempt_ms;
    assert_eq!(f.archiver.drain(due, 10).await.unwrap(), (0, 1));
    assert_eq!(f.store.failed_removals(f.guild).await.unwrap()[0].attempts, 2);

    // The key works again: the sweep removes the copy and the row.
    f.shelf.fail_delete.store(false, Ordering::SeqCst);
    let later = f.store.failed_removals(f.guild).await.unwrap()[0].next_attempt_ms;
    assert_eq!(f.archiver.drain(later, 10).await.unwrap(), (1, 0));
    assert!(f.shelf.head(&key).await.is_err());
    assert!(f.store.failed_removals(f.guild).await.unwrap().is_empty());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_sweep_finds_entries_whose_message_is_gone_without_an_event() {
    let f = fx().await;
    let a = message_with_file(&f, "a", "a.png", 4).await;
    let b = message_with_file(&f, "b", "b.png", 4).await;
    for m in [a, b] {
        request(&f, m).await;
    }
    assert_eq!(f.worker.run_requests().await.unwrap(), 2);
    // `a` is deleted with no event (a missed tap); `b` is hidden by staff.
    sqlx::query("UPDATE messages SET visibility = 'user_deleted', deleted_at = now(), purge_after = now() + interval '30 days' WHERE id = $1")
        .bind(a)
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE messages SET visibility = 'staff_removed', deleted_at = now() WHERE id = $1")
        .bind(b)
        .execute(&f.pool)
        .await
        .unwrap();
    let (_, _, reconciled) = f.worker.sweep().await.unwrap();
    assert_eq!(reconciled, 2);
    assert!(!entry_exists(&f, a).await && !entry_exists(&f, b).await);
    assert!(f.shelf.list("").await.unwrap().is_empty());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn erasing_a_member_deletes_their_entries_and_queues_the_copies() {
    let f = fx().await;
    let m = message_with_file(&f, "mine", "m.png", 6).await;
    request(&f, m).await;
    f.worker.run_requests().await.unwrap();
    let key = f.store.by_message(f.guild, m).await.unwrap().unwrap().objects[0]
        .key
        .clone();

    // The export names the entry (text, tags, file names) but never a key.
    let export = sr_db::scrapbook_legal::export_for(&f.pool, f.member)
        .await
        .unwrap();
    let about = export["entries_about_your_messages"].as_array().unwrap();
    assert_eq!(about.len(), 1);
    assert_eq!(about[0]["content"], "mine");
    assert_eq!(about[0]["files"][0]["filename"], "m.png");
    assert!(!export.to_string().contains(&key), "no bucket key in the export");

    let mut tx = f.pool.begin().await.unwrap();
    let erased = sr_db::scrapbook_legal::erase_for(&mut tx, f.member)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!((erased.entries, erased.copies_queued), (1, 1));
    assert!(!entry_exists(&f, m).await);
    // The removal is queued, and the sweep empties the bucket.
    assert!(f.shelf.head(&key).await.is_ok());
    let (removed, failing, _) = f.worker.sweep().await.unwrap();
    assert_eq!((removed, failing), (1, 0));
    assert!(f.shelf.head(&key).await.is_err());
    // Nothing left to export.
    let again = sr_db::scrapbook_legal::export_for(&f.pool, f.member)
        .await
        .unwrap();
    assert!(again["entries_about_your_messages"].as_array().unwrap().is_empty());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn erasing_the_member_who_delegated_upload_unlinks_the_shelf() {
    let f = fx().await;
    f.store
        .set_target(&Target {
            guild_id: f.guild,
            mount_id: f.mount,
            granted_by: f.member,
        })
        .await
        .unwrap();
    let mut tx = f.pool.begin().await.unwrap();
    sr_db::scrapbook_legal::erase_for(&mut tx, f.member)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(f.store.target(f.guild).await.unwrap().is_none());
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn a_rearchived_message_cancels_its_own_pending_removal() {
    // A message deleted while the bucket was down, then restored and archived again: the copy is wanted, so
    // the stale removal must not delete it behind the new entry.
    let f = fx().await;
    let m = message_with_file(&f, "back", "b.png", 3).await;
    request(&f, m).await;
    f.worker.run_requests().await.unwrap();
    f.shelf.fail_delete.store(true, Ordering::SeqCst);
    f.archiver
        .cascade(DeleteScope::Message(m), 0)
        .await
        .unwrap();
    assert_eq!(f.store.failed_removals(f.guild).await.unwrap().len(), 1);
    request(&f, m).await;
    assert_eq!(f.worker.run_requests().await.unwrap(), 1);
    assert!(f.store.failed_removals(f.guild).await.unwrap().is_empty());
    f.shelf.fail_delete.store(false, Ordering::SeqCst);
    f.worker.sweep().await.unwrap();
    let x = f.store.by_message(f.guild, m).await.unwrap().unwrap();
    assert!(f.shelf.head(&x.objects[0].key).await.is_ok());
}

// ---- the service account --------------------------------------------------------------------------

fn real_buckets(f: &Fx) -> PgScrapbookBuckets {
    let repo = Arc::new(PgMountRepo::new(f.pool.clone()));
    let facts = Arc::new(sr_db::scopes_repo::PgScopeFacts::new(f.pool.clone()));
    let storages = MountStorages::new(
        Arc::new(ObjectStoreStorage::in_memory()),
        repo.clone(),
        Arc::new(SecretBox::new([9; 32])),
    );
    PgScrapbookBuckets::new(repo, facts, storages)
}

#[tokio::test]
#[ignore = "needs SR_TEST_DATABASE_URL"]
async fn the_service_account_writes_only_to_the_servers_own_shelf_with_the_upload_grant() {
    let f = fx().await;
    let b = real_buckets(&f);
    let t = |mount_id: i64, granted_by: i64| Target {
        guild_id: f.guild,
        mount_id,
        granted_by,
    };
    // The owner holds every grant: allowed.
    assert!(b.writer(&t(f.mount, f.owner)).await.is_ok());
    // A mount that does not exist.
    assert_eq!(b.writer(&t(424_242, f.owner)).await.err(), Some(Refusal::MountGone));
    // The instance bucket is the platform's, never the module's.
    sqlx::query(
        "INSERT INTO storage_mounts (id, owner_kind, owner_id, label, provider, endpoint, bucket, access_key_id, secret_key_enc) \
         VALUES ($1, 'platform', 0, 'Instance', 'garage', 'http://127.0.0.1:3900', 'b', 'k', '\\x00') ON CONFLICT DO NOTHING",
    )
    .bind(INSTANCE_MOUNT_ID)
    .execute(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        b.writer(&t(INSTANCE_MOUNT_ID, f.owner)).await.err(),
        Some(Refusal::NotTheServers)
    );
    // Another server's shelf is refused even for this server's owner.
    let other = ChatService::new(
        PgChatRepo::new(f.pool.clone()),
        Arc::new(SystemClock) as Arc<dyn Clock>,
        SnowflakeGenerator::new(Arc::new(SystemClock) as Arc<dyn Clock>, 27, 27).unwrap(),
        ChatLimits::default(),
    )
    .create_guild(UserId::new(f.owner as u64), "Elsewhere")
    .await
    .unwrap()
    .guild
    .id
    .get() as i64;
    let theirs = guild_mount(&f.pool, 7002, other, "Theirs").await.id;
    assert_eq!(
        b.writer(&t(theirs, f.owner)).await.err(),
        Some(Refusal::NotTheServers)
    );
    // A member's own bucket is refused too.
    let mine = {
        let repo = PgMountRepo::new(f.pool.clone());
        repo.create(
            NewMount::seal(
                &SecretBox::new([9; 32]),
                7003,
                MountInput {
                    owner: MountOwner::User(f.member),
                    label: "Mine".into(),
                    provider: Provider::Garage,
                    endpoint: "http://127.0.0.1:3900".into(),
                    region: "garage".into(),
                    bucket: "mine".into(),
                    prefix: String::new(),
                    path_style: true,
                    access_key_id: "GK0123456789abcd".into(),
                },
                &Secret::new("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"),
            )
            .unwrap(),
            Some(5),
        )
        .await
        .unwrap()
        .id
    };
    assert_eq!(
        b.writer(&t(mine, f.member)).await.err(),
        Some(Refusal::NotTheServers)
    );

    // A member without the `upload` grant cannot delegate it; with the grant on @everyone they can.
    assert_eq!(
        b.writer(&t(f.mount, f.member)).await.err(),
        Some(Refusal::GrantLost)
    );
    sqlx::query("INSERT INTO guild_storage_grants (guild_id, role_id, grant_name) VALUES ($1, $1, 'upload')")
        .bind(f.guild)
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(b.writer(&t(f.mount, f.member)).await.is_ok());
    // ... and it ends the moment the grant is taken away.
    sqlx::query("DELETE FROM guild_storage_grants WHERE guild_id = $1 AND grant_name = 'upload'")
        .bind(f.guild)
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        b.writer(&t(f.mount, f.member)).await.err(),
        Some(Refusal::GrantLost)
    );
    let _ = Grant::Upload;

    // A switched-off shelf is refused.
    sqlx::query("UPDATE storage_mounts SET status = 'disabled' WHERE id = $1")
        .bind(f.mount)
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        b.writer(&t(f.mount, f.owner)).await.err(),
        Some(Refusal::Disabled)
    );
}
