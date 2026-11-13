//! The Activity package limits' collaborators (D-655, SEC-AUD-173): the operator settings
//! `activities.package.*` and the trust check for the platform tier, over Postgres and the staff model.

use async_trait::async_trait;
use serde_json::Value;
use sr_activities::package::{
    DEFAULT_MAX_FILES, DEFAULT_PLATFORM_MB, DEFAULT_REVIEWED_MB, DEFAULT_STANDARD_MB,
    MAX_FILES_SETTING, PLATFORM_MB_SETTING, PackageSettings, PackageSettingsSnapshot, PackageTrust,
    REVIEWED_MB_SETTING, STANDARD_MB_SETTING,
};
use sr_admin::AdminService;
use sr_core::PlatformPermissions;
use sr_db::{PgPool, admin_repo::PgAdminRepo, auth_repo::IdSource};

/// Reads the four settings from `platform_settings`; a missing, unreadable or out-of-range value gives the default.
pub struct PgPackageSettings {
    pool: PgPool,
}

impl PgPackageSettings {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn value(&self, key: &str) -> Option<Value> {
        match sqlx::query_scalar::<_, Value>("SELECT value FROM platform_settings WHERE key = $1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, key, "reading a package setting failed; using the default");
                None
            }
        }
    }
}

/// Turn the stored values into a snapshot (the same ranges as the settings catalog).
pub fn snapshot_from(
    standard: Option<&Value>,
    reviewed: Option<&Value>,
    platform: Option<&Value>,
    files: Option<&Value>,
) -> PackageSettingsSnapshot {
    let mb = |v: Option<&Value>, default: u64| {
        v.and_then(Value::as_u64)
            .filter(|n| (32..=1_048_576).contains(n))
            .unwrap_or(default)
    };
    PackageSettingsSnapshot {
        standard_mb: mb(standard, DEFAULT_STANDARD_MB),
        reviewed_mb: mb(reviewed, DEFAULT_REVIEWED_MB),
        platform_mb: mb(platform, DEFAULT_PLATFORM_MB),
        max_files: files
            .and_then(Value::as_u64)
            .filter(|n| (2_000..=200_000).contains(n))
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(DEFAULT_MAX_FILES),
    }
}

#[async_trait]
impl PackageSettings for PgPackageSettings {
    async fn snapshot(&self) -> PackageSettingsSnapshot {
        let a = self.value(STANDARD_MB_SETTING).await;
        let b = self.value(REVIEWED_MB_SETTING).await;
        let c = self.value(PLATFORM_MB_SETTING).await;
        let d = self.value(MAX_FILES_SETTING).await;
        snapshot_from(a.as_ref(), b.as_ref(), c.as_ref(), d.as_ref())
    }
}

/// The platform tier: the owner of the application is the Owner, or has an active appointment that holds
/// `REVIEW_APPS` (resolved by the staff model, so a revoked appointment stops counting at once).
pub struct PgPackageTrust {
    admin: AdminService,
}

impl PgPackageTrust {
    pub fn new(pool: PgPool, ids: IdSource) -> Self {
        Self {
            admin: AdminService::new(std::sync::Arc::new(PgAdminRepo::new(pool, ids))),
        }
    }
}

#[async_trait]
impl PackageTrust for PgPackageTrust {
    async fn platform_owned(&self, owner_account: i64) -> bool {
        match self.admin.resolve(owner_account, None).await {
            Ok(c) => c.is_owner || c.permissions.has(PlatformPermissions::REVIEW_APPS),
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unreadable_or_out_of_range_values_give_the_defaults() {
        let d = snapshot_from(None, None, None, None);
        assert_eq!(d, PackageSettingsSnapshot::default());
        let s = snapshot_from(
            Some(&json!(1024)),
            Some(&json!("big")),
            Some(&json!(5)),
            Some(&json!(50_000)),
        );
        assert_eq!(
            (s.standard_mb, s.reviewed_mb, s.platform_mb, s.max_files),
            (1024, DEFAULT_REVIEWED_MB, DEFAULT_PLATFORM_MB, 50_000)
        );
    }
}
