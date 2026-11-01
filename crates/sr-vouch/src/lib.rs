//! sr-vouch: profile vouches and the accountability multiplier (MOD-VCH-001, wave 44-B "Heavy Crown").
//!
//! * [`tiers`] is the pure part: Citizen 0-49, Pillar 50-499, Elder 500+, the 1.0 / 1.5 / 3.0 multiplier and the
//!   scaling helpers for timeouts, holds and strike weights.
//! * [`Vouches`] is the handle over `profile_vouches` (migration 2130): vouch, revoke, summary, the multiplier of
//!   a member, the serious-ban fallout and the erasure. It is NOT the wave 42 `arcade_build_vouches` (hash-pinned
//!   vouches for a game build); the two tables never mix.
//! * The seams other code plugs into: [`VouchNotifier`] (the private notice), [`SterilizationCheck`] (wave 44-A's
//!   sterilized inviters cannot vouch) and [`AccountabilityMultiplier`] (what the moderation engine asks).

mod error;
mod service;
pub mod tiers;

pub use error::{VouchError, VouchResult};
pub use service::{
    AccountabilityMultiplier, Fallout, MAX_VOUCHES_GIVEN, SterilizationCheck, VOUCHES_PER_DAY,
    VouchNotifier, VouchSummary, Vouches, fallout_notice,
};
pub use tiers::{Tier, multiplier_for, tier_for};
