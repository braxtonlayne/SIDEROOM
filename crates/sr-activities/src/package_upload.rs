//! Resumable upload of an Activity package (SEC-AUD-176, D-654, wave 65): the session half.
//!
//! The browser cuts the zip into parts of [`PART_SIZE`] and sends them to presigned part URLs of the platform's
//! bucket (the Library's protocol, [`sr_library::multipart`]: the server never needs the part ETags, it lists the
//! parts the store holds and completes the object itself). `complete` queues the job `activities.package_ingest`;
//! the job checks the stored zip through [`PackageIngest::ingest`] and the session ends `ready` with the bundle
//! or `refused` with the reason. Nothing here holds a package in memory.
//!
//! The checker lives in `package.rs` (the pack builder); this file sees it only through [`PackageIngest`].

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::{PgPool, Row};
use sr_library::multipart::{MAX_PARTS, PART_URL_TTL, PartStore, StoredPart, check_parts};
use sr_storage::ObjectStorage;
use utoipa::ToSchema;

use crate::error::{ActivityError, ActivityResult};
use crate::model::BundleDetail;
use crate::service::ActivityService;
use crate::service::IdSource;

/// Size of a part (the last one may be smaller). Raised only to keep the part count at most [`MAX_PARTS`].
pub const PART_SIZE: u64 = 16 * 1024 * 1024;
/// An upload not completed within this long expires and its parts are aborted.
pub const UPLOAD_TTL_HOURS: i64 = 24;
/// Package uploads one application may start per hour.
pub const UPLOADS_PER_HOUR: i64 = 6;
/// Open (not yet completed) uploads one application may have; starting another aborts the older.
pub const OPEN_UPLOADS: i64 = 2;
/// Rows older than this many days are deleted by the sweep.
pub const KEEP_DAYS: i64 = 7;
/// What the page shows when a check ends for a reason that is not the package's fault.
pub const CHECK_FAILED: &str = "The package could not be checked. Try again.";
/// Stable code of the field error that says the store cannot sign parts (the client tests for it).
pub const CODE_NO_PARTS: &str = "PACKAGE_PARTS_UNAVAILABLE";
/// Stable code of the field error that lists the parts `complete` is missing.
pub const CODE_PARTS_MISSING: &str = "PACKAGE_PARTS_MISSING";

// ---- the checker port --------------------------------------------------------------------------------------------

pub use crate::package::{IngestProgress, PackageLimits, PackageTier};

/// What the session needs from the package checker. `ActivityService` implements it (the adapter over
/// `package_limits` and `ingest_stored_zip`); tests use a fake.
#[async_trait]
pub trait PackageIngest: Send + Sync {
    async fn limits(&self, app: i64) -> ActivityResult<PackageLimits>;
    /// Check the zip at `staged_key` and make the draft bundle. The staged object is deleted on success and on a
    /// refusal; a refusal is `ActivityError::Invalid("bundle", text)`.
    async fn ingest(
        &self,
        app: i64,
        user: i64,
        staged_key: &str,
        progress: Option<&dyn IngestProgress>,
    ) -> ActivityResult<BundleDetail>;
}

/// The adapter: the checker of `package.rs` behind the session's port.
#[async_trait]
impl PackageIngest for ActivityService {
    async fn limits(&self, app: i64) -> ActivityResult<PackageLimits> {
        self.package_limits(app).await
    }

    async fn ingest(
        &self,
        app: i64,
        user: i64,
        staged_key: &str,
        progress: Option<&dyn IngestProgress>,
    ) -> ActivityResult<BundleDetail> {
        self.ingest_stored_zip(app, user, staged_key, progress)
            .await
    }
}

/// Queues the check job for an upload (the job system lives in `sr-server`).
#[async_trait]
pub trait PackageQueue: Send + Sync {
    async fn enqueue(&self, upload_id: i64) -> Result<(), String>;
}

// ---- wire types -------------------------------------------------------------------------------------------------

/// Tier of an application as shown to the developer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum PackageTierView {
    Standard,
    Reviewed,
    Platform,
}

impl From<PackageTier> for PackageTierView {
    fn from(t: PackageTier) -> Self {
        match t {
            PackageTier::Standard => Self::Standard,
            PackageTier::Reviewed => Self::Reviewed,
            PackageTier::Platform => Self::Platform,
        }
    }
}

/// `GET /applications/{id}/activity/package-limits`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct PackageLimitsView {
    /// Largest package for this application, in bytes (a decimal string).
    pub max_bytes: String,
    pub max_files: u64,
    pub tier: PackageTierView,
    /// False when the store cannot sign part URLs: only the single-request upload (32 MiB) works.
    pub resumable: bool,
    pub part_size: u64,
}

/// One presigned part URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct PackagePartUrl {
    pub part_number: u32,
    pub url: String,
}

/// `POST /applications/{id}/activity/uploads`: the new session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct PackageUploadSlot {
    pub upload_id: String,
    pub part_size: u64,
    pub part_count: u32,
    pub parts: Vec<PackagePartUrl>,
}

/// Where an upload stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum PackageUploadState {
    Uploading,
    Checking,
    Ready,
    Refused,
    Expired,
}

/// `GET .../uploads/{id}` and the answer of `complete`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct PackageUploadStatus {
    pub state: PackageUploadState,
    /// Part numbers the store holds (right size), while uploading.
    pub received: Vec<u32>,
    /// Fresh URLs for the missing parts, only while uploading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<PackagePartUrl>>,
    pub checked_bytes: String,
    pub total_bytes: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// `POST .../uploads` body. The size is a decimal string (a number is accepted too).
#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = ActivityPackageUploadBody)]
pub struct PackageUploadBody {
    #[serde(deserialize_with = "de_size")]
    #[schema(value_type = String)]
    pub size: u64,
    pub filename: String,
}

fn de_size<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Text(String),
        Number(u64),
    }
    match Raw::deserialize(d)? {
        Raw::Number(n) => Ok(n),
        Raw::Text(s) => s
            .trim()
            .parse()
            .map_err(|_| serde::de::Error::custom("size must be a whole number of bytes")),
    }
}

// ---- errors -----------------------------------------------------------------------------------------------------

/// Why an upload call is refused; the route maps each to a status.
#[derive(Debug, PartialEq)]
pub enum PackageUploadError {
    /// 413: over the application's limit (the text names the limit and the tier).
    TooLarge(String),
    /// 429: too many uploads this hour.
    RateLimited,
    /// 409: the store cannot sign parts.
    NoParts,
    /// 409: `complete` found parts missing (the text lists them).
    PartsMissing(String),
    Activity(ActivityError),
}

impl From<ActivityError> for PackageUploadError {
    fn from(e: ActivityError) -> Self {
        Self::Activity(e)
    }
}

impl From<sqlx::Error> for PackageUploadError {
    fn from(e: sqlx::Error) -> Self {
        Self::Activity(e.into())
    }
}

type UploadResult<T> = Result<T, PackageUploadError>;

// ---- the service ------------------------------------------------------------------------------------------------

/// Part signing: the store and the part size.
#[derive(Clone)]
pub struct PartsWiring {
    pub store: Arc<dyn PartStore>,
    pub part_size: u64,
}

impl PartsWiring {
    pub fn new(store: Arc<dyn PartStore>) -> Self {
        Self {
            store,
            part_size: PART_SIZE,
        }
    }

    /// A different part size (tests use tiny parts).
    #[must_use]
    pub fn with_part_size(mut self, part_size: u64) -> Self {
        self.part_size = part_size.max(1);
        self
    }

    /// Part size and count for a zip of `size` bytes: at least the configured size, larger so that the count stays
    /// at most [`MAX_PARTS`].
    pub fn plan(&self, size: u64) -> (u64, u32) {
        plan_parts(self.part_size, size)
    }
}

fn plan_parts(configured: u64, size: u64) -> (u64, u32) {
    let part = configured.max(size.div_ceil(MAX_PARTS)).max(1);
    let count = size.div_ceil(part).max(1);
    (part, u32::try_from(count).unwrap_or(u32::MAX))
}

struct Inner {
    pool: PgPool,
    ids: IdSource,
    ingest: Arc<dyn PackageIngest>,
    storage: Arc<dyn ObjectStorage>,
    parts: Option<PartsWiring>,
    queue: Arc<dyn PackageQueue>,
}

/// The package upload sessions of the Activities backend. Cheap to clone.
#[derive(Clone)]
pub struct PackageUploads {
    inner: Arc<Inner>,
}

/// One row of `activity_package_uploads`.
struct UploadRow {
    id: i64,
    application_id: i64,
    created_by: i64,
    lapsed: bool,
    size: i64,
    part_size: i64,
    part_count: i32,
    object_key: String,
    multipart_id: String,
    state: String,
    checked_bytes: i64,
    bundle_id: Option<i64>,
    error: Option<String>,
}

macro_rules! cols {
    () => {
        "id, application_id, created_by, (expires_at <= now()) AS lapsed, size, part_size, part_count, \
                    object_key, multipart_id, state, checked_bytes, bundle_id, error"
    };
}

fn row_of(r: &sqlx::postgres::PgRow) -> Result<UploadRow, sqlx::Error> {
    Ok(UploadRow {
        id: r.try_get("id")?,
        application_id: r.try_get("application_id")?,
        created_by: r.try_get("created_by")?,
        lapsed: r.try_get("lapsed")?,
        size: r.try_get("size")?,
        part_size: r.try_get("part_size")?,
        part_count: r.try_get("part_count")?,
        object_key: r.try_get("object_key")?,
        multipart_id: r.try_get("multipart_id")?,
        state: r.try_get("state")?,
        checked_bytes: r.try_get("checked_bytes")?,
        bundle_id: r.try_get("bundle_id")?,
        error: r.try_get("error")?,
    })
}

/// Staged object of an upload.
pub fn staged_key(app: i64, upload: i64) -> String {
    format!("activities/{app}/uploads/{upload}.zip")
}

fn mib(bytes: u64) -> String {
    if bytes.is_multiple_of(1024 * 1024) {
        format!("{} MiB", bytes / (1024 * 1024))
    } else {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn tier_word(t: PackageTier) -> &'static str {
    match t {
        PackageTier::Standard => "standard limit",
        PackageTier::Reviewed => "reviewed application",
        PackageTier::Platform => "platform application",
    }
}

fn storage_err(e: impl std::fmt::Display) -> ActivityError {
    ActivityError::Storage(e.to_string())
}

/// The numbers of parts `stored` lacks (absent, or of the wrong size) for an upload cut into `part_count` parts.
fn missing_parts(stored: &[StoredPart], size: u64, part_size: u64, part_count: u32) -> Vec<u32> {
    (1..=part_count)
        .filter(|n| {
            let want = if *n == part_count {
                size.saturating_sub(part_size * u64::from(part_count - 1))
            } else {
                part_size
            };
            !stored.iter().any(|p| p.number == *n && p.size == want)
        })
        .collect()
}

impl PackageUploads {
    pub fn new(
        pool: PgPool,
        ids: IdSource,
        ingest: Arc<dyn PackageIngest>,
        storage: Arc<dyn ObjectStorage>,
        parts: Option<PartsWiring>,
        queue: Arc<dyn PackageQueue>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                pool,
                ids,
                ingest,
                storage,
                parts,
                queue,
            }),
        }
    }

    fn presign(
        &self,
        w: &PartsWiring,
        key: &str,
        multipart_id: &str,
        numbers: impl Iterator<Item = u32>,
    ) -> ActivityResult<Vec<PackagePartUrl>> {
        numbers
            .map(|n| {
                w.store
                    .presign_part(key, multipart_id, n, PART_URL_TTL)
                    .map(|url| PackagePartUrl {
                        part_number: n,
                        url,
                    })
                    .map_err(storage_err)
            })
            .collect()
    }

    async fn load(&self, app: i64, id: i64) -> ActivityResult<UploadRow> {
        let r = sqlx::query(concat!(
            "SELECT ",
            cols!(),
            " FROM activity_package_uploads WHERE id = $1 AND application_id = $2"
        ))
        .bind(id)
        .bind(app)
        .fetch_optional(&self.inner.pool)
        .await?;
        match r {
            Some(r) => Ok(row_of(&r)?),
            None => Err(ActivityError::NotFound("upload")),
        }
    }

    /// `GET .../package-limits`.
    pub async fn limits(&self, app: i64) -> ActivityResult<PackageLimitsView> {
        let l = self.inner.ingest.limits(app).await?;
        Ok(PackageLimitsView {
            max_bytes: l.max_bytes.to_string(),
            max_files: l.max_files as u64,
            tier: l.tier.into(),
            resumable: self.inner.parts.is_some(),
            part_size: self.inner.parts.as_ref().map_or(PART_SIZE, |p| p.part_size),
        })
    }

    /// `POST .../uploads`: start a session for a zip of `size` bytes.
    pub async fn begin(
        &self,
        app: i64,
        user: i64,
        size: u64,
        filename: &str,
    ) -> UploadResult<PackageUploadSlot> {
        let Some(w) = self.inner.parts.clone() else {
            return Err(PackageUploadError::NoParts);
        };
        if size == 0 {
            return Err(ActivityError::Invalid("size", "The package is empty.".into()).into());
        }
        if filename.chars().count() > 255 {
            return Err(
                ActivityError::Invalid("filename", "The file name is too long.".into()).into(),
            );
        }
        let limits = self.inner.ingest.limits(app).await?;
        if size > limits.max_bytes {
            return Err(PackageUploadError::TooLarge(format!(
                "That package is {}. The limit for this application is {} ({}). The operator of this server can raise the limit.",
                mib(size),
                mib(limits.max_bytes),
                tier_word(limits.tier)
            )));
        }
        let recent: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM activity_package_uploads \
             WHERE application_id = $1 AND created_at > now() - interval '1 hour'",
        )
        .bind(app)
        .fetch_one(&self.inner.pool)
        .await?;
        if recent >= UPLOADS_PER_HOUR {
            return Err(PackageUploadError::RateLimited);
        }
        // At most OPEN_UPLOADS open sessions: the older ones are aborted to make room for this one.
        let keep = OPEN_UPLOADS - 1;
        let older = sqlx::query(
            "SELECT id FROM activity_package_uploads WHERE application_id = $1 AND state = 'uploading' \
             ORDER BY created_at DESC, id DESC OFFSET $2",
        )
        .bind(app)
        .bind(keep)
        .fetch_all(&self.inner.pool)
        .await?;
        for r in older {
            let id: i64 = r.try_get("id")?;
            if let Err(e) = self.abort(app, id).await {
                tracing::warn!(error = ?e, "aborting an older package upload failed");
            }
        }

        let id = (self.inner.ids)();
        let key = staged_key(app, id);
        let (part_size, part_count) = w.plan(size);
        let multipart_id = w
            .store
            .create(&key, "application/zip")
            .await
            .map_err(storage_err)?;
        let inserted = sqlx::query(
            "INSERT INTO activity_package_uploads \
             (id, application_id, created_by, expires_at, size, part_size, part_count, object_key, multipart_id) \
             VALUES ($1, $2, $3, now() + make_interval(hours => $4::int), $5, $6, $7, $8, $9)",
        )
        .bind(id)
        .bind(app)
        .bind(user)
        .bind(UPLOAD_TTL_HOURS as i32)
        .bind(i64::try_from(size).unwrap_or(i64::MAX))
        .bind(i64::try_from(part_size).unwrap_or(i64::MAX))
        .bind(i32::try_from(part_count).unwrap_or(i32::MAX))
        .bind(&key)
        .bind(&multipart_id)
        .execute(&self.inner.pool)
        .await;
        if let Err(e) = inserted {
            let _ = w.store.abort(&key, &multipart_id).await;
            return Err(e.into());
        }
        let parts = self.presign(&w, &key, &multipart_id, 1..=part_count)?;
        Ok(PackageUploadSlot {
            upload_id: id.to_string(),
            part_size,
            part_count,
            parts,
        })
    }

    fn view(
        r: &UploadRow,
        received: Vec<u32>,
        parts: Option<Vec<PackagePartUrl>>,
    ) -> PackageUploadStatus {
        let state = match r.state.as_str() {
            "uploading" if r.lapsed => PackageUploadState::Expired,
            "uploading" => PackageUploadState::Uploading,
            "checking" => PackageUploadState::Checking,
            "ready" => PackageUploadState::Ready,
            "refused" => PackageUploadState::Refused,
            _ => PackageUploadState::Expired,
        };
        PackageUploadStatus {
            state,
            received,
            parts: if state == PackageUploadState::Uploading {
                parts
            } else {
                None
            },
            checked_bytes: r.checked_bytes.to_string(),
            total_bytes: r.size.to_string(),
            bundle_id: r.bundle_id.map(|b| b.to_string()),
            error: r.error.clone(),
        }
    }

    /// `GET .../uploads/{id}`. While uploading it asks the store which parts it holds and signs fresh URLs for
    /// the rest, so a refreshed page sends only what is missing.
    pub async fn status(&self, app: i64, id: i64) -> ActivityResult<PackageUploadStatus> {
        let r = self.load(app, id).await?;
        if r.state != "uploading" || r.lapsed {
            return Ok(Self::view(&r, Vec::new(), None));
        }
        let w = self
            .inner
            .parts
            .as_ref()
            .ok_or_else(|| ActivityError::Conflict(NO_PARTS_TEXT.into()))?;
        let (size, part_size, count) = dims(&r);
        let stored = w
            .store
            .list_parts(&r.object_key, &r.multipart_id)
            .await
            .map_err(storage_err)?;
        let missing = missing_parts(&stored, size, part_size, count);
        let received: Vec<u32> = (1..=count).filter(|n| !missing.contains(n)).collect();
        let urls = self.presign(w, &r.object_key, &r.multipart_id, missing.into_iter())?;
        Ok(Self::view(&r, received, Some(urls)))
    }

    /// `POST .../uploads/{id}/complete`: check the parts against the announced size, let the store assemble the
    /// object and queue the check. Asking again once it is queued answers with the current state.
    pub async fn complete(&self, app: i64, id: i64) -> UploadResult<PackageUploadStatus> {
        let r = self.load(app, id).await?;
        match r.state.as_str() {
            "uploading" if !r.lapsed => {}
            "checking" | "ready" | "refused" => return Ok(Self::view(&r, Vec::new(), None)),
            _ => {
                return Err(ActivityError::Conflict(
                    "This upload has ended. Start a new one.".into(),
                )
                .into());
            }
        }
        let w = self
            .inner
            .parts
            .as_ref()
            .ok_or(PackageUploadError::NoParts)?;
        let (size, part_size, count) = dims(&r);
        let stored = w
            .store
            .list_parts(&r.object_key, &r.multipart_id)
            .await
            .map_err(storage_err)?;
        let missing = missing_parts(&stored, size, part_size, count);
        if !missing.is_empty() {
            let list = missing
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(PackageUploadError::PartsMissing(format!(
                "Parts {list} of {count} have not been uploaded."
            )));
        }
        let parts = check_parts(&stored, size, part_size, count)
            .map_err(|m| PackageUploadError::Activity(ActivityError::Conflict(m)))?;
        if let Err(e) = w
            .store
            .complete(&r.object_key, &r.multipart_id, &parts)
            .await
        {
            tracing::warn!(upload = id, error = %e, "completing a package upload failed");
            return Err(ActivityError::Conflict(
                "The storage did not accept the uploaded parts. Start a new upload.".into(),
            )
            .into());
        }
        let moved = sqlx::query(
            "UPDATE activity_package_uploads SET state = 'checking', checked_bytes = 0 \
             WHERE id = $1 AND state = 'uploading'",
        )
        .bind(id)
        .execute(&self.inner.pool)
        .await?
        .rows_affected();
        if moved == 1
            && let Err(e) = self.inner.queue.enqueue(id).await
        {
            tracing::error!(upload = id, error = %e, "queueing a package check failed");
            self.finish_refused(id, CHECK_FAILED).await?;
            return Err(ActivityError::Storage(e).into());
        }
        Ok(self.status(app, id).await?)
    }

    /// `DELETE .../uploads/{id}`: give the upload up. Parts are aborted at the store (or the staged object deleted
    /// when the check had begun); a finished upload is left as it is.
    pub async fn abort(&self, app: i64, id: i64) -> ActivityResult<()> {
        let r = self.load(app, id).await?;
        let moved = sqlx::query(
            "UPDATE activity_package_uploads SET state = 'aborted' \
             WHERE id = $1 AND state IN ('uploading', 'checking')",
        )
        .bind(id)
        .execute(&self.inner.pool)
        .await?
        .rows_affected();
        if moved == 1 {
            self.discard(&r).await;
        }
        Ok(())
    }

    /// Free what an unfinished upload holds: its open parts, or the staged object.
    async fn discard(&self, r: &UploadRow) {
        if r.state == "uploading" {
            if let Some(w) = &self.inner.parts
                && let Err(e) = w.store.abort(&r.object_key, &r.multipart_id).await
            {
                tracing::warn!(upload = r.id, error = %e, "aborting a package upload failed");
            }
        } else if let Err(e) = self.inner.storage.delete(&r.object_key).await {
            tracing::warn!(upload = r.id, error = %e, "deleting a staged package failed");
        }
    }

    async fn finish_refused(&self, id: i64, text: &str) -> ActivityResult<()> {
        sqlx::query(
            "UPDATE activity_package_uploads SET state = 'refused', error = $2 \
             WHERE id = $1 AND state = 'checking'",
        )
        .bind(id)
        .bind(text)
        .execute(&self.inner.pool)
        .await?;
        Ok(())
    }

    /// The job `activities.package_ingest`: check the stored zip. A refusal ends the session `refused` with the
    /// checker's text. Any other failure is returned (the job system retries it) until `final_attempt`, when the
    /// session ends `refused` with [`CHECK_FAILED`].
    pub async fn run_ingest(&self, id: i64, final_attempt: bool) -> Result<(), String> {
        let row = sqlx::query(concat!(
            "SELECT ",
            cols!(),
            " FROM activity_package_uploads WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(&self.inner.pool)
        .await
        .map_err(|e| e.to_string())?;
        let Some(row) = row else { return Ok(()) };
        let r = row_of(&row).map_err(|e| e.to_string())?;
        if r.state != "checking" {
            // Cancelled, expired or already decided: nothing to do.
            return Ok(());
        }
        let progress = Throttled::new(self.inner.pool.clone(), id);
        match self
            .inner
            .ingest
            .ingest(
                r.application_id,
                r.created_by,
                &r.object_key,
                Some(&progress),
            )
            .await
        {
            Ok(detail) => {
                let bundle = detail.bundle.id.parse::<i64>().ok();
                sqlx::query(
                    "UPDATE activity_package_uploads SET state = 'ready', bundle_id = $2, checked_bytes = size \
                     WHERE id = $1 AND state = 'checking'",
                )
                .bind(id)
                .bind(bundle)
                .execute(&self.inner.pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(())
            }
            Err(ActivityError::Invalid(_, text)) => self
                .finish_refused(id, &text)
                .await
                .map_err(|e| e.to_string()),
            Err(e) if final_attempt => {
                tracing::error!(upload = id, error = %e, "a package check failed for good");
                let _ = self.inner.storage.delete(&r.object_key).await;
                self.finish_refused(id, CHECK_FAILED)
                    .await
                    .map_err(|e| e.to_string())
            }
            Err(e) => Err(e.to_string()),
        }
    }

    /// The daily sweep: uploads past their time that did not finish are aborted (parts at the store, staged
    /// object deleted) and set `expired`; rows older than [`KEEP_DAYS`] days are deleted. Returns
    /// `(expired, deleted)`.
    pub async fn sweep(&self) -> ActivityResult<(u64, u64)> {
        let rows = sqlx::query(concat!(
            "SELECT ",
            cols!(),
            " FROM activity_package_uploads WHERE state IN ('uploading', 'checking') \
             AND expires_at <= now() ORDER BY id LIMIT 500"
        ))
        .fetch_all(&self.inner.pool)
        .await?;
        let mut expired = 0;
        for row in rows {
            let r = row_of(&row)?;
            let moved = sqlx::query(
                "UPDATE activity_package_uploads SET state = 'expired' \
                 WHERE id = $1 AND state IN ('uploading', 'checking')",
            )
            .bind(r.id)
            .execute(&self.inner.pool)
            .await?
            .rows_affected();
            if moved == 1 {
                self.discard(&r).await;
                expired += 1;
            }
        }
        let deleted = sqlx::query(
            "DELETE FROM activity_package_uploads WHERE created_at < now() - make_interval(days => $1::int)",
        )
        .bind(KEEP_DAYS as i32)
        .execute(&self.inner.pool)
        .await?
        .rows_affected();
        Ok((expired, deleted))
    }
}

const NO_PARTS_TEXT: &str = "Resumable uploads are not available on this server.";

fn dims(r: &UploadRow) -> (u64, u64, u32) {
    (
        u64::try_from(r.size).unwrap_or(0),
        u64::try_from(r.part_size).unwrap_or(1),
        u32::try_from(r.part_count).unwrap_or(1),
    )
}

/// Writes the checker's progress to the row, at most once a second.
struct Throttled {
    pool: PgPool,
    id: i64,
    last: Mutex<Option<Instant>>,
}

impl Throttled {
    fn new(pool: PgPool, id: i64) -> Self {
        Self {
            pool,
            id,
            last: Mutex::new(None),
        }
    }
}

impl IngestProgress for Throttled {
    fn bytes(&self, done: u64, _total: u64) {
        let Ok(mut last) = self.last.lock() else {
            return;
        };
        if last.is_some_and(|t| t.elapsed() < Duration::from_secs(1)) {
            return;
        }
        *last = Some(Instant::now());
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let (pool, id) = (self.pool.clone(), self.id);
        let done = i64::try_from(done).unwrap_or(i64::MAX);
        handle.spawn(async move {
            let _ = sqlx::query(
                "UPDATE activity_package_uploads SET checked_bytes = LEAST($2, size) \
                 WHERE id = $1 AND state = 'checking'",
            )
            .bind(id)
            .bind(done)
            .execute(&pool)
            .await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_size_is_read_from_a_string_or_a_number() {
        let a: PackageUploadBody =
            serde_json::from_str(r#"{"size":"123","filename":"a.zip"}"#).unwrap();
        let b: PackageUploadBody =
            serde_json::from_str(r#"{"size":123,"filename":"a.zip"}"#).unwrap();
        assert_eq!((a.size, b.size), (123, 123));
        assert!(
            serde_json::from_str::<PackageUploadBody>(r#"{"size":"x","filename":"a"}"#).is_err()
        );
    }

    #[test]
    fn the_part_count_stays_within_the_stores_limit() {
        assert_eq!(plan_parts(PART_SIZE, 1), (PART_SIZE, 1));
        assert_eq!(plan_parts(PART_SIZE, 3 * PART_SIZE + 1), (PART_SIZE, 4));
        // 4 GiB would need 256 parts; 1 TiB would need 65 536, so the parts grow to keep 10 000.
        let tib = 1u64 << 40;
        let (part, count) = plan_parts(PART_SIZE, tib);
        assert!(count <= 10_000 && part * u64::from(count) >= tib);
    }

    #[test]
    fn missing_parts_names_absent_and_wrong_sized_parts() {
        let held = |n, size| StoredPart {
            number: n,
            size,
            etag: String::new(),
        };
        // 10 bytes in parts of 4: 4 + 4 + 2.
        assert_eq!(
            missing_parts(&[held(1, 4), held(2, 4), held(3, 2)], 10, 4, 3),
            Vec::<u32>::new()
        );
        assert_eq!(
            missing_parts(&[held(1, 4), held(3, 1)], 10, 4, 3),
            vec![2, 3]
        );
    }
}
