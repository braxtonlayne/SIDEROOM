//! Vouch tiers and the accountability multiplier (MOD-VCH-001). Pure: no database, no clock.
//!
//! The tiers are fixed in code, like the supporter tiers, so the tooltip and the engine can never disagree: a
//! member with 0 to 49 vouches is a Citizen, 50 to 499 a Pillar, 500 or more an Elder. The multiplier works in
//! the *reverse* direction of privilege: the more people vouched for you, the heavier an automated sanction
//! lands when you break a rule (1.5x Pillar, 3.0x Elder). It scales automated timeouts, hold lengths and strike
//! weights upward only; it never lowers anything below the base value and never touches staff decisions.

/// First vouch count of the Pillar tier.
pub const PILLAR_MIN: i64 = 50;
/// First vouch count of the Elder tier.
pub const ELDER_MIN: i64 = 500;
/// Multiplier of the Pillar tier.
pub const PILLAR_MULTIPLIER: f32 = 1.5;
/// Multiplier of the Elder tier.
pub const ELDER_MULTIPLIER: f32 = 3.0;
/// The longest a timeout can be, scaled or not (28 days, as on Discord). A scaled timeout is clamped to it.
pub const MAX_TIMEOUT_SECS: u64 = 28 * 24 * 3600;

/// The vouch tier of a member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    /// 0 to 49 vouches.
    Citizen,
    /// 50 to 499 vouches.
    Pillar,
    /// 500 or more vouches (Elder, high trust).
    Elder,
}

impl Tier {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Citizen => "citizen",
            Self::Pillar => "pillar",
            Self::Elder => "elder",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "citizen" => Some(Self::Citizen),
            "pillar" => Some(Self::Pillar),
            "elder" => Some(Self::Elder),
            _ => None,
        }
    }

    /// The accountability multiplier of the tier: 1.0, 1.5 or 3.0.
    pub const fn multiplier(self) -> f32 {
        match self {
            Self::Citizen => 1.0,
            Self::Pillar => PILLAR_MULTIPLIER,
            Self::Elder => ELDER_MULTIPLIER,
        }
    }
}

/// The tier of a vouch count (a negative count is a Citizen).
pub const fn tier_for(count: i64) -> Tier {
    if count >= ELDER_MIN {
        Tier::Elder
    } else if count >= PILLAR_MIN {
        Tier::Pillar
    } else {
        Tier::Citizen
    }
}

/// The accountability multiplier of a vouch count.
pub const fn multiplier_for(count: i64) -> f32 {
    tier_for(count).multiplier()
}

/// A timeout length scaled by `multiplier`, rounded to the second and clamped to [`MAX_TIMEOUT_SECS`]. A
/// multiplier below 1.0 (or not a number) is treated as 1.0: this only ever scales upward. Never shorter than
/// the base (also clamped, so a base already past the maximum stays at the maximum).
pub fn scale_timeout_secs(base_secs: u64, multiplier: f32) -> u64 {
    let m = sane(multiplier);
    let scaled = (base_secs as f64 * f64::from(m)).round();
    let scaled = if scaled >= MAX_TIMEOUT_SECS as f64 {
        MAX_TIMEOUT_SECS
    } else {
        scaled as u64
    };
    scaled.max(base_secs.min(MAX_TIMEOUT_SECS))
}

/// A hold length (days, or any whole unit) scaled by `multiplier`, rounded up so a fraction never rounds the
/// penalty back down. Upward only.
pub fn scale_hold(base: u64, multiplier: f32) -> u64 {
    let m = sane(multiplier);
    let scaled = (base as f64 * f64::from(m)).ceil();
    if scaled >= u64::MAX as f64 {
        u64::MAX
    } else {
        (scaled as u64).max(base)
    }
}

/// A strike weight scaled by `multiplier`. Upward only; a non-finite or negative weight is returned unchanged.
pub fn scale_weight(base: f32, multiplier: f32) -> f32 {
    if !base.is_finite() || base < 0.0 {
        return base;
    }
    base * sane(multiplier)
}

fn sane(m: f32) -> f32 {
    if m.is_finite() && m > 1.0 { m } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tiers_switch_exactly_at_the_boundaries() {
        assert_eq!(tier_for(-3), Tier::Citizen);
        assert_eq!(tier_for(0), Tier::Citizen);
        assert_eq!(tier_for(49), Tier::Citizen);
        assert_eq!(tier_for(50), Tier::Pillar);
        assert_eq!(tier_for(499), Tier::Pillar);
        assert_eq!(tier_for(500), Tier::Elder);
        assert_eq!(tier_for(100_000), Tier::Elder);
    }

    #[test]
    fn the_multipliers_are_one_one_and_a_half_and_three() {
        assert_eq!(multiplier_for(49), 1.0);
        assert_eq!(multiplier_for(50), 1.5);
        assert_eq!(multiplier_for(499), 1.5);
        assert_eq!(multiplier_for(500), 3.0);
    }

    #[test]
    fn tier_names_round_trip() {
        for t in [Tier::Citizen, Tier::Pillar, Tier::Elder] {
            assert_eq!(Tier::parse(t.as_str()), Some(t));
        }
        assert_eq!(Tier::parse("king"), None);
    }

    #[test]
    fn timeouts_scale_up_and_clamp_to_28_days() {
        assert_eq!(scale_timeout_secs(600, 1.0), 600);
        assert_eq!(scale_timeout_secs(600, 1.5), 900);
        assert_eq!(scale_timeout_secs(600, 3.0), 1800);
        assert_eq!(scale_timeout_secs(21 * 24 * 3600, 3.0), MAX_TIMEOUT_SECS);
        assert_eq!(scale_timeout_secs(1, 1.5), 2); // 1.5 rounds half up
    }

    #[test]
    fn nothing_scales_downward() {
        assert_eq!(scale_timeout_secs(600, 0.5), 600);
        assert_eq!(scale_timeout_secs(600, f32::NAN), 600);
        assert_eq!(scale_timeout_secs(600, -2.0), 600);
        assert_eq!(scale_hold(7, 0.1), 7);
        assert_eq!(scale_weight(2.0, 0.0), 2.0);
        // A base already past the maximum is clamped, not lengthened.
        assert_eq!(
            scale_timeout_secs(MAX_TIMEOUT_SECS + 5, 3.0),
            MAX_TIMEOUT_SECS
        );
    }

    #[test]
    fn holds_round_up_and_weights_multiply() {
        assert_eq!(scale_hold(3, 1.5), 5); // 4.5 rounds up
        assert_eq!(scale_hold(10, 3.0), 30);
        assert_eq!(scale_weight(2.0, 1.5), 3.0);
        assert_eq!(scale_weight(1.0, 3.0), 3.0);
        assert_eq!(scale_weight(-1.0, 3.0), -1.0);
    }
}
