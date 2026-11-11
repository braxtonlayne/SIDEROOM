//! `storage.takedown_digest` (D-588, SEC-AUD-058) and `storage.takedown_backfill` (D-589, SEC-AUD-059).
//!
//! * The digest job: `disable_item` records the tag and size of an upheld storage object and enqueues one job per
//!   item. The job streams the object with no time cap (the uphold no longer waits for it) and hands the sha256 and
//!   the byte count to [`sr_moderation::Moderation::record_digest`]. A transient answer (a backend error) is retried
//!   with the queue's backoff, 14 attempts; after the last attempt, or at once on a definite answer (no mount, the
//!   object is gone), the item keeps its tag and size and staff get ONE alert naming the notice and the item. An item
//!   restored before the job ran is skipped. While it streams, the job refreshes its lock so a long read is not
//!   returned to the queue as a crashed worker's.
//! * The backfill: a one-off job armed at every boot but run once (its row is never re-armed), that gives earlier
//!   derived items their owners and sends the removal DM, and queues a digest job for each earlier item without a
//!   sha256 ([`sr_moderation::Moderation::backfill_byte_takedowns`]). Owners it cannot resolve are counted in one
//!   staff alert.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use sr_db::jobs::{self, Job};
use sr_db::runner::{HandlerError, JobRunner};
use sr_db::{DbResult, PgPool};
use sr_events::{DomainEvent, EventBus, StaffAlertKind, Topic};
use sr_moderation::Moderation;
use sr_moderation::byte_digest::{
    BACKFILL_BOOT_DELAY, BACKFILL_JOB_ID, BACKFILL_JOB_KIND, BackfillReport, DIGEST_JOB_KIND,
    DigestRecorded, DigestTarget,
};
use sr_storage::mount_storage::MountStorages;
use sr_storage::{ObjectStorage, StorageError};

/// How often a running digest job refreshes its lock (the stale-job reaper takes jobs idle for 30 minutes).
const HEARTBEAT: Duration = Duration::from_secs(5 * 60);
/// Items named in the unresolved-owners alert.
const ALERT_NAMES: usize = 10;

/// Opens the storage of a mount: the server's [`MountStorages`], or buckets in a test.
#[async_trait]
pub trait MountReader: Send + Sync {
    async fn open(&self, mount_id: i64) -> Result<Arc<dyn ObjectStorage>, StorageError>;
}

#[async_trait]
impl MountReader for MountStorages {
    async fn open(&self, mount_id: i64) -> Result<Arc<dyn ObjectStorage>, StorageError> {
        self.for_mount(mount_id).await
    }
}

/// What a digest job decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing to do: the item was restored, rejected or already has its sha256 (or another run wrote it).
    Skipped,
    /// The sha256 was written.
    Recorded(DigestRecorded),
    /// A definite answer, or the last attempt failed: staff were alerted, the item keeps tag and size.
    GaveUp(String),
    /// D-591: the file is not the size the item recorded: nothing was written, staff were alerted once.
    Changed { recorded: i64, read: u64 },
    /// D-592: the item's mount is switched off (not deleted): nothing was written and nobody was told. Enabling
    /// the mount queues the job again.
    Waiting,
}

/// The alert text for an item whose sha256 could not be taken.
pub fn gave_up_detail(t: &DigestTarget, reason: &str) -> String {
    format!(
        "Takedown TK-{}: the sha256 of the file in item {} (\"{}\") could not be taken: {reason}. The file stays \
         delisted, but it is found again only by its entity tag and size, so the same bytes uploaded through \
         another client are not matched. Check the bucket, or restore the item and uphold it again.",
        t.notice_id, t.item_id, t.label
    )
}

/// D-591: the alert text for a file whose size is not the one the item recorded.
pub fn changed_detail(t: &DigestTarget, recorded: i64, read: u64) -> String {
    format!(
        "Takedown TK-{}: the file in item {} (\"{}\") is {read} bytes now; the item recorded {recorded}. The file          changed after the catalog saw it, so no fingerprint of the bytes was recorded and the item keeps only its          entity tag and size. If the takedown was meant for the file as it is now, restore the item and uphold it          again once the index has run.",
        t.notice_id, t.item_id, t.label
    )
}

/// The alert text for derived items whose owner could not be found.
pub fn unresolved_detail(r: &BackfillReport) -> String {
    let names: Vec<String> = r
        .unresolved
        .iter()
        .take(ALERT_NAMES)
        .map(|(notice, item)| format!("TK-{notice} item {item}"))
        .collect();
    let more = r.unresolved.len().saturating_sub(ALERT_NAMES);
    let tail = if more > 0 {
        format!(" and {more} more")
    } else {
        String::new()
    };
    format!(
        "{} file(s) delisted by a byte match have no owner to tell (their bucket is gone): {}{tail}. They stay \
         delisted; nobody was sent the removal message.",
        r.unresolved.len(),
        names.join(", ")
    )
}

async fn alert(bus: &Arc<dyn EventBus>, detail: String) {
    let ev = DomainEvent::StaffAlert {
        kind: StaffAlertKind::TakedownBytes,
        detail,
    };
    if let Err(e) = bus.publish(Topic::Platform, ev).await {
        tracing::warn!(error = %e, "could not publish a takedown bytes alert");
    }
}

/// A definite answer: retrying cannot change it.
fn definite(e: &StorageError) -> bool {
    matches!(e, StorageError::NotFound(_) | StorageError::InvalidPath(_))
}

/// Run one digest job for `item_id`. `last_attempt`: a transient failure now is final. `Err` asks the queue to
/// retry; every `Ok` is final.
pub async fn run_digest(
    moderation: &Moderation,
    reader: &dyn MountReader,
    bus: &Arc<dyn EventBus>,
    item_id: i64,
    last_attempt: bool,
) -> Result<Outcome, HandlerError> {
    let Some(target) = moderation.digest_target(item_id).await? else {
        return Ok(Outcome::Skipped);
    };
    let read = async {
        let storage = reader.open(target.mount_id).await?;
        sr_storage::digest::sha256_object(storage.as_ref(), &target.key).await
    }
    .await;
    match read {
        Ok((sha, bytes)) => {
            let done = moderation.record_digest(item_id, &sha, bytes).await?;
            if let DigestRecorded::SizeChanged { recorded, read } = done {
                tracing::warn!(
                    item = item_id,
                    notice = target.notice_id,
                    recorded,
                    read,
                    "takedown digest: the file changed"
                );
                alert(bus, changed_detail(&target, recorded, read)).await;
                return Ok(Outcome::Changed { recorded, read });
            }
            Ok(Outcome::Recorded(done))
        }
        // A mount that is switched off (not deleted) cannot be opened, but its bytes are still there: not a
        // definite answer. The job ends; enabling the mount queues it again (D-592).
        Err(StorageError::NotFound(_)) if moderation.mount_is_disabled(target.mount_id).await? => {
            tracing::info!(
                item = item_id,
                mount = target.mount_id,
                "takedown digest waits for a disabled mount"
            );
            Ok(Outcome::Waiting)
        }
        Err(e) if definite(&e) || last_attempt => {
            let reason = if definite(&e) {
                match e {
                    StorageError::NotFound(_) => "the bucket or the file is gone".to_owned(),
                    other => other.to_string(),
                }
            } else {
                format!("the bucket kept failing ({e})")
            };
            tracing::warn!(item = item_id, notice = target.notice_id, %reason, "takedown digest gave up");
            alert(bus, gave_up_detail(&target, &reason)).await;
            Ok(Outcome::GaveUp(reason))
        }
        Err(e) => {
            tracing::warn!(item = item_id, error = %e, "takedown digest failed; will retry");
            Err(Box::new(e))
        }
    }
}

/// Run the backfill once: the report, and one staff alert when some owners could not be found.
pub async fn run_backfill(
    moderation: &Moderation,
    bus: &Arc<dyn EventBus>,
) -> Result<BackfillReport, HandlerError> {
    let report = moderation.backfill_byte_takedowns().await?;
    tracing::info!(
        owners = report.owners_assigned,
        unresolved = report.unresolved.len(),
        told = report.told,
        digests = report.digests_enqueued,
        "takedown byte backfill done"
    );
    if !report.unresolved.is_empty() {
        alert(bus, unresolved_detail(&report)).await;
    }
    Ok(report)
}

/// Keeps `job`'s lock fresh until dropped.
struct Heartbeat(tokio::task::JoinHandle<()>);

impl Heartbeat {
    fn start(pool: PgPool, job_id: i64) -> Self {
        Self(tokio::spawn(async move {
            loop {
                tokio::time::sleep(HEARTBEAT).await;
                let _ = sqlx::query(
                    "UPDATE jobs SET locked_at = now() WHERE id = $1 AND state = 'running'",
                )
                .bind(job_id)
                .execute(&pool)
                .await;
            }
        }))
    }
}

impl Drop for Heartbeat {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Register both jobs on `runner`. Pair with [`schedule`].
pub fn register(
    runner: JobRunner,
    pool: &PgPool,
    moderation: Moderation,
    reader: Arc<dyn MountReader>,
    bus: Arc<dyn EventBus>,
) -> JobRunner {
    let (p, m, r, b) = (pool.clone(), moderation.clone(), reader, bus.clone());
    let runner = runner.register(DIGEST_JOB_KIND, 1, move |job: Job| {
        let (pool, moderation, reader, bus) = (p.clone(), m.clone(), r.clone(), b.clone());
        Box::pin(async move {
            let Some(item) = job.payload.get("item_id").and_then(|v| v.as_i64()) else {
                // A malformed payload can never succeed: drop it.
                tracing::error!(job = job.id, "takedown digest job without an item");
                return Ok(());
            };
            let _beat = Heartbeat::start(pool, job.id);
            let last = job.attempts >= job.max_attempts;
            run_digest(&moderation, reader.as_ref(), &bus, item, last).await?;
            Ok(())
        })
    });
    let (m, b) = (moderation, bus);
    runner.register(BACKFILL_JOB_KIND, 1, move |_job: Job| {
        let (moderation, bus) = (m.clone(), b.clone());
        Box::pin(async move {
            run_backfill(&moderation, &bus).await?;
            Ok(())
        })
    })
}

/// Arm the D-589 one-off (at every boot; a row that already ran is left alone). Call only when [`register`] is on
/// the runner.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_once(
        pool,
        BACKFILL_JOB_ID,
        BACKFILL_JOB_KIND,
        &json!({}),
        BACKFILL_BOOT_DELAY,
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI64, Ordering};

    use bytes::Bytes;
    use futures::StreamExt;
    use sr_admin::Caller;
    use sr_core::PlatformPermissions as P;
    use sr_db::accounts::{self, NewAccount};
    use sr_db::testing::TestDb;
    use sr_events::InProcessBus;
    use sr_moderation::model::{ItemKind, NewItem, NewNotice, TakedownKind, Who};
    use sr_storage::ObjectStoreStorage;

    const FILM: &[u8] = b"the very same film bytes";
    const TAG: &str = "9e107d9d372bb6826bd81d3542a419d6";

    fn uid() -> i64 {
        static N: AtomicI64 = AtomicI64::new(0);
        let base = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as i64)
            .unwrap_or(0);
        base * 1000 + N.fetch_add(1, Ordering::Relaxed) % 1000
    }

    /// How a bucket answers.
    enum Answer {
        Bucket(Arc<ObjectStoreStorage>),
        Down,
        NoMount,
    }

    #[async_trait]
    impl MountReader for Answer {
        async fn open(&self, _mount: i64) -> Result<Arc<dyn ObjectStorage>, StorageError> {
            match self {
                Self::Bucket(b) => Ok(b.clone()),
                Self::Down => Err(StorageError::Backend("connection reset".into())),
                Self::NoMount => Err(StorageError::NotFound("storage mount".into())),
            }
        }
    }

    struct World {
        db: TestDb,
        m: Moderation,
        bus: Arc<dyn EventBus>,
        staff: i64,
        owner: i64,
    }

    async fn account(db: &TestDb, name: &str) -> i64 {
        let id = uid();
        assert!(
            accounts::create_account(
                db.pool(),
                &NewAccount {
                    id,
                    person_id: uid(),
                    username: name,
                    display_name: None,
                    email: &format!("{name}@example.test"),
                    password_hash: "x",
                    state: "active",
                    email_verified: true,
                    initial_credits: 0,
                    flags: 0,
                },
                uid(),
            )
            .await
            .unwrap()
        );
        id
    }

    async fn mount(db: &TestDb, owner: i64, label: &str) -> i64 {
        let id = uid();
        sqlx::query(
            "INSERT INTO storage_mounts (id, owner_kind, owner_id, label, provider, endpoint, bucket,
                                         access_key_id, secret_key_enc, key_hint)
             VALUES ($1, 'user', $2, $3, 'garage', 'http://127.0.0.1:3900', 'shelf', 'GK0123456789abcd',
                     '\\xdeadbeef', 'abcd')",
        )
        .bind(id)
        .bind(owner)
        .bind(label)
        .execute(db.pool())
        .await
        .unwrap();
        id
    }

    async fn catalogued(db: &TestDb, mount: i64, key: &str) {
        sqlx::query(
            "INSERT INTO storage_objects (id, mount_id, key, size_bytes, kind, class, content_tag)
             VALUES ($1, $2, $3, $4, 'cinema', 'movie', $5)",
        )
        .bind(uid())
        .bind(mount)
        .bind(key)
        .bind(FILM.len() as i64)
        .bind(TAG)
        .execute(db.pool())
        .await
        .unwrap();
    }

    async fn world() -> World {
        let db = TestDb::new().await.unwrap();
        let m = Moderation::new(db.pool().clone(), Arc::new(uid));
        let staff = account(&db, "p57staff").await;
        let owner = account(&db, "p57owner").await;
        World {
            db,
            m,
            bus: Arc::new(InProcessBus::new()),
            staff,
            owner,
        }
    }

    impl World {
        fn caller(&self) -> Caller {
            Caller {
                user_id: self.staff,
                permissions: P::MANAGE_TAKEDOWNS,
                is_owner: false,
                ip: Some("127.0.0.1".into()),
            }
        }

        /// A notice upheld on the given `(mount, key)` items; returns the notice id.
        async fn upheld(&self, items: &[(i64, &str)]) -> i64 {
            let c = self.caller();
            let who = Who::Staff(&c);
            let n = self
                .m
                .create_notice(
                    &who,
                    &NewNotice {
                        kind: TakedownKind::Copyright,
                        notifier_name: "Rights Holder Ltd".into(),
                        notifier_contact: "legal@rights.example".into(),
                        legal_basis: "Copyright in the film".into(),
                        locations: vec![],
                        original_text: "Please remove it.".into(),
                        received_at_ms: None,
                    },
                )
                .await
                .unwrap()
                .summary
                .id;
            for (mount, key) in items {
                self.m
                    .add_item(
                        &who,
                        n,
                        &NewItem {
                            kind: ItemKind::StorageObject,
                            id: *mount,
                            key: Some((*key).into()),
                        },
                    )
                    .await
                    .unwrap();
            }
            self.m.uphold_notice(&who, n, Some("valid")).await.unwrap();
            n
        }

        async fn item(&self, notice: i64, key: &str) -> i64 {
            sqlx::query_scalar(
                "SELECT id FROM takedown_items WHERE notice_id = $1 AND object_key = $2",
            )
            .bind(notice)
            .bind(key)
            .fetch_one(self.db.pool())
            .await
            .unwrap()
        }

        async fn fingerprint(&self, item: i64) -> (Option<String>, Option<String>, Option<i64>) {
            sqlx::query_as(
                "SELECT object_hash, object_sha256, object_size FROM takedown_items WHERE id = $1",
            )
            .bind(item)
            .fetch_one(self.db.pool())
            .await
            .unwrap()
        }
    }

    /// The staff alerts published so far on a subscription made before.
    async fn alerts(sub: &mut futures::stream::BoxStream<'static, DomainEvent>) -> Vec<String> {
        let mut out = Vec::new();
        while let Ok(Some(ev)) = tokio::time::timeout(Duration::from_millis(150), sub.next()).await
        {
            if let DomainEvent::StaffAlert { kind, detail } = ev {
                assert_eq!(kind, StaffAlertKind::TakedownBytes);
                out.push(detail);
            }
        }
        out
    }

    /// D-588: a transient failure asks the queue to retry (no alert); the last attempt gives up with ONE alert naming
    /// the notice and the item, and the item keeps its tag and size.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn a_transient_failure_is_retried_and_the_last_attempt_alerts_once() {
        let w = world().await;
        let mnt = mount(&w.db, w.owner, "Shelf").await;
        catalogued(&w.db, mnt, "films/leak.mkv").await;
        let notice = w.upheld(&[(mnt, "films/leak.mkv")]).await;
        let item = w.item(notice, "films/leak.mkv").await;
        let mut sub = w.bus.subscribe(Topic::Platform);

        for _ in 0..3 {
            let r = run_digest(&w.m, &Answer::Down, &w.bus, item, false).await;
            assert!(r.is_err(), "the queue is asked to retry");
        }
        assert!(alerts(&mut sub).await.is_empty(), "no alert while retrying");

        let last = run_digest(&w.m, &Answer::Down, &w.bus, item, true)
            .await
            .unwrap();
        assert!(matches!(last, Outcome::GaveUp(_)), "{last:?}");
        let sent = alerts(&mut sub).await;
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert!(sent[0].contains(&format!("TK-{notice}")), "{}", sent[0]);
        assert!(sent[0].contains(&format!("item {item}")), "{}", sent[0]);
        assert_eq!(
            w.fingerprint(item).await,
            (Some(TAG.into()), None, Some(FILM.len() as i64)),
            "tag and size stay"
        );
    }

    /// D-588: a definite answer (no mount, object gone) alerts at once, on the first attempt; and a read that works
    /// writes the sha256 (and a second run finds nothing to do).
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn a_definite_answer_alerts_at_once_and_a_good_read_records_the_sha256() {
        let w = world().await;
        let mnt = mount(&w.db, w.owner, "Shelf").await;
        for k in ["a.mkv", "b.mkv", "c.mkv"] {
            catalogued(&w.db, mnt, k).await;
        }
        let notice = w
            .upheld(&[(mnt, "a.mkv"), (mnt, "b.mkv"), (mnt, "c.mkv")])
            .await;
        let (a, b, c) = (
            w.item(notice, "a.mkv").await,
            w.item(notice, "b.mkv").await,
            w.item(notice, "c.mkv").await,
        );
        let mut sub = w.bus.subscribe(Topic::Platform);

        // No mount.
        let r = run_digest(&w.m, &Answer::NoMount, &w.bus, a, false)
            .await
            .unwrap();
        assert!(matches!(r, Outcome::GaveUp(_)), "{r:?}");
        // Object gone from a reachable bucket (only c.mkv is in it).
        let bucket = Arc::new(ObjectStoreStorage::in_memory());
        bucket.put("c.mkv", Bytes::from_static(FILM)).await.unwrap();
        let r = run_digest(&w.m, &Answer::Bucket(bucket.clone()), &w.bus, b, false)
            .await
            .unwrap();
        assert!(matches!(r, Outcome::GaveUp(_)), "{r:?}");
        let sent = alerts(&mut sub).await;
        assert_eq!(sent.len(), 2, "one each: {sent:?}");
        assert!(sent[0].contains(&format!("item {a}")) && sent[1].contains(&format!("item {b}")));

        // A good read.
        let r = run_digest(&w.m, &Answer::Bucket(bucket.clone()), &w.bus, c, false)
            .await
            .unwrap();
        assert!(
            matches!(r, Outcome::Recorded(DigestRecorded::Written { .. })),
            "{r:?}"
        );
        let (_, sha, size) = w.fingerprint(c).await;
        assert_eq!(sha.map(|s| s.len()), Some(64));
        assert_eq!(size, Some(FILM.len() as i64));
        assert_eq!(
            run_digest(&w.m, &Answer::Down, &w.bus, c, false)
                .await
                .unwrap(),
            Outcome::Skipped,
            "an item that has its sha256 is not read again"
        );
        assert!(alerts(&mut sub).await.is_empty());
    }

    /// D-588: an item restored before the job runs is skipped: the bucket is not even opened (it is down here) and no
    /// alert is sent.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn an_item_restored_before_the_job_runs_is_skipped() {
        let w = world().await;
        let mnt = mount(&w.db, w.owner, "Shelf").await;
        catalogued(&w.db, mnt, "films/leak.mkv").await;
        let notice = w.upheld(&[(mnt, "films/leak.mkv")]).await;
        let item = w.item(notice, "films/leak.mkv").await;
        let c = w.caller();
        w.m.restore_notice(&Who::Staff(&c), notice, Some("counter-notice accepted"))
            .await
            .unwrap();
        let mut sub = w.bus.subscribe(Topic::Platform);
        let r = run_digest(&w.m, &Answer::Down, &w.bus, item, true)
            .await
            .unwrap();
        assert_eq!(r, Outcome::Skipped);
        assert!(alerts(&mut sub).await.is_empty());
        assert_eq!(w.fingerprint(item).await.1, None);
    }

    /// D-589: the backfill sends ONE staff alert counting the owners it could not find, and arms once: after the
    /// job ran, a later boot does not run it again.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn the_backfill_alerts_once_for_unresolved_owners_and_is_armed_once() {
        let w = world().await;
        let theirs_owner = account(&w.db, "p57theirs").await;
        let mnt = mount(&w.db, w.owner, "Shelf").await;
        let theirs = mount(&w.db, theirs_owner, "Their shelf").await;
        catalogued(&w.db, mnt, "films/leak.mkv").await;
        let notice = w.upheld(&[(mnt, "films/leak.mkv")]).await;
        sr_db::storage_legal::delist_byte_matches(
            w.db.pool(),
            &uid,
            theirs,
            &[("x/copy.mkv".to_owned(), TAG.to_owned(), FILM.len() as i64)],
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE takedown_items SET subject_account_id = NULL WHERE derived_from IS NOT NULL",
        )
        .execute(w.db.pool())
        .await
        .unwrap();
        sqlx::query("UPDATE storage_mounts SET deleted_at = now() WHERE id = $1")
            .bind(theirs)
            .execute(w.db.pool())
            .await
            .unwrap();
        let mut sub = w.bus.subscribe(Topic::Platform);
        let report = run_backfill(&w.m, &w.bus).await.unwrap();
        assert_eq!(report.unresolved.len(), 1);
        let sent = alerts(&mut sub).await;
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert!(
            sent[0].contains(&format!("TK-{notice}")) && sent[0].starts_with("1 file(s)"),
            "{}",
            sent[0]
        );

        // Armed at every boot, run once.
        sqlx::query("DELETE FROM jobs WHERE id = $1")
            .bind(BACKFILL_JOB_ID)
            .execute(w.db.pool())
            .await
            .unwrap();
        schedule(w.db.pool()).await.unwrap();
        sqlx::query("UPDATE jobs SET state = 'succeeded' WHERE id = $1")
            .bind(BACKFILL_JOB_ID)
            .execute(w.db.pool())
            .await
            .unwrap();
        schedule(w.db.pool()).await.unwrap();
        let state: String = sqlx::query_scalar("SELECT state::text FROM jobs WHERE id = $1")
            .bind(BACKFILL_JOB_ID)
            .fetch_one(w.db.pool())
            .await
            .unwrap();
        assert_eq!(state, "succeeded");
    }

    async fn set_status(w: &World, mount: i64, status: &str) {
        sqlx::query("UPDATE storage_mounts SET status = $2 WHERE id = $1")
            .bind(mount)
            .bind(status)
            .execute(w.db.pool())
            .await
            .unwrap();
    }

    async fn queued_digests(w: &World, item: i64) -> i64 {
        sqlx::query_scalar(
            "SELECT count(*) FROM jobs WHERE kind = $1 AND state IN ('queued', 'running')
               AND (payload ->> 'item_id') = $2",
        )
        .bind(DIGEST_JOB_KIND)
        .bind(item.to_string())
        .fetch_one(w.db.pool())
        .await
        .unwrap()
    }

    /// Ends the digest jobs the uphold queued, as the runner does when the handler returns `Ok`.
    async fn finish_jobs(w: &World) {
        sqlx::query("UPDATE jobs SET state = 'succeeded' WHERE kind = $1")
            .bind(DIGEST_JOB_KIND)
            .execute(w.db.pool())
            .await
            .unwrap();
    }

    async fn item_on(w: &World, notice: i64, mount: i64) -> i64 {
        sqlx::query_scalar("SELECT id FROM takedown_items WHERE notice_id = $1 AND target_id = $2")
            .bind(notice)
            .bind(mount)
            .fetch_one(w.db.pool())
            .await
            .unwrap()
    }

    /// D-591: a file whose size is not the recorded one writes nothing (tag, size and sha256 stay), the job ends, and
    /// staff get ONE alert with both sizes and the restore-and-uphold advice.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn a_file_of_another_size_writes_nothing_and_alerts_once() {
        let w = world().await;
        let mnt = mount(&w.db, w.owner, "Shelf").await;
        catalogued(&w.db, mnt, "films/leak.mkv").await;
        let notice = w.upheld(&[(mnt, "films/leak.mkv")]).await;
        let item = w.item(notice, "films/leak.mkv").await;
        let changed: &'static [u8] = b"replaced by something else entirely";
        let bucket = Arc::new(ObjectStoreStorage::in_memory());
        bucket
            .put("films/leak.mkv", Bytes::from_static(changed))
            .await
            .unwrap();
        let mut sub = w.bus.subscribe(Topic::Platform);
        let r = run_digest(&w.m, &Answer::Bucket(bucket), &w.bus, item, false)
            .await
            .unwrap();
        let read = changed.len() as u64;
        assert_eq!(
            r,
            Outcome::Changed {
                recorded: FILM.len() as i64,
                read
            }
        );
        assert_eq!(
            w.fingerprint(item).await,
            (Some(TAG.into()), None, Some(FILM.len() as i64)),
            "tag and size stay, no sha256"
        );
        let sent = alerts(&mut sub).await;
        assert_eq!(sent.len(), 1, "{sent:?}");
        for needle in [
            format!("TK-{notice}"),
            format!("item {item}"),
            format!("{read} bytes"),
            format!("recorded {}", FILM.len()),
            "restore the item and uphold it again".to_owned(),
        ] {
            assert!(sent[0].contains(&needle), "{needle}: {}", sent[0]);
        }
    }

    /// D-592: a digest job whose mount is `disabled` ends with no alert and writes nothing; a mount that is gone, or
    /// an object missing on an enabled mount, still alerts at once.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn a_disabled_mount_ends_the_job_quietly_but_a_gone_mount_or_object_alerts() {
        let w = world().await;
        let off = mount(&w.db, w.owner, "Off").await;
        let gone = mount(&w.db, w.owner, "Gone").await;
        let on = mount(&w.db, w.owner, "On").await;
        for m in [off, gone, on] {
            catalogued(&w.db, m, "a.mkv").await;
        }
        let notice = w
            .upheld(&[(off, "a.mkv"), (gone, "a.mkv"), (on, "a.mkv")])
            .await;
        let (i_off, i_gone, i_on) = (
            item_on(&w, notice, off).await,
            item_on(&w, notice, gone).await,
            item_on(&w, notice, on).await,
        );
        set_status(&w, off, "disabled").await;
        sqlx::query("UPDATE storage_mounts SET deleted_at = now() WHERE id = $1")
            .bind(gone)
            .execute(w.db.pool())
            .await
            .unwrap();
        let mut sub = w.bus.subscribe(Topic::Platform);

        // The mount cannot be opened (NotFound), as `MountStorages` answers for a disabled or a missing mount.
        let r = run_digest(&w.m, &Answer::NoMount, &w.bus, i_off, false)
            .await
            .unwrap();
        assert_eq!(r, Outcome::Waiting);
        assert!(
            alerts(&mut sub).await.is_empty(),
            "no alert for a switched-off mount"
        );
        assert_eq!(
            w.fingerprint(i_off).await,
            (Some(TAG.into()), None, Some(FILM.len() as i64)),
            "nothing written"
        );

        let r = run_digest(&w.m, &Answer::NoMount, &w.bus, i_gone, false)
            .await
            .unwrap();
        assert!(matches!(r, Outcome::GaveUp(_)), "a deleted mount: {r:?}");
        let empty = Arc::new(ObjectStoreStorage::in_memory());
        let r = run_digest(&w.m, &Answer::Bucket(empty), &w.bus, i_on, false)
            .await
            .unwrap();
        assert!(
            matches!(r, Outcome::GaveUp(_)),
            "object gone on an enabled mount: {r:?}"
        );
        let sent = alerts(&mut sub).await;
        assert_eq!(sent.len(), 2, "{sent:?}");
    }

    /// D-592: when a disabled mount goes `active` one digest job is queued per waiting item (not for an item that has
    /// its sha256, nor another mount's, nor twice), and the job reads the file and records the sha256. Other status
    /// changes queue nothing.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn re_enabling_a_disabled_mount_queues_its_waiting_items_once() {
        use sr_storage::mounts::{MountPatch, MountRepo, MountStatus};
        let w = world().await;
        let off = mount(&w.db, w.owner, "Off").await;
        let other = mount(&w.db, w.owner, "Other").await;
        for k in ["a.mkv", "b.mkv", "c.mkv"] {
            catalogued(&w.db, off, k).await;
        }
        catalogued(&w.db, other, "z.mkv").await;
        let notice = w
            .upheld(&[
                (off, "a.mkv"),
                (off, "b.mkv"),
                (off, "c.mkv"),
                (other, "z.mkv"),
            ])
            .await;
        let (a, b, c) = (
            w.item(notice, "a.mkv").await,
            w.item(notice, "b.mkv").await,
            w.item(notice, "c.mkv").await,
        );
        let on_other = sqlx::query_scalar::<_, i64>(
            "SELECT id FROM takedown_items WHERE notice_id = $1 AND target_id = $2",
        )
        .bind(notice)
        .bind(other)
        .fetch_one(w.db.pool())
        .await
        .unwrap();
        // The uphold's jobs have run and ended quietly; c already has its sha256.
        finish_jobs(&w).await;
        sqlx::query("UPDATE takedown_items SET object_sha256 = $2 WHERE id = $1")
            .bind(c)
            .bind("1".repeat(64))
            .execute(w.db.pool())
            .await
            .unwrap();
        let repo = sr_db::mounts_repo::PgMountRepo::new(w.db.pool().clone());
        set_status(&w, off, "disabled").await;
        set_status(&w, other, "disabled").await;
        let patch = |s| MountPatch {
            status: Some(s),
            ..MountPatch::default()
        };

        // Not a re-enable: disabled again, or failing.
        repo.update(off, patch(MountStatus::Disabled))
            .await
            .unwrap();
        repo.update(off, patch(MountStatus::Failing)).await.unwrap();
        assert_eq!(queued_digests(&w, a).await, 0);
        set_status(&w, off, "disabled").await;

        repo.update(off, patch(MountStatus::Active)).await.unwrap();
        assert_eq!(queued_digests(&w, a).await, 1);
        assert_eq!(queued_digests(&w, b).await, 1);
        assert_eq!(queued_digests(&w, c).await, 0, "it has its sha256");
        assert_eq!(queued_digests(&w, on_other).await, 0, "another mount");

        // Active -> active queues nothing; a job already queued is not doubled.
        repo.update(off, patch(MountStatus::Active)).await.unwrap();
        set_status(&w, off, "disabled").await;
        repo.update(off, patch(MountStatus::Active)).await.unwrap();
        assert_eq!(
            (queued_digests(&w, a).await, queued_digests(&w, b).await),
            (1, 1)
        );

        // The queued job reads the file and records the sha256.
        let bucket = Arc::new(ObjectStoreStorage::in_memory());
        bucket.put("a.mkv", Bytes::from_static(FILM)).await.unwrap();
        let mut sub = w.bus.subscribe(Topic::Platform);
        let r = run_digest(&w.m, &Answer::Bucket(bucket), &w.bus, a, false)
            .await
            .unwrap();
        assert!(
            matches!(r, Outcome::Recorded(DigestRecorded::Written { .. })),
            "{r:?}"
        );
        assert_eq!(w.fingerprint(a).await.1.map(|s| s.len()), Some(64));
        assert!(alerts(&mut sub).await.is_empty());
    }

    /// D-592: a passing test of a disabled mount (`record_test` with `active`) queues the same jobs.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn a_passing_test_of_a_disabled_mount_queues_its_waiting_items() {
        use sr_storage::mounts::{MountRepo, MountStatus};
        let w = world().await;
        let off = mount(&w.db, w.owner, "Off").await;
        catalogued(&w.db, off, "a.mkv").await;
        let notice = w.upheld(&[(off, "a.mkv")]).await;
        let a = w.item(notice, "a.mkv").await;
        finish_jobs(&w).await;
        set_status(&w, off, "disabled").await;
        let repo = sr_db::mounts_repo::PgMountRepo::new(w.db.pool().clone());
        repo.record_test(off, 1, "{}".into(), Some(MountStatus::Active))
            .await
            .unwrap();
        assert_eq!(queued_digests(&w, a).await, 1);
    }

    /// D-592: deleting a disabled mount queues its waiting items once; the job then finds the mount gone and alerts
    /// once. Deleting an enabled mount queues nothing.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn deleting_a_disabled_mount_yields_the_one_alert() {
        use sr_storage::mounts::MountRepo;
        let w = world().await;
        let off = mount(&w.db, w.owner, "Off").await;
        let on = mount(&w.db, w.owner, "On").await;
        catalogued(&w.db, off, "a.mkv").await;
        catalogued(&w.db, on, "a.mkv").await;
        let notice = w.upheld(&[(off, "a.mkv"), (on, "a.mkv")]).await;
        let item_off = item_on(&w, notice, off).await;
        let item_on_ = item_on(&w, notice, on).await;
        finish_jobs(&w).await;
        set_status(&w, off, "disabled").await;
        let repo = sr_db::mounts_repo::PgMountRepo::new(w.db.pool().clone());
        assert!(repo.soft_delete(on).await.unwrap());
        assert_eq!(queued_digests(&w, item_on_).await, 0, "an enabled mount");
        assert_eq!(queued_digests(&w, item_off).await, 0);
        assert!(repo.soft_delete(off).await.unwrap());
        assert_eq!(queued_digests(&w, item_off).await, 1);

        let mut sub = w.bus.subscribe(Topic::Platform);
        let r = run_digest(&w.m, &Answer::NoMount, &w.bus, item_off, false)
            .await
            .unwrap();
        assert!(matches!(r, Outcome::GaveUp(_)), "{r:?}");
        let sent = alerts(&mut sub).await;
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert!(sent[0].contains(&format!("item {item_off}")), "{}", sent[0]);
    }
}
