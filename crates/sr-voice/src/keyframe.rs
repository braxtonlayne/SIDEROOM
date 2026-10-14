//! Keyframe request coalescing (VOX-SFU-007).
//!
//! Many things ask a publisher for a keyframe: every new subscriber, every layer switch,
//! and every receiver that lost a packet it could not recover (PLI or FIR). Each keyframe
//! is expensive for the publisher (a burst of 5 to 10 times a normal frame) and every
//! subscriber of that layer receives it. [`KeyframeCoalescer`] therefore turns any number of
//! requests into at most one upstream request per [`Config::min_interval`] per key.
//!
//! * The **first** request goes out at once.
//! * Requests inside the interval are **remembered, not dropped**: one request is sent when
//!   the interval ends ([`KeyframeCoalescer::poll`]), because a requester that arrived just
//!   after a keyframe still needs one it can decode from.
//! * A request satisfied by a keyframe that went out meanwhile is cleared
//!   ([`KeyframeCoalescer::keyframe_seen`]): the subscriber that asked has what it needs.
//! * FIR is stronger than PLI: if any pending request is a FIR, the one sent is a FIR.
//!
//! NACK and RTX are not handled here. str0m answers NACKs from its send buffer and
//! negotiates RTX in the SDP (see the codec setup in `core.rs`); a subscriber that loses more
//! than the buffer holds escalates to PLI, which ends up here.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

/// How strong the request is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Pli,
    Fir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    /// At most one upstream request per key in this time.
    pub min_interval: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            min_interval: Duration::from_millis(500),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    /// When the last request went upstream.
    last_sent: Option<Instant>,
    /// A request waiting for the interval to end, with the strongest kind seen.
    pending: Option<Kind>,
}

/// Coalesces requests per `K` (a publisher track and layer).
#[derive(Debug)]
pub struct KeyframeCoalescer<K> {
    cfg: Config,
    entries: HashMap<K, Entry>,
}

impl<K: Copy + Eq + Hash> KeyframeCoalescer<K> {
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg,
            entries: HashMap::new(),
        }
    }

    /// Someone needs a keyframe of `key`. `Some(kind)`: send a request upstream now.
    pub fn request(&mut self, key: K, kind: Kind, now: Instant) -> Option<Kind> {
        let e = self.entries.entry(key).or_insert(Entry {
            last_sent: None,
            pending: None,
        });
        let due = e
            .last_sent
            .is_none_or(|t| now.saturating_duration_since(t) >= self.cfg.min_interval);
        if due {
            e.last_sent = Some(now);
            // Anything that queued up while waiting is satisfied by this one.
            let strongest = e.pending.take().map_or(kind, |p| p.max(kind));
            Some(strongest)
        } else {
            e.pending = Some(e.pending.map_or(kind, |p| p.max(kind)));
            None
        }
    }

    /// Requests whose interval ended: send these upstream now.
    pub fn poll(&mut self, now: Instant) -> Vec<(K, Kind)> {
        let interval = self.cfg.min_interval;
        let mut out = Vec::new();
        for (k, e) in &mut self.entries {
            if let Some(kind) = e.pending
                && e.last_sent
                    .is_none_or(|t| now.saturating_duration_since(t) >= interval)
            {
                e.pending = None;
                e.last_sent = Some(now);
                out.push((*k, kind));
            }
        }
        out
    }

    /// When [`poll`](Self::poll) next has something to send, for the caller's timer.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.entries
            .values()
            .filter(|e| e.pending.is_some())
            .filter_map(|e| e.last_sent.map(|t| t + self.cfg.min_interval))
            .min()
    }

    /// A keyframe of `key` just arrived: requests waiting for one are satisfied.
    pub fn keyframe_seen(&mut self, key: K) {
        if let Some(e) = self.entries.get_mut(&key) {
            e.pending = None;
        }
    }

    /// Forget a key (the track is gone).
    pub fn forget(&mut self, key: &K) {
        self.entries.remove(key);
    }

    /// Forget every key matching `pred`.
    pub fn forget_where(&mut self, pred: impl Fn(&K) -> bool) {
        self.entries.retain(|k, _| !pred(k));
    }

    pub fn is_pending(&self, key: &K) -> bool {
        self.entries.get(key).is_some_and(|e| e.pending.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn co() -> (KeyframeCoalescer<u32>, Instant) {
        (KeyframeCoalescer::new(Config::default()), Instant::now())
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn the_first_request_goes_out_at_once() {
        let (mut c, t) = co();
        assert_eq!(c.request(1, Kind::Pli, t), Some(Kind::Pli));
        assert!(!c.is_pending(&1));
    }

    #[test]
    fn a_crowd_of_requests_becomes_one_upstream_request_per_interval() {
        let (mut c, t) = co();
        let mut sent = 0;
        // Fifty subscribers all ask within 400 ms.
        for i in 0..50u64 {
            if c.request(1, Kind::Pli, t + ms(i * 8)).is_some() {
                sent += 1;
            }
        }
        assert_eq!(sent, 1, "one PLI went up");
        assert!(c.is_pending(&1));
        // Nothing more until the interval is over.
        assert!(c.poll(t + ms(499)).is_empty());
        // Then exactly one more, for the ones that asked meanwhile.
        assert_eq!(c.poll(t + ms(500)), vec![(1, Kind::Pli)]);
        assert!(!c.is_pending(&1));
        assert!(c.poll(t + ms(2_000)).is_empty());
    }

    #[test]
    fn requests_after_the_interval_are_sent_directly() {
        let (mut c, t) = co();
        assert!(c.request(1, Kind::Pli, t).is_some());
        assert!(c.request(1, Kind::Pli, t + ms(499)).is_none());
        let mut c2 = KeyframeCoalescer::new(Config::default());
        assert!(c2.request(1, Kind::Pli, t).is_some());
        assert!(c2.request(1, Kind::Pli, t + ms(500)).is_some());
    }

    #[test]
    fn fir_outranks_pli() {
        let (mut c, t) = co();
        c.request(1, Kind::Pli, t);
        assert!(c.request(1, Kind::Pli, t + ms(100)).is_none());
        assert!(c.request(1, Kind::Fir, t + ms(200)).is_none());
        assert!(c.request(1, Kind::Pli, t + ms(300)).is_none());
        assert_eq!(c.poll(t + ms(500)), vec![(1, Kind::Fir)]);
        // And a due request that finds a FIR waiting sends the FIR.
        let (mut c, t) = co();
        c.request(2, Kind::Pli, t);
        c.request(2, Kind::Fir, t + ms(10));
        assert_eq!(c.request(2, Kind::Pli, t + ms(600)), Some(Kind::Fir));
    }

    #[test]
    fn keys_are_independent() {
        let (mut c, t) = co();
        assert!(c.request(1, Kind::Pli, t).is_some());
        assert!(
            c.request(2, Kind::Pli, t).is_some(),
            "another layer or track"
        );
        assert!(c.request(1, Kind::Pli, t + ms(1)).is_none());
        assert!(c.request(2, Kind::Pli, t + ms(1)).is_none());
        let mut due = c.poll(t + ms(500));
        due.sort();
        assert_eq!(due, vec![(1, Kind::Pli), (2, Kind::Pli)]);
    }

    #[test]
    fn a_keyframe_that_arrives_satisfies_the_waiting_requests() {
        let (mut c, t) = co();
        c.request(1, Kind::Pli, t);
        c.request(1, Kind::Pli, t + ms(100));
        assert!(c.is_pending(&1));
        c.keyframe_seen(1);
        assert!(!c.is_pending(&1));
        assert!(c.poll(t + ms(900)).is_empty());
        // The next request still honours the interval since the last upstream request.
        assert!(c.request(1, Kind::Pli, t + ms(300)).is_none());
    }

    #[test]
    fn the_timer_points_at_the_next_pending_request() {
        let (mut c, t) = co();
        assert_eq!(c.next_deadline(), None);
        c.request(1, Kind::Pli, t);
        assert_eq!(c.next_deadline(), None, "nothing waits");
        c.request(1, Kind::Pli, t + ms(10));
        assert_eq!(c.next_deadline(), Some(t + ms(500)));
    }

    #[test]
    fn forgetting_a_track_drops_its_state() {
        let (mut c, t) = co();
        c.request(1, Kind::Pli, t);
        c.request(1, Kind::Pli, t + ms(1));
        c.request(2, Kind::Pli, t);
        c.forget(&1);
        assert!(c.poll(t + ms(600)).is_empty());
        assert!(c.request(1, Kind::Pli, t + ms(2)).is_some(), "fresh again");
        c.forget_where(|k| *k == 2);
        assert!(c.request(2, Kind::Pli, t + ms(3)).is_some());
    }
}
