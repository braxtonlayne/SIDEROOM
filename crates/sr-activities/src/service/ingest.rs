//! `ingest_stored_zip` (SEC-AUD-173, D-654): check a zip that is already in the object store and, if it passes,
//! store its files and the zip itself under the bundle's keys and create the draft bundle.
//!
//! The package is never held in memory. The staged zip is hashed as a stream; its index is read with ranged `get`s
//! ([`crate::zipstore`]) and the first pass of [`crate::bundle`] refuses every rule that needs no data before a byte
//! is inflated; then each file is inflated one at a time and written to the store (a file of at most one part
//! with `put`, a larger one through `multipart_create` in parts), counting real bytes. Only the manifest
//! (`sideroom.json`, at most 8 KiB) is kept.
//!
//! **Order and cleanup** (what the collector, `ActivityService::gc`, allows):
//!
//! 1. Everything that can be refused from the index is refused first, so most refusals touch nothing.
//! 2. Then a *claim row* is committed: a bundle row in state `superseded` with `superseded_at = now()`, empty
//!    manifest and size 0. The collector keeps the objects of a bundle superseded within its grace period, so from
//!    this commit on it cannot delete a file of the check in flight, and the claim is invisible in the history
//!    (`bundles_of` hides superseded rows with an empty manifest). The collector lists objects before it reads
//!    the live set and no object is written before the claim is committed, so a listed object always has a live row.
//! 3. The files are stored, then the zip. A refusal or failure here deletes every object this check wrote (after
//!    aborting a multipart upload in progress) and the claim row: a half-way failure leaves no object and no row.
//! 4. One transaction makes the claim the draft bundle (superseding earlier drafts and rejected bundles, writing the
//!    listing, capabilities, tier, tip links, code hash and `connect` hosts) and refreshes the review state.
//!
//! If the same zip is already a live bundle of the application (a repeated upload) there is no claim: the objects are
//! written again over the live bundle, which repairs a half-written one as before, and a failure deletes nothing.
//! A check that dies between 2 and 4 (a crash) leaves the claim, which the collector deletes with the objects after the
//! grace period. The staged object is deleted on success and on refusal; after a failure that is not the package's
//! fault it stays, so the job can try again.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use bytes::Bytes;
use futures::StreamExt;
use futures::future::{Either, select};
use serde_json::json;
use sha2::{Digest, Sha256};
use sr_storage::{MultipartSession, ObjectStorage, StorageError};

use super::{ActivityService, object_key, zip_key};
use crate::arcade::Manifest;
use crate::bundle::{self, CheckError, EntrySink, PlanFile};
use crate::error::{ActivityError, ActivityResult};
use crate::model::{BundleDetail, BundleEntry};
use crate::package::{IngestProgress, PackageLimits};
use crate::sandbox::{self, CodeHasher};
use crate::zipstore::ZipStoreReader;

/// How often the progress callback runs at most.
const PROGRESS_EVERY: Duration = Duration::from_secs(1);

enum Out {
    Small(Vec<u8>),
    Multi {
        session: Box<dyn MultipartSession>,
        buf: Vec<u8>,
    },
}

struct Cur {
    key: String,
    out: Out,
    manifest: bool,
}

/// Writes the inflated files to the object store from the blocking thread of the second pass.
struct StoreSink {
    handle: tokio::runtime::Handle,
    storage: Arc<dyn ObjectStorage>,
    app: i64,
    sha: String,
    part: usize,
    cur: Option<Cur>,
    manifest_bytes: Vec<u8>,
    manifest: Option<Manifest>,
    connect: Vec<String>,
    hasher: CodeHasher,
    /// Objects fully written by this check.
    written: Vec<String>,
}

fn failed(e: impl std::fmt::Display) -> CheckError {
    CheckError::Failed(e.to_string())
}

impl StoreSink {
    /// Abort a multipart upload in progress (after a refusal or failure).
    fn abort(&mut self) {
        if let Some(Cur {
            out: Out::Multi { mut session, .. },
            ..
        }) = self.cur.take()
        {
            let _ = self.handle.block_on(session.abort());
        }
    }
}

impl EntrySink for StoreSink {
    fn begin(&mut self, f: &PlanFile) -> Result<(), CheckError> {
        self.hasher.begin_file(&f.path, f.size);
        let key = object_key(self.app, &self.sha, &f.path);
        let out = if f.size <= self.part as u64 {
            Out::Small(Vec::with_capacity(usize::try_from(f.size).unwrap_or(0)))
        } else {
            let session = self
                .handle
                .block_on(self.storage.multipart_create(&key))
                .map_err(failed)?;
            Out::Multi {
                session,
                buf: Vec::with_capacity(self.part),
            }
        };
        self.cur = Some(Cur {
            key,
            out,
            manifest: f.path == bundle::MANIFEST,
        });
        Ok(())
    }

    fn write(&mut self, _f: &PlanFile, data: &[u8]) -> Result<(), CheckError> {
        self.hasher.update(data);
        let part = self.part;
        let Some(cur) = self.cur.as_mut() else {
            return Err(failed("no file is open"));
        };
        if cur.manifest {
            // The second pass counted the file against its declared size, which the manifest rule caps at 8 KiB
            // when parsed; never keep more than that plus one byte.
            let room = (bundle::MAX_MANIFEST_BYTES + 1).saturating_sub(self.manifest_bytes.len());
            self.manifest_bytes
                .extend_from_slice(&data[..data.len().min(room)]);
        }
        match &mut cur.out {
            Out::Small(v) => v.extend_from_slice(data),
            Out::Multi { session, buf } => {
                buf.extend_from_slice(data);
                while buf.len() >= part {
                    let rest = buf.split_off(part);
                    let full = std::mem::replace(buf, rest);
                    self.handle
                        .block_on(session.put_part(Bytes::from(full)))
                        .map_err(failed)?;
                }
            }
        }
        Ok(())
    }

    fn finish(&mut self, _f: &PlanFile) -> Result<(), CheckError> {
        let Some(cur) = self.cur.take() else {
            return Err(failed("no file is open"));
        };
        match cur.out {
            Out::Small(v) => {
                self.handle
                    .block_on(self.storage.put(&cur.key, Bytes::from(v)))
                    .map_err(failed)?;
            }
            Out::Multi {
                mut session,
                mut buf,
            } => {
                let done = (|| {
                    if !buf.is_empty() {
                        self.handle
                            .block_on(session.put_part(Bytes::from(std::mem::take(&mut buf))))?;
                    }
                    self.handle.block_on(session.complete())
                })();
                if let Err(e) = done {
                    let _ = self.handle.block_on(session.abort());
                    return Err(failed(e));
                }
            }
        }
        self.written.push(cur.key);
        if cur.manifest {
            // Checked before the next file is stored: a bad manifest refuses the package at once.
            let m = bundle::MANIFEST;
            let manifest = crate::arcade::parse_manifest(&self.manifest_bytes)?;
            self.connect = sandbox::parse_connect(&self.manifest_bytes)
                .map_err(|why| bundle::BundleError::at(m, why))?;
            self.manifest = Some(manifest);
        }
        Ok(())
    }
}

/// What a finished check hands back.
struct Checked {
    total: u64,
    manifest: Manifest,
    connect: Vec<String>,
    code_hash: Option<String>,
    written: Vec<String>,
}

/// The claim of a check on the bundle table.
struct Claim {
    id: i64,
    /// A claim row was created (a new bundle); `false` for a repeated upload of a live bundle.
    fresh: bool,
}

impl ActivityService {
    /// Check the zip stored at `staged_key` (any key under `activities/{app}/uploads/`) and, if it passes, store
    /// its files and the zip under the bundle's keys and create the draft bundle exactly as `upload_bundle` does.
    /// The staged object is deleted on success and on refusal. A refusal is `ActivityError::Invalid("bundle", text)`
    /// and names the entry or the limit. `progress` is called at most once a second while the files are checked.
    pub async fn ingest_stored_zip(
        &self,
        app: i64,
        user: i64,
        staged_key: &str,
        progress: Option<&dyn IngestProgress>,
    ) -> ActivityResult<BundleDetail> {
        let storage = self.storage()?.clone();
        if !staged_key.starts_with(&format!("activities/{app}/uploads/"))
            || staged_key.contains("..")
        {
            return Err(ActivityError::Invalid(
                "bundle",
                "The upload is not in this application's staging area.".to_owned(),
            ));
        }
        let result = self.ingest(&storage, app, user, staged_key, progress).await;
        if matches!(&result, Ok(_) | Err(ActivityError::Invalid(..))) {
            let _ = storage.delete(staged_key).await;
        }
        result
    }

    async fn ingest(
        &self,
        storage: &Arc<dyn ObjectStorage>,
        app: i64,
        user: i64,
        staged: &str,
        progress: Option<&dyn IngestProgress>,
    ) -> ActivityResult<BundleDetail> {
        let limits = self.package_limits(app).await?;
        let refused = |text: String| ActivityError::Invalid("bundle", text);
        let info = match storage.head(staged).await {
            Ok(i) => i,
            Err(StorageError::NotFound(_)) => {
                return Err(ActivityError::Conflict(
                    "The uploaded file is gone. Upload it again.".to_owned(),
                ));
            }
            Err(e) => return Err(ActivityError::Storage(e.to_string())),
        };
        let zip_len = info.size;
        // The zip's own length first: nothing is read for a zip over the limit.
        bundle::check_zip_len(zip_len, &limits).map_err(|e| refused(e.text()))?;
        let sha = self.hash_object(storage, staged).await?;

        // First pass: the index.
        let handle = tokio::runtime::Handle::current();
        let (archive, plan) = {
            let (storage, key, limits) = (storage.clone(), staged.to_owned(), limits);
            tokio::task::spawn_blocking(move || {
                let reader = ZipStoreReader::for_object(storage, &key, zip_len, handle);
                bundle::open_and_plan(reader, zip_len, &limits)
            })
            .await
            .map_err(|e| ActivityError::Storage(e.to_string()))?
            .map_err(|e| match e {
                CheckError::Refused(b) => refused(b.text()),
                CheckError::Failed(m) => ActivityError::Storage(m),
            })?
        };

        // The claim, then the files, then the zip.
        let claim = self.claim(app, user, &sha).await?;
        let stored = self
            .store_files(storage, app, &sha, archive, &plan, limits, progress)
            .await;
        let checked = match stored {
            Ok(c) => c,
            Err((e, written)) => {
                self.undo(storage, app, &claim, &sha, written).await;
                return Err(e);
            }
        };
        let zk = zip_key(app, &sha);
        if let Err(e) = self.copy_object(storage, staged, &zk, zip_len).await {
            let mut written = checked.written.clone();
            written.push(zk);
            self.undo(storage, app, &claim, &sha, written).await;
            return Err(e);
        }

        let entries: Vec<BundleEntry> = plan
            .files
            .iter()
            .map(|f| BundleEntry {
                path: f.path.clone(),
                size: i64::try_from(f.size).unwrap_or(i64::MAX),
                content_type: f.content_type.to_owned(),
            })
            .collect();
        let id = match self
            .finalize(app, user, &claim, &sha, &plan, &entries, &checked)
            .await
        {
            Ok(id) => id,
            Err(e) => {
                let mut written = checked.written.clone();
                written.push(zip_key(app, &sha));
                self.undo(storage, app, &claim, &sha, written).await;
                return Err(e);
            }
        };
        self.bundle_detail(app, id).await
    }

    /// SHA-256 (lowercase hex) of a stored object, read as a stream.
    async fn hash_object(
        &self,
        storage: &Arc<dyn ObjectStorage>,
        key: &str,
    ) -> ActivityResult<String> {
        let mut body = storage
            .get_stream(key, None)
            .await
            .map_err(|e| ActivityError::Storage(e.to_string()))?
            .body;
        let mut h = Sha256::new();
        while let Some(chunk) = body.next().await {
            h.update(chunk.map_err(|e| ActivityError::Storage(e.to_string()))?);
        }
        Ok(data_encoding::HEXLOWER.encode(&h.finalize()))
    }

    /// Commit the claim of a check on the bundle table (see the module header, step 2).
    async fn claim(&self, app: i64, user: i64, sha: &str) -> ActivityResult<Claim> {
        let mut tx = self.pool().begin().await?;
        Self::lock_row(&mut tx, app).await?;
        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM application_activity_bundles \
             WHERE application_id = $1 AND sha256 = $2 AND state <> 'superseded'",
        )
        .bind(app)
        .bind(sha)
        .fetch_optional(&mut *tx)
        .await?;
        let claim = match existing {
            Some(id) => Claim { id, fresh: false },
            None => {
                let id = (self.inner.ids)();
                sqlx::query(
                    "INSERT INTO application_activity_bundles \
                     (id, application_id, sha256, byte_size, entry_count, uploaded_by, state, superseded_at) \
                     VALUES ($1, $2, $3, 0, 0, $4, 'superseded', now())",
                )
                .bind(id)
                .bind(app)
                .bind(sha)
                .bind(user)
                .execute(&mut *tx)
                .await?;
                Claim { id, fresh: true }
            }
        };
        tx.commit().await?;
        Ok(claim)
    }

    /// The second pass into the store, on a blocking thread, with progress on the async side.
    #[allow(clippy::too_many_arguments)]
    async fn store_files(
        &self,
        storage: &Arc<dyn ObjectStorage>,
        app: i64,
        sha: &str,
        archive: zip::ZipArchive<ZipStoreReader>,
        plan: &bundle::Plan,
        limits: PackageLimits,
        progress: Option<&dyn IngestProgress>,
    ) -> Result<Checked, (ActivityError, Vec<String>)> {
        let done = Arc::new(AtomicU64::new(0));
        let part = self
            .inner
            .part_bytes
            .load(std::sync::atomic::Ordering::Relaxed);
        let mut sink = StoreSink {
            handle: tokio::runtime::Handle::current(),
            storage: storage.clone(),
            app,
            sha: sha.to_owned(),
            part,
            cur: None,
            manifest_bytes: Vec::new(),
            manifest: None,
            connect: Vec::new(),
            hasher: CodeHasher::new(),
            written: Vec::new(),
        };
        let plan_for_task = plan.clone();
        let done_for_task = done.clone();
        let mut archive = archive;
        let task = tokio::task::spawn_blocking(move || {
            let r = bundle::inflate(&mut archive, &plan_for_task, &limits, &mut sink, &mut |n| {
                done_for_task.store(n, Ordering::Relaxed);
            });
            if r.is_err() {
                sink.abort();
            }
            (r, sink)
        });
        let total = plan.declared_total;
        let mut task = task;
        let mut tick =
            tokio::time::interval_at(tokio::time::Instant::now() + PROGRESS_EVERY, PROGRESS_EVERY);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let joined = loop {
            match select(Box::pin(tick.tick()), &mut task).await {
                Either::Left(_) => {
                    if let Some(p) = progress {
                        p.bytes(done.load(Ordering::Relaxed), total);
                    }
                }
                Either::Right((r, _)) => break r,
            }
        };
        let (result, sink) =
            joined.map_err(|e| (ActivityError::Storage(e.to_string()), Vec::new()))?;
        let written = sink.written.clone();
        match result {
            Ok(total) => {
                if let Some(p) = progress {
                    p.bytes(total, total);
                }
                Ok(Checked {
                    total,
                    manifest: sink.manifest.clone().unwrap_or_default(),
                    connect: sink.connect.clone(),
                    code_hash: sink.hasher.finish(),
                    written,
                })
            }
            Err(CheckError::Refused(b)) => {
                Err((ActivityError::Invalid("bundle", b.text()), written))
            }
            Err(CheckError::Failed(m)) => Err((ActivityError::Storage(m), written)),
        }
    }

    /// Copy a stored object to another key as a stream: one `put` up to one part, else a multipart upload.
    async fn copy_object(
        &self,
        storage: &Arc<dyn ObjectStorage>,
        from: &str,
        to: &str,
        size: u64,
    ) -> ActivityResult<()> {
        let part = self
            .inner
            .part_bytes
            .load(std::sync::atomic::Ordering::Relaxed);
        let storage_err = |e: StorageError| ActivityError::Storage(e.to_string());
        let mut body = storage
            .get_stream(from, None)
            .await
            .map_err(storage_err)?
            .body;
        let mut buf: Vec<u8> = Vec::new();
        if size <= part as u64 {
            while let Some(chunk) = body.next().await {
                buf.extend_from_slice(&chunk.map_err(storage_err)?);
            }
            return storage.put(to, Bytes::from(buf)).await.map_err(storage_err);
        }
        let mut session = storage.multipart_create(to).await.map_err(storage_err)?;
        let copied: Result<(), StorageError> = async {
            while let Some(chunk) = body.next().await {
                buf.extend_from_slice(&chunk?);
                while buf.len() >= part {
                    let rest = buf.split_off(part);
                    let full = std::mem::replace(&mut buf, rest);
                    session.put_part(Bytes::from(full)).await?;
                }
            }
            if !buf.is_empty() {
                session
                    .put_part(Bytes::from(std::mem::take(&mut buf)))
                    .await?;
            }
            session.complete().await
        }
        .await;
        if let Err(e) = copied {
            let _ = session.abort().await;
            return Err(storage_err(e));
        }
        Ok(())
    }

    /// A check failed after its claim: delete what it wrote and the claim row (a repeated upload of a live bundle
    /// deletes nothing).
    async fn undo(
        &self,
        storage: &Arc<dyn ObjectStorage>,
        app: i64,
        claim: &Claim,
        sha: &str,
        written: Vec<String>,
    ) {
        if !claim.fresh {
            return;
        }
        // Another live bundle with the same hash (a parallel upload that finished first) owns those keys.
        let shared: Result<Option<i64>, sqlx::Error> = sqlx::query_scalar(
            "SELECT id FROM application_activity_bundles \
             WHERE application_id = $1 AND sha256 = $2 AND id <> $3 AND state <> 'superseded'",
        )
        .bind(app)
        .bind(sha)
        .bind(claim.id)
        .fetch_optional(self.pool())
        .await;
        if matches!(shared, Ok(None)) {
            for key in written {
                if let Err(e) = storage.delete(&key).await {
                    tracing::warn!(error = %e, key, "deleting the objects of a refused package failed");
                }
            }
        }
        if let Err(e) = sqlx::query("DELETE FROM application_activity_bundles WHERE id = $1")
            .bind(claim.id)
            .execute(self.pool())
            .await
        {
            tracing::warn!(error = %e, "deleting the claim of a refused package failed");
        }
    }

    /// Step 4: the claim becomes the draft bundle.
    #[allow(clippy::too_many_arguments)]
    async fn finalize(
        &self,
        app: i64,
        user: i64,
        claim: &Claim,
        sha: &str,
        plan: &bundle::Plan,
        entries: &[BundleEntry],
        c: &Checked,
    ) -> ActivityResult<i64> {
        let mut tx = self.pool().begin().await?;
        Self::lock_row(&mut tx, app).await?;
        let mut id = claim.id;
        let mut fresh = claim.fresh;
        if fresh {
            // A parallel upload of the same zip may have finished while this one was checking.
            let other: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM application_activity_bundles \
                 WHERE application_id = $1 AND sha256 = $2 AND state <> 'superseded' AND id <> $3",
            )
            .bind(app)
            .bind(sha)
            .bind(claim.id)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some(other) = other {
                sqlx::query("DELETE FROM application_activity_bundles WHERE id = $1")
                    .bind(claim.id)
                    .execute(&mut *tx)
                    .await?;
                id = other;
                fresh = false;
            }
        }
        if fresh {
            sqlx::query(
                "UPDATE application_activity_bundles SET state = 'superseded' \
                 WHERE application_id = $1 AND state IN ('draft', 'rejected')",
            )
            .bind(app)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE application_activity_bundles SET state = 'draft', byte_size = $2, entry_count = $3, \
                 manifest = $4, uploaded_by = $5, capabilities = $6, tier = $7, tip_links = $8, \
                 uploaded_at = now() WHERE id = $1",
            )
            .bind(id)
            .bind(i64::try_from(c.total).unwrap_or(i64::MAX))
            .bind(i32::try_from(plan.entries).unwrap_or(i32::MAX))
            .bind(json!(entries))
            .bind(user)
            .bind(&c.manifest.capabilities)
            .bind(c.manifest.tier.map(|t| t.as_str()))
            .bind(json!(c.manifest.tip_links))
            .execute(&mut *tx)
            .await?;
        }
        // The code hash and the sandbox's `connect` list come from the files, not from the zip's hash (ARC-SBX-001).
        sqlx::query(
            "UPDATE application_activity_bundles SET code_sha256 = $2, connect_hosts = $3 WHERE id = $1",
        )
        .bind(id)
        .bind(c.code_hash.as_deref())
        .bind(&c.connect)
        .execute(&mut *tx)
        .await?;
        Self::refresh_state(&mut tx, app).await?;
        tx.commit().await?;
        Ok(id)
    }
}
