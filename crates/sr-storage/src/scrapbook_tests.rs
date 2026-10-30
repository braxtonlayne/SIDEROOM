//! Tests of the scrapbook core (PRF-SCR-001): the key layout, the one allowed copy and its cap, archive,
//! idempotence, rollback, the delete cascade and the visible-failure path.

use super::*;
use crate::storage::{MultipartSession, ObjectInfo, ObjectStoreStorage, ObjectStream, PresignMethod};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;

const G: i64 = 10;
const MOUNT: i64 = 77;
const MANAGER: i64 = 5;
const SENT: i64 = 1_782_000_000_000; // 2026-06-21

/// An in-memory bucket whose writes and deletes can be made to fail.
struct Flaky {
    inner: ObjectStoreStorage,
    fail_put: AtomicBool,
    fail_delete: AtomicBool,
}

impl Flaky {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: ObjectStoreStorage::in_memory(),
            fail_put: AtomicBool::new(false),
            fail_delete: AtomicBool::new(false),
        })
    }
    fn down() -> StorageError {
        StorageError::Backend("connection refused".into())
    }
}

#[async_trait]
impl ObjectStorage for Flaky {
    async fn put(&self, key: &str, data: Bytes) -> crate::Result<()> {
        if self.fail_put.load(Ordering::SeqCst) {
            return Err(Self::down());
        }
        self.inner.put(key, data).await
    }
    async fn get(&self, key: &str) -> crate::Result<Bytes> {
        self.inner.get(key).await
    }
    async fn get_range(&self, key: &str, s: u64, e: u64) -> crate::Result<Bytes> {
        self.inner.get_range(key, s, e).await
    }
    async fn get_stream(
        &self,
        key: &str,
        range: Option<std::ops::Range<u64>>,
    ) -> crate::Result<ObjectStream> {
        self.inner.get_stream(key, range).await
    }
    async fn head(&self, key: &str) -> crate::Result<ObjectInfo> {
        self.inner.head(key).await
    }
    async fn delete(&self, key: &str) -> crate::Result<()> {
        if self.fail_delete.load(Ordering::SeqCst) {
            return Err(Self::down());
        }
        self.inner.delete(key).await
    }
    async fn list(&self, prefix: &str) -> crate::Result<Vec<ObjectInfo>> {
        self.inner.list(prefix).await
    }
    async fn presign(&self, m: PresignMethod, key: &str, ttl: Duration) -> crate::Result<String> {
        self.inner.presign(m, key, ttl).await
    }
    async fn multipart_create(&self, key: &str) -> crate::Result<Box<dyn MultipartSession>> {
        if self.fail_put.load(Ordering::SeqCst) {
            return Err(Self::down());
        }
        self.inner.multipart_create(key).await
    }
}

struct Buckets {
    shelf: Arc<Flaky>,
    refusal: Mutex<Option<Refusal>>,
}

#[async_trait]
impl ScrapbookBuckets for Buckets {
    async fn writer(&self, _t: &Target) -> Result<Arc<dyn ObjectStorage>, Refusal> {
        if let Some(r) = self.refusal.lock().unwrap().clone() {
            return Err(r);
        }
        Ok(self.shelf.clone())
    }
    async fn remover(&self, _m: i64) -> Result<Arc<dyn ObjectStorage>, String> {
        Ok(self.shelf.clone())
    }
}

struct Rig {
    archiver: Archiver,
    store: Arc<MemoryScrapbook>,
    shelf: Arc<Flaky>,
    buckets: Arc<Buckets>,
    source: Arc<Flaky>,
}

async fn rig() -> Rig {
    let store = Arc::new(MemoryScrapbook::new());
    let shelf = Flaky::new();
    let source = Flaky::new();
    let buckets = Arc::new(Buckets {
        shelf: shelf.clone(),
        refusal: Mutex::new(None),
    });
    let next = Arc::new(AtomicI64::new(1000));
    let archiver = Archiver::new(
        store.clone(),
        buckets.clone(),
        source.clone(),
        Arc::new(move || next.fetch_add(1, Ordering::SeqCst)),
    );
    store
        .set_target(&Target {
            guild_id: G,
            mount_id: MOUNT,
            granted_by: MANAGER,
        })
        .await
        .unwrap();
    Rig {
        archiver,
        store,
        shelf,
        buckets,
        source,
    }
}

fn message(id: i64, atts: Vec<SourceAttachment>) -> SourceMessage {
    SourceMessage {
        guild_id: G,
        channel_id: 20,
        message_id: id,
        author_id: 3,
        author_name: "Ada".into(),
        content: "look at this".into(),
        sent_at_ms: SENT,
        attachments: atts,
    }
}

fn request(m: SourceMessage) -> ArchiveRequest {
    ArchiveRequest {
        message: m,
        by: Some(MANAGER),
        via: Via::Manual,
        tags: vec!["#Funny".into(), "funny".into()],
        max_object_bytes: 1024,
        now_ms: SENT + 5,
    }
}

async fn upload(r: &Rig, key: &str, bytes: usize) -> SourceAttachment {
    r.source
        .put(key, Bytes::from(vec![7u8; bytes]))
        .await
        .unwrap();
    SourceAttachment {
        key: key.into(),
        filename: "My Photo (1).png".into(),
        content_type: "image/png".into(),
        size: bytes as u64,
    }
}

#[test]
fn keys_follow_the_layout_and_are_always_safe() {
    assert_eq!(
        object_key(SENT, 42, 0, "My Photo (1).png"),
        "scrapbook/2026/06/42/0-My_Photo__1_.png"
    );
    for name in [
        "../../etc/passwd",
        "a\\b.png",
        ".hidden",
        "",
        "日本語.png",
        "x/y/z.mp4",
    ] {
        let k = object_key(SENT, 1, 2, name);
        assert!(safe_key(&k, false).is_ok(), "{name} -> {k}");
        assert!(k.starts_with("scrapbook/2026/06/1/2-"));
    }
    assert!(safe_name(&"a".repeat(300)).len() <= 80);
    assert!(safe_name(&format!("{}.mp4", "a".repeat(300))).ends_with(".mp4"));
    assert_eq!(year_month(0), (1970, 1));
    assert_eq!(year_month(-1), (1969, 12));
}

#[test]
fn tags_are_cleaned() {
    let t = clean_tags(&[
        " #Funny ".into(),
        "FUNNY".into(),
        "".into(),
        "x".repeat(99),
    ]);
    assert_eq!(t.len(), 2);
    assert_eq!(t[0], "funny");
    assert_eq!(t[1].chars().count(), MAX_TAG_LEN);
    let many: Vec<String> = (0..50).map(|i| format!("t{i}")).collect();
    assert_eq!(clean_tags(&many).len(), MAX_TAGS);
}

#[test]
fn backoff_doubles_and_caps() {
    assert_eq!(backoff_ms(0), 60_000);
    assert_eq!(backoff_ms(1), 120_000);
    assert_eq!(backoff_ms(2), 240_000);
    assert_eq!(backoff_ms(40), MAX_BACKOFF_MS);
}

#[tokio::test]
async fn copy_respects_the_cap_up_front() {
    let src = Flaky::new();
    let dst = Flaky::new();
    src.put("a", Bytes::from(vec![1u8; 100])).await.unwrap();
    assert!(matches!(
        copy_object(&*src, "a", &*dst, "b", 99).await,
        Err(CopyError::TooLarge { size: 100, max: 99 })
    ));
    assert!(dst.get("b").await.is_err(), "nothing written over the cap");
    assert_eq!(copy_object(&*src, "a", &*dst, "b", 100).await.unwrap(), 100);
    assert_eq!(dst.get("b").await.unwrap().len(), 100);
    assert!(matches!(
        copy_object(&*src, "missing", &*dst, "c", 100).await,
        Err(CopyError::SourceGone)
    ));
}

#[tokio::test]
async fn a_large_object_is_streamed_in_parts_and_nothing_appears_on_failure() {
    let src = Flaky::new();
    let dst = Flaky::new();
    let big = PART_BYTES * 2 + 123;
    src.put("big", Bytes::from(vec![9u8; big])).await.unwrap();
    let n = copy_object(&*src, "big", &*dst, "out/big", big as u64)
        .await
        .unwrap();
    assert_eq!(n as usize, big);
    assert_eq!(dst.head("out/big").await.unwrap().size as usize, big);
    // Over the cap: refused before a byte is written.
    assert!(matches!(
        copy_object(&*src, "big", &*dst, "out/big2", (big - 1) as u64).await,
        Err(CopyError::TooLarge { .. })
    ));
    assert!(dst.head("out/big2").await.is_err());
    // A destination that is down: the error says so and no object appears.
    dst.fail_put.store(true, Ordering::SeqCst);
    assert!(matches!(
        copy_object(&*src, "big", &*dst, "out/big3", big as u64).await,
        Err(CopyError::Bucket(_))
    ));
    assert!(dst.head("out/big3").await.is_err());
}

#[tokio::test]
async fn archive_copies_into_the_shelf_and_is_idempotent() {
    let r = rig().await;
    let a = upload(&r, "att/1", 10).await;
    let out = r
        .archiver
        .archive(request(message(42, vec![a.clone()])))
        .await
        .unwrap();
    let ArchiveOutcome::Archived(done) = out else {
        panic!("expected a new entry")
    };
    assert_eq!(done.entry.tags, vec!["funny".to_string()]);
    assert_eq!(done.objects.len(), 1);
    let o = &done.objects[0];
    assert_eq!(o.key, "scrapbook/2026/06/42/0-My_Photo__1_.png");
    assert_eq!(o.mount_id, MOUNT);
    assert_eq!(o.kind, ObjectKind::Image);
    assert_eq!(r.shelf.get(&o.key).await.unwrap().len(), 10);
    // Again: nothing new.
    let again = r
        .archiver
        .archive(request(message(42, vec![a])))
        .await
        .unwrap();
    assert!(matches!(again, ArchiveOutcome::AlreadyArchived(_)));
    assert_eq!(r.shelf.list("").await.unwrap().len(), 1);
}

#[tokio::test]
async fn oversize_attachments_are_skipped_not_fatal() {
    let r = rig().await;
    let small = upload(&r, "att/s", 10).await;
    let huge = upload(&r, "att/h", 2000).await; // over the 1024 cap of `request`
    let out = r
        .archiver
        .archive(request(message(43, vec![huge, small])))
        .await
        .unwrap();
    let ArchiveOutcome::Archived(done) = out else {
        panic!()
    };
    assert_eq!(done.objects.len(), 1);
    assert_eq!(done.entry.skipped, 1);
    // Position counts copies, so the surviving file is `0-`.
    assert!(done.objects[0].key.contains("/0-"));
}

#[tokio::test]
async fn nothing_to_keep_is_refused() {
    let r = rig().await;
    let mut m = message(44, vec![]);
    m.content = "  ".into();
    assert!(matches!(
        r.archiver.archive(request(m)).await,
        Err(ArchiveError::Empty)
    ));
}

#[tokio::test]
async fn a_missing_or_refused_shelf_writes_nothing() {
    let r = rig().await;
    let a = upload(&r, "att/1", 10).await;
    r.store.clear_target(G).await.unwrap();
    assert!(matches!(
        r.archiver
            .archive(request(message(45, vec![a.clone()])))
            .await,
        Err(ArchiveError::NoTarget)
    ));
    r.store
        .set_target(&Target {
            guild_id: G,
            mount_id: MOUNT,
            granted_by: MANAGER,
        })
        .await
        .unwrap();
    // The member who delegated `upload` lost it.
    *r.buckets.refusal.lock().unwrap() = Some(Refusal::GrantLost);
    let err = r
        .archiver
        .archive(request(message(45, vec![a])))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        ArchiveError::TargetRefused(Refusal::GrantLost)
    ));
    assert!(r.shelf.list("").await.unwrap().is_empty());
    assert!(r.store.by_message(G, 45).await.unwrap().is_none());
}

#[tokio::test]
async fn a_failed_copy_leaves_no_entry_and_no_stray_copy() {
    let r = rig().await;
    let first = upload(&r, "att/1", 10).await;
    let mut second = upload(&r, "att/2", 10).await;
    second.filename = "b.png".into();
    let ok = r
        .archiver
        .archive(request(message(46, vec![first.clone()])))
        .await
        .unwrap();
    assert!(matches!(ok, ArchiveOutcome::Archived(_)));
    // Now the shelf goes down: a message whose copy fails outright.
    r.shelf.fail_put.store(true, Ordering::SeqCst);
    let err = r
        .archiver
        .archive(request(message(47, vec![first, second])))
        .await
        .unwrap_err();
    assert!(matches!(err, ArchiveError::Copy { .. }));
    assert!(r.store.by_message(G, 47).await.unwrap().is_none());
    r.shelf.fail_put.store(false, Ordering::SeqCst);
    let keys: Vec<String> = r
        .shelf
        .list("")
        .await
        .unwrap()
        .into_iter()
        .map(|o| o.key)
        .collect();
    assert_eq!(keys.len(), 1, "only message 46's copy: {keys:?}");
    assert!(keys[0].contains("/46/"));
}

#[tokio::test]
async fn deleting_the_message_removes_the_entry_and_its_copies() {
    let r = rig().await;
    let a = upload(&r, "att/1", 10).await;
    r.archiver
        .archive(request(message(50, vec![a])))
        .await
        .unwrap();
    assert_eq!(r.shelf.list("").await.unwrap().len(), 1);
    let report = r
        .archiver
        .cascade(DeleteScope::Message(50), SENT + 10)
        .await
        .unwrap();
    assert_eq!(
        report,
        CascadeReport {
            entries: 1,
            removed: 1,
            failed: 0
        }
    );
    assert!(r.shelf.list("").await.unwrap().is_empty());
    assert!(r.store.by_message(G, 50).await.unwrap().is_none());
    assert_eq!(r.store.removal_count(), 0, "a clean cascade leaves no row");
    // Deleting a message that was never archived does nothing.
    let none = r
        .archiver
        .cascade(DeleteScope::Message(999), SENT)
        .await
        .unwrap();
    assert_eq!(none, CascadeReport::default());
}

/// The roadmap's risk: the cascade runs against a third-party bucket and must never fail silently.
#[tokio::test]
async fn a_bucket_that_refuses_the_delete_is_listed_for_managers_and_retried() {
    let r = rig().await;
    let a = upload(&r, "att/1", 10).await;
    r.archiver
        .archive(request(message(51, vec![a])))
        .await
        .unwrap();
    r.shelf.fail_delete.store(true, Ordering::SeqCst);

    let report = r
        .archiver
        .cascade(DeleteScope::Message(51), SENT + 10)
        .await
        .unwrap();
    // The entry is gone from the gallery at once, the copy is not, and that is on record.
    assert_eq!(report.entries, 1);
    assert_eq!(report.failed, 1);
    assert_eq!(report.removed, 0);
    assert!(r.store.by_message(G, 51).await.unwrap().is_none());
    assert_eq!(r.shelf.list("").await.unwrap().len(), 1);
    let failed = r.store.failed_removals(G).await.unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].state, RemovalState::Failed);
    assert_eq!(failed[0].attempts, 1);
    assert!(failed[0].last_error.contains("connection refused"));
    assert_eq!(failed[0].message_id, Some(51));
    assert_eq!(failed[0].next_attempt_ms, SENT + 10 + backoff_ms(0));

    // Not due yet: the sweep leaves it alone.
    assert_eq!(r.archiver.drain(SENT + 20, 10).await.unwrap(), (0, 0));
    // Due, still down: the attempt count grows and the row stays visible.
    let later = SENT + 10 + backoff_ms(0);
    assert_eq!(r.archiver.drain(later, 10).await.unwrap(), (0, 1));
    let failed = r.store.failed_removals(G).await.unwrap();
    assert_eq!(failed[0].attempts, 2);
    assert_eq!(failed[0].next_attempt_ms, later + backoff_ms(1));

    // The bucket comes back: the next sweep removes the copy and the row disappears.
    r.shelf.fail_delete.store(false, Ordering::SeqCst);
    let much_later = later + backoff_ms(1);
    assert_eq!(r.archiver.drain(much_later, 10).await.unwrap(), (1, 0));
    assert!(r.shelf.list("").await.unwrap().is_empty());
    assert!(r.store.failed_removals(G).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_manager_can_retry_now_or_dismiss_and_only_in_their_own_server() {
    let r = rig().await;
    let a = upload(&r, "att/1", 10).await;
    r.archiver
        .archive(request(message(52, vec![a])))
        .await
        .unwrap();
    r.shelf.fail_delete.store(true, Ordering::SeqCst);
    r.archiver
        .cascade(DeleteScope::Message(52), SENT)
        .await
        .unwrap();
    let id = r.store.failed_removals(G).await.unwrap()[0].id;
    // Another server's manager cannot touch it.
    assert!(!r.store.retry_now(G + 1, id, SENT).await.unwrap());
    assert!(!r.store.dismiss(G + 1, id).await.unwrap());
    assert!(r.store.retry_now(G, id, SENT + 1).await.unwrap());
    assert_eq!(r.store.due_removals(SENT + 1, 10).await.unwrap().len(), 1);
    assert!(r.store.dismiss(G, id).await.unwrap());
    assert!(r.store.failed_removals(G).await.unwrap().is_empty());
}

#[tokio::test]
async fn erasure_scope_takes_every_entry_about_a_member() {
    let r = rig().await;
    for (id, author) in [(60, 3), (61, 3), (62, 4)] {
        let mut m = message(id, vec![]);
        m.author_id = author;
        r.archiver.archive(request(m)).await.unwrap();
    }
    let report = r
        .archiver
        .cascade(DeleteScope::Author(3), SENT)
        .await
        .unwrap();
    assert_eq!(report.entries, 2);
    assert!(r.store.by_message(G, 62).await.unwrap().is_some());
    assert!(r.store.by_message(G, 60).await.unwrap().is_none());
}

#[tokio::test]
async fn the_gallery_filter_only_lists_viewable_channels() {
    let r = rig().await;
    for (id, ch) in [(70, 20), (71, 21), (72, 20)] {
        let mut m = message(id, vec![]);
        m.channel_id = ch;
        r.archiver.archive(request(m)).await.unwrap();
    }
    let f = Filter {
        viewable: vec![20],
        ..Filter::default()
    };
    let got = r.store.list(G, &f, None, 10).await.unwrap();
    let ids: Vec<i64> = got.iter().map(|x| x.entry.message_id).collect();
    assert_eq!(ids.len(), 2, "channel 21 is not viewable");
    assert!(ids.contains(&70) && ids.contains(&72));
    let none = Filter::default();
    assert!(r.store.list(G, &none, None, 10).await.unwrap().is_empty());
    let facets = r.store.facets(G, &[20]).await.unwrap();
    assert_eq!(facets.channels, vec![(20, 2)]);
    assert_eq!(facets.tags, vec![("funny".to_string(), 2)]);
}
