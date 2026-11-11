//! The sha256 of an upheld storage object, taken by a job after the uphold (D-588), and the one-off backfill that
//! brings earlier takedown items under D-583 and D-584 (D-589).
//!
//! `disable_item` records the catalog tag and size of the object and enqueues one `storage.takedown_digest` job per
//! storage-object item ([`digest_job`]). The server's handler streams the object with no time cap and hands the
//! sha256 and the byte count to [`Moderation::record_digest`], which writes `object_sha256` when the item is still
//! `disabled` (an item restored before the job ran is skipped), copies it to the items derived from it and, on a
//! size mismatch, drops the catalog tag (it described other bytes). The indexer is unchanged: its sieve sees the
//! item once the sha256 is there.
//!
//! [`Moderation::backfill_byte_takedowns`] is the body of the D-589 one-off boot job: (1) every `disabled` derived
//! storage-object item with no `subject_account_id` gets the owner the D-584 resolver gives (never a person, so no
//! strike) and `notify_byte_matches` sends the removal DM once; an item whose owner cannot be resolved is left and
//! returned for one staff alert; (2) every `disabled` storage-object item with a size and no sha256 gets a digest job.

use std::time::Duration;

use serde_json::json;
use sqlx::Row;
use sr_db::jobs::{self, NewJob};
use sr_db::storage_legal;

use crate::error::{ModError, ModResult};
use crate::service::Moderation;

/// Kind of the digest job.
pub const DIGEST_JOB_KIND: &str = "storage.takedown_digest";
/// Attempts of a digest job. The backoff doubles from 5 s and is capped at an hour, so 14 attempts reach about
/// four hours: a bucket that is down for an afternoon is waited out before staff are told.
pub const DIGEST_JOB_ATTEMPTS: i32 = 14;
/// Reserved id of the D-589 one-off job (the first number of its migration range; below any Snowflake).
pub const BACKFILL_JOB_ID: i64 = 2540;
/// Kind of the D-589 one-off job.
pub const BACKFILL_JOB_KIND: &str = "storage.takedown_backfill";
/// How long after boot the D-589 one-off waits, so the server is serving first.
pub const BACKFILL_BOOT_DELAY: Duration = Duration::from_secs(90);

const _: () = assert!(
    BACKFILL_JOB_ID < sr_db::jobs::SINGLETON_ID_LIMIT,
    "reserved ids stay below the snowflakes"
);

/// The object a digest job has to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestTarget {
    pub item_id: i64,
    pub notice_id: i64,
    pub mount_id: i64,
    /// Mount-relative key.
    pub key: String,
    /// The item's label, for the staff alert.
    pub label: String,
}

/// What [`Moderation::record_digest`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestRecorded {
    /// The item was restored, rejected, already has a sha256 or is gone: nothing was written.
    Skipped,
    /// The sha256 is on the item. `tag_dropped`: the item's size differed from the bytes read, so its catalog tag
    /// (which described other bytes) was dropped and the size now is the bytes read. `copied`: derived items given
    /// the sha256.
    Written { tag_dropped: bool, copied: u64 },
}

/// What one backfill pass did (D-589).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackfillReport {
    /// Derived items that got an owner.
    pub owners_assigned: usize,
    /// `(notice id, item id)` of derived items whose owner could not be resolved: left as they are.
    pub unresolved: Vec<(i64, i64)>,
    /// Owners sent the removal DM.
    pub told: i64,
    /// Digest jobs enqueued.
    pub digests_enqueued: usize,
}

/// The job that reads `item_id`'s object, ready to enqueue.
pub fn digest_job(id: i64, item_id: i64) -> NewJob {
    let mut j = NewJob::new(id, DIGEST_JOB_KIND, json!({ "item_id": item_id }));
    j.max_attempts = DIGEST_JOB_ATTEMPTS;
    j
}

fn db(e: sr_db::DbError) -> ModError {
    ModError::Storage(e.to_string())
}

impl Moderation {
    /// What a digest job for `item_id` has to read; `None` when there is nothing to do: the item is not a
    /// `disabled` storage-object item (restored or rejected before the job ran, or gone) or already has a sha256.
    pub async fn digest_target(&self, item_id: i64) -> ModResult<Option<DigestTarget>> {
        let Some(r) = sqlx::query(
            "SELECT id, notice_id, target_id, object_key, label FROM takedown_items \
             WHERE id = $1 AND kind = 'storage_object' AND state = 'disabled' \
               AND object_key IS NOT NULL AND object_sha256 IS NULL",
        )
        .bind(item_id)
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };
        Ok(Some(DigestTarget {
            item_id: r.try_get("id")?,
            notice_id: r.try_get("notice_id")?,
            mount_id: r.try_get("target_id")?,
            key: r.try_get("object_key")?,
            label: r.try_get("label")?,
        }))
    }

    /// Write the sha256 and byte count a digest job read for `item_id` (D-588), if the item is still `disabled` and
    /// has no sha256. When the item's size equals the bytes read the sha256 is added to its fingerprint; when it
    /// does not (the catalog was stale, or had no row) the item takes the bytes read as its size and its catalog tag
    /// is dropped, as the inline read did. Items derived from it whose size equals the bytes read get the same
    /// sha256.
    pub async fn record_digest(
        &self,
        item_id: i64,
        sha256: &str,
        bytes: u64,
    ) -> ModResult<DigestRecorded> {
        let size = i64::try_from(bytes).unwrap_or(i64::MAX);
        let mut tx = self.pool.begin().await?;
        let Some(r) = sqlx::query(
            "SELECT object_size FROM takedown_items \
             WHERE id = $1 AND kind = 'storage_object' AND state = 'disabled' AND object_sha256 IS NULL \
             FOR UPDATE",
        )
        .bind(item_id)
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Ok(DigestRecorded::Skipped);
        };
        let had: Option<i64> = r.try_get("object_size")?;
        let same = had == Some(size);
        if same {
            sqlx::query("UPDATE takedown_items SET object_sha256 = $2 WHERE id = $1")
                .bind(item_id)
                .bind(sha256)
                .execute(&mut *tx)
                .await?;
        } else {
            sqlx::query(
                "UPDATE takedown_items SET object_hash = NULL, object_sha256 = $2, object_size = $3 WHERE id = $1",
            )
            .bind(item_id)
            .bind(sha256)
            .bind(size)
            .execute(&mut *tx)
            .await?;
        }
        let copied = sqlx::query(
            "UPDATE takedown_items SET object_sha256 = $2 \
             WHERE derived_from = $1 AND state = 'disabled' AND object_sha256 IS NULL AND object_size = $3",
        )
        .bind(item_id)
        .bind(sha256)
        .bind(size)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;
        Ok(DigestRecorded::Written {
            // `had == None` is a catalog with no row: there was no tag to drop.
            tag_dropped: had.is_some() && !same,
            copied,
        })
    }

    /// The D-589 backfill (see the module doc). Safe to run again: it only touches what is still missing.
    pub async fn backfill_byte_takedowns(&self) -> ModResult<BackfillReport> {
        let mut report = BackfillReport::default();

        // (1) Owners of derived items that have none.
        let rows = sqlx::query(
            "SELECT id, notice_id, target_id, object_key FROM takedown_items \
             WHERE kind = 'storage_object' AND state = 'disabled' AND derived_from IS NOT NULL \
               AND subject_account_id IS NULL AND object_key IS NOT NULL \
             ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;
        for r in rows {
            let (item, notice, mount, key): (i64, i64, i64, String) = (
                r.try_get("id")?,
                r.try_get("notice_id")?,
                r.try_get("target_id")?,
                r.try_get("object_key")?,
            );
            let mut conn = self.pool.acquire().await?;
            let owner = storage_legal::owner_of_object(&mut conn, mount, &key)
                .await
                .map_err(db)?;
            drop(conn);
            match owner {
                Some(owner) => {
                    let set = sqlx::query(
                        "UPDATE takedown_items SET subject_account_id = $2 \
                         WHERE id = $1 AND subject_account_id IS NULL AND state = 'disabled'",
                    )
                    .bind(item)
                    .bind(owner)
                    .execute(&self.pool)
                    .await?
                    .rows_affected();
                    report.owners_assigned += usize::try_from(set).unwrap_or(0);
                }
                None => report.unresolved.push((notice, item)),
            }
        }
        // The removal DM, once each (`notified_at`); one call takes up to 50 notices.
        for _ in 0..200 {
            let told = self.notify_byte_matches().await;
            if told == 0 {
                break;
            }
            report.told += told;
        }

        // (2) A digest job for every disabled item with a size and no sha256 that has none queued already.
        let items: Vec<i64> = sqlx::query_scalar(
            "SELECT i.id FROM takedown_items i \
             WHERE i.kind = 'storage_object' AND i.state = 'disabled' AND i.object_size IS NOT NULL \
               AND i.object_sha256 IS NULL AND i.object_key IS NOT NULL \
               AND NOT EXISTS (SELECT 1 FROM jobs j WHERE j.kind = $1 AND j.state IN ('queued', 'running') \
                               AND (j.payload ->> 'item_id') = i.id::text) \
             ORDER BY i.id",
        )
        .bind(DIGEST_JOB_KIND)
        .fetch_all(&self.pool)
        .await?;
        for chunk in items.chunks(200) {
            let batch: Vec<NewJob> = chunk
                .iter()
                .map(|item| digest_job(self.next_id(), *item))
                .collect();
            jobs::enqueue_many(&self.pool, &batch).await.map_err(db)?;
            report.digests_enqueued += batch.len();
        }
        Ok(report)
    }
}
