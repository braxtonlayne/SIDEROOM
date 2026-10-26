//! Object storage trait and the `object_store`-backed implementation.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use futures::TryStreamExt;
use futures::stream::BoxStream;
use object_store::path::Path;
use object_store::signer::{Method, Signer};
use object_store::{
    GetOptions, GetRange, MultipartUpload, ObjectStore, ObjectStoreExt, PutPayload,
};

use crate::error::{Result, StorageError};

/// HTTP method a presigned URL is valid for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresignMethod {
    /// Download.
    Get,
    /// Upload.
    Put,
}

/// Metadata about a stored object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectInfo {
    /// Object key.
    pub key: String,
    /// Size in bytes.
    pub size: u64,
    /// Backend entity tag, when the backend provides one.
    pub etag: Option<String>,
}

/// A streaming object body: `body` yields exactly the bytes of `range`
/// (`start..end`, end exclusive) of an object whose metadata is `info`.
pub struct ObjectStream {
    /// Metadata of the whole object (size is the total length).
    pub info: ObjectInfo,
    /// The byte range the stream covers.
    pub range: std::ops::Range<u64>,
    /// The body, never fully buffered by the storage layer.
    pub body: BoxStream<'static, Result<Bytes>>,
}

/// An in-progress server-side multipart upload.
///
/// Parts are uploaded in order; call `abort` instead of `complete` to discard.
#[async_trait]
pub trait MultipartSession: Send {
    /// Upload the next part (S3 requires each part except the last to be >= 5 MiB).
    async fn put_part(&mut self, data: Bytes) -> Result<()>;
    /// Finish the upload, making the object visible.
    async fn complete(&mut self) -> Result<()>;
    /// Abort and discard uploaded parts.
    async fn abort(&mut self) -> Result<()>;
}

/// Storage abstraction used by the rest of the platform.
#[async_trait]
pub trait ObjectStorage: Send + Sync {
    /// Store an object, replacing any existing one.
    async fn put(&self, key: &str, data: Bytes) -> Result<()>;
    /// Fetch a whole object.
    async fn get(&self, key: &str) -> Result<Bytes>;
    /// Fetch a byte range `[start, end)` of an object.
    async fn get_range(&self, key: &str, start: u64, end: u64) -> Result<Bytes>;
    /// Stream the byte range `range` (`start..end`, end exclusive; `None` = whole
    /// object) without buffering it. The default implementation falls back to
    /// [`ObjectStorage::get_range`] as a single chunk; real backends override it.
    async fn get_stream(
        &self,
        key: &str,
        range: Option<std::ops::Range<u64>>,
    ) -> Result<ObjectStream> {
        let info = self.head(key).await?;
        let range = range.unwrap_or(0..info.size);
        let data = self.get_range(key, range.start, range.end).await?;
        Ok(ObjectStream {
            info,
            range,
            body: Box::pin(futures::stream::once(async move { Ok(data) })),
        })
    }
    /// Object size without fetching the body.
    async fn head(&self, key: &str) -> Result<ObjectInfo>;
    /// Delete an object.
    async fn delete(&self, key: &str) -> Result<()>;
    /// List objects under a prefix (recursive).
    async fn list(&self, prefix: &str) -> Result<Vec<ObjectInfo>>;
    /// One listing walk over the whole bucket for a size census (RDY-OBS-002): `visit(key, size)` is called for each
    /// object, at most `max_objects` of them and for at most `budget`. Returns `true` when the walk reached the end,
    /// `false` when it stopped at the cap or the budget. The default lists everything first; the `object_store`
    /// implementation streams the listing page by page and checks the cap and the clock between pages.
    async fn walk(
        &self,
        max_objects: u64,
        budget: Duration,
        visit: &mut (dyn for<'a> FnMut(&'a str, u64) + Send),
    ) -> Result<bool> {
        let started = std::time::Instant::now();
        let mut seen = 0u64;
        for o in self.list("").await? {
            if seen >= max_objects || started.elapsed() > budget {
                return Ok(false);
            }
            visit(&o.key, o.size);
            seen += 1;
        }
        Ok(true)
    }
    /// Produce a time-limited presigned URL.
    async fn presign(&self, method: PresignMethod, key: &str, ttl: Duration) -> Result<String>;
    /// Begin a multipart upload.
    async fn multipart_create(&self, key: &str) -> Result<Box<dyn MultipartSession>>;
}

/// [`ObjectStorage`] over any `object_store` backend, with an optional signer.
#[derive(Clone)]
pub struct ObjectStoreStorage {
    store: Arc<dyn ObjectStore>,
    signer: Option<Arc<dyn Signer>>,
}

impl std::fmt::Debug for ObjectStoreStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjectStoreStorage")
            .field("store", &self.store.to_string())
            .field("can_presign", &self.signer.is_some())
            .finish()
    }
}

impl ObjectStoreStorage {
    /// Wrap an arbitrary store, optionally with a signer for presigned URLs.
    pub fn new(store: Arc<dyn ObjectStore>, signer: Option<Arc<dyn Signer>>) -> Self {
        Self { store, signer }
    }

    /// Connect to an S3-compatible service (Garage, B2, R2, ...).
    pub fn s3(
        endpoint: &str,
        bucket: &str,
        region: &str,
        access_key: &str,
        secret_key: &str,
    ) -> Result<Self> {
        let s3 = Arc::new(
            object_store::aws::AmazonS3Builder::new()
                .with_endpoint(endpoint)
                .with_bucket_name(bucket)
                .with_region(region)
                .with_access_key_id(access_key)
                .with_secret_access_key(secret_key)
                .with_virtual_hosted_style_request(false)
                .with_allow_http(endpoint.starts_with("http://"))
                .build()?,
        );
        Ok(Self {
            store: s3.clone(),
            signer: Some(s3),
        })
    }

    /// In-memory backend for tests. Presigned URLs are fake `memory://` URLs.
    pub fn in_memory() -> Self {
        Self {
            store: Arc::new(object_store::memory::InMemory::new()),
            signer: Some(Arc::new(FakeSigner)),
        }
    }
}

/// Signer for the in-memory backend. Not a security mechanism.
#[derive(Debug)]
struct FakeSigner;

#[async_trait]
impl Signer for FakeSigner {
    async fn signed_url(
        &self,
        method: Method,
        path: &Path,
        expires_in: Duration,
    ) -> object_store::Result<object_store::signer::Url> {
        let url = format!(
            "memory://bucket/{}?method={}&expires_in={}",
            path,
            method,
            expires_in.as_secs()
        );
        object_store::signer::Url::parse(&url).map_err(|e| object_store::Error::Generic {
            store: "memory",
            source: Box::new(e),
        })
    }
}

struct Session {
    upload: Box<dyn MultipartUpload>,
    pending: Vec<object_store::UploadPart>,
}

#[async_trait]
impl MultipartSession for Session {
    async fn put_part(&mut self, data: Bytes) -> Result<()> {
        self.pending
            .push(self.upload.put_part(PutPayload::from(data)));
        Ok(())
    }

    async fn complete(&mut self) -> Result<()> {
        for part in std::mem::take(&mut self.pending) {
            part.await?;
        }
        self.upload.complete().await?;
        Ok(())
    }

    async fn abort(&mut self) -> Result<()> {
        self.pending.clear();
        self.upload.abort().await?;
        Ok(())
    }
}

#[async_trait]
impl ObjectStorage for ObjectStoreStorage {
    async fn put(&self, key: &str, data: Bytes) -> Result<()> {
        let path = Path::parse(key)?;
        self.store.put(&path, PutPayload::from(data)).await?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Bytes> {
        let path = Path::parse(key)?;
        Ok(self.store.get(&path).await?.bytes().await?)
    }

    async fn get_range(&self, key: &str, start: u64, end: u64) -> Result<Bytes> {
        let path = Path::parse(key)?;
        Ok(self.store.get_range(&path, start..end).await?)
    }

    async fn head(&self, key: &str) -> Result<ObjectInfo> {
        let path = Path::parse(key)?;
        let meta = self.store.head(&path).await?;
        Ok(ObjectInfo {
            key: meta.location.to_string(),
            size: meta.size,
            etag: meta.e_tag,
        })
    }

    async fn get_stream(
        &self,
        key: &str,
        range: Option<std::ops::Range<u64>>,
    ) -> Result<ObjectStream> {
        let path = Path::parse(key)?;
        let opts = GetOptions {
            range: range.clone().map(GetRange::Bounded),
            ..GetOptions::default()
        };
        let res = self.store.get_opts(&path, opts).await?;
        let info = ObjectInfo {
            key: res.meta.location.to_string(),
            size: res.meta.size,
            etag: res.meta.e_tag.clone(),
        };
        let range = if range.is_some() {
            res.range.clone()
        } else {
            0..info.size
        };
        Ok(ObjectStream {
            info,
            range,
            body: Box::pin(res.into_stream().map_err(StorageError::from)),
        })
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let path = Path::parse(key)?;
        self.store.delete(&path).await?;
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<ObjectInfo>> {
        let path = Path::parse(prefix)?;
        let metas: Vec<_> = self.store.list(Some(&path)).try_collect().await?;
        Ok(metas
            .into_iter()
            .map(|m| ObjectInfo {
                key: m.location.to_string(),
                size: m.size,
                etag: m.e_tag,
            })
            .collect())
    }

    async fn walk(
        &self,
        max_objects: u64,
        budget: Duration,
        visit: &mut (dyn for<'a> FnMut(&'a str, u64) + Send),
    ) -> Result<bool> {
        use futures::StreamExt;
        let started = std::time::Instant::now();
        let mut seen = 0u64;
        let mut stream = self.store.list(None);
        loop {
            let left = budget.saturating_sub(started.elapsed());
            if seen >= max_objects || left.is_zero() {
                return Ok(false);
            }
            match stream.next().await {
                None => return Ok(true),
                Some(meta) => {
                    let meta = meta?;
                    visit(meta.location.as_ref(), meta.size);
                    seen += 1;
                }
            }
        }
    }

    async fn presign(&self, method: PresignMethod, key: &str, ttl: Duration) -> Result<String> {
        let signer = self
            .signer
            .as_ref()
            .ok_or(StorageError::PresignUnsupported)?;
        let path = Path::parse(key)?;
        let m = match method {
            PresignMethod::Get => Method::GET,
            PresignMethod::Put => Method::PUT,
        };
        Ok(signer.signed_url(m, &path, ttl).await?.to_string())
    }

    async fn multipart_create(&self, key: &str) -> Result<Box<dyn MultipartSession>> {
        let path = Path::parse(key)?;
        let upload = self.store.put_multipart(&path).await?;
        Ok(Box::new(Session {
            upload,
            pending: Vec::new(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn walk_visits_every_object_and_stops_at_the_cap() {
        let s = ObjectStoreStorage::in_memory();
        for (k, n) in [
            ("a/1", 1usize),
            ("a/2", 2),
            ("b/3", 3),
            ("c/4", 4),
            ("c/5", 5),
        ] {
            s.put(k, Bytes::from(vec![0u8; n])).await.unwrap();
        }
        let mut total = 0u64;
        let done = s
            .walk(100, Duration::from_secs(5), &mut |_, size| total += size)
            .await
            .unwrap();
        assert!(done);
        assert_eq!(total, 15);
        let (mut seen, mut keys) = (0u64, Vec::new());
        let done = s
            .walk(3, Duration::from_secs(5), &mut |k, _| {
                seen += 1;
                keys.push(k.to_owned());
            })
            .await
            .unwrap();
        assert!(!done, "the cap stops the walk early");
        assert_eq!((seen, keys.len()), (3, 3));
        // A zero budget visits nothing and says so.
        let mut n = 0;
        assert!(
            !s.walk(100, Duration::ZERO, &mut |_, _| n += 1)
                .await
                .unwrap()
        );
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn put_get_head_range_list_delete() {
        let s = ObjectStoreStorage::in_memory();
        s.put("a/one.bin", Bytes::from_static(b"0123456789"))
            .await
            .unwrap();
        s.put("a/two.bin", Bytes::from_static(b"xy")).await.unwrap();
        s.put("b/three.bin", Bytes::from_static(b"z"))
            .await
            .unwrap();
        assert_eq!(
            s.get("a/one.bin").await.unwrap(),
            Bytes::from_static(b"0123456789")
        );
        assert_eq!(
            s.get_range("a/one.bin", 2, 5).await.unwrap(),
            Bytes::from_static(b"234")
        );
        assert_eq!(s.head("a/two.bin").await.unwrap().size, 2);
        let mut keys: Vec<_> = s
            .list("a")
            .await
            .unwrap()
            .into_iter()
            .map(|o| o.key)
            .collect();
        keys.sort();
        assert_eq!(keys, ["a/one.bin", "a/two.bin"]);
        s.delete("a/one.bin").await.unwrap();
        assert!(matches!(
            s.get("a/one.bin").await,
            Err(StorageError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn get_stream_whole_and_range() {
        let s = ObjectStoreStorage::in_memory();
        s.put("a/one.bin", Bytes::from_static(b"0123456789"))
            .await
            .unwrap();
        let collect = |st: ObjectStream| async move {
            let v: Vec<Bytes> = st.body.try_collect().await.unwrap();
            v.concat()
        };
        let st = s.get_stream("a/one.bin", None).await.unwrap();
        assert_eq!((st.info.size, st.range.clone()), (10, 0..10));
        assert!(st.info.etag.is_some());
        assert_eq!(collect(st).await, b"0123456789");
        let st = s.get_stream("a/one.bin", Some(2..5)).await.unwrap();
        assert_eq!((st.info.size, st.range.clone()), (10, 2..5));
        assert_eq!(collect(st).await, b"234");
        assert!(matches!(
            s.get_stream("a/none", None).await,
            Err(StorageError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn presign_get_and_put() {
        let s = ObjectStoreStorage::in_memory();
        let put = s
            .presign(PresignMethod::Put, "u/x", Duration::from_secs(60))
            .await
            .unwrap();
        let get = s
            .presign(PresignMethod::Get, "u/x", Duration::from_secs(60))
            .await
            .unwrap();
        assert!(put.contains("method=PUT") && put.contains("expires_in=60"));
        assert!(get.contains("method=GET"));
    }

    #[tokio::test]
    async fn presign_unsupported_without_signer() {
        let s = ObjectStoreStorage::new(Arc::new(object_store::memory::InMemory::new()), None);
        let r = s
            .presign(PresignMethod::Get, "k", Duration::from_secs(1))
            .await;
        assert!(matches!(r, Err(StorageError::PresignUnsupported)));
    }

    #[tokio::test]
    async fn multipart_complete_and_abort() {
        let s = ObjectStoreStorage::in_memory();
        let mut m = s.multipart_create("big/file").await.unwrap();
        m.put_part(Bytes::from_static(b"hello ")).await.unwrap();
        m.put_part(Bytes::from_static(b"world")).await.unwrap();
        m.complete().await.unwrap();
        assert_eq!(
            s.get("big/file").await.unwrap(),
            Bytes::from_static(b"hello world")
        );

        let mut m = s.multipart_create("big/aborted").await.unwrap();
        m.put_part(Bytes::from_static(b"x")).await.unwrap();
        m.abort().await.unwrap();
        assert!(s.get("big/aborted").await.is_err());
    }

    #[test]
    fn s3_builder_accepts_config() {
        let s = ObjectStoreStorage::s3("http://127.0.0.1:3900", "b", "garage", "k", "s").unwrap();
        assert!(format!("{s:?}").contains("can_presign: true"));
    }
}
