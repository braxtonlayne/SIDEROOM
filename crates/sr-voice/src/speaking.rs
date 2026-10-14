//! Speaking detection from the RTP audio-level header extension (RFC 6464).
//!
//! The extension is a plain header value, so this works under end-to-end encryption
//! (the SFU never decodes audio). Levels are negative dBov: 0 is the loudest, a normal
//! voice sits around -30, room noise below -50.

use std::time::{Duration, Instant};

/// Packets at or above this level (dBov) count as speech when the sender gives no voice-activity bit.
pub const ACTIVE_LEVEL_DBOV: i8 = -50;
/// How long after the last active packet a speaker stays "speaking". Bridges the gaps
/// between words and DTX (comfort-noise) intervals so the indicator does not flicker.
pub const HOLD: Duration = Duration::from_millis(250);

#[derive(Debug, Default, Clone)]
pub struct SpeakingDetector {
    speaking: bool,
    last_active: Option<Instant>,
    last_level: Option<i8>,
}

impl SpeakingDetector {
    pub fn is_speaking(&self) -> bool {
        self.speaking
    }

    /// Last level seen on an active packet.
    pub fn level(&self) -> Option<i8> {
        self.last_level
    }

    /// Feed one audio packet's header values. Returns the new state on a transition.
    pub fn on_packet(
        &mut self,
        now: Instant,
        level: Option<i8>,
        voice_activity: Option<bool>,
    ) -> Option<bool> {
        let active = match (voice_activity, level) {
            (Some(false), _) => false,
            (_, Some(l)) => l >= ACTIVE_LEVEL_DBOV,
            (Some(true), None) => true,
            (None, None) => false,
        };
        if !active {
            return None;
        }
        self.last_active = Some(now);
        self.last_level = level;
        if self.speaking {
            None
        } else {
            self.speaking = true;
            Some(true)
        }
    }

    /// Advance time. Returns `Some(false)` when the hold period ran out.
    pub fn on_tick(&mut self, now: Instant) -> Option<bool> {
        match self.last_active {
            Some(t) if self.speaking && now.saturating_duration_since(t) > HOLD => {
                self.speaking = false;
                Some(false)
            }
            _ => None,
        }
    }

    /// When [`on_tick`](Self::on_tick) next needs to run.
    pub fn deadline(&self) -> Option<Instant> {
        self.speaking
            .then(|| {
                self.last_active
                    .map(|t| t + HOLD + Duration::from_millis(1))
            })
            .flatten()
    }

    /// Forget everything (mute, leave). Returns whether the user was speaking.
    pub fn reset(&mut self) -> bool {
        let was = self.speaking;
        *self = Self::default();
        was
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loud_packets_start_speech_and_silence_ends_it() {
        let t0 = Instant::now();
        let mut d = SpeakingDetector::default();
        assert_eq!(d.on_packet(t0, Some(-30), None), Some(true));
        assert_eq!(
            d.on_packet(t0 + Duration::from_millis(20), Some(-28), None),
            None
        );
        assert!(d.is_speaking());
        assert_eq!(d.level(), Some(-28));
        // Within the hold period nothing changes.
        assert_eq!(d.on_tick(t0 + Duration::from_millis(200)), None);
        // After it, the speaker is silent exactly once.
        assert_eq!(d.on_tick(t0 + Duration::from_millis(400)), Some(false));
        assert_eq!(d.on_tick(t0 + Duration::from_millis(800)), None);
        assert!(!d.is_speaking());
        assert_eq!(d.deadline(), None);
    }

    #[test]
    fn quiet_packets_and_a_cleared_vad_bit_are_not_speech() {
        let t0 = Instant::now();
        let mut d = SpeakingDetector::default();
        assert_eq!(d.on_packet(t0, Some(-70), None), None);
        assert_eq!(d.on_packet(t0, Some(-10), Some(false)), None);
        assert_eq!(d.on_packet(t0, None, None), None);
        assert!(!d.is_speaking());
        // The VAD bit alone is enough when there is no level.
        assert_eq!(d.on_packet(t0, None, Some(true)), Some(true));
    }

    #[test]
    fn dtx_gaps_inside_the_hold_do_not_flicker() {
        let t0 = Instant::now();
        let mut d = SpeakingDetector::default();
        d.on_packet(t0, Some(-35), Some(true));
        // A comfort-noise packet in the gap is quiet and neither extends nor ends speech.
        assert_eq!(
            d.on_packet(t0 + Duration::from_millis(100), Some(-80), Some(false)),
            None
        );
        assert_eq!(d.on_tick(t0 + Duration::from_millis(200)), None);
        assert_eq!(
            d.on_packet(t0 + Duration::from_millis(220), Some(-33), Some(true)),
            None
        );
        assert!(d.is_speaking());
    }

    #[test]
    fn reset_reports_whether_it_was_speaking() {
        let t0 = Instant::now();
        let mut d = SpeakingDetector::default();
        assert!(!d.reset());
        d.on_packet(t0, Some(-20), None);
        assert!(d.reset());
        assert!(!d.is_speaking());
    }
}
