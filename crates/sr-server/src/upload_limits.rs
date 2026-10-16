//! The chat upload limit from the donations runtime (D-238, H-34): `max(free, server level perk,
//! Patron perk)` in a server and the Patron perk alone in DMs and groups, read from
//! `Entitlements::limits(user, guild)`. REST only: the gateway never carries the number.

use async_trait::async_trait;
use sr_api::routes::messages::UploadEntitlements;
use sr_core::{GuildId, UserId};
use sr_payments::Entitlements;
use std::sync::Arc;

/// [`UploadEntitlements`] over the payments entitlement service.
pub struct PaymentsUploadLimits {
    entitlements: Arc<dyn Entitlements>,
    /// Used when the lookup fails, so uploads keep working at the free size.
    fallback: u64,
}

impl PaymentsUploadLimits {
    pub fn new(entitlements: Arc<dyn Entitlements>, fallback: u64) -> Self {
        Self {
            entitlements,
            fallback,
        }
    }
}

#[async_trait]
impl UploadEntitlements for PaymentsUploadLimits {
    async fn max_upload_bytes(&self, user: UserId, guild: Option<GuildId>) -> u64 {
        let (Ok(user), guild) = (
            i64::try_from(user.get()),
            guild.and_then(|g| i64::try_from(g.get()).ok()),
        ) else {
            return self.fallback;
        };
        match self.entitlements.limits(user, guild).await {
            Ok(l) => l.upload_bytes,
            Err(e) => {
                tracing::warn!(error = %e, "upload entitlement lookup failed; using the free size");
                self.fallback
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
        let lim = PaymentsUploadLimits::new(Arc::new(Fake), 30 * MIB);
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
}
