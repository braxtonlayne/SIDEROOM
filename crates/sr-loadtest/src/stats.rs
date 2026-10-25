//! Latency samples, percentiles and the pass/fail verdict.

use std::time::Duration;

/// Latency samples in microseconds.
#[derive(Debug, Clone, Default)]
pub struct Samples {
    us: Vec<u32>,
    errors: u64,
}

impl Samples {
    pub fn record(&mut self, d: Duration) {
        self.us
            .push(u32::try_from(d.as_micros()).unwrap_or(u32::MAX));
    }

    pub fn record_us(&mut self, us: u64) {
        self.us.push(u32::try_from(us).unwrap_or(u32::MAX));
    }

    pub fn error(&mut self) {
        self.errors += 1;
    }

    pub fn merge(&mut self, other: Samples) {
        self.us.extend(other.us);
        self.errors += other.errors;
    }

    /// Successful samples.
    pub fn count(&self) -> usize {
        self.us.len()
    }

    pub fn is_empty(&self) -> bool {
        self.us.is_empty()
    }

    pub fn errors(&self) -> u64 {
        self.errors
    }

    /// Failed share of all attempts, 0 when there were none.
    pub fn error_rate(&self) -> f64 {
        let total = self.us.len() as u64 + self.errors;
        if total == 0 {
            0.0
        } else {
            self.errors as f64 / total as f64
        }
    }

    /// The `p`th percentile (0 to 100) in milliseconds, by the nearest-rank method; 0 for no samples.
    pub fn percentile_ms(&self, p: f64) -> f64 {
        if self.us.is_empty() {
            return 0.0;
        }
        let mut v = self.us.clone();
        v.sort_unstable();
        let rank = ((p / 100.0) * v.len() as f64).ceil().max(1.0) as usize;
        f64::from(v[rank.min(v.len()) - 1]) / 1000.0
    }

    pub fn max_ms(&self) -> f64 {
        self.us
            .iter()
            .copied()
            .max()
            .map_or(0.0, |m| f64::from(m) / 1000.0)
    }
}

/// What the run has to achieve to pass.
#[derive(Debug, Clone, Copy)]
pub struct Targets {
    /// Share of the requested gateway sessions that must reach READY.
    pub min_ready_ratio: f64,
    /// REST p95 in milliseconds (the inventory's "REST p95 < 100 ms").
    pub max_rest_p95_ms: f64,
    /// Share of REST requests allowed to fail.
    pub max_rest_error_rate: f64,
    /// Share of the target message rate that must actually be sent.
    pub min_send_ratio: f64,
    /// Share of expected gateway deliveries that must arrive.
    pub min_delivery_ratio: f64,
    /// Fan-out (POST to dispatch) p95 in milliseconds (RDY-PERF-002: 250 ms).
    pub max_fanout_p95_ms: f64,
    /// First paint of a member of many servers, in milliseconds (K-22: 1.5 s with 60 servers).
    pub max_first_paint_ms: f64,
}

impl Default for Targets {
    fn default() -> Self {
        Self {
            min_ready_ratio: 0.99,
            max_rest_p95_ms: 100.0,
            max_rest_error_rate: 0.01,
            min_send_ratio: 0.95,
            min_delivery_ratio: 0.99,
            max_fanout_p95_ms: 250.0,
            max_first_paint_ms: 1500.0,
        }
    }
}

/// The measured outcome of a run.
#[derive(Debug, Clone, Default)]
pub struct Measured {
    pub sessions_requested: usize,
    pub sessions_ready: usize,
    pub rest: Samples,
    pub send_rate_target: f64,
    pub send_rate_achieved: f64,
    pub deliveries_expected: u64,
    pub deliveries_received: u64,
    pub fanout: Samples,
    /// The first-paint probe, when it ran.
    pub first_paint: Option<FirstPaint>,
}

/// What a member of many servers sees when the Lounge loads: the gateway READY and the REST calls of the shell
/// at the same time, while the rest of the load runs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FirstPaint {
    /// Servers the probe account is in.
    pub guilds: usize,
    /// Wall time until READY and every REST call had answered.
    pub millis: f64,
    /// READY frame size in bytes.
    pub ready_bytes: usize,
    /// REST calls of the probe, and how many answered 429 or failed otherwise.
    pub requests: usize,
    pub rate_limited: usize,
    pub failed: usize,
    /// READY never arrived (within the probe's wait); counted apart from the REST calls, which run 40226523570
    /// reported as "1 of 3 requests failed".
    pub ready_missing: bool,
}

impl Measured {
    pub fn ready_ratio(&self) -> f64 {
        ratio(self.sessions_ready as f64, self.sessions_requested as f64)
    }

    pub fn send_ratio(&self) -> f64 {
        ratio(self.send_rate_achieved, self.send_rate_target)
    }

    pub fn delivery_ratio(&self) -> f64 {
        ratio(
            self.deliveries_received as f64,
            self.deliveries_expected as f64,
        )
    }

    /// Every failed target, as a readable line; empty when the run passed.
    pub fn failures(&self, t: &Targets) -> Vec<String> {
        let mut out = Vec::new();
        if self.ready_ratio() < t.min_ready_ratio {
            out.push(format!(
                "only {}/{} gateway sessions reached READY ({:.1}% < {:.1}%)",
                self.sessions_ready,
                self.sessions_requested,
                self.ready_ratio() * 100.0,
                t.min_ready_ratio * 100.0
            ));
        }
        let p95 = self.rest.percentile_ms(95.0);
        if p95 > t.max_rest_p95_ms {
            out.push(format!(
                "REST p95 {p95:.1} ms is over {:.0} ms",
                t.max_rest_p95_ms
            ));
        }
        if self.rest.error_rate() > t.max_rest_error_rate {
            out.push(format!(
                "{:.2}% of REST requests failed (allowed {:.2}%)",
                self.rest.error_rate() * 100.0,
                t.max_rest_error_rate * 100.0
            ));
        }
        if self.send_ratio() < t.min_send_ratio {
            out.push(format!(
                "sent {:.0} messages/s of the {:.0}/s target",
                self.send_rate_achieved, self.send_rate_target
            ));
        }
        if self.delivery_ratio() < t.min_delivery_ratio {
            out.push(format!(
                "{} of {} gateway deliveries arrived ({:.2}%)",
                self.deliveries_received,
                self.deliveries_expected,
                self.delivery_ratio() * 100.0
            ));
        }
        if !self.fanout.is_empty() {
            let f95 = self.fanout.percentile_ms(95.0);
            if f95 > t.max_fanout_p95_ms {
                out.push(format!(
                    "fan-out p95 {f95:.1} ms is over {:.0} ms",
                    t.max_fanout_p95_ms
                ));
            }
        }
        if let Some(p) = &self.first_paint {
            if p.rate_limited > 0 || p.failed > 0 {
                out.push(format!(
                    "first paint with {} servers: {} of {} requests failed ({} were 429)",
                    p.guilds,
                    p.rate_limited + p.failed,
                    p.requests,
                    p.rate_limited
                ));
            }
            if p.ready_missing {
                out.push(format!(
                    "first paint with {} servers: no READY arrived",
                    p.guilds
                ));
            }
            if p.millis > t.max_first_paint_ms {
                out.push(format!(
                    "first paint with {} servers took {:.0} ms (over {:.0} ms)",
                    p.guilds, p.millis, t.max_first_paint_ms
                ));
            }
        }
        out
    }
}

/// `a / b`, treating "nothing was asked for" as met.
fn ratio(a: f64, b: f64) -> f64 {
    if b <= 0.0 { 1.0 } else { a / b }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(ms: &[u64]) -> Samples {
        let mut s = Samples::default();
        for m in ms {
            s.record_us(m * 1000);
        }
        s
    }

    #[test]
    fn nearest_rank_percentiles() {
        let s = samples(&(1..=100).collect::<Vec<_>>());
        assert_eq!(s.percentile_ms(50.0), 50.0);
        assert_eq!(s.percentile_ms(95.0), 95.0);
        assert_eq!(s.percentile_ms(99.0), 99.0);
        assert_eq!(s.percentile_ms(100.0), 100.0);
        assert_eq!(s.max_ms(), 100.0);
        assert_eq!(Samples::default().percentile_ms(95.0), 0.0);
        assert_eq!(samples(&[7]).percentile_ms(1.0), 7.0);
    }

    #[test]
    fn errors_count_against_the_rate_and_merge() {
        let mut a = samples(&[1, 2, 3]);
        a.error();
        let mut b = samples(&[4, 5, 6]);
        b.error();
        a.merge(b);
        assert_eq!((a.count(), a.errors()), (6, 2));
        assert!((a.error_rate() - 0.25).abs() < 1e-9);
        assert_eq!(Samples::default().error_rate(), 0.0);
    }

    fn passing() -> Measured {
        Measured {
            sessions_requested: 2000,
            sessions_ready: 2000,
            rest: samples(&[5, 6, 7, 8, 9, 10, 11, 12, 13, 14]),
            send_rate_target: 200.0,
            send_rate_achieved: 199.0,
            deliveries_expected: 1000,
            deliveries_received: 1000,
            fanout: samples(&[3, 4]),
            first_paint: None,
        }
    }

    #[test]
    fn a_healthy_run_passes_the_inventory_targets() {
        assert!(passing().failures(&Targets::default()).is_empty());
    }

    #[test]
    fn every_target_can_fail_on_its_own() {
        let t = Targets::default();
        let mut m = passing();
        m.sessions_ready = 1900;
        assert!(m.failures(&t)[0].contains("1900/2000"));

        let mut m = passing();
        m.rest = samples(&[150; 20]);
        assert!(m.failures(&t)[0].contains("REST p95"));

        let mut m = passing();
        for _ in 0..5 {
            m.rest.error();
        }
        assert!(m.failures(&t)[0].contains("REST requests failed"));

        let mut m = passing();
        m.send_rate_achieved = 120.0;
        assert!(m.failures(&t)[0].contains("120 messages/s"));

        let mut m = passing();
        m.deliveries_received = 800;
        assert!(m.failures(&t)[0].contains("800 of 1000"));

        let mut m = passing();
        m.fanout = samples(&[400; 10]);
        assert!(m.failures(&t)[0].contains("fan-out p95"));

        let probe = |millis: f64, rate_limited: usize| FirstPaint {
            guilds: 60,
            millis,
            ready_bytes: 1000,
            requests: 3,
            rate_limited,
            failed: 0,
            ready_missing: false,
        };
        let mut m = passing();
        m.first_paint = Some(probe(900.0, 0));
        assert!(m.failures(&t).is_empty());
        m.first_paint = Some(probe(1800.0, 0));
        assert!(m.failures(&t)[0].contains("took 1800 ms"));
        m.first_paint = Some(probe(100.0, 2));
        assert!(m.failures(&t)[0].contains("2 of 3 requests failed (2 were 429)"));
        m.first_paint = Some(FirstPaint {
            ready_missing: true,
            ..probe(100.0, 0)
        });
        assert_eq!(
            m.failures(&t),
            vec!["first paint with 60 servers: no READY arrived".to_owned()]
        );
    }

    #[test]
    fn nothing_requested_counts_as_met() {
        let m = Measured::default();
        assert!(m.failures(&Targets::default()).is_empty());
    }
}
