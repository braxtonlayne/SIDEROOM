//! A loss-based bandwidth fallback for layer selection (VOX-SFU-005).
//!
//! The preferred signal is str0m's TWCC estimate (`SfuConfig::video_bwe`). It has a cost,
//! though: with the estimator on, str0m pads every sending connection to 50 kbit/s to keep
//! the estimate alive, audio-only participants included. For a node that mostly carries
//! voice that is a lot of wasted egress, so the estimator is off by default and this
//! fallback stands in: it turns the loss the subscriber reports (RTCP receiver reports, once
//! a second) into a bitrate **budget** for the layer selector.
//!
//! * Loss at or above [`BACK_OFF_LOSS`] cuts the budget to [`BACK_OFF`] of what is being
//!   sent now (never below [`FLOOR_BPS`]).
//! * Sustained quiet (below [`CALM_LOSS`]) for [`PROBE_EVERY`] raises the budget by
//!   [`PROBE_GAIN`]; once it passes [`UNLIMITED_BPS`] the budget is dropped and the
//!   subscriber is limited by its hint only.
//! * Anything in between holds the budget.
//!
//! It is deliberately slow and coarse; it only has to stop an obviously congested
//! subscriber from being fed the full layer.

use std::time::{Duration, Instant};

/// Loss fraction that triggers a back-off.
pub const BACK_OFF_LOSS: f32 = 0.10;
/// Loss fraction below which the link counts as healthy.
pub const CALM_LOSS: f32 = 0.03;
/// Multiplier applied to the current send rate on a back-off.
pub const BACK_OFF: f64 = 0.7;
/// The budget never falls below this.
pub const FLOOR_BPS: f64 = 150_000.0;
/// How often a healthy link may raise the budget.
pub const PROBE_EVERY: Duration = Duration::from_secs(8);
/// Multiplier per raise.
pub const PROBE_GAIN: f64 = 1.3;
/// Above this the budget is dropped.
pub const UNLIMITED_BPS: f64 = 6_000_000.0;

#[derive(Debug, Clone, Copy, Default)]
pub struct LossBudget {
    budget: Option<f64>,
    last_change: Option<Instant>,
}

impl LossBudget {
    /// The current budget in bits per second, if loss has limited this subscriber.
    pub fn budget(&self) -> Option<f64> {
        self.budget
    }

    /// Feed one receiver report: `loss` is the fraction lost, `sending_bps` what is being
    /// sent to this subscriber now.
    pub fn on_report(&mut self, now: Instant, loss: f32, sending_bps: f64) {
        if loss >= BACK_OFF_LOSS {
            let cut = (sending_bps * BACK_OFF).max(FLOOR_BPS);
            // Never raise on a bad report, and keep the lower of two consecutive cuts.
            self.budget = Some(self.budget.map_or(cut, |b| b.min(cut)));
            self.last_change = Some(now);
            return;
        }
        let Some(b) = self.budget else { return };
        if loss > CALM_LOSS {
            // Hold, and do not let a mediocre second count towards the calm period.
            self.last_change = Some(now);
            return;
        }
        let since = self.last_change.get_or_insert(now);
        if now.saturating_duration_since(*since) >= PROBE_EVERY {
            let raised = b * PROBE_GAIN;
            self.budget = (raised <= UNLIMITED_BPS).then_some(raised);
            self.last_change = Some(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn a_healthy_link_has_no_budget() {
        let mut l = LossBudget::default();
        let t = Instant::now();
        for i in 0..30 {
            l.on_report(t + s(i), 0.0, 2_000_000.0);
        }
        assert_eq!(l.budget(), None);
    }

    #[test]
    fn heavy_loss_cuts_the_budget_below_what_was_being_sent() {
        let mut l = LossBudget::default();
        let t = Instant::now();
        l.on_report(t, 0.15, 2_000_000.0);
        assert_eq!(l.budget(), Some(1_400_000.0));
        // More loss while still sending a lot cuts again; a quieter send rate cuts lower.
        l.on_report(t + s(1), 0.2, 1_000_000.0);
        assert_eq!(l.budget(), Some(700_000.0));
        // It never goes below the floor.
        l.on_report(t + s(2), 0.5, 10_000.0);
        assert_eq!(l.budget(), Some(FLOOR_BPS));
    }

    #[test]
    fn the_budget_recovers_slowly_when_the_link_is_calm() {
        let mut l = LossBudget::default();
        let t = Instant::now();
        l.on_report(t, 0.2, 1_000_000.0);
        assert_eq!(l.budget(), Some(700_000.0));
        // Calm, but not for long enough yet.
        l.on_report(t + s(4), 0.0, 700_000.0);
        assert_eq!(l.budget(), Some(700_000.0));
        l.on_report(t + s(9), 0.0, 700_000.0);
        assert_eq!(l.budget(), Some(910_000.0));
        // Eventually the limit disappears.
        for i in 1..40 {
            l.on_report(t + s(9 + 9 * i), 0.0, 900_000.0);
        }
        assert_eq!(l.budget(), None);
    }

    #[test]
    fn mediocre_loss_holds_the_budget_and_restarts_the_calm_clock() {
        let mut l = LossBudget::default();
        let t = Instant::now();
        l.on_report(t, 0.2, 1_000_000.0);
        // 5 % loss is neither a back-off nor calm.
        l.on_report(t + s(7), 0.05, 700_000.0);
        l.on_report(t + s(9), 0.0, 700_000.0);
        assert_eq!(
            l.budget(),
            Some(700_000.0),
            "only 2 s of calm since the bad report"
        );
        l.on_report(t + s(16), 0.0, 700_000.0);
        assert_eq!(l.budget(), Some(910_000.0));
    }
}
