//! Which `ObjectStorage` serves a media row (BYOS-INS-001).
//!
//! Cinema, Library and the media pipeline used to hold one `Arc<dyn ObjectStorage>`. They now ask a
//! [`MountStorages`] for the storage of the mount a row names (`mount_id`, default 1). Mount 1 is the instance
//! bucket: it always answers with the handle built from `storage.*`, never through the database, so the
//! instance path behaves exactly as before and keeps working while the database is unreachable. Any other
//! mount is resolved through the [`MountResolver`], its client built once per `updated_at` and reused.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;

use crate::error::{Result, StorageError};
use crate::mounts::{INSTANCE_MOUNT_ID, Mount, MountResolver, MountStatus, Sealer};
use crate::storage::{MultipartSession, ObjectInfo, ObjectStorage, ObjectStream, PresignMethod};

type Cache = Arc<Mutex<HashMap<i64, (i64, Arc<dyn ObjectStorage>)>>>;

/// Hands out the storage of a mount.
#[derive(Clone)]
pub struct MountStorages {
    instance: Arc<dyn ObjectStorage>,
    others: Option<Others>,
}

#[derive(Clone)]
struct Others {
    resolver: Arc<dyn MountResolver>,
    sealer: Arc<dyn Sealer>,
    cache: Cache,
}

fn poisoned() -> StorageError {
    StorageError::Backend("mount cache poisoned".into())
}

impl MountStorages {
    /// Only the instance bucket (mount 1). Any other mount id is `NotFound`.
    pub fn instance_only(instance: Arc<dyn ObjectStorage>) -> Self {
        Self {
            instance,
            others: None,
        }
    }

    /// The instance bucket plus every other mount the resolver knows.
    pub fn new(
        instance: Arc<dyn ObjectStorage>,
        resolver: Arc<dyn MountResolver>,
        sealer: Arc<dyn Sealer>,
    ) -> Self {
        Self {
            instance,
            others: Some(Others {
                resolver,
                sealer,
                cache: Arc::default(),
            }),
        }
    }

    /// The instance bucket's handle (mount 1), for code that has no row to ask about.
    pub fn instance(&self) -> Arc<dyn ObjectStorage> {
        self.instance.clone()
    }

    /// The storage for `mount_id`. Keys passed to it are mount-relative: a mount's prefix is applied inside.
    pub async fn for_mount(&self, mount_id: i64) -> Result<Arc<dyn ObjectStorage>> {
        if mount_id == INSTANCE_MOUNT_ID {
            return Ok(self.instance.clone());
        }
        let not_found = || StorageError::NotFound(format!("storage mount {mount_id}"));
        let others = self.others.as_ref().ok_or_else(not_found)?;
        let mount = others
            .resolver
            .by_id(mount_id)
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?
            .ok_or_else(not_found)?;
        if mount.status == MountStatus::Disabled {
            return Err(not_found());
        }
        {
            let cache = others.cache.lock().map_err(|_| poisoned())?;
            if let Some((stamp, s)) = cache.get(&mount_id)
                && *stamp == mount.updated_at
            {
                return Ok(s.clone());
            }
        }
        let client = mount
            .storage(others.sealer.as_ref())
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        let built: Arc<dyn ObjectStorage> =
            Arc::new(PrefixedStorage::new(Arc::new(client), &mount));
        others
            .cache
            .lock()
            .map_err(|_| poisoned())?
            .insert(mount_id, (mount.updated_at, built.clone()));
        Ok(built)
    }
}

/// A mount's client with the mount's key prefix applied to every key and removed from every key it returns.
pub struct PrefixedStorage {
    inner: Arc<dyn ObjectStorage>,
    /// `""` or `"a/b/"`.
    prefix: String,
}

impl PrefixedStorage {
    /// Wrap `inner` (which works on bucket keys) with `mount`'s prefix.
    pub fn new(inner: Arc<dyn ObjectStorage>, mount: &Mount) -> Self {
        let p = mount.prefix.trim_matches('/');
        Self {
            inner,
            prefix: if p.is_empty() {
                String::new()
            } else {
                format!("{p}/")
            },
        }
    }

    fn k(&self, key: &str) -> String {
        format!("{}{}", self.prefix, key.trim_start_matches('/'))
    }

    fn strip(&self, mut info: ObjectInfo) -> ObjectInfo {
        if let Some(rest) = info.key.strip_prefix(&self.prefix) {
            info.key = rest.to_owned();
        }
        info
    }
}

#[async_trait]
impl ObjectStorage for PrefixedStorage {
    async fn put(&self, key: &str, data: Bytes) -> Result<()> {
        self.inner.put(&self.k(key), data).await
    }
    async fn get(&self, key: &str) -> Result<Bytes> {
        self.inner.get(&self.k(key)).await
    }
    async fn get_range(&self, key: &str, start: u64, end: u64) -> Result<Bytes> {
        self.inner.get_range(&self.k(key), start, end).await
    }
    async fn get_stream(
        &self,
        key: &str,
        range: Option<std::ops::Range<u64>>,
    ) -> Result<ObjectStream> {
        let mut s = self.inner.get_stream(&self.k(key), range).await?;
        s.info = self.strip(s.info);
        Ok(s)
    }
    async fn head(&self, key: &str) -> Result<ObjectInfo> {
        Ok(self.strip(self.inner.head(&self.k(key)).await?))
    }
    async fn delete(&self, key: &str) -> Result<()> {
        self.inner.delete(&self.k(key)).await
    }
    async fn list(&self, prefix: &str) -> Result<Vec<ObjectInfo>> {
        Ok(self
            .inner
            .list(&self.k(prefix))
            .await?
            .into_iter()
            .map(|o| self.strip(o))
            .collect())
    }
    async fn walk(
        &self,
        max_objects: u64,
        budget: Duration,
        visit: &mut (dyn for<'a> FnMut(&'a str, u64) + Send),
    ) -> Result<bool> {
        // A census of a shelf counts only the shelf, not the rest of the bucket.
        let started = std::time::Instant::now();
        for (seen, o) in self.list("").await?.into_iter().enumerate() {
            if seen as u64 >= max_objects || started.elapsed() > budget {
                return Ok(false);
            }
            visit(&o.key, o.size);
        }
        Ok(true)
    }
    async fn presign(&self, method: PresignMethod, key: &str, ttl: Duration) -> Result<String> {
        self.inner.presign(method, &self.k(key), ttl).await
    }
    async fn multipart_create(&self, key: &str) -> Result<Box<dyn MultipartSession>> {
        self.inner.multipart_create(&self.k(key)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ObjectStoreStorage;
    use crate::mounts::{
        MemoryMountRepo, MountError, MountInput, MountOwner, MountRepo, NewMount, Provider, Secret,
    };

    /// Not secure: tags the blob with the AAD.
    struct TagSealer;
    impl Sealer for TagSealer {
        fn seal(&self, aad: &[u8], plain: &[u8]) -> std::result::Result<Vec<u8>, MountError> {
            let mut v = aad.to_vec();
            v.push(b'|');
            v.extend_from_slice(plain);
            Ok(v)
        }
        fn open(&self, aad: &[u8], blob: &[u8]) -> std::result::Result<Vec<u8>, MountError> {
            let mut head = aad.to_vec();
            head.push(b'|');
            blob.strip_prefix(head.as_slice())
                .map(<[u8]>::to_vec)
                .ok_or(MountError::Unseal)
        }
    }

    fn input(owner: MountOwner, prefix: &str) -> MountInput {
        MountInput {
            owner,
            label: "shelf".into(),
            provider: Provider::Garage,
            endpoint: "http://127.0.0.1:1".into(),
            region: "garage".into(),
            bucket: "b".into(),
            prefix: prefix.into(),
            path_style: true,
            access_key_id: "AKIAEXAMPLE".into(),
        }
    }

    async fn make(repo: &MemoryMountRepo, id: i64, owner: MountOwner, prefix: &str) -> Mount {
        let new =
            NewMount::seal(&TagSealer, id, input(owner, prefix), &Secret::new("s3cret")).unwrap();
        repo.create(new, None).await.unwrap()
    }

    #[tokio::test]
    async fn mount_one_is_the_instance_handle_without_a_resolver() {
        let mem = Arc::new(ObjectStoreStorage::in_memory());
        let m = MountStorages::instance_only(mem.clone());
        let s = m.for_mount(INSTANCE_MOUNT_ID).await.unwrap();
        s.put("video/1/a", Bytes::from_static(b"x")).await.unwrap();
        // The same bucket, same key: nothing was prefixed or moved.
        assert_eq!(mem.get("video/1/a").await.unwrap().as_ref(), b"x");
        assert!(matches!(
            m.for_mount(2).await,
            Err(StorageError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn mount_one_never_asks_the_resolver() {
        let repo = Arc::new(MemoryMountRepo::new());
        // A row for mount 1 that would build a client against a dead endpoint must not be used.
        make(&repo, 1, MountOwner::Platform, "other").await;
        let mem = Arc::new(ObjectStoreStorage::in_memory());
        let m = MountStorages::new(mem.clone(), repo, Arc::new(TagSealer));
        let s = m.for_mount(1).await.unwrap();
        s.put("k", Bytes::from_static(b"1")).await.unwrap();
        assert!(mem.head("k").await.is_ok(), "no prefix, no network");
    }

    #[tokio::test]
    async fn another_mount_is_resolved_and_cached_per_version() {
        let repo = Arc::new(MemoryMountRepo::new());
        make(&repo, 77, MountOwner::User(9), "shelf").await;
        let m = MountStorages::new(
            Arc::new(ObjectStoreStorage::in_memory()),
            repo.clone(),
            Arc::new(TagSealer),
        );
        let a = m.for_mount(77).await.unwrap();
        let b = m.for_mount(77).await.unwrap();
        assert!(Arc::ptr_eq(&a, &b), "one client per mount version");
        assert!(matches!(
            m.for_mount(78).await,
            Err(StorageError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn the_prefix_is_applied_and_stripped() {
        let mem = Arc::new(ObjectStoreStorage::in_memory());
        let repo = MemoryMountRepo::new();
        let mount = make(&repo, 5, MountOwner::Platform, "/lib/").await;
        let p = PrefixedStorage::new(mem.clone(), &mount);
        p.put("a/b.bin", Bytes::from_static(b"1")).await.unwrap();
        assert!(mem.head("lib/a/b.bin").await.is_ok());
        assert_eq!(p.head("a/b.bin").await.unwrap().key, "a/b.bin");
        let listed = p.list("a/").await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].key, "a/b.bin");
        p.delete("a/b.bin").await.unwrap();
        assert!(mem.head("lib/a/b.bin").await.is_err());
    }
}
