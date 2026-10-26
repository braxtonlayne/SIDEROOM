//! Mount 1: the instance bucket as a row of `storage_mounts` (BYOS-INS-001).
//!
//! `storage.*` stays the configuration of the instance bucket and the handle every service uses (see
//! `sr_storage::mount_storage::MountStorages`: mount 1 never goes through the database). The row exists so
//! the bucket shows up beside user and guild mounts, can be attached to a guild as a shelf, and so a changed
//! `storage.bucket` is noticed instead of silently pointing existing rows at another bucket.

use sr_core::config::StorageConfig;
use sr_storage::mounts::{
    INSTANCE_MOUNT_ID, MountInput, MountOwner, MountRepo, NewMount, Provider, Sealer, Secret,
};

/// What the start-up check did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceMount {
    /// Mount 1 did not exist and was created.
    Seeded,
    /// Mount 1 exists and names the configured bucket.
    Present,
}

/// Seed mount 1 from `storage.*` when it is missing; refuse when it names another bucket.
///
/// Idempotent: a second call finds the row and changes nothing (a rotated access key in the config is not
/// written back; the instance path reads the config directly).
pub async fn ensure_instance_mount(
    repo: &dyn MountRepo,
    sealer: &dyn Sealer,
    cfg: &StorageConfig,
) -> anyhow::Result<InstanceMount> {
    match repo.by_id(INSTANCE_MOUNT_ID).await? {
        Some(m) => {
            if m.bucket != cfg.bucket {
                anyhow::bail!(
                    "storage mount 1 (the instance bucket) is \"{}\" but storage.bucket is \"{}\"; every \
                     existing media row names mount 1, so changing the bucket would point them at another \
                     bucket. Move the objects and run `sr-cli storage set-instance-mount`, or restore \
                     storage.bucket",
                    m.bucket,
                    cfg.bucket
                );
            }
            Ok(InstanceMount::Present)
        }
        None => {
            let input = MountInput {
                owner: MountOwner::Platform,
                label: "Instance".into(),
                provider: Provider::Garage,
                endpoint: cfg.endpoint.clone(),
                region: cfg.region.clone(),
                bucket: cfg.bucket.clone(),
                prefix: String::new(),
                path_style: true,
                access_key_id: cfg.access_key.expose().to_owned(),
            };
            let new = NewMount::seal(
                sealer,
                INSTANCE_MOUNT_ID,
                input,
                &Secret::new(cfg.secret_key.expose()),
            )?;
            repo.create(new, None).await?;
            Ok(InstanceMount::Seeded)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_storage::mounts::{MemoryMountRepo, MountError, MountResolver};

    struct TagSealer;
    impl Sealer for TagSealer {
        fn seal(&self, aad: &[u8], plain: &[u8]) -> Result<Vec<u8>, MountError> {
            let mut v = aad.to_vec();
            v.extend_from_slice(plain);
            Ok(v)
        }
        fn open(&self, aad: &[u8], blob: &[u8]) -> Result<Vec<u8>, MountError> {
            blob.strip_prefix(aad)
                .map(<[u8]>::to_vec)
                .ok_or(MountError::Unseal)
        }
    }

    fn cfg(bucket: &str) -> StorageConfig {
        StorageConfig {
            bucket: bucket.into(),
            access_key: sr_core::config::Secret::new("AK123456"),
            secret_key: sr_core::config::Secret::new("sekret"),
            ..StorageConfig::default()
        }
    }

    #[tokio::test]
    async fn seeds_once_and_is_idempotent() {
        let repo = MemoryMountRepo::new();
        let c = cfg("sideroom");
        assert_eq!(
            ensure_instance_mount(&repo, &TagSealer, &c).await.unwrap(),
            InstanceMount::Seeded
        );
        let m = repo.by_id(1).await.unwrap().unwrap();
        assert_eq!(m.owner, MountOwner::Platform);
        assert_eq!(m.bucket, "sideroom");
        assert_eq!(m.key_hint, "3456");
        let again = ensure_instance_mount(&repo, &TagSealer, &c).await.unwrap();
        assert_eq!(again, InstanceMount::Present);
        assert_eq!(repo.for_owner(MountOwner::Platform).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn refuses_a_changed_bucket() {
        let repo = MemoryMountRepo::new();
        ensure_instance_mount(&repo, &TagSealer, &cfg("old"))
            .await
            .unwrap();
        let err = ensure_instance_mount(&repo, &TagSealer, &cfg("new"))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("set-instance-mount"), "{err}");
        assert!(err.contains("\"old\"") && err.contains("\"new\""));
    }
}
