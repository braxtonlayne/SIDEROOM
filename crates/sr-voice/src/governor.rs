//! The egress governor (VOX-SFU-008, doc 08 section 3): a safety valve for the node's uplink.
//!
//! Perks are "up to" limits. A Level 3 server with 25 senders and 100 viewers asks for more
//! than a 1 Gbit port has, so the SFU watches its total egress against a budget
//! (`voice.egress_budget_mbps`, by default 70 % of the measured uplink) and, when it runs
//! over, caps the simulcast layers it hands out: **Level 0 first, Patrons last**. One heavy
//! server therefore cannot starve everyone else on the box.
//!
//! # Severity
//!
//! The governor has one number, the *severity* `0..=`[`MAX_SEVERITY`]. It rises one step
//! every [`GovernorConfig::step_interval`] while egress is over the budget (so high layers
//! come down **gradually**, not all at once) and falls one step per
//! [`GovernorConfig::release_hold`] while egress stays comfortably under it (so a brief lull
//! does not undo the cap). A tier's cap follows from the severity:
//!
//! | tier | rank | half layer from severity | thumbnail only from severity |
//! |---|---|---|---|
//! | server level 0 | 0 | 1 | 2 |
//! | server level 1 | 1 | 3 | 4 |
//! | server level 2 | 2 | 5 | 6 |
//! | server level 3 | 3 | 7 | 8 |
//! | Patron | 4 | 9 | 10 |
//!
//! New subscribers start under the current cap; existing ones are stepped down by the core
//! at the next opportunity. Whoever is affected gets a `QUALITY` message ("Reduced quality:
//! the room is busy") and another when the cap lifts.

use std::time::{Duration, Instant};

use sr_voice_proto::video::{Layer, Quality, QualityReason};

/// The highest severity.
pub const MAX_SEVERITY: u8 = 10;

/// Which allowance a subscriber is on. Higher ranks are capped later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Tier(pub u8);

impl Tier {
    pub const FREE: Tier = Tier(0);
    pub const PATRON: Tier = Tier(4);

    /// From a server level (`0..=3`) and Patron status. A Patron is always the top tier.
    pub fn of(guild_level: u8, patron: bool) -> Self {
        if patron {
            Self::PATRON
        } else {
            Tier(guild_level.min(3))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GovernorConfig {
    /// Egress budget in bits per second. `0` turns the governor off.
    pub budget_bps: f64,
    /// Engaged above this fraction of the budget.
    pub engage_at: f64,
    /// A calm period needs egress below this fraction.
    pub release_below: f64,
    /// Time between severity steps up.
    pub step_interval: Duration,
    /// Calm time before each step down.
    pub release_hold: Duration,
}

impl GovernorConfig {
    pub fn with_budget_mbps(mbps: f64) -> Self {
        Self {
            budget_bps: mbps * 1_000_000.0,
            ..Self::default()
        }
    }
}

impl Default for GovernorConfig {
    fn default() -> Self {
        Self {
            budget_bps: 0.0,
            engage_at: 1.0,
            release_below: 0.8,
            step_interval: Duration::from_secs(5),
            release_hold: Duration::from_secs(15),
        }
    }
}

/// The cap a tier gets at a severity.
pub fn cap_for(severity: u8, tier: Tier) -> Layer {
    let own = u16::from(tier.0) * 2;
    let s = u16::from(severity);
    if s <= own {
        Layer::High
    } else if s == own + 1 {
        Layer::Mid
    } else {
        Layer::Low
    }
}

/// The `QUALITY` message for a cap.
pub fn notice(cap: Layer) -> Quality {
    if cap == Layer::High {
        Quality {
            reduced: false,
            max_layer: Layer::High,
            reason: QualityReason::Normal,
        }
    } else {
        Quality {
            reduced: true,
            max_layer: cap,
            reason: QualityReason::RoomBusy,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EgressGovernor {
    cfg: GovernorConfig,
    severity: u8,
    last_step_up: Option<Instant>,
    calm_since: Option<Instant>,
}

impl EgressGovernor {
    pub fn new(cfg: GovernorConfig) -> Self {
        Self {
            cfg,
            severity: 0,
            last_step_up: None,
            calm_since: None,
        }
    }

    pub fn severity(&self) -> u8 {
        self.severity
    }

    pub fn engaged(&self) -> bool {
        self.severity > 0
    }

    pub fn cap(&self, tier: Tier) -> Layer {
        cap_for(self.severity, tier)
    }

    /// Feed the node's current egress. Returns the new severity when it changed.
    pub fn update(&mut self, now: Instant, egress_bps: f64) -> Option<u8> {
        if self.cfg.budget_bps <= 0.0 {
            return None;
        }
        let load = egress_bps / self.cfg.budget_bps;
        let before = self.severity;
        if load > self.cfg.engage_at {
            self.calm_since = None;
            let due = self
                .last_step_up
                .is_none_or(|t| now.saturating_duration_since(t) >= self.cfg.step_interval);
            if due && self.severity < MAX_SEVERITY {
                self.severity += 1;
                self.last_step_up = Some(now);
            }
        } else if load < self.cfg.release_below {
            let since = *self.calm_since.get_or_insert(now);
            if self.severity > 0 && now.saturating_duration_since(since) >= self.cfg.release_hold {
                self.severity -= 1;
                // Each step down needs its own calm period.
                self.calm_since = Some(now);
            }
            if self.severity == 0 {
                self.last_step_up = None;
            }
        } else {
            // Between the two marks: hold where we are.
            self.calm_since = None;
        }
        (self.severity != before).then_some(self.severity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gov() -> (EgressGovernor, Instant) {
        (
            EgressGovernor::new(GovernorConfig::with_budget_mbps(100.0)),
            Instant::now(),
        )
    }

    const MBPS: f64 = 1_000_000.0;

    #[test]
    fn lower_tiers_are_capped_first_and_patrons_last() {
        // Severity 0: nobody is capped.
        for t in 0..=4 {
            assert_eq!(cap_for(0, Tier(t)), Layer::High);
        }
        // 1: free tier drops to the half layer, everyone else is untouched.
        assert_eq!(cap_for(1, Tier(0)), Layer::Mid);
        assert_eq!(cap_for(1, Tier(1)), Layer::High);
        // 2: free tier thumbnails.
        assert_eq!(cap_for(2, Tier(0)), Layer::Low);
        assert_eq!(cap_for(2, Tier(1)), Layer::High);
        // 3 and 4: level 1.
        assert_eq!(cap_for(3, Tier(1)), Layer::Mid);
        assert_eq!(cap_for(4, Tier(1)), Layer::Low);
        // The ranking is monotone at every severity.
        for s in 0..=MAX_SEVERITY {
            for t in 0..4 {
                assert!(
                    cap_for(s, Tier(t)) <= cap_for(s, Tier(t + 1)),
                    "s={s} t={t}"
                );
            }
        }
        // Patrons are the last to go and keep the half layer at 9.
        assert_eq!(cap_for(8, Tier::PATRON), Layer::High);
        assert_eq!(cap_for(9, Tier::PATRON), Layer::Mid);
        assert_eq!(cap_for(MAX_SEVERITY, Tier::PATRON), Layer::Low);
    }

    #[test]
    fn tiers_come_from_level_and_patron_status() {
        assert_eq!(Tier::of(0, false), Tier::FREE);
        assert_eq!(Tier::of(3, false), Tier(3));
        assert_eq!(Tier::of(9, false), Tier(3), "clamped");
        assert_eq!(Tier::of(0, true), Tier::PATRON);
        assert_eq!(Tier::of(2, true), Tier::PATRON);
    }

    #[test]
    fn it_steps_down_gradually_while_over_budget() {
        let (mut g, t) = gov();
        assert_eq!(g.update(t, 90.0 * MBPS), None, "under the budget");
        assert!(!g.engaged());
        // Over budget: one step now, then one per 5 s, not all at once.
        assert_eq!(g.update(t, 120.0 * MBPS), Some(1));
        assert_eq!(g.update(t + Duration::from_secs(2), 120.0 * MBPS), None);
        assert_eq!(g.update(t + Duration::from_secs(5), 120.0 * MBPS), Some(2));
        assert_eq!(g.cap(Tier::FREE), Layer::Low);
        assert_eq!(g.cap(Tier(1)), Layer::High);
        assert_eq!(g.update(t + Duration::from_secs(10), 120.0 * MBPS), Some(3));
        assert_eq!(g.cap(Tier(1)), Layer::Mid);
    }

    #[test]
    fn severity_never_passes_the_maximum() {
        let (mut g, t) = gov();
        for i in 0..40 {
            g.update(t + Duration::from_secs(5 * i), 300.0 * MBPS);
        }
        assert_eq!(g.severity(), MAX_SEVERITY);
        assert_eq!(g.update(t + Duration::from_secs(500), 300.0 * MBPS), None);
    }

    #[test]
    fn it_releases_slowly_after_a_calm_period() {
        let (mut g, t) = gov();
        for i in 0..3 {
            g.update(t + Duration::from_secs(5 * i), 130.0 * MBPS);
        }
        assert_eq!(g.severity(), 3);
        let calm = t + Duration::from_secs(20);
        // Calm starts here; nothing changes for the hold period.
        assert_eq!(g.update(calm, 50.0 * MBPS), None);
        assert_eq!(g.update(calm + Duration::from_secs(14), 50.0 * MBPS), None);
        assert_eq!(
            g.update(calm + Duration::from_secs(15), 50.0 * MBPS),
            Some(2)
        );
        // Each further step needs its own calm period.
        assert_eq!(g.update(calm + Duration::from_secs(20), 50.0 * MBPS), None);
        assert_eq!(
            g.update(calm + Duration::from_secs(30), 50.0 * MBPS),
            Some(1)
        );
        assert_eq!(
            g.update(calm + Duration::from_secs(45), 50.0 * MBPS),
            Some(0)
        );
        assert!(!g.engaged());
    }

    #[test]
    fn the_band_between_the_marks_holds_the_cap() {
        let (mut g, t) = gov();
        g.update(t, 130.0 * MBPS);
        assert_eq!(g.severity(), 1);
        // 90 % is neither over budget nor calm: no step either way, and no calm clock runs.
        assert_eq!(g.update(t + Duration::from_secs(60), 90.0 * MBPS), None);
        assert_eq!(g.update(t + Duration::from_secs(120), 90.0 * MBPS), None);
        assert_eq!(g.severity(), 1);
        // Calm only counts from the first calm reading.
        assert_eq!(g.update(t + Duration::from_secs(121), 70.0 * MBPS), None);
        assert_eq!(g.update(t + Duration::from_secs(136), 70.0 * MBPS), Some(0));
    }

    #[test]
    fn a_spike_during_calm_restarts_the_clock() {
        let (mut g, t) = gov();
        g.update(t, 130.0 * MBPS);
        g.update(t + Duration::from_secs(10), 50.0 * MBPS);
        // Over budget again before the hold ends: steps up instead of releasing.
        assert_eq!(g.update(t + Duration::from_secs(20), 130.0 * MBPS), Some(2));
        assert_eq!(g.update(t + Duration::from_secs(24), 50.0 * MBPS), None);
        assert_eq!(g.update(t + Duration::from_secs(30), 50.0 * MBPS), None);
        assert_eq!(g.update(t + Duration::from_secs(39), 50.0 * MBPS), Some(1));
    }

    #[test]
    fn a_zero_budget_turns_it_off() {
        let mut g = EgressGovernor::new(GovernorConfig::default());
        assert_eq!(g.update(Instant::now(), 1e12), None);
        assert_eq!(g.severity(), 0);
    }

    #[test]
    fn notices_say_reduced_or_normal() {
        let n = notice(Layer::Mid);
        assert!(n.reduced);
        assert_eq!(n.max_layer, Layer::Mid);
        assert_eq!(n.reason, QualityReason::RoomBusy);
        let ok = notice(Layer::High);
        assert!(!ok.reduced);
        assert_eq!(ok.reason, QualityReason::Normal);
    }
}
