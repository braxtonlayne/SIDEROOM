//! sr-payments: self-custodial, watch-only Bitcoin donations, entitlements and limits.
//!
//! See docs/specs/donations.md and docs/08-donations-perks-and-cost-model.md. The crate has no
//! spending, signing or broadcasting code: it derives receive addresses from an operator-supplied
//! watch-only descriptor and reads the chain through [`chain::ChainSource`].

pub mod chain;
pub mod config;
pub mod descriptor;
pub mod entitlements;
pub mod error;
pub mod invoice;
pub mod memory;
pub mod rates;
pub mod repo;
pub mod service;

pub use config::{DonationConfig, Limits, PerkSet, VideoProfile, effective_limits};
pub use entitlements::{
    EntitlementService, Entitlements, GuildEntitlement, PatronStatus, UserEntitlement,
};
pub use error::{PayError, PayResult};
pub use invoice::{Invoice, InvoiceState, Purpose};
pub use service::PaymentService;

/// The `SR_PAYMENT_UPDATE` gateway event name (user-scoped, to the payer).
pub const PAYMENT_EVENT: &str = "SR_PAYMENT_UPDATE";
/// The `SR_ENTITLEMENT_UPDATE` gateway event name.
pub const ENTITLEMENT_EVENT: &str = "SR_ENTITLEMENT_UPDATE";

#[cfg(test)]
mod service_tests;

/// Unix milliseconds of 00:00 UTC on the first day of the month containing `now_ms`.
pub fn utc_month_start_ms(now_ms: i64) -> i64 {
    let days = now_ms.div_euclid(86_400_000);
    // Civil-from-days (Howard Hinnant), then days-from-civil of the 1st.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    // days-from-civil(y, m, 1)
    let y2 = if m <= 2 { y - 1 } else { y };
    let era2 = y2.div_euclid(400);
    let yoe2 = y2.rem_euclid(400);
    let mp2 = if m > 2 { m - 3 } else { m + 9 };
    let doy2 = (153 * mp2 + 2) / 5;
    let doe2 = yoe2 * 365 + yoe2 / 4 - yoe2 / 100 + doy2;
    let first = era2 * 146_097 + doe2 - 719_468;
    let _ = d;
    first * 86_400_000
}

#[cfg(test)]
mod month_tests {
    use super::utc_month_start_ms;

    #[test]
    fn month_start() {
        // 2026-10-14 12:00 UTC -> 2026-10-01 00:00 UTC.
        assert_eq!(utc_month_start_ms(1_792_065_600_000), 1_790_812_800_000);
        // Already the first instant of a month.
        assert_eq!(utc_month_start_ms(1_790_812_800_000), 1_790_812_800_000);
        // Leap day 2024-02-29 -> 2024-02-01.
        assert_eq!(utc_month_start_ms(1_709_164_800_000), 1_706_745_600_000);
        // Epoch.
        assert_eq!(utc_month_start_ms(0), 0);
        assert_eq!(utc_month_start_ms(31 * 86_400_000), 31 * 86_400_000);
    }
}
