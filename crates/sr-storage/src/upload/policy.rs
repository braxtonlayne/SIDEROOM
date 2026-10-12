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
        entitled_limit.clamp(self.standard_limit, self.system_max)
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
    fn empty_rejected() {
        assert_eq!(SizePolicy::default().check(0, 1), Err(SizeError::Empty));
    }
}
