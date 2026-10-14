//! [`OutputStorage`] implementations over S3-compatible storage.
//!
//! * [`S3OutputStorage`]: real multipart uploads with part-level presigned URLs
//!   (`sr_storage::s3multipart`).
//! * [`SinglePutStorage`]: the fallback for storage that cannot presign parts. Every output is
//!   one presigned `PUT` of the whole object; pair it with `ServiceConfig::single_part()` so the
//!   service never asks for more than one part.

use crate::worker_api::{OutputStorage, PartDone};
use async_trait::async_trait;
use sr_storage::s3multipart::S3Multipart;
use sr_storage::{ObjectStorage, PresignMethod};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Multipart uploads against S3 / Garage.
#[derive(Debug, Clone)]
pub struct S3OutputStorage {
    s3: S3Multipart,
}

impl S3OutputStorage {
    pub fn new(s3: S3Multipart) -> Self {
        Self { s3 }
    }
}

#[async_trait]
impl OutputStorage for S3OutputStorage {
    async fn init_multipart(&self, key: &str, content_type: &str) -> Result<String, String> {
        self.s3
            .create(key, content_type)
            .await
            .map_err(|e| e.to_string())
    }

    async fn presign_part(
        &self,
        key: &str,
        upload_id: &str,
        part_number: u32,
        ttl_secs: u64,
    ) -> Result<String, String> {
        self.s3
            .presign_part(key, upload_id, part_number, Duration::from_secs(ttl_secs))
            .map_err(|e| e.to_string())
    }

    async fn complete_multipart(
        &self,
        key: &str,
        upload_id: &str,
        parts: &[PartDone],
    ) -> Result<(), String> {
        let parts: Vec<(u32, String)> = parts
            .iter()
            .map(|p| (p.part_number, p.etag.clone()))
            .collect();
        self.s3
            .complete(key, upload_id, &parts)
            .await
            .map_err(|e| e.to_string())
    }
}

/// One presigned `PUT` per output, over any [`ObjectStorage`] that can presign uploads.
pub struct SinglePutStorage {
    storage: Arc<dyn ObjectStorage>,
    counter: AtomicU64,
}

impl SinglePutStorage {
    pub fn new(storage: Arc<dyn ObjectStorage>) -> Self {
        Self {
            storage,
            counter: AtomicU64::new(0),
        }
    }
}

#[async_trait]
impl OutputStorage for SinglePutStorage {
    async fn init_multipart(&self, _key: &str, _content_type: &str) -> Result<String, String> {
        Ok(format!(
            "single-{}",
            self.counter.fetch_add(1, Ordering::Relaxed) + 1
        ))
    }

    async fn presign_part(
        &self,
        key: &str,
        _upload_id: &str,
        part_number: u32,
        ttl_secs: u64,
    ) -> Result<String, String> {
        if part_number != 1 {
            return Err("single-part storage has exactly one part".into());
        }
        self.storage
            .presign(PresignMethod::Put, key, Duration::from_secs(ttl_secs))
            .await
            .map_err(|e| e.to_string())
    }

    /// The object must exist; the size is checked against the declaration by the caller.
    async fn complete_multipart(
        &self,
        key: &str,
        _upload_id: &str,
        parts: &[PartDone],
    ) -> Result<(), String> {
        if parts.len() != 1 {
            return Err("single-part storage has exactly one part".into());
        }
        self.storage
            .head(key)
            .await
            .map(|_| ())
            .map_err(|e| format!("the uploaded object is not there: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use sr_storage::ObjectStoreStorage;

    #[tokio::test]
    async fn single_put_presigns_one_part_and_checks_the_object() {
        let mem = Arc::new(ObjectStoreStorage::in_memory());
        let s = SinglePutStorage::new(mem.clone());
        let id = s
            .init_multipart("transcode/1/1/v/a.m4s", "video/mp4")
            .await
            .unwrap();
        let id2 = s
            .init_multipart("transcode/1/1/v/b.m4s", "video/mp4")
            .await
            .unwrap();
        assert_ne!(id, id2);
        let url = s
            .presign_part("transcode/1/1/v/a.m4s", &id, 1, 600)
            .await
            .unwrap();
        assert!(url.contains("transcode/1/1/v/a.m4s") && url.contains("method=PUT"));
        assert!(
            s.presign_part("transcode/1/1/v/a.m4s", &id, 2, 600)
                .await
                .is_err()
        );
        let part = PartDone {
            part_number: 1,
            etag: "e".into(),
        };
        assert!(
            s.complete_multipart("transcode/1/1/v/a.m4s", &id, std::slice::from_ref(&part))
                .await
                .is_err(),
            "the object was never uploaded"
        );
        mem.put("transcode/1/1/v/a.m4s", Bytes::from_static(b"x"))
            .await
            .unwrap();
        s.complete_multipart("transcode/1/1/v/a.m4s", &id, &[part])
            .await
            .unwrap();
    }
}
