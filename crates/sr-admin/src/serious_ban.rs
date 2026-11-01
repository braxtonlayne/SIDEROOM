//! MOD-GRD-001: the two seams wave 44 shares between the graduated moderation toolset (44-A) and the profile
//! vouches (44-B).
//!
//! * **Serious ban.** [`AdminService::ban_person`](crate::AdminService::ban_person) calls every subscribed
//!   [`SeriousBanObserver`] after a ban whose stored category is `serious` has been committed. 44-A subscribes the
//!   subtree-case opener; 44-B subscribes the voucher notice (vouchers are told, never penalised). A failing
//!   observer is logged by its implementation and never fails the ban.
//! * **Strike weight.** [`StrikeWeight::strike_weight_multiplier`] is read by the moderation engine whenever it
//!   scales an automated timeout, hold or strike. The default impl answers `1.0`; 44-B supplies the real one
//!   (Pillar 1.5x, Elder 3.0x) and the integrator wires it with `Moderation::with_strike_weight`.

use async_trait::async_trait;
use std::sync::{Arc, RwLock};

/// A ban with a serious category has been committed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriousBan {
    /// `person_bans.id`.
    pub ban_id: i64,
    /// The banned Person.
    pub person_id: i64,
    /// The account the staff member acted on (the root of a subtree case), when there was one.
    pub source_account_id: Option<i64>,
    /// The staff member who banned.
    pub actor_id: i64,
    /// The `ban_reason_category` value (`spam`, `fraud`, `ban_evasion`, `child_safety`, `illegal_content`, ...),
    /// when the caller named one.
    pub reason_category: Option<String>,
    /// The ban's written note.
    pub note: String,
}

/// Subscribes to serious bans.
#[async_trait]
pub trait SeriousBanObserver: Send + Sync {
    async fn serious_ban(&self, ban: &SeriousBan);
}

/// The subscribers, shared by every clone of the admin service.
#[derive(Default)]
pub struct SeriousBanSlot(RwLock<Vec<Arc<dyn SeriousBanObserver>>>);

impl SeriousBanSlot {
    /// Add a subscriber (any number may subscribe).
    pub fn subscribe(&self, o: Arc<dyn SeriousBanObserver>) {
        if let Ok(mut v) = self.0.write() {
            v.push(o);
        }
    }

    /// Tell every subscriber, in subscription order.
    pub async fn emit(&self, ban: &SeriousBan) {
        let list: Vec<_> = self.0.read().map(|v| v.clone()).unwrap_or_default();
        for o in list {
            o.serious_ban(ban).await;
        }
    }
}

/// The accountability multiplier of an account (44-B): `1.0` for an ordinary member, more for a high-vouch one.
#[async_trait]
pub trait StrikeWeight: Send + Sync {
    async fn strike_weight_multiplier(&self, account: i64) -> f32;
}

/// The seam's default: every account weighs `1.0`.
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultStrikeWeight;

#[async_trait]
impl StrikeWeight for DefaultStrikeWeight {
    async fn strike_weight_multiplier(&self, _account: i64) -> f32 {
        1.0
    }
}

/// Scale a whole number (minutes of timeout, hold days, strike points) by a multiplier, rounding up so a
/// multiplier above 1 never rounds back down; a multiplier below 1 or not finite counts as 1 (weights only rise).
pub fn scale_up(base: i64, multiplier: f32) -> i64 {
    if !multiplier.is_finite() || multiplier <= 1.0 {
        return base;
    }
    (base as f64 * f64::from(multiplier)).ceil() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_up_only_raises() {
        assert_eq!(scale_up(10, 1.0), 10);
        assert_eq!(scale_up(10, 1.5), 15);
        assert_eq!(scale_up(7, 3.0), 21);
        assert_eq!(scale_up(10, 0.5), 10);
        assert_eq!(scale_up(10, f32::NAN), 10);
    }

    #[tokio::test]
    async fn default_weighs_one() {
        assert_eq!(DefaultStrikeWeight.strike_weight_multiplier(42).await, 1.0);
    }

    struct Count(std::sync::atomic::AtomicUsize);
    #[async_trait]
    impl SeriousBanObserver for Count {
        async fn serious_ban(&self, _: &SeriousBan) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn every_subscriber_hears_the_ban() {
        let slot = SeriousBanSlot::default();
        let a = Arc::new(Count(Default::default()));
        let b = Arc::new(Count(Default::default()));
        slot.subscribe(a.clone());
        slot.subscribe(b.clone());
        slot.emit(&SeriousBan {
            ban_id: 1,
            person_id: 2,
            source_account_id: Some(3),
            actor_id: 4,
            reason_category: Some("fraud".into()),
            note: "n".into(),
        })
        .await;
        assert_eq!(a.0.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(b.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
