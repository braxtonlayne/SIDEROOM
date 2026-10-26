//! Storage-mount commands (BYOS-MNT-001, 003): `rotate-secret-key` for mounts, `storage ensure-cors --mount|--all`
//! and `storage set-instance-mount`. The logic takes the repository and the sealer, so tests run it over the
//! in-memory repository.

use anyhow::{Context, Result, bail};
use sr_core::config::StorageConfig;
use sr_storage::cors_check::{self, CorsState};
use sr_storage::mounts::{
    INSTANCE_MOUNT_ID, Mount, MountPatch, MountRepo, SealedSecret, Sealer, Secret,
};
use sr_storage::s3admin::S3Admin;

/// Re-seal every mount's secret under the sealer's current key (it may also hold the previous keys).
/// Returns how many were re-sealed.
pub async fn rotate_mounts(repo: &dyn MountRepo, sealer: &dyn Sealer) -> Result<usize> {
    let mut n = 0;
    for m in repo.list_all().await.context("listing storage mounts")? {
        let sealed = m.secret_key_enc.reseal(sealer, m.id).with_context(|| {
            format!(
                "re-sealing mount {} (is the old key still listed in server.secret_key_previous?)",
                m.id
            )
        })?;
        if repo
            .set_sealed(m.id, sealed)
            .await
            .context("saving a re-sealed mount")?
        {
            n += 1;
        }
    }
    Ok(n)
}

/// Point mount 1 (the instance bucket) at the bucket, endpoint, region and keys now in `storage.*`.
/// The new secret is sealed for mount 1; CORS has to be proven again for the new bucket.
pub async fn set_instance_mount(
    repo: &dyn MountRepo,
    sealer: &dyn Sealer,
    storage: &StorageConfig,
) -> Result<Mount> {
    let Some(current) = repo
        .by_id(INSTANCE_MOUNT_ID)
        .await
        .context("reading mount 1")?
    else {
        bail!(
            "there is no instance mount yet; start the server once and it creates mount 1 from storage.*"
        );
    };
    let sealed = SealedSecret::seal(
        sealer,
        INSTANCE_MOUNT_ID,
        &Secret::new(storage.secret_key.expose()),
    )
    .context("sealing the secret key")?;
    let access = storage.access_key.expose().to_owned();
    let hint = Secret::key_hint(&access);
    let patch = MountPatch {
        endpoint: Some(storage.endpoint.clone()),
        region: Some(storage.region.clone()),
        bucket: Some(storage.bucket.clone()),
        credentials: Some((access, sealed, hint)),
        ..Default::default()
    };
    let updated = repo
        .update(current.id, patch)
        .await
        .context("updating mount 1")?
        .context("mount 1 disappeared")?;
    repo.set_cors_ok(INSTANCE_MOUNT_ID, None)
        .await
        .context("clearing the CORS proof of mount 1")?;
    Ok(repo
        .by_id(INSTANCE_MOUNT_ID)
        .await
        .context("reading mount 1 again")?
        .unwrap_or(updated))
}

/// Which mounts `storage ensure-cors` touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    /// One mount.
    One(i64),
    /// Every live mount that is not disabled.
    All,
}

/// One mount's outcome.
#[derive(Debug)]
pub struct CorsOutcome {
    /// Mount id.
    pub id: i64,
    /// Mount label.
    pub label: String,
    /// The state after applying, or why it could not be applied.
    pub result: std::result::Result<CorsState, String>,
}

/// Apply and re-check the CORS rule for `origin` on the chosen mounts and stamp or clear `cors_ok_at`.
pub async fn ensure_mount_cors(
    repo: &dyn MountRepo,
    sealer: &dyn Sealer,
    which: Which,
    origin: &str,
) -> Result<Vec<CorsOutcome>> {
    let mounts: Vec<Mount> = match which {
        Which::One(id) => vec![
            repo.by_id(id)
                .await
                .context("reading the mount")?
                .with_context(|| format!("no mount {id}"))?,
        ],
        Which::All => repo.list_enabled().await.context("listing mounts")?,
    };
    let mut out = Vec::new();
    for m in mounts {
        let result = apply_one(repo, sealer, &m, origin).await;
        out.push(CorsOutcome {
            id: m.id,
            label: m.label,
            result,
        });
    }
    Ok(out)
}

async fn apply_one(
    repo: &dyn MountRepo,
    sealer: &dyn Sealer,
    m: &Mount,
    origin: &str,
) -> std::result::Result<CorsState, String> {
    let secret = m
        .secret_key_enc
        .open(sealer, m.id)
        .map_err(|_| "its secret cannot be opened (wrong server.secret_key?)".to_owned())?;
    let admin = S3Admin::new(
        &m.endpoint,
        &m.bucket,
        &m.provider.signing_region(&m.region),
        &m.access_key_id,
        secret.expose(),
    )
    .map_err(|e| e.to_string())?;
    let report = cors_check::apply(&admin, m.provider, origin)
        .await
        .map_err(|e| e.to_string())?;
    let stamp = (report.state == CorsState::Ok).then(now_ms);
    repo.set_cors_ok(m.id, stamp)
        .await
        .map_err(|e| e.to_string())?;
    if report.state == CorsState::Manual
        && let Some(steps) = &report.manual_steps
    {
        eprintln!("mount {} ({}): {steps}", m.id, m.label);
    }
    Ok(report.state)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_db::sealed::SecretBox;
    use sr_storage::mounts::{
        MemoryMountRepo, MountInput, MountOwner, MountResolver, NewMount, Provider,
    };

    fn mount(sb: &SecretBox, id: i64, owner: MountOwner, bucket: &str, secret: &str) -> NewMount {
        NewMount::seal(
            sb,
            id,
            MountInput {
                owner,
                label: format!("m{id}"),
                provider: Provider::Garage,
                endpoint: "http://127.0.0.1:1".into(),
                region: "garage".into(),
                bucket: bucket.into(),
                prefix: String::new(),
                path_style: true,
                access_key_id: "GKOLD0001".into(),
            },
            &Secret::new(secret),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn rotate_reseals_every_mount_and_the_old_key_stops_working() {
        let old = SecretBox::new([1; 32]);
        let repo = MemoryMountRepo::new();
        repo.create(mount(&old, 1, MountOwner::Platform, "inst", "sec-1"), None)
            .await
            .unwrap();
        repo.create(mount(&old, 50, MountOwner::User(7), "u", "sec-50"), None)
            .await
            .unwrap();
        let rotated = SecretBox::new([2; 32]).with_previous_keys(vec![[1; 32]]);
        assert_eq!(rotate_mounts(&repo, &rotated).await.unwrap(), 2);
        let only_new = SecretBox::new([2; 32]);
        for (id, want) in [(1, "sec-1"), (50, "sec-50")] {
            let m = repo.by_id(id).await.unwrap().unwrap();
            assert_eq!(m.secret_key_enc.open(&only_new, id).unwrap().expose(), want);
            assert!(m.secret_key_enc.open(&old, id).is_err());
        }
        // Without the previous key listed, rotation fails with a hint instead of losing secrets.
        let lonely = SecretBox::new([3; 32]);
        let err = rotate_mounts(&repo, &lonely).await.unwrap_err().to_string();
        assert!(err.contains("secret_key_previous"), "{err}");
    }

    #[tokio::test]
    async fn set_instance_mount_repoints_mount_one_and_seals_the_new_secret() {
        let sb = SecretBox::new([4; 32]);
        let repo = MemoryMountRepo::new();
        let cfg = StorageConfig {
            endpoint: "https://s3.new.example".into(),
            region: "eu-1".into(),
            bucket: "new-bucket".into(),
            access_key: sr_core::config::Secret::new("GKNEWKEY0042"),
            secret_key: sr_core::config::Secret::new("brand-new-secret"),
        };
        // No mount 1 yet: a clear error.
        assert!(set_instance_mount(&repo, &sb, &cfg).await.is_err());

        repo.create(
            mount(&sb, 1, MountOwner::Platform, "old-bucket", "old-secret"),
            None,
        )
        .await
        .unwrap();
        repo.set_cors_ok(1, Some(5)).await.unwrap();
        let m = set_instance_mount(&repo, &sb, &cfg).await.unwrap();
        assert_eq!(m.id, INSTANCE_MOUNT_ID);
        assert_eq!(
            (m.bucket.as_str(), m.endpoint.as_str(), m.region.as_str()),
            ("new-bucket", "https://s3.new.example", "eu-1")
        );
        assert_eq!(
            (m.access_key_id.as_str(), m.key_hint.as_str()),
            ("GKNEWKEY0042", "0042")
        );
        assert_eq!(
            m.secret_key_enc.open(&sb, 1).unwrap().expose(),
            "brand-new-secret"
        );
        assert!(
            m.cors_ok_at.is_none(),
            "the new bucket must be proven again"
        );
        assert_eq!(m.owner, MountOwner::Platform);
        // Idempotent.
        let again = set_instance_mount(&repo, &sb, &cfg).await.unwrap();
        assert_eq!(again.bucket, "new-bucket");
    }

    #[tokio::test]
    async fn ensure_cors_reports_each_mount_and_never_stamps_a_failure() {
        let sb = SecretBox::new([5; 32]);
        let repo = MemoryMountRepo::new();
        repo.create(mount(&sb, 60, MountOwner::User(1), "a", "s"), None)
            .await
            .unwrap();
        repo.create(mount(&sb, 61, MountOwner::User(2), "b", "s"), None)
            .await
            .unwrap();
        repo.update(
            61,
            MountPatch {
                status: Some(sr_storage::mounts::MountStatus::Disabled),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        // Port 1 refuses connections: the outcome is an error, not a panic, and nothing is stamped.
        let all = ensure_mount_cors(&repo, &sb, Which::All, "https://chat.example.org")
            .await
            .unwrap();
        assert_eq!(all.len(), 1, "a disabled mount is skipped by --all");
        assert!(all[0].result.is_err());
        assert!(repo.by_id(60).await.unwrap().unwrap().cors_ok_at.is_none());
        let one = ensure_mount_cors(&repo, &sb, Which::One(61), "https://chat.example.org")
            .await
            .unwrap();
        assert_eq!(one.len(), 1);
        assert!(
            ensure_mount_cors(&repo, &sb, Which::One(999), "https://chat.example.org")
                .await
                .is_err()
        );
    }
}
