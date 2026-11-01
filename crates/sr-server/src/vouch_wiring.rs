//! MOD-VCH-001 (wave 44-B): the glue between profile vouches (`sr-vouch`) and wave 44-A's two seams in
//! `sr_admin::serious_ban`.
//!
//! * [`VouchStrikeWeight`] answers `strike_weight_multiplier(account)` from the live vouch count (1.0 Citizen,
//!   1.5 Pillar, 3.0 Elder), which the moderation engine applies to automated timeouts, holds and strike weights.
//! * [`VouchFallout`] subscribes to serious bans. A ban is on a Person; the vouches are on accounts, so every
//!   account of the Person loses the vouches it received and each voucher gets a private notice (category only,
//!   no handle) and no penalty.
//! * [`SterilizationState`] answers `SterilizationCheck::is_sterilized` from wave 44-A's `member_restrictions`
//!   (an active, unexpired `sterilization` row): a sterilized member cannot vouch (integrator, wave 44).

use async_trait::async_trait;
use sqlx::PgPool;
use sr_admin::serious_ban::{SeriousBan, SeriousBanObserver, StrikeWeight};
use sr_vouch::{AccountabilityMultiplier, SterilizationCheck, Vouches};

/// The real [`StrikeWeight`]: the vouch tier of the account.
pub struct VouchStrikeWeight(pub Vouches);

#[async_trait]
impl StrikeWeight for VouchStrikeWeight {
    async fn strike_weight_multiplier(&self, account: i64) -> f32 {
        self.0.strike_weight_multiplier(account).await
    }
}

/// The real [`SterilizationCheck`]: wave 44-A's sterilization state. A database fault answers "sterilized" so
/// a vouch is refused rather than given by someone who may not (fails closed; the voucher can try again).
pub struct SterilizationState(pub PgPool);

#[async_trait]
impl SterilizationCheck for SterilizationState {
    async fn is_sterilized(&self, account: i64) -> bool {
        match sr_moderation::is_sterilized(&self.0, account).await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, "vouch: reading the sterilization state failed; refusing the vouch");
                true
            }
        }
    }
}

/// The serious-ban subscriber of the vouch system.
pub struct VouchFallout {
    pub vouches: Vouches,
    pub pool: PgPool,
}

/// `spam_ring` or `ban evasion` as read by a person: lower case words. An unnamed category is a plain phrase.
pub fn category_label(category: Option<&str>) -> String {
    let words: String = category
        .unwrap_or("")
        .chars()
        .map(|c| if c == '_' || c == '-' { ' ' } else { c })
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .collect::<String>()
        .trim()
        .to_lowercase();
    if words.is_empty() {
        "a serious category".to_owned()
    } else {
        words
    }
}

#[async_trait]
impl SeriousBanObserver for VouchFallout {
    async fn serious_ban(&self, ban: &SeriousBan) {
        let label = category_label(ban.reason_category.as_deref());
        let accounts: Vec<i64> = match sqlx::query_scalar(
            "SELECT id FROM accounts WHERE person_id = $1",
        )
        .bind(ban.person_id)
        .fetch_all(&self.pool)
        .await
        {
            Ok(a) => a,
            Err(e) => {
                tracing::error!(error = %e, "vouch fallout: reading the accounts of a banned person failed");
                return;
            }
        };
        for account in accounts {
            if let Err(e) = self.vouches.on_serious_ban(account, &label).await {
                tracing::error!(error = %e, "vouch fallout failed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::category_label;

    #[test]
    fn categories_read_as_words() {
        assert_eq!(category_label(Some("spam_ring")), "spam ring");
        assert_eq!(category_label(Some("ban_evasion")), "ban evasion");
        assert_eq!(category_label(Some("Child-Safety")), "child safety");
        assert_eq!(category_label(Some("")), "a serious category");
        assert_eq!(category_label(None), "a serious category");
        // Nothing but letters, digits and spaces reaches the notice.
        assert_eq!(category_label(Some("fraud\n@everyone")), "fraudeveryone");
    }
}
