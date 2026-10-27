//! Browser uploads and object changes in a member's bucket (BYOS-BRW-001).
//!
//! Three pieces, each behind a trait so the routes are tested without a bucket:
//!
//! * [`UploadLedger`]: the server's record of open multipart uploads (`storage_mount_uploads`, Postgres in
//!   `sr_db::mount_uploads_repo`, [`MemoryLedger`] here) and of the part URLs a member asked for in the last
//!   hour (the `byos.upload_parts_per_hour` quota);
//! * [`MountIo`]: the bucket calls the write routes make with the mount's credentials ([`S3MountIo`] signs real
//!   requests through [`crate::s3multipart::S3Multipart`], [`MemoryMountIo`] is an in-memory bucket);
//! * the limits and the folder-move planner ([`plan_move`]) the routes and the sweep job share.
//!
//! The bytes of an upload never pass through the server: the browser `PUT`s each part to a presigned URL. The
//! server only signs, lists parts and completes. Keys are mount-relative and have passed
//! [`crate::path::safe_key`]; the implementations add the mount prefix.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;

use crate::error::{Result, StorageError};
use crate::mounts::{Mount, Sealer};
pub use crate::s3multipart::{COPY_MAX_BYTES, UploadedPart};

/// Size of every part but the last (a part of a presigned upload is one `PUT`).
pub const PART_SIZE: u64 = 16 * 1024 * 1024;
/// S3 allows 10 000 parts.
pub const MAX_PARTS: u32 = 10_000;
/// Largest object the browser can upload through this path (`PART_SIZE * MAX_PARTS`).
pub const MAX_UPLOAD_BYTES: u64 = PART_SIZE * MAX_PARTS as u64;
/// Lifetime of a part URL.
pub const PART_URL_TTL: Duration = Duration::from_secs(3600);
/// An open upload nobody touched for this long is abandoned: the sweep aborts it at the bucket.
pub const ABANDON_AFTER_MS: i64 = 24 * 3600 * 1000;
/// Most keys one folder rename, move or delete touches in a single call.
pub const FOLDER_OP_CAP: usize = 500;
/// Most uploads a member may have open at once.
pub const MAX_OPEN_UPLOADS: usize = 50;

/// Where an upload stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadState {
    /// Parts may still arrive.
    Open,
    /// The object exists.
    Completed,
    /// Abandoned by the member or swept.
    Aborted,
}

impl UploadState {
    /// Column value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Completed => "completed",
            Self::Aborted => "aborted",
        }
    }
    /// Parse the column value.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "open" => Some(Self::Open),
            "completed" => Some(Self::Completed),
            "aborted" => Some(Self::Aborted),
            _ => None,
        }
    }
}

/// One row of the ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountUpload {
    /// Snowflake; what the browser calls the upload id.
    pub id: i64,
    /// The mount.
    pub mount_id: i64,
    /// Who started it.
    pub user_id: i64,
    /// Mount-relative key.
    pub key: String,
    /// The bucket's upload id (never sent to a client).
    pub s3_upload_id: String,
    /// Declared total size in bytes.
    pub size: u64,
    /// Bytes per part.
    pub part_size: u64,
    /// Parts the declared size needs.
    pub part_count: u32,
    /// Declared content type.
    pub content_type: String,
    /// State.
    pub state: UploadState,
    /// Started (unix ms).
    pub created_at: i64,
    /// Last touched (unix ms): a part URL asked for, or the state changed.
    pub updated_at: i64,
}

/// The ledger could not be read or written.
#[derive(Debug, thiserror::Error)]
#[error("upload ledger: {0}")]
pub struct LedgerError(pub String);

/// The ledger of browser uploads.
#[async_trait]
pub trait UploadLedger: Send + Sync {
    /// Record a new open upload.
    async fn insert(&self, upload: MountUpload) -> std::result::Result<(), LedgerError>;
    /// One upload of a mount.
    async fn get(
        &self,
        mount_id: i64,
        id: i64,
    ) -> std::result::Result<Option<MountUpload>, LedgerError>;
    /// The open uploads of a mount started by `user_id`, newest first.
    async fn list_open(
        &self,
        mount_id: i64,
        user_id: i64,
    ) -> std::result::Result<Vec<MountUpload>, LedgerError>;
    /// How many uploads `user_id` has open across all mounts.
    async fn count_open(&self, user_id: i64) -> std::result::Result<usize, LedgerError>;
    /// Move an open upload to `Completed` or `Aborted`; `false` when it was not open.
    async fn finish(
        &self,
        mount_id: i64,
        id: i64,
        state: UploadState,
        at_ms: i64,
    ) -> std::result::Result<bool, LedgerError>;
    /// Open uploads last touched before `cutoff_ms`, oldest first, at most `limit` (the sweep).
    async fn open_before(
        &self,
        cutoff_ms: i64,
        limit: usize,
    ) -> std::result::Result<Vec<MountUpload>, LedgerError>;
    /// Note that `n` part URLs were issued to `user_id` at `at_ms`, and touch the upload.
    async fn note_parts(
        &self,
        user_id: i64,
        upload_id: i64,
        n: u32,
        at_ms: i64,
    ) -> std::result::Result<(), LedgerError>;
    /// Part URLs issued to `user_id` since `since_ms`.
    async fn parts_since(
        &self,
        user_id: i64,
        since_ms: i64,
    ) -> std::result::Result<u64, LedgerError>;
    /// Forget part-URL notes older than `before_ms` (the sweep).
    async fn purge_part_notes(&self, before_ms: i64) -> std::result::Result<u64, LedgerError>;
}

/// In-memory ledger for tests.
#[derive(Debug, Default)]
pub struct MemoryLedger {
    rows: Mutex<BTreeMap<i64, MountUpload>>,
    notes: Mutex<Vec<(i64, i64, u32)>>,
}

impl MemoryLedger {
    /// An empty ledger.
    pub fn new() -> Self {
        Self::default()
    }
}

fn poisoned<T>(_: T) -> LedgerError {
    LedgerError("lock poisoned".into())
}

#[async_trait]
impl UploadLedger for MemoryLedger {
    async fn insert(&self, upload: MountUpload) -> std::result::Result<(), LedgerError> {
        self.rows
            .lock()
            .map_err(poisoned)?
            .insert(upload.id, upload);
        Ok(())
    }
    async fn get(
        &self,
        mount_id: i64,
        id: i64,
    ) -> std::result::Result<Option<MountUpload>, LedgerError> {
        Ok(self
            .rows
            .lock()
            .map_err(poisoned)?
            .get(&id)
            .filter(|u| u.mount_id == mount_id)
            .cloned())
    }
    async fn list_open(
        &self,
        mount_id: i64,
        user_id: i64,
    ) -> std::result::Result<Vec<MountUpload>, LedgerError> {
        let mut v: Vec<MountUpload> = self
            .rows
            .lock()
            .map_err(poisoned)?
            .values()
            .filter(|u| {
                u.mount_id == mount_id && u.user_id == user_id && u.state == UploadState::Open
            })
            .cloned()
            .collect();
        v.sort_by_key(|u| std::cmp::Reverse(u.id));
        Ok(v)
    }
    async fn count_open(&self, user_id: i64) -> std::result::Result<usize, LedgerError> {
        Ok(self
            .rows
            .lock()
            .map_err(poisoned)?
            .values()
            .filter(|u| u.user_id == user_id && u.state == UploadState::Open)
            .count())
    }
    async fn finish(
        &self,
        mount_id: i64,
        id: i64,
        state: UploadState,
        at_ms: i64,
    ) -> std::result::Result<bool, LedgerError> {
        let mut rows = self.rows.lock().map_err(poisoned)?;
        match rows.get_mut(&id) {
            Some(u) if u.mount_id == mount_id && u.state == UploadState::Open => {
                u.state = state;
                u.updated_at = at_ms;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    async fn open_before(
        &self,
        cutoff_ms: i64,
        limit: usize,
    ) -> std::result::Result<Vec<MountUpload>, LedgerError> {
        let mut v: Vec<MountUpload> = self
            .rows
            .lock()
            .map_err(poisoned)?
            .values()
            .filter(|u| u.state == UploadState::Open && u.updated_at < cutoff_ms)
            .cloned()
            .collect();
        v.sort_by_key(|u| u.updated_at);
        v.truncate(limit);
        Ok(v)
    }
    async fn note_parts(
        &self,
        user_id: i64,
        upload_id: i64,
        n: u32,
        at_ms: i64,
    ) -> std::result::Result<(), LedgerError> {
        self.notes
            .lock()
            .map_err(poisoned)?
            .push((user_id, at_ms, n));
        if let Some(u) = self.rows.lock().map_err(poisoned)?.get_mut(&upload_id) {
            u.updated_at = at_ms;
        }
        Ok(())
    }
    async fn parts_since(
        &self,
        user_id: i64,
        since_ms: i64,
    ) -> std::result::Result<u64, LedgerError> {
        Ok(self
            .notes
            .lock()
            .map_err(poisoned)?
            .iter()
            .filter(|(u, at, _)| *u == user_id && *at >= since_ms)
            .map(|(_, _, n)| u64::from(*n))
            .sum())
    }
    async fn purge_part_notes(&self, before_ms: i64) -> std::result::Result<u64, LedgerError> {
        let mut notes = self.notes.lock().map_err(poisoned)?;
        let before = notes.len();
        notes.retain(|(_, at, _)| *at >= before_ms);
        Ok((before - notes.len()) as u64)
    }
}

/// The bucket calls of the write routes. Keys are mount-relative; a key ending in `/` is a folder marker.
#[async_trait]
pub trait MountIo: Send + Sync {
    /// Start a multipart upload; the bucket's upload id.
    async fn begin(&self, mount: &Mount, key: &str, content_type: &str) -> Result<String>;
    /// A presigned `PUT` for one part (local signing, no round trip).
    fn part_url(
        &self,
        mount: &Mount,
        key: &str,
        upload_id: &str,
        part: u32,
        ttl: Duration,
    ) -> Result<String>;
    /// The parts the bucket holds for an upload.
    async fn parts(&self, mount: &Mount, key: &str, upload_id: &str) -> Result<Vec<UploadedPart>>;
    /// Assemble the object from `(part number, etag)`.
    async fn complete(
        &self,
        mount: &Mount,
        key: &str,
        upload_id: &str,
        parts: &[(u32, String)],
    ) -> Result<()>;
    /// Abandon an upload at the bucket.
    async fn abort(&self, mount: &Mount, key: &str, upload_id: &str) -> Result<()>;
    /// Write a zero-byte folder marker (`key` ends in `/`).
    async fn put_marker(&self, mount: &Mount, key: &str) -> Result<()>;
    /// Delete an object or marker.
    async fn delete(&self, mount: &Mount, key: &str) -> Result<()>;
    /// Copy inside the bucket (objects up to [`COPY_MAX_BYTES`]).
    async fn copy(&self, mount: &Mount, from: &str, to: &str) -> Result<()>;
    /// Every key under `prefix` (empty or ending in `/`; markers included) with its size, at most `cap` of them;
    /// one more than `cap` means "there are more".
    async fn list_under(
        &self,
        mount: &Mount,
        prefix: &str,
        cap: usize,
    ) -> Result<Vec<(String, u64)>>;
}

/// [`MountIo`] over real signed requests with the mount's own credentials.
pub struct S3MountIo {
    sealer: Arc<dyn Sealer>,
    timeout: Duration,
}

impl S3MountIo {
    /// A client factory; every call builds the mount's client (a struct of a few strings, no I/O).
    pub fn new(sealer: Arc<dyn Sealer>) -> Self {
        Self {
            sealer,
            timeout: Duration::from_secs(8),
        }
    }

    fn client(&self, mount: &Mount) -> Result<crate::s3multipart::S3Multipart> {
        mount
            .multipart(self.sealer.as_ref(), self.timeout)
            .map_err(|e| StorageError::Backend(e.to_string()))
    }
}

#[async_trait]
impl MountIo for S3MountIo {
    async fn begin(&self, mount: &Mount, key: &str, content_type: &str) -> Result<String> {
        self.client(mount)?
            .create(&mount.bucket_key(key), content_type)
            .await
    }
    fn part_url(
        &self,
        mount: &Mount,
        key: &str,
        upload_id: &str,
        part: u32,
        ttl: Duration,
    ) -> Result<String> {
        self.client(mount)?
            .presign_part(&mount.bucket_key(key), upload_id, part, ttl)
    }
    async fn parts(&self, mount: &Mount, key: &str, upload_id: &str) -> Result<Vec<UploadedPart>> {
        self.client(mount)?
            .list_parts(&mount.bucket_key(key), upload_id)
            .await
    }
    async fn complete(
        &self,
        mount: &Mount,
        key: &str,
        upload_id: &str,
        parts: &[(u32, String)],
    ) -> Result<()> {
        // Assembling a large object can take a while at the provider.
        let c = mount
            .multipart(self.sealer.as_ref(), Duration::from_secs(60))
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        c.complete(&mount.bucket_key(key), upload_id, parts).await
    }
    async fn abort(&self, mount: &Mount, key: &str, upload_id: &str) -> Result<()> {
        self.client(mount)?
            .abort(&mount.bucket_key(key), upload_id)
            .await
    }
    async fn put_marker(&self, mount: &Mount, key: &str) -> Result<()> {
        self.client(mount)?.put_marker(&mount.bucket_key(key)).await
    }
    async fn delete(&self, mount: &Mount, key: &str) -> Result<()> {
        self.client(mount)?
            .delete_object(&mount.bucket_key(key))
            .await
    }
    async fn copy(&self, mount: &Mount, from: &str, to: &str) -> Result<()> {
        let c = mount
            .multipart(self.sealer.as_ref(), Duration::from_secs(30))
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        c.copy_object(&mount.bucket_key(from), &mount.bucket_key(to))
            .await
    }
    async fn list_under(
        &self,
        mount: &Mount,
        prefix: &str,
        cap: usize,
    ) -> Result<Vec<(String, u64)>> {
        let base = mount.prefix.trim_matches('/');
        let bucket_prefix = if prefix.is_empty() {
            if base.is_empty() {
                String::new()
            } else {
                format!("{base}/")
            }
        } else {
            mount.bucket_key(prefix)
        };
        let rows = self.client(mount)?.list_keys(&bucket_prefix, cap).await?;
        let strip = format!("{base}/");
        Ok(rows
            .into_iter()
            .map(|(k, s)| {
                let rel = if base.is_empty() {
                    k
                } else {
                    k.strip_prefix(&strip).map(str::to_owned).unwrap_or(k)
                };
                (rel, s)
            })
            .collect())
    }
}

/// One in-flight upload of the in-memory bucket.
#[derive(Debug, Default, Clone)]
struct MemUpload {
    key: String,
    parts: BTreeMap<u32, (u64, String)>,
}

/// An in-memory bucket for route tests: a map of `(mount id, key)` to size, and uploads whose parts a test
/// "sends" with [`MemoryMountIo::send_part`] the way the browser's `PUT` would.
#[derive(Debug, Default)]
pub struct MemoryMountIo {
    objects: Mutex<BTreeMap<(i64, String), u64>>,
    uploads: Mutex<BTreeMap<String, MemUpload>>,
    next: Mutex<u64>,
    /// When set, every call fails like an unreachable bucket.
    pub broken: Mutex<bool>,
}

impl MemoryMountIo {
    /// An empty bucket.
    pub fn new() -> Self {
        Self::default()
    }

    /// Put an object (a test fixture).
    pub fn seed(&self, mount_id: i64, key: &str, size: u64) {
        if let Ok(mut o) = self.objects.lock() {
            o.insert((mount_id, key.to_owned()), size);
        }
    }

    /// Whether the bucket holds the key.
    pub fn has(&self, mount_id: i64, key: &str) -> bool {
        self.objects
            .lock()
            .is_ok_and(|o| o.contains_key(&(mount_id, key.to_owned())))
    }

    /// Size of an object.
    pub fn size_of(&self, mount_id: i64, key: &str) -> Option<u64> {
        self.objects
            .lock()
            .ok()
            .and_then(|o| o.get(&(mount_id, key.to_owned())).copied())
    }

    /// Every key of a mount, sorted.
    pub fn keys(&self, mount_id: i64) -> Vec<String> {
        self.objects
            .lock()
            .map(|o| {
                o.keys()
                    .filter(|(m, _)| *m == mount_id)
                    .map(|(_, k)| k.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The browser's part `PUT`: the bucket stores the part and would answer its etag.
    pub fn send_part(&self, upload_id: &str, part: u32, size: u64) -> Option<String> {
        let etag = format!("\"etag-{part}-{size}\"");
        let mut u = self.uploads.lock().ok()?;
        u.get_mut(upload_id)?
            .parts
            .insert(part, (size, etag.clone()));
        Some(etag)
    }

    /// Whether an upload is still open at the bucket.
    pub fn is_open(&self, upload_id: &str) -> bool {
        self.uploads.lock().is_ok_and(|u| u.contains_key(upload_id))
    }

    fn check(&self) -> Result<()> {
        if self.broken.lock().is_ok_and(|b| *b) {
            return Err(StorageError::Backend("connection refused".into()));
        }
        Ok(())
    }
}

#[async_trait]
impl MountIo for MemoryMountIo {
    async fn begin(&self, mount: &Mount, key: &str, _content_type: &str) -> Result<String> {
        self.check()?;
        let mut n = self
            .next
            .lock()
            .map_err(|_| StorageError::Backend("lock".into()))?;
        *n += 1;
        let id = format!("mem-upload-{}-{}", mount.id, *n);
        self.uploads
            .lock()
            .map_err(|_| StorageError::Backend("lock".into()))?
            .insert(
                id.clone(),
                MemUpload {
                    key: key.to_owned(),
                    parts: BTreeMap::new(),
                },
            );
        Ok(id)
    }
    fn part_url(
        &self,
        mount: &Mount,
        key: &str,
        upload_id: &str,
        part: u32,
        ttl: Duration,
    ) -> Result<String> {
        self.check()?;
        Ok(format!(
            "memory://bucket/{}/{key}?uploadId={upload_id}&partNumber={part}&expires_in={}",
            mount.id,
            ttl.as_secs()
        ))
    }
    async fn parts(&self, _: &Mount, _key: &str, upload_id: &str) -> Result<Vec<UploadedPart>> {
        self.check()?;
        let u = self
            .uploads
            .lock()
            .map_err(|_| StorageError::Backend("lock".into()))?;
        let up = u
            .get(upload_id)
            .ok_or_else(|| StorageError::NotFound(upload_id.to_owned()))?;
        Ok(up
            .parts
            .iter()
            .map(|(n, (size, etag))| UploadedPart {
                number: *n,
                size: *size,
                etag: etag.clone(),
            })
            .collect())
    }
    async fn complete(
        &self,
        mount: &Mount,
        key: &str,
        upload_id: &str,
        parts: &[(u32, String)],
    ) -> Result<()> {
        self.check()?;
        let mut u = self
            .uploads
            .lock()
            .map_err(|_| StorageError::Backend("lock".into()))?;
        let up = u
            .get(upload_id)
            .ok_or_else(|| StorageError::NotFound(upload_id.to_owned()))?
            .clone();
        let mut total = 0u64;
        for (n, etag) in parts {
            match up.parts.get(n) {
                Some((size, e)) if e == etag => total += size,
                _ => return Err(StorageError::Backend("InvalidPart".into())),
            }
        }
        if up.key != key {
            return Err(StorageError::Backend("key mismatch".into()));
        }
        u.remove(upload_id);
        drop(u);
        self.objects
            .lock()
            .map_err(|_| StorageError::Backend("lock".into()))?
            .insert((mount.id, key.to_owned()), total);
        Ok(())
    }
    async fn abort(&self, _: &Mount, _key: &str, upload_id: &str) -> Result<()> {
        self.check()?;
        self.uploads
            .lock()
            .map_err(|_| StorageError::Backend("lock".into()))?
            .remove(upload_id);
        Ok(())
    }
    async fn put_marker(&self, mount: &Mount, key: &str) -> Result<()> {
        self.check()?;
        self.seed(mount.id, key, 0);
        Ok(())
    }
    async fn delete(&self, mount: &Mount, key: &str) -> Result<()> {
        self.check()?;
        self.objects
            .lock()
            .map_err(|_| StorageError::Backend("lock".into()))?
            .remove(&(mount.id, key.to_owned()));
        Ok(())
    }
    async fn copy(&self, mount: &Mount, from: &str, to: &str) -> Result<()> {
        self.check()?;
        let mut o = self
            .objects
            .lock()
            .map_err(|_| StorageError::Backend("lock".into()))?;
        let size = *o
            .get(&(mount.id, from.to_owned()))
            .ok_or_else(|| StorageError::NotFound(from.to_owned()))?;
        o.insert((mount.id, to.to_owned()), size);
        Ok(())
    }
    async fn list_under(
        &self,
        mount: &Mount,
        prefix: &str,
        cap: usize,
    ) -> Result<Vec<(String, u64)>> {
        self.check()?;
        let o = self
            .objects
            .lock()
            .map_err(|_| StorageError::Backend("lock".into()))?;
        Ok(o.iter()
            .filter(|((m, k), _)| *m == mount.id && k.starts_with(prefix))
            .map(|((_, k), s)| (k.clone(), *s))
            .take(cap + 1)
            .collect())
    }
}

/// Why a rename or move was refused before anything was touched.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MoveError {
    /// Source and target are the same.
    #[error("the source and the target are the same")]
    Same,
    /// A folder cannot move into itself.
    #[error("a folder cannot move into itself")]
    IntoItself,
    /// One side is a folder and the other is not.
    #[error("a folder moves to a folder and an object to an object")]
    KindMismatch,
    /// More keys than [`FOLDER_OP_CAP`].
    #[error("the folder has more than {FOLDER_OP_CAP} entries")]
    TooMany,
    /// An object is over [`COPY_MAX_BYTES`].
    #[error("an object is larger than 5 GiB; move it at the provider")]
    TooLarge,
    /// Nothing under the source.
    #[error("nothing there")]
    Missing,
}

/// The copy plan of a rename or move: `(from, to, size)` for every object, markers included (size 0).
/// `from` and `to` are both folders (end in `/`) or both objects. `keys` is what the bucket holds under the
/// source (for an object: its own entry). Pure, so every refusal is a unit test.
pub fn plan_move(
    from: &str,
    to: &str,
    keys: &[(String, u64)],
) -> std::result::Result<Vec<(String, String, u64)>, MoveError> {
    let folder = from.ends_with('/');
    if folder != to.ends_with('/') {
        return Err(MoveError::KindMismatch);
    }
    if from == to {
        return Err(MoveError::Same);
    }
    if folder && to.starts_with(from) {
        return Err(MoveError::IntoItself);
    }
    if keys.len() > FOLDER_OP_CAP {
        return Err(MoveError::TooMany);
    }
    let mut plan = Vec::new();
    for (k, size) in keys {
        let target = if folder {
            match k.strip_prefix(from) {
                Some(rest) => format!("{to}{rest}"),
                None => continue,
            }
        } else if k == from {
            to.to_owned()
        } else {
            continue;
        };
        if *size > COPY_MAX_BYTES {
            return Err(MoveError::TooLarge);
        }
        plan.push((k.clone(), target, *size));
    }
    if plan.is_empty() {
        return Err(MoveError::Missing);
    }
    Ok(plan)
}

/// Parts an upload of `size` bytes needs.
pub fn part_count(size: u64) -> u32 {
    let n = size.div_ceil(PART_SIZE).max(1);
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(s: &str, n: u64) -> (String, u64) {
        (s.to_owned(), n)
    }

    #[test]
    fn part_counts() {
        assert_eq!(part_count(0), 1);
        assert_eq!(part_count(1), 1);
        assert_eq!(part_count(PART_SIZE), 1);
        assert_eq!(part_count(PART_SIZE + 1), 2);
        assert_eq!(part_count(40 * 1024 * 1024), 3);
        assert_eq!(part_count(MAX_UPLOAD_BYTES), MAX_PARTS);
        assert!(part_count(MAX_UPLOAD_BYTES + 1) > MAX_PARTS);
    }

    #[test]
    fn move_plans_cover_objects_folders_and_markers() {
        let keys = [k("a/", 0), k("a/x.mp3", 5), k("a/b/", 0), k("a/b/y.mp3", 7)];
        let plan = plan_move("a/", "z/", &keys).unwrap();
        assert_eq!(
            plan,
            vec![
                ("a/".into(), "z/".into(), 0),
                ("a/x.mp3".into(), "z/x.mp3".into(), 5),
                ("a/b/".into(), "z/b/".into(), 0),
                ("a/b/y.mp3".into(), "z/b/y.mp3".into(), 7),
            ]
        );
        let one = plan_move("a/x.mp3", "c/x.mp3", &keys).unwrap();
        assert_eq!(one, vec![("a/x.mp3".into(), "c/x.mp3".into(), 5)]);
    }

    #[test]
    fn move_refusals() {
        let keys = [k("a/x", 1)];
        assert_eq!(plan_move("a/", "a/", &keys), Err(MoveError::Same));
        assert_eq!(plan_move("a/", "a/b/", &keys), Err(MoveError::IntoItself));
        assert_eq!(plan_move("a/", "b", &keys), Err(MoveError::KindMismatch));
        assert_eq!(plan_move("a/x", "b/", &keys), Err(MoveError::KindMismatch));
        assert_eq!(plan_move("q/", "r/", &keys), Err(MoveError::Missing));
        let many: Vec<_> = (0..=FOLDER_OP_CAP)
            .map(|i| k(&format!("a/{i}"), 1))
            .collect();
        assert_eq!(plan_move("a/", "b/", &many), Err(MoveError::TooMany));
        let big = [k("a/huge.mkv", COPY_MAX_BYTES + 1)];
        assert_eq!(plan_move("a/", "b/", &big), Err(MoveError::TooLarge));
        assert_eq!(
            plan_move("a/huge.mkv", "b.mkv", &big),
            Err(MoveError::TooLarge)
        );
        // A sibling that merely shares the textual prefix is not inside the folder.
        let near = [k("ab/x", 1)];
        assert_eq!(plan_move("a/", "z/", &near), Err(MoveError::Missing));
    }

    #[tokio::test]
    async fn the_ledger_tracks_state_and_the_hourly_part_count() {
        let l = MemoryLedger::new();
        let up = |id: i64, user: i64, at: i64| MountUpload {
            id,
            mount_id: 7,
            user_id: user,
            key: format!("k{id}"),
            s3_upload_id: "u".into(),
            size: 1,
            part_size: PART_SIZE,
            part_count: 1,
            content_type: "audio/mpeg".into(),
            state: UploadState::Open,
            created_at: at,
            updated_at: at,
        };
        l.insert(up(1, 10, 100)).await.unwrap();
        l.insert(up(2, 10, 200)).await.unwrap();
        l.insert(up(3, 11, 300)).await.unwrap();
        assert_eq!(l.count_open(10).await.unwrap(), 2);
        assert_eq!(l.list_open(7, 10).await.unwrap().len(), 2);
        // Another mount's id does not see it.
        assert!(l.get(8, 1).await.unwrap().is_none());
        assert!(l.finish(7, 1, UploadState::Completed, 400).await.unwrap());
        assert!(!l.finish(7, 1, UploadState::Aborted, 401).await.unwrap());
        assert_eq!(l.count_open(10).await.unwrap(), 1);
        // The sweep sees open rows older than the cutoff.
        assert_eq!(l.open_before(250, 10).await.unwrap().len(), 1);
        l.note_parts(10, 2, 3, 500).await.unwrap();
        l.note_parts(10, 2, 2, 600).await.unwrap();
        assert_eq!(l.parts_since(10, 0).await.unwrap(), 5);
        assert_eq!(l.parts_since(10, 550).await.unwrap(), 2);
        assert_eq!(l.parts_since(11, 0).await.unwrap(), 0);
        // Touching the upload keeps it out of the sweep.
        assert!(l.open_before(250, 10).await.unwrap().is_empty());
        assert_eq!(l.purge_part_notes(550).await.unwrap(), 1);
    }
}
