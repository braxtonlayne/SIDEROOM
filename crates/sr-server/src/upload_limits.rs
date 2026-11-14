//! The chat upload limit from the donations runtime (D-238, H-34): `max(free, server level perk,
//! Patron perk)` in a server and the Patron perk alone in DMs and groups, read from
//! `Entitlements::limits(user, guild)`. REST only: the gateway never carries the number.

use async_trait::async_trait;
use sr_api::routes::messages::UploadEntitlements;
use sr_core::{GuildId, UserId};
use sr_payments::Entitlements;
use sr_storage::upload::SizePolicy;
use std::sync::Arc;
use tokio::sync::watch;

/// [`UploadEntitlements`] over the payments entitlement service.
pub struct PaymentsUploadLimits {
    entitlements: Arc<dyn Entitlements>,
    /// The operator's size policy (D-662): its standard limit is the free size, and follows edits live.
    policy: watch::Receiver<SizePolicy>,
}

impl PaymentsUploadLimits {
    pub fn new(entitlements: Arc<dyn Entitlements>, policy: watch::Receiver<SizePolicy>) -> Self {
        Self {
            entitlements,
            policy,
        }
    }

    /// The free size now: used when nobody's perk lifts the limit, and when the lookup fails.
    fn free(&self) -> u64 {
        self.policy.borrow().standard_limit
    }
}

#[async_trait]
impl UploadEntitlements for PaymentsUploadLimits {
    async fn max_upload_bytes(&self, user: UserId, guild: Option<GuildId>) -> u64 {
        let (Ok(user), guild) = (
            i64::try_from(user.get()),
            guild.and_then(|g| i64::try_from(g.get()).ok()),
        ) else {
            return self.free();
        };
        match self.entitlements.limits(user, guild).await {
            // A perk above the free tier keeps its own number; with none, the free size is the operator's
            // standard limit (the payments free tier is only the default of that setting).
            Ok(l) if l.upload_bytes > l.standard_upload_bytes => l.upload_bytes.max(self.free()),
            Ok(_) => self.free(),
            Err(e) => {
                tracing::warn!(error = %e, "upload entitlement lookup failed; using the free size");
                self.free()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_payments::PayResult;
    use sr_payments::config::{DonationConfig, MIB, effective_limits};
    use sr_payments::entitlements::{GuildEntitlement, PatronStatus, UserEntitlement};

    /// User 1 is a Patron; guild 10 is at level 2; guild 11 is free.
    struct Fake;

    #[async_trait]
    impl Entitlements for Fake {
        async fn user(&self, user_id: i64) -> PayResult<UserEntitlement> {
            Ok(UserEntitlement {
                user_id,
                patron: PatronStatus {
                    active: user_id == 1,
                    ..PatronStatus::none()
                },
            })
        }
        async fn guild(&self, guild_id: i64) -> PayResult<GuildEntitlement> {
            Ok(GuildEntitlement {
                guild_id,
                level: if guild_id == 10 { 2 } else { 0 },
                pool_cents: 0,
                grace_until_ms: None,
            })
        }
        async fn limits(
            &self,
            user_id: i64,
            guild_id: Option<i64>,
        ) -> PayResult<sr_payments::config::Limits> {
            Ok(effective_limits(
                &DonationConfig::default(),
                user_id == 1,
                if guild_id == Some(10) { 2 } else { 0 },
            ))
        }
    }

    #[tokio::test]
    async fn the_server_level_and_the_patron_perk_combine_by_maximum() {
        let (_tx, rx) = watch::channel(SizePolicy::default());
        let lim = PaymentsUploadLimits::new(Arc::new(Fake), rx);
        let (free, patron) = (UserId::new(2), UserId::new(1));
        let (level2, plain) = (Some(GuildId::new(10)), Some(GuildId::new(11)));
        let free_size = lim.max_upload_bytes(free, plain).await;
        let level_size = lim.max_upload_bytes(free, level2).await;
        assert!(level_size > free_size, "{level_size} vs {free_size}");
        // DMs and groups: the member's own perk only.
        assert_eq!(lim.max_upload_bytes(free, None).await, free_size);
        let patron_dm = lim.max_upload_bytes(patron, None).await;
        assert!(patron_dm > level_size);
        assert_eq!(lim.max_upload_bytes(patron, level2).await, patron_dm);
    }

    /// D-662: the free size is the operator's standard limit and follows an edit without a restart; a perk above
    /// the free tier keeps its own number.
    #[tokio::test]
    async fn the_standard_limit_follows_the_setting_without_a_restart() {
        let (tx, rx) = watch::channel(SizePolicy::default());
        let lim = PaymentsUploadLimits::new(Arc::new(Fake), rx);
        let (free, patron) = (UserId::new(2), UserId::new(1));
        let plain = Some(GuildId::new(11));
        assert_eq!(lim.max_upload_bytes(free, plain).await, 30 * MIB);
        // Lowered to 10 MiB: the free size drops; the Patron perk does not.
        tx.send_replace(SizePolicy::from_settings(Some(10 * 1024 * 1024), None));
        assert_eq!(lim.max_upload_bytes(free, plain).await, 10 * MIB);
        assert_eq!(lim.max_upload_bytes(free, None).await, 10 * MIB);
        assert_eq!(lim.max_upload_bytes(patron, None).await, 500 * MIB);
        // Raised to 50 MiB: the free size follows.
        tx.send_replace(SizePolicy::from_settings(Some(50 * 1024 * 1024), None));
        assert_eq!(lim.max_upload_bytes(free, plain).await, 50 * MIB);
    }
}
