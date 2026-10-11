//! In-memory throttles for the auth endpoints (single node, like the global
//! rate limiter): per-IP sliding windows, failed-login lockout, and the
//! pending-2FA table. All take explicit Unix-second timestamps.

use sr_auth::session::TokenHash;
use std::{
    collections::{HashMap, VecDeque},
    sync::{Mutex, MutexGuard},
};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// `limit` events per `secs` seconds.
#[derive(Debug, Clone, Copy)]
pub struct Rule {
    /// Events allowed per window.
    pub limit: usize,
    /// Window length in seconds.
    pub secs: i64,
}

/// Sliding-window event limiter keyed by `(scope, key)`.
#[derive(Default)]
pub struct WindowLimiter {
    events: Mutex<HashMap<String, VecDeque<i64>>>,
}

impl WindowLimiter {
    /// Count one event against every rule. `Err(retry_after_secs)` when any
    /// rule is exceeded (the event is then not recorded).
    pub fn hit(&self, scope: &str, key: &str, now: i64, rules: &[Rule]) -> Result<(), i64> {
        let longest = rules.iter().map(|r| r.secs).max().unwrap_or(0);
        let mut g = lock(&self.events);
        if g.len() > 50_000 {
            g.retain(|_, q| q.back().is_some_and(|t| now - t < 86_400));
        }
        let q = g.entry(format!("{scope}:{key}")).or_default();
        while q.front().is_some_and(|t| now - t >= longest) {
            q.pop_front();
        }
        let mut wait = 0;
        for r in rules {
            let in_window = q.iter().filter(|t| now - **t < r.secs).count();
            if in_window >= r.limit {
                // The oldest event inside the window leaves it after `secs`.
                let idx = in_window - r.limit;
                let anchor = q
                    .iter()
                    .filter(|t| now - **t < r.secs)
                    .nth(idx)
                    .copied()
                    .unwrap_or(now);
                wait = wait.max(anchor + r.secs - now);
            }
        }
        if wait > 0 {
            return Err(wait.max(1));
        }
        q.push_back(now);
        Ok(())
    }
}

/// Failed-attempt lockout with exponential back-off (doc 06 section 4):
/// the first `free` failures are free, then the lock lasts
/// `base * 2^(n - free - 1)` seconds, capped at `max`.
pub struct Lockout {
    free: u32,
    base: i64,
    max: i64,
    state: Mutex<HashMap<String, (u32, i64, i64)>>, // failures, locked_until, last_failure
}

impl Lockout {
    /// New policy.
    pub fn new(free: u32, base_secs: i64, max_secs: i64) -> Self {
        Self {
            free,
            base: base_secs,
            max: max_secs,
            state: Mutex::default(),
        }
    }

    /// `Err(retry_after)` while locked.
    pub fn check(&self, key: &str, now: i64) -> Result<(), i64> {
        match lock(&self.state).get(key) {
            Some((_, until, _)) if *until > now => Err(until - now),
            _ => Ok(()),
        }
    }

    /// Record a failure; returns the new lock duration if one started.
    pub fn fail(&self, key: &str, now: i64) -> Option<i64> {
        let mut g = lock(&self.state);
        if g.len() > 50_000 {
            g.retain(|_, (_, until, last)| *until > now || now - *last < self.max * 2);
        }
        let e = g.entry(key.to_owned()).or_insert((0, 0, now));
        // Forget old failures after a quiet period.
        if now - e.2 > self.max * 2 {
            e.0 = 0;
        }
        e.0 += 1;
        e.2 = now;
        if e.0 > self.free {
            let shift = (e.0 - self.free - 1).min(20);
            let dur = (self.base.saturating_mul(1 << shift)).min(self.max);
            e.1 = now + dur;
            Some(dur)
        } else {
            None
        }
    }

    /// Clear after a success.
    pub fn clear(&self, key: &str) {
        lock(&self.state).remove(key);
    }
}

/// A password-verified login waiting for its second factor.
#[derive(Debug, Clone)]
pub struct PendingMfa {
    /// Account that passed the password step.
    pub user_id: i64,
    /// Expiry (Unix seconds).
    pub expires_at: i64,
    /// Wrong codes so far.
    pub attempts: u32,
}

/// Pending second-factor logins keyed by the hash of the `sr_mfa` cookie.
#[derive(Default)]
pub struct PendingMfaTable {
    map: Mutex<HashMap<String, PendingMfa>>,
}

impl PendingMfaTable {
    /// Register a pending login.
    pub fn insert(&self, hash: &TokenHash, user_id: i64, expires_at: i64, now: i64) {
        let mut g = lock(&self.map);
        g.retain(|_, p| p.expires_at > now);
        g.insert(
            hash.to_hex(),
            PendingMfa {
                user_id,
                expires_at,
                attempts: 0,
            },
        );
    }

    /// Look up an unexpired pending login.
    pub fn get(&self, hash: &TokenHash, now: i64) -> Option<PendingMfa> {
        lock(&self.map)
            .get(&hash.to_hex())
            .filter(|p| p.expires_at > now)
            .cloned()
    }

    /// Count a wrong code; returns the new attempt count (entry removed at
    /// `max_attempts`).
    pub fn fail(&self, hash: &TokenHash, max_attempts: u32) -> u32 {
        let mut g = lock(&self.map);
        let key = hash.to_hex();
        let n = g.get_mut(&key).map_or(max_attempts, |p| {
            p.attempts += 1;
            p.attempts
        });
        if n >= max_attempts {
            g.remove(&key);
        }
        n
    }

    /// Remove (on success).
    pub fn remove(&self, hash: &TokenHash) {
        lock(&self.map).remove(&hash.to_hex());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_limits_and_reports_wait() {
        let l = WindowLimiter::default();
        let rules = [Rule { limit: 3, secs: 60 }];
        for t in 0..3 {
            assert!(l.hit("s", "ip", t, &rules).is_ok());
        }
        assert_eq!(l.hit("s", "ip", 10, &rules), Err(50));
        // Other keys and scopes are independent.
        assert!(l.hit("s", "other", 10, &rules).is_ok());
        assert!(l.hit("t", "ip", 10, &rules).is_ok());
        // Window slides.
        assert!(l.hit("s", "ip", 61, &rules).is_ok());
    }

    #[test]
    fn multiple_rules() {
        let l = WindowLimiter::default();
        let rules = [
            Rule {
                limit: 10,
                secs: 60,
            },
            Rule {
                limit: 12,
                secs: 3600,
            },
        ];
        for i in 0..10 {
            assert!(l.hit("s", "ip", i, &rules).is_ok());
        }
        assert!(l.hit("s", "ip", 30, &rules).is_err());
        for i in 0..2 {
            assert!(l.hit("s", "ip", 100 + i, &rules).is_ok());
        }
        // Hourly rule now bites even though the minute rule is clear.
        assert!(l.hit("s", "ip", 200, &rules).is_err());
    }

    #[test]
    fn lockout_backs_off_exponentially() {
        let l = Lockout::new(5, 30, 900);
        for _ in 0..5 {
            assert_eq!(l.fail("a", 0), None);
        }
        assert!(l.check("a", 0).is_ok());
        assert_eq!(l.fail("a", 0), Some(30));
        assert_eq!(l.check("a", 10), Err(20));
        assert!(l.check("a", 30).is_ok());
        assert_eq!(l.fail("a", 30), Some(60));
        assert_eq!(l.fail("a", 100), Some(120));
        l.clear("a");
        assert!(l.check("a", 100).is_ok());
        assert_eq!(l.fail("a", 100), None);
        // Cap.
        for _ in 0..40 {
            l.fail("b", 0);
        }
        assert_eq!(l.fail("b", 0), Some(900));
    }

    #[test]
    fn pending_mfa_expires_and_counts() {
        let t = PendingMfaTable::default();
        let h = sr_auth::session::SessionToken::generate().unwrap().hash();
        t.insert(&h, 7, 100, 0);
        assert_eq!(t.get(&h, 50).map(|p| p.user_id), Some(7));
        assert!(t.get(&h, 100).is_none());
        assert_eq!(t.fail(&h, 3), 1);
        assert_eq!(t.fail(&h, 3), 2);
        assert_eq!(t.fail(&h, 3), 3);
        assert!(t.get(&h, 50).is_none());
    }
}
