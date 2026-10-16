//! Resumable multipart uploads for chapter files (REV-UP-001, spec section 3).
//!
//! The browser cuts a file into parts of [`DEFAULT_PART_SIZE`] (64 MB) and `PUT`s up to four of
//! them at once to presigned part URLs. The server never needs the part ETags from the browser: at
//! completion it asks the store which parts it holds (`ListParts`), checks them against the
//! announced size and completes the upload itself. A refreshed page asks for the parts again
//! ([`crate::LibraryService::upload_parts`]) and sends only the missing ones, so an interrupted
//! 40 GB upload resumes where it stopped.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use serde::Serialize;
use sr_storage::ObjectStorage;
use sr_storage::s3multipart::S3Multipart;
use utoipa::ToSchema;

/// Part size the browser uses (spec: 64 MB parts).
pub const DEFAULT_PART_SIZE: u64 = 64 * 1024 * 1024;
/// S3 refuses non-final parts below this size.
pub const MIN_PART_SIZE: u64 = 5 * 1024 * 1024;
/// S3 allows at most this many parts.
pub const MAX_PARTS: u64 = 10_000;
/// How long a part URL works. Long, because a 40 GB upload takes hours; the browser asks for fresh
/// URLs when it resumes anyway.
pub const PART_URL_TTL: Duration = Duration::from_secs(6 * 3600);

/// A part the store holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPart {
    pub number: u32,
    pub size: u64,
    pub etag: String,
}

/// The multipart operations of the object store. `sr-storage`'s [`S3Multipart`] implements it;
/// tests use [`MemoryPartStore`].
#[async_trait]
pub trait PartStore: Send + Sync {
    async fn create(&self, key: &str, content_type: &str) -> Result<String, String>;
    fn presign_part(
        &self,
        key: &str,
        upload_id: &str,
        part_number: u32,
        ttl: Duration,
    ) -> Result<String, String>;
    async fn list_parts(&self, key: &str, upload_id: &str) -> Result<Vec<StoredPart>, String>;
    async fn complete(
        &self,
        key: &str,
        upload_id: &str,
        parts: &[StoredPart],
    ) -> Result<(), String>;
    async fn abort(&self, key: &str, upload_id: &str) -> Result<(), String>;
}

#[async_trait]
impl PartStore for S3Multipart {
    async fn create(&self, key: &str, content_type: &str) -> Result<String, String> {
        S3Multipart::create(self, key, content_type)
            .await
            .map_err(|e| e.to_string())
    }

    fn presign_part(
        &self,
        key: &str,
        upload_id: &str,
        part_number: u32,
        ttl: Duration,
    ) -> Result<String, String> {
        S3Multipart::presign_part(self, key, upload_id, part_number, ttl).map_err(|e| e.to_string())
    }

    async fn list_parts(&self, key: &str, upload_id: &str) -> Result<Vec<StoredPart>, String> {
        S3Multipart::list_parts(self, key, upload_id)
            .await
            .map(|v| {
                v.into_iter()
                    .map(|p| StoredPart {
                        number: p.number,
                        size: p.size,
                        etag: p.etag,
                    })
                    .collect()
            })
            .map_err(|e| e.to_string())
    }

    async fn complete(
        &self,
        key: &str,
        upload_id: &str,
        parts: &[StoredPart],
    ) -> Result<(), String> {
        let parts: Vec<(u32, String)> = parts.iter().map(|p| (p.number, p.etag.clone())).collect();
        S3Multipart::complete(self, key, upload_id, &parts)
            .await
            .map_err(|e| e.to_string())
    }

    async fn abort(&self, key: &str, upload_id: &str) -> Result<(), String> {
        S3Multipart::abort(self, key, upload_id)
            .await
            .map_err(|e| e.to_string())
    }
}

/// Where multipart uploads go, and how big the parts are.
#[derive(Clone)]
pub struct MultipartWiring {
    pub store: Arc<dyn PartStore>,
    pub part_size: u64,
}

impl MultipartWiring {
    pub fn new(store: Arc<dyn PartStore>) -> Self {
        Self {
            store,
            part_size: DEFAULT_PART_SIZE,
        }
    }

    /// A different part size (tests use tiny parts).
    pub fn with_part_size(mut self, part_size: u64) -> Self {
        self.part_size = part_size.max(1);
        self
    }

    /// Part size and count for a file of `size` bytes: at least [`DEFAULT_PART_SIZE`]-sized parts,
    /// larger when the file would otherwise need more than [`MAX_PARTS`].
    pub fn plan(&self, size: u64) -> (u64, u32) {
        let part = self.part_size.max(size.div_ceil(MAX_PARTS)).max(1);
        let count = size.div_ceil(part).max(1);
        (part, u32::try_from(count).unwrap_or(u32::MAX))
    }
}

/// One presigned part URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct PartUrl {
    pub part_number: u32,
    pub url: String,
}

/// The multipart part of a new upload slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct MultipartSlot {
    /// Every part but the last has exactly this size.
    pub part_size: u64,
    pub part_count: u32,
    pub parts: Vec<PartUrl>,
}

/// A part that already arrived.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ReceivedPart {
    pub part_number: u32,
    pub size: u64,
}

/// What a resuming client needs: which parts are in, and URLs for the rest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct UploadParts {
    pub part_size: u64,
    pub part_count: u32,
    pub received: Vec<ReceivedPart>,
    /// Fresh URLs for the parts still missing (or received with the wrong size).
    pub missing: Vec<PartUrl>,
    pub expires_in_secs: u64,
}

/// Check the stored parts against the announced size. Returns the parts to complete with, or the
/// reason they cannot be completed.
pub fn check_parts(
    stored: &[StoredPart],
    size: u64,
    part_size: u64,
    part_count: u32,
) -> Result<Vec<StoredPart>, String> {
    let mut by_number: BTreeMap<u32, &StoredPart> = BTreeMap::new();
    for p in stored {
        by_number.insert(p.number, p);
    }
    let mut out = Vec::new();
    let mut total: u64 = 0;
    for n in 1..=part_count {
        let want = if n == part_count {
            size.saturating_sub(part_size * u64::from(part_count - 1))
        } else {
            part_size
        };
        match by_number.get(&n) {
            None => return Err(format!("part {n} of {part_count} has not been uploaded")),
            Some(p) if p.size != want => {
                return Err(format!(
                    "part {n} has {} bytes, expected {want}; upload it again",
                    p.size
                ));
            }
            Some(p) => {
                total += p.size;
                out.push((*p).clone());
            }
        }
    }
    if by_number.len() != out.len() {
        return Err("the store holds parts beyond the announced size".into());
    }
    if total != size {
        return Err("the uploaded parts do not add up to the announced size".into());
    }
    Ok(out)
}

/// An in-memory [`PartStore`] for tests: parts are put with [`MemoryPartStore::put_part`] (the
/// stand-in for the browser's `PUT`), completion assembles the object in the given storage.
pub struct MemoryPartStore {
    storage: Arc<dyn ObjectStorage>,
    state: Mutex<MemoryState>,
}

#[derive(Default)]
struct MemoryState {
    next: u64,
    /// upload id -> part number -> bytes
    uploads: BTreeMap<String, BTreeMap<u32, Bytes>>,
    aborted: Vec<String>,
}

impl MemoryPartStore {
    pub fn new(storage: Arc<dyn ObjectStorage>) -> Self {
        Self {
            storage,
            state: Mutex::new(MemoryState::default()),
        }
    }

    /// What the browser's part `PUT` does.
    pub fn put_part(&self, upload_id: &str, number: u32, data: Bytes) {
        if let Ok(mut s) = self.state.lock() {
            s.uploads
                .entry(upload_id.to_owned())
                .or_default()
                .insert(number, data);
        }
    }

    /// Upload ids that were aborted.
    pub fn aborted(&self) -> Vec<String> {
        self.state
            .lock()
            .map(|s| s.aborted.clone())
            .unwrap_or_default()
    }
}

#[async_trait]
impl PartStore for MemoryPartStore {
    async fn create(&self, _key: &str, _content_type: &str) -> Result<String, String> {
        let mut s = self.state.lock().map_err(|e| e.to_string())?;
        s.next += 1;
        let id = format!("mem-{}", s.next);
        s.uploads.insert(id.clone(), BTreeMap::new());
        Ok(id)
    }

    fn presign_part(
        &self,
        key: &str,
        upload_id: &str,
        part_number: u32,
        _ttl: Duration,
    ) -> Result<String, String> {
        Ok(format!(
            "https://storage.test/{key}?uploadId={upload_id}&partNumber={part_number}"
        ))
    }

    async fn list_parts(&self, _key: &str, upload_id: &str) -> Result<Vec<StoredPart>, String> {
        let s = self.state.lock().map_err(|e| e.to_string())?;
        let parts = s.uploads.get(upload_id).ok_or("no such upload")?;
        Ok(parts
            .iter()
            .map(|(n, b)| StoredPart {
                number: *n,
                size: b.len() as u64,
                etag: format!("\"etag-{n}\""),
            })
            .collect())
    }

    async fn complete(
        &self,
        key: &str,
        upload_id: &str,
        parts: &[StoredPart],
    ) -> Result<(), String> {
        let data = {
            let s = self.state.lock().map_err(|e| e.to_string())?;
            let held = s.uploads.get(upload_id).ok_or("no such upload")?;
            let mut buf = Vec::new();
            for p in parts {
                buf.extend_from_slice(held.get(&p.number).ok_or("missing part")?);
            }
            buf
        };
        self.storage
            .put(key, Bytes::from(data))
            .await
            .map_err(|e| e.to_string())
    }

    async fn abort(&self, _key: &str, upload_id: &str) -> Result<(), String> {
        let mut s = self.state.lock().map_err(|e| e.to_string())?;
        s.uploads.remove(upload_id);
        s.aborted.push(upload_id.to_owned());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(number: u32, size: u64) -> StoredPart {
        StoredPart {
            number,
            size,
            etag: format!("e{number}"),
        }
    }

    #[test]
    fn plans_parts_of_the_default_size() {
        let w = MultipartWiring::new(Arc::new(MemoryPartStore::new(Arc::new(
            sr_storage::ObjectStoreStorage::in_memory(),
        ))));
        assert_eq!(w.plan(1), (DEFAULT_PART_SIZE, 1));
        assert_eq!(w.plan(DEFAULT_PART_SIZE), (DEFAULT_PART_SIZE, 1));
        assert_eq!(w.plan(DEFAULT_PART_SIZE + 1), (DEFAULT_PART_SIZE, 2));
        // A 40 GB season needs 640 parts of 64 MB.
        assert_eq!(w.plan(40 * 1024 * 1024 * 1024), (DEFAULT_PART_SIZE, 640));
    }

    #[test]
    fn grows_the_part_size_to_stay_under_the_part_limit() {
        let w = MultipartWiring::new(Arc::new(MemoryPartStore::new(Arc::new(
            sr_storage::ObjectStoreStorage::in_memory(),
        ))))
        .with_part_size(10);
        let (size, count) = w.plan(1_000_000);
        assert!(u64::from(count) <= MAX_PARTS);
        assert!(size * u64::from(count) >= 1_000_000);
    }

    #[test]
    fn complete_parts_pass_the_check() {
        let got = check_parts(&[part(2, 4), part(1, 10), part(3, 3)], 17, 10, 3);
        // Part 2 is a middle part and must have the full part size.
        assert!(got.is_err());
        let got = check_parts(&[part(2, 10), part(1, 10), part(3, 3)], 23, 10, 3).unwrap();
        assert_eq!(
            got.iter().map(|p| p.number).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn a_missing_short_or_extra_part_is_named() {
        let e = check_parts(&[part(1, 10)], 23, 10, 3).unwrap_err();
        assert!(e.contains("part 2"), "{e}");
        let e = check_parts(&[part(1, 10), part(2, 10), part(3, 2)], 23, 10, 3).unwrap_err();
        assert!(e.contains("part 3"), "{e}");
        let e = check_parts(
            &[part(1, 10), part(2, 10), part(3, 3), part(4, 1)],
            23,
            10,
            3,
        )
        .unwrap_err();
        assert!(e.contains("beyond"), "{e}");
    }
}
