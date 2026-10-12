//! Injectable clock (FND-CORE-006). Tests use [`ManualClock`] for purge/expiry/TV time travel.

use std::sync::atomic::{AtomicI64, Ordering};

/// Source of wall-clock time in milliseconds since the Unix epoch.
pub trait Clock: Send + Sync {
    fn now_unix_ms(&self) -> i64;
}

/// Real clock. Not available on `wasm32-unknown-unknown` (std time panics there);
/// browser code uses `sr_permissions_wasm::JsClock`, backed by `Date.now()`.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

#[cfg(not(target_arch = "wasm32"))]
impl Clock for SystemClock {
    fn now_unix_ms(&self) -> i64 {
        match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => i64::try_from(d.as_millis()).unwrap_or(i64::MAX),
            Err(e) => -i64::try_from(e.duration().as_millis()).unwrap_or(i64::MAX),
        }
    }
}

/// Manually driven clock for tests.
#[derive(Debug, Default)]
pub struct ManualClock(AtomicI64);

impl ManualClock {
    pub fn new(unix_ms: i64) -> Self {
        Self(AtomicI64::new(unix_ms))
    }
    pub fn set(&self, unix_ms: i64) {
        self.0.store(unix_ms, Ordering::SeqCst);
    }
    pub fn advance_ms(&self, ms: i64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_unix_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

impl<C: Clock + ?Sized> Clock for std::sync::Arc<C> {
    fn now_unix_ms(&self) -> i64 {
        (**self).now_unix_ms()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_clock_advances() {
        let c = ManualClock::new(1000);
        c.advance_ms(500);
        assert_eq!(c.now_unix_ms(), 1500);
        c.set(7);
        assert_eq!(c.now_unix_ms(), 7);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn system_clock_is_after_2026() {
        assert!(SystemClock.now_unix_ms() > 1_767_225_600_000);
    }
}
