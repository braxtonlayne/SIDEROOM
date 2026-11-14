//! Size-limit policy (decisions D-042, D-043; architecture 4.5).

use std::time::Duration;

use thiserror::Error;

const MIB: u64 = 1024 * 1024;

/// Upload size policy. The per-user limit comes from entitlements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizePolicy {
    /// Default limit for everyone (30 MiB).
    pub standard_limit: u64,
    /// System maximum for donors (500 MiB).
    pub system_max: u64,
    /// Above this size, clients must use multipart (64 MiB).
    pub multipart_threshold: u64,
    /// Lifetime of "large attachments" above `standard_limit`.
    pub large_ttl: Duration,
}

/// Allowed range of the operator's standard attachment limit (`uploads.standard_limit_bytes`): 1 MiB up to the
/// system maximum (D-662).
pub const STANDARD_LIMIT_RANGE: std::ops::RangeInclusive<u64> = MIB..=500 * MIB;
/// Allowed range of the operator's large-attachment lifetime in days (`retention.large_file_days`).
pub const LARGE_FILE_DAYS_RANGE: std::ops::RangeInclusive<u64> = 1..=365;

impl SizePolicy {
    /// The policy for the operator's two stored settings (D-662): `uploads.standard_limit_bytes` and
    /// `retention.large_file_days`. A missing, non-numeric or out-of-range value reads as the default, never as
    /// something the policy cannot work with.
    #[must_use]
    pub fn from_settings(standard_limit_bytes: Option<i64>, large_file_days: Option<i64>) -> Self {
        let d = Self::default();
        let standard = standard_limit_bytes
            .and_then(|n| u64::try_from(n).ok())
            .filter(|n| STANDARD_LIMIT_RANGE.contains(n))
            .unwrap_or(d.standard_limit);
        let days = large_file_days
            .and_then(|n| u64::try_from(n).ok())
            .filter(|n| LARGE_FILE_DAYS_RANGE.contains(n));
        Self {
            standard_limit: standard,
            large_ttl: days.map_or(d.large_ttl, |n| Duration::from_secs(n * 24 * 3600)),
            ..d
        }
    }
}

impl Default for SizePolicy {
    fn default() -> Self {
        Self {
            standard_limit: 30 * MIB,
            system_max: 500 * MIB,
            multipart_threshold: 64 * MIB,
            large_ttl: Duration::from_secs(7 * 24 * 3600),
        }
    }
}

/// Outcome of an accepted size check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeDecision {
    /// Client must use multipart upload.
    pub multipart: bool,
    /// Expiry for large attachments (`None` = lives with the message).
    pub expires_after: Option<Duration>,
}

/// Rejections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SizeError {
    /// Empty uploads are not allowed.
    #[error("file is empty")]
    Empty,
    /// Over the user's entitlement.
    #[error("file exceeds your limit of {limit} bytes")]
    ExceedsUserLimit {
        /// Effective limit.
        limit: u64,
    },
}

impl SizePolicy {
    /// Effective limit for a user whose entitlement grants `entitled_limit` bytes
    /// (clamped to `[standard_limit, system_max]`).
    pub fn effective_limit(&self, entitled_limit: u64) -> u64 {
        // `clamp` panics when min > max: a standard limit above the system maximum can never be asked for.
        let floor = self.standard_limit.min(self.system_max);
        entitled_limit.clamp(floor, self.system_max)
    }

    /// Check a declared upload `size` against the user's entitlement.
    pub fn check(&self, size: u64, entitled_limit: u64) -> Result<SizeDecision, SizeError> {
        if size == 0 {
            return Err(SizeError::Empty);
        }
        let limit = self.effective_limit(entitled_limit);
        if size > limit {
            return Err(SizeError::ExceedsUserLimit { limit });
        }
        Ok(SizeDecision {
            multipart: size > self.multipart_threshold,
            expires_after: (size > self.standard_limit).then_some(self.large_ttl),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_user() {
        let p = SizePolicy::default();
        let d = p.check(10 * MIB, p.standard_limit).unwrap();
        assert_eq!(
            d,
            SizeDecision {
                multipart: false,
                expires_after: None
            }
        );
        assert_eq!(
            p.check(31 * MIB, 0),
            Err(SizeError::ExceedsUserLimit { limit: 30 * MIB })
        );
    }

    #[test]
    fn donor_large_expires_and_multipart() {
        let p = SizePolicy::default();
        let d = p.check(40 * MIB, 500 * MIB).unwrap();
        assert!(!d.multipart);
        assert_eq!(d.expires_after, Some(Duration::from_secs(7 * 86400)));
        let d = p.check(65 * MIB, 500 * MIB).unwrap();
        assert!(d.multipart);
        assert!(p.check(501 * MIB, u64::MAX).is_err());
    }

    #[test]
    fn a_standard_limit_above_the_system_maximum_cannot_panic() {
        let p = SizePolicy {
            standard_limit: 600 * MIB,
            ..SizePolicy::default()
        };
        assert_eq!(p.effective_limit(0), 500 * MIB);
        assert_eq!(p.effective_limit(u64::MAX), 500 * MIB);
        assert!(p.check(10, 0).is_ok());
        // The settings constructor never builds one: a stored value above the range reads as the default.
        for stored in [Some(600 * 1024 * 1024), Some(0), Some(-5), Some(1), None] {
            let p = SizePolicy::from_settings(stored, None);
            assert_eq!(p.standard_limit, 30 * MIB, "{stored:?}");
        }
        assert_eq!(
            SizePolicy::from_settings(Some(500 * 1024 * 1024), None).standard_limit,
            500 * MIB
        );
        assert_eq!(
            SizePolicy::from_settings(Some(1024 * 1024), None).standard_limit,
            MIB
        );
    }

    #[test]
    fn the_large_file_days_set_the_lifetime_and_unreadable_ones_read_as_seven() {
        let day = |n: u64| Duration::from_secs(n * 86_400);
        assert_eq!(SizePolicy::from_settings(None, Some(3)).large_ttl, day(3));
        assert_eq!(
            SizePolicy::from_settings(None, Some(365)).large_ttl,
            day(365)
        );
        for bad in [Some(0), Some(366), Some(-1), None] {
            assert_eq!(
                SizePolicy::from_settings(None, bad).large_ttl,
                day(7),
                "{bad:?}"
            );
        }
        let p = SizePolicy::from_settings(Some(10 * 1024 * 1024), Some(2));
        let d = p.check(11 * MIB, 500 * MIB).unwrap();
        assert_eq!(
            d.expires_after,
            Some(day(2)),
            "11 MiB is large above a 10 MiB standard"
        );
        assert_eq!(p.check(10 * MIB, 0).unwrap().expires_after, None);
    }

    #[test]
    fn empty_rejected() {
        assert_eq!(SizePolicy::default().check(0, 1), Err(SizeError::Empty));
    }
}
