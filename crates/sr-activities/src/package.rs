//! Package limits (D-655, SEC-AUD-173): how large an Activity package may be, by how much trust it has earned.
//!
//! Three operator settings, in MiB, and one file count (`activities.package.*`, group `activities` of the Admin
//! settings page):
//!
//! | tier | who | setting | default |
//! |---|---|---|---|
//! | [`PackageTier::Standard`] | every developer | `max_mb` | 512 |
//! | [`PackageTier::Reviewed`] | an application that has a staff-approved bundle | `reviewed_max_mb` | 4096 |
//! | [`PackageTier::Platform`] | an application owned by the Owner or by staff who review applications | `platform_max_mb` | 32 768 |
//!
//! An application gets the **largest** setting among the tiers it qualifies for (so raising the standard setting
//! above the others gives everybody the bigger number), and its `tier` is the highest it qualifies for. The limit
//! applies to the zip and to the unpacked total. The settings and the trust check are ports, wired by `sr-server`
//! (`package_wiring.rs`); without them (tests) the static defaults apply.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::{ActivityError, ActivityResult};
use crate::service::ActivityService;

const MIB: u64 = 1024 * 1024;

/// Default of `activities.package.max_mb`.
pub const DEFAULT_STANDARD_MB: u64 = 512;
/// Default of `activities.package.reviewed_max_mb`.
pub const DEFAULT_REVIEWED_MB: u64 = 4_096;
/// Default of `activities.package.platform_max_mb`.
pub const DEFAULT_PLATFORM_MB: u64 = 32_768;
/// Default of `activities.package.max_files`.
pub const DEFAULT_MAX_FILES: usize = 20_000;
/// The unpacked total of a package may be at most this many times the zip's own length (the zip-bomb rule, D-655).
pub const MAX_RATIO: u64 = 100;

/// Setting keys (declared in `sr-admin`'s catalog).
pub const STANDARD_MB_SETTING: &str = "activities.package.max_mb";
pub const REVIEWED_MB_SETTING: &str = "activities.package.reviewed_max_mb";
pub const PLATFORM_MB_SETTING: &str = "activities.package.platform_max_mb";
pub const MAX_FILES_SETTING: &str = "activities.package.max_files";

/// Progress of a check, called at most once per second with the bytes inflated so far and the bytes the zip declares
/// in all.
pub trait IngestProgress: Send + Sync {
    fn bytes(&self, done: u64, total: u64);
}

/// How much trust an application has earned, which sets its package limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum PackageTier {
    Standard,
    Reviewed,
    Platform,
}

impl PackageTier {
    /// The word used in refusal texts.
    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Reviewed => "reviewed",
            Self::Platform => "platform",
        }
    }
}

/// What a package of one application may be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageLimits {
    /// The most bytes the zip may have, and the most the unpacked files may total.
    pub max_bytes: u64,
    /// The most entries (files and directories) in the zip.
    pub max_files: usize,
    /// The highest tier the application qualifies for.
    pub tier: PackageTier,
}

impl PackageLimits {
    /// The limit as the whole MiB the settings are written in (rounded up, so the text never understates it).
    pub fn max_mib(&self) -> u64 {
        self.max_bytes.div_ceil(MIB)
    }
}

impl Default for PackageLimits {
    /// The standard tier at its default setting.
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_STANDARD_MB * MIB,
            max_files: DEFAULT_MAX_FILES,
            tier: PackageTier::Standard,
        }
    }
}

/// The operator's settings, as read now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageSettingsSnapshot {
    pub standard_mb: u64,
    pub reviewed_mb: u64,
    pub platform_mb: u64,
    pub max_files: usize,
}

impl Default for PackageSettingsSnapshot {
    fn default() -> Self {
        Self {
            standard_mb: DEFAULT_STANDARD_MB,
            reviewed_mb: DEFAULT_REVIEWED_MB,
            platform_mb: DEFAULT_PLATFORM_MB,
            max_files: DEFAULT_MAX_FILES,
        }
    }
}

/// Source of the operator settings, read afresh for every upload and every job. Unreadable values give the
/// defaults, never zero.
#[async_trait]
pub trait PackageSettings: Send + Sync {
    async fn snapshot(&self) -> PackageSettingsSnapshot;
}

/// Fixed settings (tests, tools).
pub struct StaticPackageSettings(pub PackageSettingsSnapshot);

#[async_trait]
impl PackageSettings for StaticPackageSettings {
    async fn snapshot(&self) -> PackageSettingsSnapshot {
        self.0
    }
}

/// Whether an application's owner is the platform itself: the Owner, or staff with an active appointment that holds
/// `REVIEW_APPS`.
#[async_trait]
pub trait PackageTrust: Send + Sync {
    async fn platform_owned(&self, owner_account: i64) -> bool;
}

/// Nobody is platform-owned (tests, installs without staff).
pub struct NoPackageTrust;

#[async_trait]
impl PackageTrust for NoPackageTrust {
    async fn platform_owned(&self, _owner_account: i64) -> bool {
        false
    }
}

/// The limits of an application that qualifies for the tiers `reviewed` and `platform` (Standard always).
pub fn limits_for(s: &PackageSettingsSnapshot, reviewed: bool, platform: bool) -> PackageLimits {
    let mut mb = s.standard_mb;
    let mut tier = PackageTier::Standard;
    if reviewed {
        mb = mb.max(s.reviewed_mb);
        tier = PackageTier::Reviewed;
    }
    if platform {
        mb = mb.max(s.platform_mb);
        tier = PackageTier::Platform;
    }
    PackageLimits {
        max_bytes: mb.saturating_mul(MIB),
        max_files: s.max_files,
        tier,
    }
}

impl ActivityService {
    /// The limits of `app` now (settings + tier). Never fails closed to zero: a settings read error gives the
    /// defaults, and an application whose standing cannot be read is treated as a standard one.
    pub async fn package_limits(&self, app: i64) -> ActivityResult<PackageLimits> {
        let snapshot = match self.package_settings() {
            Some(p) => p.snapshot().await,
            None => PackageSettingsSnapshot::default(),
        };
        let row = sqlx::query_as::<_, (i64, bool)>(
            "SELECT a.owner_id, COALESCE(c.reviewed_bundle_id IS NOT NULL, false) \
             FROM applications a LEFT JOIN application_activities c ON c.application_id = a.id \
             WHERE a.id = $1",
        )
        .bind(app)
        .fetch_optional(self.pool())
        .await;
        let (owner, reviewed) = match row {
            Ok(Some(r)) => r,
            Ok(None) => return Err(ActivityError::NotFound("application")),
            Err(e) => {
                tracing::warn!(error = %e, app, "reading an application's package standing failed; using the standard limit");
                return Ok(limits_for(&snapshot, false, false));
            }
        };
        let platform = match self.package_trust() {
            Some(t) => t.platform_owned(owner).await,
            None => false,
        };
        Ok(limits_for(&snapshot, reviewed, platform))
    }
}

/// The text of a refusal for a package over the limit, naming the limit and who can raise it.
pub fn over_limit_text(limits: &PackageLimits) -> String {
    format!(
        "The limit for this application is {} MiB ({} tier). The operator of this server can raise the limit.",
        limits.max_mib(),
        limits.tier.label()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limit_is_the_largest_setting_among_the_tiers_the_application_qualifies_for() {
        let d = PackageSettingsSnapshot::default();
        assert_eq!(limits_for(&d, false, false).max_bytes, 512 * MIB);
        assert_eq!(limits_for(&d, true, false).max_bytes, 4_096 * MIB);
        assert_eq!(limits_for(&d, false, true).max_bytes, 32_768 * MIB);
        assert_eq!(limits_for(&d, true, true).tier, PackageTier::Platform);
        assert_eq!(limits_for(&d, true, false).tier, PackageTier::Reviewed);
        // The operator raises the standard setting above the others: everybody gets it, the tier stays the highest
        // the application qualifies for.
        let raised = PackageSettingsSnapshot {
            standard_mb: 100_000,
            ..d
        };
        let l = limits_for(&raised, true, false);
        assert_eq!(
            (l.max_bytes, l.tier),
            (100_000 * MIB, PackageTier::Reviewed)
        );
        assert_eq!(
            limits_for(&raised, false, false).tier,
            PackageTier::Standard
        );
        assert_eq!(limits_for(&raised, false, false).max_bytes, 100_000 * MIB);
    }

    #[test]
    fn the_refusal_names_the_limit_and_who_can_raise_it() {
        let t = over_limit_text(&PackageLimits::default());
        assert!(
            t.contains("512 MiB")
                && t.ends_with("The operator of this server can raise the limit.")
        );
    }
}
