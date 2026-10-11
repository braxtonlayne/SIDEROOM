//! Browser clock backed by `Date.now()` (Q7). `sr_core::SystemClock` is not available on
//! `wasm32-unknown-unknown`, so the exported `compute` uses this instead.

use sr_core::Clock;

#[derive(Debug, Default, Clone, Copy)]
pub struct JsClock;

impl Clock for JsClock {
    fn now_unix_ms(&self) -> i64 {
        // Date.now() is an f64 of milliseconds since the epoch; it fits i64 exactly.
        js_sys::Date::now() as i64
    }
}
