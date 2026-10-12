//! Live presence for invite previews (`approximate_presence_count`).
//!
//! Presence lives in the gateway's memory, which `sr-api` cannot see. The
//! composition root registers a [`PresenceCounter`] over it; without one,
//! `approximate_presence_count` is left out of the invite object (only
//! `approximate_member_count` is reported).

use sr_core::UserId;

pub trait PresenceCounter: Send + Sync {
    /// How many of `users` are visibly online (not offline or invisible).
    fn count_online(&self, users: &[UserId]) -> u64;
}
