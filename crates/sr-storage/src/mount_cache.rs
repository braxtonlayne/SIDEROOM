//! A 60-second cache in front of any [`MountResolver`] (BYOS-MNT-001). Reads of a mount by id are the hot path
//! (every presign); writes call [`CachedMountResolver::invalidate`] so a change shows at once on this node.
//! Other nodes see a change after at most the TTL. Misses are not cached, so a new mount is visible immediately.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use moka::sync::Cache;

use crate::mounts::{Mount, MountOwner, MountResolver, MountResult};

/// How long a mount stays cached.
pub const MOUNT_CACHE_TTL: Duration = Duration::from_secs(60);

/// [`MountResolver`] with a per-id cache.
pub struct CachedMountResolver {
    inner: Arc<dyn MountResolver>,
    cache: Cache<i64, Mount>,
}

impl CachedMountResolver {
    /// Cache `inner` for [`MOUNT_CACHE_TTL`].
    pub fn new(inner: Arc<dyn MountResolver>) -> Self {
        Self::with_ttl(inner, MOUNT_CACHE_TTL)
    }
    /// Cache with an explicit TTL (tests).
    pub fn with_ttl(inner: Arc<dyn MountResolver>, ttl: Duration) -> Self {
        Self {
            inner,
            cache: Cache::builder()
                .max_capacity(10_000)
                .time_to_live(ttl)
                .build(),
        }
    }
    /// Drop one mount from the cache (call after every write to it).
    pub fn invalidate(&self, id: i64) {
        self.cache.invalidate(&id);
    }
}

#[async_trait]
impl MountResolver for CachedMountResolver {
    async fn by_id(&self, id: i64) -> MountResult<Option<Mount>> {
        if let Some(m) = self.cache.get(&id) {
            return Ok(Some(m));
        }
        let found = self.inner.by_id(id).await?;
        if let Some(m) = &found {
            self.cache.insert(id, m.clone());
        }
        Ok(found)
    }
    async fn for_owner(&self, owner: MountOwner) -> MountResult<Vec<Mount>> {
        self.inner.for_owner(owner).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mounts::{
        MemoryMountRepo, MountError, MountInput, MountPatch, MountRepo, NewMount, Provider,
        SealedSecret,
    };

    fn new_mount(id: i64) -> NewMount {
        NewMount {
            id,
            input: MountInput {
                owner: MountOwner::User(1),
                label: format!("m{id}"),
                provider: Provider::Garage,
                endpoint: "http://127.0.0.1:3900".into(),
                region: "garage".into(),
                bucket: "b".into(),
                prefix: String::new(),
                path_style: true,
                access_key_id: "AKIA1234".into(),
            },
            secret_key_enc: SealedSecret::from_bytes(vec![1, 2, 3]),
            key_hint: "1234".into(),
        }
    }

    #[tokio::test]
    async fn caches_hits_until_invalidated_and_never_caches_misses() -> Result<(), MountError> {
        let repo = Arc::new(MemoryMountRepo::new());
        let cached = CachedMountResolver::new(repo.clone());
        assert!(cached.by_id(5).await?.is_none());
        repo.create(new_mount(5), None).await?;
        assert_eq!(cached.by_id(5).await?.map(|m| m.label), Some("m5".into()));
        let rename = MountPatch {
            label: Some("renamed".into()),
            ..Default::default()
        };
        repo.update(5, rename).await?;
        assert_eq!(cached.by_id(5).await?.map(|m| m.label), Some("m5".into()));
        cached.invalidate(5);
        assert_eq!(
            cached.by_id(5).await?.map(|m| m.label),
            Some("renamed".into())
        );
        Ok(())
    }

    #[tokio::test]
    async fn entries_expire() -> Result<(), MountError> {
        let repo = Arc::new(MemoryMountRepo::new());
        repo.create(new_mount(6), None).await?;
        let cached = CachedMountResolver::with_ttl(repo.clone(), Duration::from_millis(30));
        assert!(cached.by_id(6).await?.is_some());
        repo.soft_delete(6).await?;
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(cached.by_id(6).await?.is_none());
        Ok(())
    }
}
