//! `abuse.bridge_messages_per_minute`: how many messages one bridge brings in from Discord a minute. The figure
//! is read live on every message, so an operator's change applies to the next one. Over the figure a message is
//! dropped (never queued) and counted; the count is what the status shows.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(60);

/// A fixed one-minute window per bridge.
#[derive(Default)]
pub struct BridgeLimiter {
    windows: Mutex<HashMap<i64, Window>>,
}

struct Window {
    started: Instant,
    used: u32,
    dropped: u64,
}

impl BridgeLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the bridge of `guild` may bring in one more message now, given the live `figure` per minute.
    pub fn allow(&self, guild: i64, figure: u32) -> bool {
        self.allow_at(guild, figure, Instant::now())
    }

    pub(crate) fn allow_at(&self, guild: i64, figure: u32, now: Instant) -> bool {
        let mut map = self
            .windows
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let w = map.entry(guild).or_insert(Window {
            started: now,
            used: 0,
            dropped: 0,
        });
        if now.duration_since(w.started) >= WINDOW {
            w.started = now;
            w.used = 0;
        }
        if w.used < figure.max(1) {
            w.used += 1;
            true
        } else {
            w.dropped += 1;
            false
        }
    }

    /// Messages dropped for `guild` since the process started.
    pub fn dropped(&self, guild: i64) -> u64 {
        self.windows
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&guild)
            .map_or(0, |w| w.dropped)
    }

    /// Forget a bridge (it was switched off).
    pub fn forget(&self, guild: i64) {
        self.windows
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&guild);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bridge_is_held_to_its_figure_per_minute_and_follows_a_live_change() {
        let l = BridgeLimiter::new();
        let t0 = Instant::now();
        for _ in 0..3 {
            assert!(l.allow_at(1, 3, t0));
        }
        assert!(!l.allow_at(1, 3, t0), "the fourth in the minute is dropped");
        assert_eq!(l.dropped(1), 1);
        // Another bridge has its own allowance.
        assert!(l.allow_at(2, 3, t0));
        // A live raise applies to the very next message.
        assert!(l.allow_at(1, 5, t0));
        // The window rolls over after a minute.
        let t1 = t0 + Duration::from_secs(61);
        assert!(l.allow_at(1, 3, t1));
        // Switching the bridge off forgets it.
        l.forget(1);
        assert_eq!(l.dropped(1), 0);
    }
}
