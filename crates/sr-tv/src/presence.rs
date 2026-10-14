//! Viewer counting (TV-HLS-004): a member who fetches a live media playlist is a viewer of the channel for
//! [`VIEWER_TTL_MS`]. Metrics only, kept in memory, nothing persistent (D-101). Other work (watch parties,
//! the guide) reads the count or plugs its own implementation of [`Presence`] in.

use std::collections::HashMap;
use std::sync::Mutex;

/// A viewer counts for this long after the last playlist request (a few reloads of a 6 s playlist).
pub const VIEWER_TTL_MS: i64 = 45_000;

pub trait Presence: Send + Sync {
    /// `user_id` fetched a live playlist of `channel_id` at `now_ms`.
    fn touch(&self, channel_id: i64, user_id: i64, now_ms: i64);
    /// Distinct members seen within [`VIEWER_TTL_MS`].
    fn count(&self, channel_id: i64, now_ms: i64) -> u32;
}

/// In-memory presence.
#[derive(Default)]
pub struct MemoryPresence {
    seen: Mutex<HashMap<(i64, i64), i64>>,
}

impl MemoryPresence {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Presence for MemoryPresence {
    fn touch(&self, channel_id: i64, user_id: i64, now_ms: i64) {
        let mut seen = self.seen.lock().unwrap_or_else(|p| p.into_inner());
        seen.insert((channel_id, user_id), now_ms);
        // Keep the map small: drop lapsed entries now and then.
        if seen.len() > 256 && seen.len().is_power_of_two() {
            seen.retain(|_, at| now_ms - *at <= VIEWER_TTL_MS);
        }
    }

    fn count(&self, channel_id: i64, now_ms: i64) -> u32 {
        let seen = self.seen.lock().unwrap_or_else(|p| p.into_inner());
        let n = seen
            .iter()
            .filter(|((c, _), at)| *c == channel_id && now_ms - **at <= VIEWER_TTL_MS)
            .count();
        u32::try_from(n).unwrap_or(u32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewers_are_distinct_and_lapse() {
        let p = MemoryPresence::new();
        p.touch(1, 10, 0);
        p.touch(1, 10, 5_000);
        p.touch(1, 11, 5_000);
        p.touch(2, 10, 5_000);
        assert_eq!(p.count(1, 6_000), 2);
        assert_eq!(p.count(2, 6_000), 1);
        assert_eq!(p.count(1, 5_000 + VIEWER_TTL_MS), 2);
        assert_eq!(p.count(1, 5_001 + VIEWER_TTL_MS), 0);
        assert_eq!(p.count(3, 0), 0);
    }
}
