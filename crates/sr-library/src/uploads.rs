//! HTTP upload path and background ingest of chapters (LIB-ING-002/004/005), and the storage
//! garbage collector.
//!
//! Flow: [`LibraryService::begin_upload`] creates an upload row and a presigned `PUT` URL;
//! the client sends the file straight to object storage; [`LibraryService::complete_upload`]
//! checks the object and queues the `library.ingest` job; [`LibraryService::run_ingest`] (the
//! job handler) converts it off the request path and writes the outcome to the row the client
//! polls. Pages are stored under a per-upload revision folder, so a replaced chapter never
//! overwrites live objects; whatever is no longer referenced is collected by
//! [`LibraryService::run_gc`].

use std::io::{Seek, SeekFrom};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use serde_json::{Value, json};
use sr_storage::{ObjectStorage, PresignMethod};
use tokio::io::AsyncWriteExt;

use crate::error::{LibraryError, LibraryResult};
use crate::ingest::archive::ArchiveLimits;
use crate::ingest::pipeline::{IngestLimits, IngestedChapter, ingest_archive_rev};
use crate::ingest::{IngestError, epub, pdf};
use crate::model::*;
use crate::multipart::MultipartWiring;
use crate::rules;
use crate::service::{Actor, LibraryService};

/// How long a presigned upload URL works.
pub const UPLOAD_URL_TTL: Duration = Duration::from_secs(3600);
/// An upload that was never completed expires after this long.
pub const UPLOAD_EXPIRY_MS: i64 = 24 * 60 * 60 * 1000;
/// Objects queued for collection wait this long first, so an in-flight change that is about
/// to reference them can commit.
pub const GC_GRACE_MS: i64 = 10 * 60 * 1000;
/// Most queued objects handled per sweep.
pub const GC_BATCH: u32 = 500;
/// A cached export nobody downloaded for this long is deleted (it is rebuilt on demand).
pub const EXPORT_IDLE_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// Prefix of the cached chapter exports in the object store.
pub const EXPORT_PREFIX: &str = "library/exports/";
/// Default largest upload.
pub const DEFAULT_MAX_UPLOAD_MB: u64 = 512;

const ARCHIVE_EXTS: [&str; 6] = ["cbz", "zip", "cb7", "7z", "cbr", "rar"];

/// Operator limits for one ingest, from the platform settings
/// (`library.ingest.rar_enabled`, `library.ingest.max_upload_mb`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IngestSettingsSnapshot {
    pub rar_enabled: bool,
    pub max_upload_bytes: u64,
}

impl Default for IngestSettingsSnapshot {
    fn default() -> Self {
        Self {
            rar_enabled: true,
            max_upload_bytes: DEFAULT_MAX_UPLOAD_MB * 1024 * 1024,
        }
    }
}

/// Source of the operator settings, read afresh for every upload and every job.
#[async_trait]
pub trait IngestSettings: Send + Sync {
    async fn snapshot(&self) -> IngestSettingsSnapshot;
}

/// Fixed settings (tests, tools).
pub struct StaticIngestSettings(pub IngestSettingsSnapshot);

#[async_trait]
impl IngestSettings for StaticIngestSettings {
    async fn snapshot(&self) -> IngestSettingsSnapshot {
        self.0
    }
}

/// Queues the `library.ingest` job for an upload.
#[async_trait]
pub trait IngestQueue: Send + Sync {
    async fn enqueue(&self, upload_id: i64) -> Result<(), String>;
}

/// Writes the outcome of background ingests to the staff access log (the begin and complete
/// calls are logged by the HTTP layer).
#[async_trait]
pub trait LibraryAudit: Send + Sync {
    async fn record(&self, user: i64, action: &str, target: (&str, i64), detail: Value);
}

/// Storage, settings and queue of the upload path.
#[derive(Clone)]
pub struct IngestWiring {
    pub storage: Arc<dyn ObjectStorage>,
    pub settings: Arc<dyn IngestSettings>,
    pub queue: Arc<dyn IngestQueue>,
}

/// Storage port of uploads and the collector queue. `sr-db` implements it.
#[async_trait]
pub trait UploadRepository: Send + Sync {
    /// Insert the upload; `Conflict` when the chapter already has one in flight.
    async fn create_upload(&self, u: &UploadRecord) -> LibraryResult<()>;
    async fn get_upload(&self, id: i64) -> LibraryResult<Option<UploadRecord>>;
    /// Newest first.
    async fn list_uploads(&self, chapter: i64, limit: u32) -> LibraryResult<Vec<UploadRecord>>;
    /// Move to `to` when currently in one of `from`. Returns the updated row, or `None` when the
    /// upload is gone or in another state.
    async fn transition_upload(
        &self,
        id: i64,
        from: &[UploadStatus],
        to: UploadStatus,
    ) -> LibraryResult<Option<UploadRecord>>;
    /// Record the outcome (`Done` or `Failed`) of a running upload.
    async fn finish_upload(
        &self,
        id: i64,
        status: UploadStatus,
        error: Option<&str>,
        warnings: &[String],
        page_count: Option<i32>,
        metadata: &Value,
    ) -> LibraryResult<Option<UploadRecord>>;
    /// Set the original file of an EPUB/PDF chapter; the previous one is queued for collection.
    /// `false` when the chapter is gone.
    async fn set_chapter_source(
        &self,
        chapter: i64,
        key: &str,
        page_count: i32,
    ) -> LibraryResult<bool>;
    /// Uploads still `awaiting_upload` that were created before `before_ms`, marked `expired`.
    async fn expire_uploads(&self, before_ms: i64, limit: u32) -> LibraryResult<Vec<UploadRecord>>;
    /// Remember the multipart upload id and part size of an upload slot.
    async fn set_multipart(
        &self,
        _upload: i64,
        _multipart_id: &str,
        _part_size: u64,
    ) -> LibraryResult<()> {
        Err(LibraryError::Conflict(
            "multipart uploads are not available on this server".into(),
        ))
    }
    /// `(multipart id, part size)` of a multipart upload slot, `None` for a single-PUT one.
    async fn get_multipart(&self, _upload: i64) -> LibraryResult<Option<(String, u64)>> {
        Ok(None)
    }
    /// Queue objects for collection.
    async fn queue_gc(&self, keys: &[String], chapter: Option<i64>) -> LibraryResult<()>;
    /// Queued objects older than `before_ms` that nothing references; referenced ones are
    /// dropped from the queue.
    async fn gc_due(&self, before_ms: i64, limit: u32) -> LibraryResult<Vec<String>>;
    /// Remove collected keys from the queue.
    async fn gc_done(&self, keys: &[String]) -> LibraryResult<u64>;
    /// Record that the cached export `key` of `chapter` was just served or built (H-18).
    async fn export_touch(&self, _chapter: i64, _key: &str) -> LibraryResult<()> {
        Ok(())
    }
    /// Start tracking export objects found in the store that have no row yet (built before the
    /// index existed), as read now. Returns how many were new.
    async fn export_adopt(&self, _keys: &[String]) -> LibraryResult<u64> {
        Ok(0)
    }
    /// Cached exports not read since `before_ms`.
    async fn exports_idle(&self, _before_ms: i64, _limit: u32) -> LibraryResult<Vec<String>> {
        Ok(Vec::new())
    }
    /// Forget collected exports.
    async fn exports_forget(&self, _keys: &[String]) -> LibraryResult<u64> {
        Ok(0)
    }
}

/// What one collector sweep did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GcReport {
    pub objects_deleted: u64,
    pub uploads_expired: u64,
    /// Objects whose deletion failed and stay queued.
    pub failed: u64,
    /// Cached chapter exports deleted because nobody read them for [`EXPORT_IDLE_MS`].
    pub exports_deleted: u64,
}

fn wiring(s: &LibraryService) -> LibraryResult<&IngestWiring> {
    s.ingest
        .as_ref()
        .ok_or_else(|| LibraryError::Conflict("uploads are not available on this server".into()))
}

/// Validate the file name and return `(clean name, lower-case extension)`.
pub fn upload_filename(raw: &str, kind: UploadKind) -> LibraryResult<(String, String)> {
    let name = raw
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .trim()
        .to_owned();
    if name.is_empty() || name.chars().count() > 255 || name.chars().any(char::is_control) {
        return Err(LibraryError::Invalid("filename is not valid".into()));
    }
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    let ok = match kind {
        UploadKind::Archive => ARCHIVE_EXTS.contains(&ext.as_str()),
        UploadKind::Epub => ext == "epub",
        UploadKind::Pdf => ext == "pdf",
    };
    if !ok {
        let want = match kind {
            UploadKind::Archive => ".cbz, .zip, .cb7, .7z, .cbr or .rar",
            UploadKind::Epub => ".epub",
            UploadKind::Pdf => ".pdf",
        };
        return Err(LibraryError::Invalid(format!(
            "this chapter takes a {want} file"
        )));
    }
    Ok((name, ext))
}

fn kind_of(format: ChapterFormat) -> UploadKind {
    match format {
        ChapterFormat::Images => UploadKind::Archive,
        ChapterFormat::Epub => UploadKind::Epub,
        ChapterFormat::Pdf => UploadKind::Pdf,
    }
}

pub(crate) fn to_upload(r: UploadRecord) -> Upload {
    Upload {
        id: r.id,
        chapter_id: r.chapter_id,
        series_id: r.series_id,
        filename: r.filename,
        kind: r.kind,
        size_bytes: r.size_bytes,
        status: r.status,
        error: r.error,
        warnings: r.warnings,
        page_count: r.page_count,
        metadata: r.metadata,
        created_at_ms: r.created_at_ms,
        updated_at_ms: r.updated_at_ms,
        finished_at_ms: r.finished_at_ms,
    }
}

fn storage_err(e: impl std::fmt::Display) -> LibraryError {
    LibraryError::Storage(e.to_string())
}

/// Facts read from ComicInfo.xml and series.json, for the editor to offer.
fn archive_metadata(out: &IngestedChapter) -> Value {
    let mut m = serde_json::Map::new();
    if let Some(c) = &out.comic_info {
        let mut put = |k: &str, v: Option<Value>| {
            if let Some(v) = v {
                m.insert(k.to_owned(), v);
            }
        };
        put("title", c.title.as_ref().map(|s| json!(s)));
        put("series", c.series.as_ref().map(|s| json!(s)));
        put("number", c.number.as_ref().map(|s| json!(s)));
        put("volume", c.volume.map(|v| json!(v)));
        put("language", c.language.as_ref().map(|s| json!(s)));
        put("summary", c.summary.as_ref().map(|s| json!(s)));
        put("year", c.year.map(|v| json!(v)));
        if !c.writers.is_empty() {
            put("writers", Some(json!(c.writers)));
        }
        if !c.pencillers.is_empty() {
            put("artists", Some(json!(c.pencillers)));
        }
    }
    if let Some(s) = &out.series_file {
        m.insert("series_title".into(), json!(s.series.title));
    }
    Value::Object(m)
}

fn epub_metadata(i: &epub::EpubInfo) -> Value {
    json!({
        "title": i.title,
        "authors": i.authors,
        "language": i.language,
        "description": i.description,
        "identifier": i.identifier,
        "spine_items": i.spine_len,
        "has_cover": i.cover.is_some(),
    })
}

/// Download `key` into an anonymous temp file, refusing more than `max` bytes.
async fn spool(
    storage: &dyn ObjectStorage,
    key: &str,
    max: u64,
) -> Result<std::fs::File, IngestError> {
    let io = |e: std::io::Error| IngestError::Storage(e.to_string());
    let file = tokio::task::spawn_blocking(tempfile::tempfile)
        .await
        .map_err(|e| IngestError::Storage(e.to_string()))?
        .map_err(io)?;
    let mut out = tokio::fs::File::from_std(file);
    let mut stream = storage
        .get_stream(key, None)
        .await
        .map_err(|e| IngestError::Storage(e.to_string()))?
        .body;
    let mut total: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| IngestError::Storage(e.to_string()))?;
        total += chunk.len() as u64;
        if total > max {
            return Err(IngestError::TooLarge("uploaded file"));
        }
        out.write_all(&chunk).await.map_err(io)?;
    }
    out.flush().await.map_err(io)?;
    let mut file = out.into_std().await;
    file.seek(SeekFrom::Start(0)).map_err(io)?;
    Ok(file)
}

/// What a successful job produced, before it is written to the row.
struct Outcome {
    page_count: i32,
    warnings: Vec<String>,
    metadata: Value,
}

impl LibraryService {
    /// Attach the upload path: object storage, operator settings and the job queue.
    pub fn with_ingest(
        mut self,
        storage: Arc<dyn ObjectStorage>,
        settings: Arc<dyn IngestSettings>,
        queue: Arc<dyn IngestQueue>,
    ) -> Self {
        self.ingest = Some(IngestWiring {
            storage,
            settings,
            queue,
        });
        self
    }

    /// Allow resumable multipart uploads (`UploadRequest.multipart`).
    pub fn with_multipart(mut self, wiring: MultipartWiring) -> Self {
        self.multipart = Some(wiring);
        self
    }

    /// Write ingest outcomes to the staff access log.
    pub fn with_audit(mut self, audit: Arc<dyn LibraryAudit>) -> Self {
        self.audit = Some(audit);
        self
    }

    /// Step 1: reserve an upload slot for `chapter` and hand out the presigned `PUT` URL.
    pub async fn begin_upload(
        &self,
        a: Actor,
        chapter: i64,
        req: UploadRequest,
    ) -> LibraryResult<UploadSlot> {
        a.publish()?;
        let w = wiring(self)?;
        if req.multipart && self.multipart.is_none() {
            return Err(LibraryError::Conflict(
                "resumable uploads are not available on this server".into(),
            ));
        }
        let c = self
            .repo
            .get_chapter(chapter, None)
            .await?
            .ok_or(LibraryError::NotFound("chapter"))?;
        let kind = kind_of(c.format);
        let (filename, ext) = upload_filename(&req.filename, kind)?;
        let limits = w.settings.snapshot().await;
        if req.size_bytes == 0 || req.size_bytes > limits.max_upload_bytes {
            return Err(LibraryError::Invalid(format!(
                "size_bytes must be between 1 and {} bytes",
                limits.max_upload_bytes
            )));
        }
        let size = i64::try_from(req.size_bytes)
            .map_err(|_| LibraryError::Invalid("size_bytes is too large".into()))?;
        let id = (self.ids)();
        let key = format!(
            "{}{}/{}/src-{id}.{ext}",
            rules::KEY_PREFIX,
            c.series_id,
            c.id
        );
        let now = self.clock.now_unix_ms();
        let rec = UploadRecord {
            id,
            chapter_id: c.id,
            series_id: c.series_id,
            user_id: Some(a.user_id),
            filename,
            kind,
            size_bytes: size,
            object_key: key.clone(),
            status: UploadStatus::AwaitingUpload,
            error: None,
            warnings: Vec::new(),
            page_count: None,
            metadata: json!({}),
            created_at_ms: now,
            updated_at_ms: now,
            finished_at_ms: None,
        };
        self.repo.create_upload(&rec).await?;
        if req.multipart {
            return self.begin_multipart(w, rec, limits.max_upload_bytes).await;
        }
        let url = match w
            .storage
            .presign(PresignMethod::Put, &key, UPLOAD_URL_TTL)
            .await
        {
            Ok(u) => u,
            Err(e) => {
                let _ = self
                    .repo
                    .transition_upload(id, &[UploadStatus::AwaitingUpload], UploadStatus::Expired)
                    .await;
                return Err(storage_err(e));
            }
        };
        Ok(UploadSlot {
            upload: to_upload(rec),
            multipart: None,
            upload_url: url,
            upload_method: "PUT".into(),
            expires_in_secs: UPLOAD_URL_TTL.as_secs(),
            max_bytes: limits.max_upload_bytes,
        })
    }

    /// Step 3: the client says the file is uploaded. Checks it and queues the ingest job.
    /// Calling it again for an upload already queued, running or finished returns that upload.
    pub async fn complete_upload(&self, a: Actor, id: i64) -> LibraryResult<Upload> {
        a.publish()?;
        let w = wiring(self)?;
        let rec = self
            .repo
            .get_upload(id)
            .await?
            .ok_or(LibraryError::NotFound("upload"))?;
        match rec.status {
            UploadStatus::AwaitingUpload => {}
            UploadStatus::Queued | UploadStatus::Running | UploadStatus::Done => {
                return Ok(to_upload(rec));
            }
            UploadStatus::Failed | UploadStatus::Expired => {
                return Err(LibraryError::Conflict(format!(
                    "this upload is {}; start a new one",
                    rec.status.as_str()
                )));
            }
        }
        if let Some((multipart_id, part_size)) = self.repo.get_multipart(id).await? {
            self.assemble_multipart(&rec, &multipart_id, part_size)
                .await?;
        }
        let limits = w.settings.snapshot().await;
        let info = match w.storage.head(&rec.object_key).await {
            Ok(i) => i,
            Err(sr_storage::StorageError::NotFound(_)) => {
                return Err(LibraryError::Invalid(
                    "the file has not been uploaded".into(),
                ));
            }
            Err(e) => return Err(storage_err(e)),
        };
        if info.size == 0 || info.size > limits.max_upload_bytes {
            let msg = format!(
                "the uploaded file is {} bytes; the limit is {} bytes",
                info.size, limits.max_upload_bytes
            );
            self.fail_upload(&rec, &msg, &[]).await;
            return Err(LibraryError::Invalid(msg));
        }
        let queued = self
            .repo
            .transition_upload(id, &[UploadStatus::AwaitingUpload], UploadStatus::Queued)
            .await?
            .ok_or_else(|| LibraryError::Conflict("the upload changed; reload it".into()))?;
        if let Err(e) = w.queue.enqueue(id).await {
            let _ = self
                .repo
                .transition_upload(id, &[UploadStatus::Queued], UploadStatus::AwaitingUpload)
                .await;
            return Err(LibraryError::Storage(format!("queueing the ingest: {e}")));
        }
        Ok(to_upload(queued))
    }

    /// The storage key of an upload slot's file (for server-side imports that fill the slot).
    pub(crate) async fn upload_object_key(&self, id: i64) -> LibraryResult<String> {
        self.repo
            .get_upload(id)
            .await?
            .map(|r| r.object_key)
            .ok_or(LibraryError::NotFound("upload"))
    }

    /// Pre-inspection (REV-UP-004): list the archive of an upload slot without converting it.
    /// Works from the moment the file is in storage until the conversion has run.
    pub async fn inspect_upload(
        &self,
        a: Actor,
        id: i64,
    ) -> LibraryResult<crate::ingest::inspect::ArchiveReport> {
        a.publish()?;
        let w = wiring(self)?;
        let rec = self
            .repo
            .get_upload(id)
            .await?
            .ok_or(LibraryError::NotFound("upload"))?;
        if rec.kind != UploadKind::Archive {
            return Err(LibraryError::Invalid(
                "only archives (CBZ, CB7, CBR) can be inspected".into(),
            ));
        }
        if !matches!(
            rec.status,
            UploadStatus::AwaitingUpload | UploadStatus::Queued
        ) {
            return Err(LibraryError::Conflict(
                "the archive is already converted; there is nothing left to inspect".into(),
            ));
        }
        let s = w.settings.snapshot().await;
        let limits = ArchiveLimits {
            rar_enabled: s.rar_enabled,
            max_archive_bytes: s.max_upload_bytes,
            ..ArchiveLimits::default()
        };
        crate::ingest::inspect::inspect_stored(w.storage.clone(), &rec.object_key, limits)
            .await
            .map_err(|_| LibraryError::Invalid("the file has not been uploaded yet".into()))
    }

    /// The status of an upload.
    pub async fn upload_status(&self, a: Actor, id: i64) -> LibraryResult<Upload> {
        a.publish()?;
        self.repo
            .get_upload(id)
            .await?
            .map(to_upload)
            .ok_or(LibraryError::NotFound("upload"))
    }

    /// Recent uploads of a chapter, newest first.
    pub async fn chapter_uploads(&self, a: Actor, chapter: i64) -> LibraryResult<UploadList> {
        a.publish()?;
        self.repo
            .get_chapter(chapter, None)
            .await?
            .ok_or(LibraryError::NotFound("chapter"))?;
        Ok(UploadList {
            uploads: self
                .repo
                .list_uploads(chapter, 20)
                .await?
                .into_iter()
                .map(to_upload)
                .collect(),
        })
    }

    /// Mark an upload failed, tell the uploader and remove its object.
    async fn fail_upload(&self, rec: &UploadRecord, error: &str, warnings: &[String]) {
        match self
            .repo
            .finish_upload(
                rec.id,
                UploadStatus::Failed,
                Some(error),
                warnings,
                None,
                &json!({}),
            )
            .await
        {
            Ok(Some(done)) => {
                self.audit_upload(
                    &done,
                    "library.chapter.ingest_failed",
                    json!({ "upload": done.id.to_string(), "error": error }),
                )
                .await;
                self.notify_upload(&done).await;
            }
            Ok(None) => {}
            Err(e) => tracing::error!(upload = rec.id, error = %e, "recording a failed upload"),
        }
        self.discard_object(&rec.object_key, Some(rec.chapter_id))
            .await;
    }

    /// Delete an object now, or queue it for the collector when that fails.
    async fn discard_object(&self, key: &str, chapter: Option<i64>) {
        let Some(w) = &self.ingest else { return };
        if let Err(e) = w.storage.delete(key).await {
            tracing::warn!(key, error = %e, "deleting an upload object failed; queued for collection");
            if let Err(e) = self.repo.queue_gc(&[key.to_owned()], chapter).await {
                tracing::error!(key, error = %e, "queueing an object for collection failed");
            }
        }
    }

    async fn audit_upload(&self, rec: &UploadRecord, action: &str, detail: Value) {
        if let (Some(audit), Some(user)) = (&self.audit, rec.user_id) {
            audit
                .record(user, action, ("chapter", rec.chapter_id), detail)
                .await;
        }
    }

    async fn notify_upload(&self, rec: &UploadRecord) {
        let (Some(events), Some(user)) = (&self.events, rec.user_id) else {
            return;
        };
        let mut p = serde_json::to_value(to_upload(rec.clone())).unwrap_or(Value::Null);
        if let Value::Object(m) = &mut p {
            m.insert("type".into(), json!("upload"));
        }
        events.emit(vec![user], p).await;
    }

    /// The `library.ingest` job: convert the uploaded file and record the outcome. A bad file
    /// fails the upload (the row says why) and returns `Ok`; only infrastructure errors (database,
    /// storage unreachable) return `Err`, which makes the runner retry the job; on the `final_attempt`
    /// they fail the upload instead.
    pub async fn run_ingest(&self, id: i64, final_attempt: bool) -> LibraryResult<()> {
        let w = wiring(self)?;
        let Some(rec) = self
            .repo
            .transition_upload(
                id,
                &[UploadStatus::Queued, UploadStatus::Running],
                UploadStatus::Running,
            )
            .await?
        else {
            // Done, failed, expired or deleted along with its chapter: nothing to do.
            return Ok(());
        };
        let limits = w.settings.snapshot().await;
        match self.ingest_one(w, &rec, limits).await {
            Ok(out) => {
                // Archives are transient; EPUB and PDF keep the object as the chapter's source.
                if rec.kind == UploadKind::Archive {
                    self.discard_object(&rec.object_key, Some(rec.chapter_id))
                        .await;
                }
                // `None`: the chapter was deleted meanwhile.
                if let Some(done) = self
                    .repo
                    .finish_upload(
                        id,
                        UploadStatus::Done,
                        None,
                        &out.warnings,
                        Some(out.page_count),
                        &out.metadata,
                    )
                    .await?
                {
                    self.audit_upload(
                        &done,
                        "library.chapter.ingest",
                        json!({
                            "upload": done.id.to_string(),
                            "kind": done.kind.as_str(),
                            "pages": done.page_count,
                            "series": done.series_id.to_string(),
                        }),
                    )
                    .await;
                    self.notify_upload(&done).await;
                }
                Ok(())
            }
            Err(IngestFailure::Bad(msg)) => {
                self.fail_upload(&rec, &msg, &[]).await;
                Ok(())
            }
            Err(IngestFailure::Infra(e)) if final_attempt => {
                // No retries left: say so instead of leaving the upload "running" forever.
                self.fail_upload(
                    &rec,
                    &format!("the server could not finish the conversion: {e}"),
                    &[],
                )
                .await;
                Ok(())
            }
            Err(IngestFailure::Infra(e)) => Err(e),
        }
    }

    async fn ingest_one(
        &self,
        w: &IngestWiring,
        rec: &UploadRecord,
        limits: IngestSettingsSnapshot,
    ) -> Result<Outcome, IngestFailure> {
        let chapter = self
            .repo
            .get_chapter(rec.chapter_id, None)
            .await?
            .ok_or_else(|| IngestFailure::Bad("the chapter was deleted".into()))?;
        if kind_of(chapter.format) != rec.kind {
            return Err(IngestFailure::Bad(
                "the chapter's format changed since the upload started".into(),
            ));
        }
        let file = spool(w.storage.as_ref(), &rec.object_key, limits.max_upload_bytes)
            .await
            .map_err(IngestFailure::from_ingest)?;
        let archive_limits = ArchiveLimits {
            max_archive_bytes: limits.max_upload_bytes,
            rar_enabled: limits.rar_enabled,
            ..ArchiveLimits::default()
        };
        match rec.kind {
            UploadKind::Archive => {
                let ingested = ingest_archive_rev(
                    file,
                    w.storage.clone(),
                    rec.series_id,
                    rec.chapter_id,
                    Some(rec.id),
                    IngestLimits {
                        archive: archive_limits,
                        ..IngestLimits::default()
                    },
                )
                .await
                .map_err(IngestFailure::from_ingest)?;
                let metadata = archive_metadata(&ingested);
                let stored_keys: Vec<String> = ingested
                    .pages
                    .iter()
                    .flat_map(|p| {
                        std::iter::once(p.key.clone()).chain(p.variants.values().cloned())
                    })
                    .collect();
                let rows = match rules::pages(rec.series_id, rec.chapter_id, &ingested.pages) {
                    Ok(r) => r,
                    Err(e) => {
                        self.repo
                            .queue_gc(&stored_keys, Some(rec.chapter_id))
                            .await?;
                        return Err(IngestFailure::Bad(e.to_string()));
                    }
                };
                match self.repo.replace_pages(rec.chapter_id, &rows).await? {
                    Some(_) => Ok(Outcome {
                        page_count: i32::try_from(rows.len()).unwrap_or(i32::MAX),
                        warnings: ingested.warnings,
                        metadata,
                    }),
                    None => {
                        self.repo.queue_gc(&stored_keys, None).await?;
                        Err(IngestFailure::Bad("the chapter was deleted".into()))
                    }
                }
            }
            UploadKind::Epub => {
                let info =
                    epub::inspect(file, &archive_limits).map_err(IngestFailure::from_ingest)?;
                let spine = i32::try_from(info.spine_len).unwrap_or(i32::MAX);
                if !self
                    .repo
                    .set_chapter_source(rec.chapter_id, &rec.object_key, spine)
                    .await?
                {
                    return Err(IngestFailure::Bad("the chapter was deleted".into()));
                }
                Ok(Outcome {
                    page_count: spine,
                    warnings: info.warnings.clone(),
                    metadata: epub_metadata(&info),
                })
            }
            UploadKind::Pdf => {
                let info = pdf::validate(file, limits.max_upload_bytes)
                    .map_err(IngestFailure::from_ingest)?;
                if !self
                    .repo
                    .set_chapter_source(rec.chapter_id, &rec.object_key, 0)
                    .await?
                {
                    return Err(IngestFailure::Bad("the chapter was deleted".into()));
                }
                Ok(Outcome {
                    page_count: 0,
                    warnings: info.warnings,
                    metadata: json!({ "pdf_version": info.version, "size_bytes": info.size }),
                })
            }
        }
    }

    /// One sweep of the collector: expire abandoned upload slots, then delete queued objects
    /// that nothing references any more. Objects whose deletion fails stay queued.
    pub async fn run_gc(&self) -> LibraryResult<GcReport> {
        let w = wiring(self)?;
        let now = self.clock.now_unix_ms();
        let mut report = GcReport::default();
        for u in self
            .repo
            .expire_uploads(now - UPLOAD_EXPIRY_MS, GC_BATCH)
            .await?
        {
            report.uploads_expired += 1;
            // A multipart slot also holds parts in the store; free them.
            if let Some(m) = &self.multipart
                && let Ok(Some((multipart_id, _))) = self.repo.get_multipart(u.id).await
            {
                let _ = m.store.abort(&u.object_key, &multipart_id).await;
            }
            self.repo
                .queue_gc(std::slice::from_ref(&u.object_key), None)
                .await?;
        }
        let keys = self.repo.gc_due(now - GC_GRACE_MS, GC_BATCH).await?;
        let mut done = Vec::with_capacity(keys.len());
        for key in keys {
            match w.storage.delete(&key).await {
                // Deleting what is already gone succeeds in object stores; treat NotFound the same.
                Ok(()) | Err(sr_storage::StorageError::NotFound(_)) => {
                    done.push(key);
                }
                Err(e) => {
                    report.failed += 1;
                    tracing::warn!(key, error = %e, "deleting an unreferenced library object failed; retrying later");
                }
            }
        }
        report.objects_deleted = self.repo.gc_done(&done).await?;

        // Cached chapter exports: adopt any the index has not seen (the store listing is the
        // fallback for exports built before it existed), then delete the ones idle for 7 days.
        match w.storage.list(EXPORT_PREFIX).await {
            Ok(found) => {
                let keys: Vec<String> = found.into_iter().map(|o| o.key).collect();
                for chunk in keys.chunks(500) {
                    self.repo.export_adopt(chunk).await?;
                }
            }
            Err(e) => tracing::warn!(error = %e, "listing cached library exports failed"),
        }
        let idle = self
            .repo
            .exports_idle(now - EXPORT_IDLE_MS, GC_BATCH)
            .await?;
        let mut gone = Vec::with_capacity(idle.len());
        for key in idle {
            match w.storage.delete(&key).await {
                Ok(()) | Err(sr_storage::StorageError::NotFound(_)) => gone.push(key),
                Err(e) => {
                    report.failed += 1;
                    tracing::warn!(key, error = %e, "deleting an idle library export failed; retrying later");
                }
            }
        }
        report.exports_deleted = self.repo.exports_forget(&gone).await?;
        Ok(report)
    }
}

/// Why an ingest did not produce pages.
enum IngestFailure {
    /// The file or the request is wrong; the message is for the uploader.
    Bad(String),
    /// Database or storage trouble worth retrying.
    Infra(LibraryError),
}

impl IngestFailure {
    fn from_ingest(e: IngestError) -> Self {
        match e {
            IngestError::Storage(m) => Self::Infra(LibraryError::Storage(m)),
            other => Self::Bad(other.to_string()),
        }
    }
}

impl From<LibraryError> for IngestFailure {
    fn from(e: LibraryError) -> Self {
        Self::Infra(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_must_fit_the_chapter_format() {
        assert_eq!(
            upload_filename("C:\\scans\\Ch 12.CBZ", UploadKind::Archive).unwrap(),
            ("Ch 12.CBZ".to_owned(), "cbz".to_owned())
        );
        for ok in ["a.zip", "a.cb7", "a.7z", "a.cbr", "a.rar"] {
            assert!(upload_filename(ok, UploadKind::Archive).is_ok(), "{ok}");
        }
        assert!(upload_filename("a.epub", UploadKind::Archive).is_err());
        assert!(upload_filename("a.pdf", UploadKind::Epub).is_err());
        assert!(upload_filename("a.epub", UploadKind::Epub).is_ok());
        assert!(upload_filename("a.pdf", UploadKind::Pdf).is_ok());
        assert!(upload_filename("noext", UploadKind::Pdf).is_err());
        assert!(upload_filename("", UploadKind::Pdf).is_err());
        assert!(upload_filename("a\u{0}.pdf", UploadKind::Pdf).is_err());
        assert!(upload_filename(&format!("{}.pdf", "x".repeat(260)), UploadKind::Pdf).is_err());
        // Directories are stripped, never trusted.
        assert_eq!(
            upload_filename("../../etc/x.pdf", UploadKind::Pdf)
                .unwrap()
                .0,
            "x.pdf"
        );
    }

    #[test]
    fn default_settings_keep_rar_on() {
        let s = IngestSettingsSnapshot::default();
        assert!(s.rar_enabled);
        assert_eq!(s.max_upload_bytes, 512 * 1024 * 1024);
    }
}
