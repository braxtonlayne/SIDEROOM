//! The playback state machine (spec section 4). Pure: time is always passed in, so every transition is
//! unit tested without a clock or a database.
//!
//! A [`Playback`] is `{playing, position_ms, rate, at_ms}` plus a `version`. The expected position of
//! the media at server time `t` is `position_ms + (t - at_ms) * rate` while playing, `position_ms`
//! while paused. Every command re-bases the state at "now" and bumps the version by one, so versions
//! order the states totally. A command that changes nothing (play while playing) leaves the version
//! alone, which keeps duplicate or racing requests from churning clients.

use crate::error::PartyError;

/// Slowest and fastest shared playback speed, in thousandths.
pub const MIN_RATE_MILLI: i32 = 500;
pub const MAX_RATE_MILLI: i32 = 2000;
/// An `ended` report is believed when the expected position is within this of the end.
pub const END_TOLERANCE_MS: i64 = 2_000;

/// What a controller (or the buffering rule) asks of the playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Play,
    Pause,
    /// Jump to an absolute position (clamped to the media).
    Seek(i64),
    /// Playback speed in thousandths.
    Rate(i32),
    /// The buffering rule paused everybody.
    AutoPause,
    /// The buffering rule resumes (nobody is stalled any more, or the wait ran out).
    AutoResume,
    /// A participant reports that the media ended.
    Ended,
}

/// Why a command was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateError {
    /// Live parties follow the live edge: no seeking, pausing or speed changes.
    Live,
    /// `Rate` outside [`MIN_RATE_MILLI`], [`MAX_RATE_MILLI`].
    BadRate,
    /// `Seek` below zero.
    BadPosition,
    /// `Ended` while the expected position is not near the end.
    NotAtEnd,
}

impl StateError {
    pub fn message(self) -> &'static str {
        match self {
            Self::Live => "A live party follows the live edge.",
            Self::BadRate => "Speed must be between 0.5x and 2x.",
            Self::BadPosition => "Position must not be negative.",
            Self::NotAtEnd => "The media has not ended.",
        }
    }
}

impl From<StateError> for PartyError {
    fn from(e: StateError) -> Self {
        Self::Invalid(e.message().to_owned())
    }
}

/// Server-authoritative playback state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Playback {
    pub playing: bool,
    pub position_ms: i64,
    pub rate_milli: i32,
    /// Server clock (unix ms) at which `position_ms` was true.
    pub at_ms: i64,
    pub version: i64,
    /// Paused by the buffering rule, not by a controller.
    pub auto_paused: bool,
    /// Length of the media; `None` when unknown.
    pub duration_ms: Option<i64>,
    pub live: bool,
}

/// Result of applying a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Applied {
    pub state: Playback,
    /// `false`: the command was a no-op and the version did not move.
    pub changed: bool,
}

impl Playback {
    /// A fresh paused state at `start_ms` (a live party starts playing).
    pub fn new(now_ms: i64, start_ms: i64, duration_ms: Option<i64>, live: bool) -> Self {
        Self {
            playing: live,
            position_ms: start_ms.max(0),
            rate_milli: 1000,
            at_ms: now_ms,
            version: 1,
            auto_paused: false,
            duration_ms,
            live,
        }
    }

    /// The position the media should be at on the server clock `now_ms`.
    pub fn position_at(&self, now_ms: i64) -> i64 {
        let mut pos = self.position_ms;
        if self.playing {
            let elapsed = i128::from((now_ms - self.at_ms).max(0));
            let advanced = elapsed * i128::from(self.rate_milli) / 1000;
            pos = i64::try_from(i128::from(pos) + advanced).unwrap_or(i64::MAX);
        }
        match self.duration_ms {
            Some(d) => pos.clamp(0, d),
            None => pos.max(0),
        }
    }

    /// Whether the expected position has reached the end of a known-length media.
    pub fn at_end(&self, now_ms: i64) -> bool {
        self.duration_ms
            .is_some_and(|d| self.position_at(now_ms) >= d)
    }

    fn rebased(&self, now_ms: i64) -> Self {
        Self {
            position_ms: self.position_at(now_ms),
            at_ms: now_ms,
            version: self.version + 1,
            ..*self
        }
    }

    /// Apply `cmd` at server time `now_ms`.
    pub fn apply(&self, cmd: Command, now_ms: i64) -> Result<Applied, StateError> {
        let unchanged = Applied {
            state: *self,
            changed: false,
        };
        let moved = |state: Playback| Applied {
            state,
            changed: true,
        };
        match cmd {
            Command::Play => {
                if self.live {
                    return Ok(unchanged);
                }
                if self.playing {
                    return Ok(unchanged);
                }
                let mut s = self.rebased(now_ms);
                // Playing from the very end restarts the media.
                if self.duration_ms.is_some_and(|d| s.position_ms >= d) {
                    s.position_ms = 0;
                }
                s.playing = true;
                s.auto_paused = false;
                Ok(moved(s))
            }
            Command::Pause => {
                if self.live {
                    return Err(StateError::Live);
                }
                if !self.playing && !self.auto_paused {
                    return Ok(unchanged);
                }
                let mut s = self.rebased(now_ms);
                s.playing = false;
                s.auto_paused = false;
                Ok(moved(s))
            }
            Command::Seek(to) => {
                if self.live {
                    return Err(StateError::Live);
                }
                if to < 0 {
                    return Err(StateError::BadPosition);
                }
                let mut s = self.rebased(now_ms);
                s.position_ms = match self.duration_ms {
                    Some(d) => to.min(d),
                    None => to,
                };
                Ok(moved(s))
            }
            Command::Rate(r) => {
                if self.live {
                    return Err(StateError::Live);
                }
                if !(MIN_RATE_MILLI..=MAX_RATE_MILLI).contains(&r) {
                    return Err(StateError::BadRate);
                }
                if r == self.rate_milli {
                    return Ok(unchanged);
                }
                let mut s = self.rebased(now_ms);
                s.rate_milli = r;
                Ok(moved(s))
            }
            Command::AutoPause => {
                if self.live || !self.playing {
                    return Ok(unchanged);
                }
                let mut s = self.rebased(now_ms);
                s.playing = false;
                s.auto_paused = true;
                Ok(moved(s))
            }
            Command::AutoResume => {
                if !self.auto_paused {
                    return Ok(unchanged);
                }
                let mut s = self.rebased(now_ms);
                s.playing = true;
                s.auto_paused = false;
                Ok(moved(s))
            }
            Command::Ended => {
                let Some(d) = self.duration_ms else {
                    return Err(StateError::NotAtEnd);
                };
                if self.live || self.position_at(now_ms) < d - END_TOLERANCE_MS {
                    return Err(StateError::NotAtEnd);
                }
                if !self.playing && self.position_ms >= d {
                    return Ok(unchanged);
                }
                let mut s = self.rebased(now_ms);
                s.position_ms = d;
                s.playing = false;
                s.auto_paused = false;
                Ok(moved(s))
            }
        }
    }

    /// Check an optimistic-concurrency token. `None` means "last writer wins".
    pub fn check_version(&self, expected: Option<i64>, now_ms: i64) -> Result<(), PartyError> {
        match expected {
            Some(v) if v != self.version => Err(PartyError::VersionConflict {
                current: Box::new(crate::model::PlaybackView::from_state(self, now_ms)),
            }),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_000_000;

    fn vod() -> Playback {
        Playback::new(T0, 0, Some(100_000), false)
    }

    fn ok(r: Result<Applied, StateError>) -> Playback {
        r.unwrap().state
    }

    #[test]
    fn paused_position_does_not_advance() {
        let s = vod();
        assert_eq!(s.position_at(T0 + 50_000), 0);
    }

    #[test]
    fn playing_advances_with_server_time_and_rate() {
        let s = ok(vod().apply(Command::Play, T0));
        assert!(s.playing);
        assert_eq!(s.position_at(T0 + 10_000), 10_000);
        let fast = ok(s.apply(Command::Rate(2000), T0 + 10_000));
        assert_eq!(fast.position_ms, 10_000);
        assert_eq!(fast.position_at(T0 + 15_000), 20_000);
        let slow = ok(fast.apply(Command::Rate(500), T0 + 15_000));
        assert_eq!(slow.position_at(T0 + 19_000), 22_000);
    }

    #[test]
    fn pause_freezes_the_expected_position_and_resume_continues_from_it() {
        let playing = ok(vod().apply(Command::Play, T0));
        let paused = ok(playing.apply(Command::Pause, T0 + 7_000));
        assert!(!paused.playing);
        assert_eq!(paused.position_ms, 7_000);
        assert_eq!(paused.position_at(T0 + 999_999), 7_000);
        let resumed = ok(paused.apply(Command::Play, T0 + 30_000));
        assert_eq!(resumed.position_at(T0 + 33_000), 10_000);
    }

    #[test]
    fn seek_while_playing_keeps_playing_from_the_target() {
        let s = ok(vod().apply(Command::Play, T0));
        let s = ok(s.apply(Command::Seek(60_000), T0 + 5_000));
        assert!(s.playing);
        assert_eq!(s.position_at(T0 + 8_000), 63_000);
    }

    #[test]
    fn seek_while_paused_stays_paused() {
        let s = ok(vod().apply(Command::Seek(40_000), T0));
        assert!(!s.playing);
        assert_eq!(s.position_at(T0 + 9_000), 40_000);
    }

    #[test]
    fn seek_clamps_to_the_duration_and_refuses_negative() {
        let s = ok(vod().apply(Command::Seek(10_000_000), T0));
        assert_eq!(s.position_ms, 100_000);
        assert_eq!(
            vod().apply(Command::Seek(-1), T0),
            Err(StateError::BadPosition)
        );
    }

    #[test]
    fn position_never_passes_the_end() {
        let s = ok(vod().apply(Command::Play, T0));
        assert_eq!(s.position_at(T0 + 500_000), 100_000);
        assert!(s.at_end(T0 + 500_000));
    }

    #[test]
    fn versions_increase_by_one_per_change_and_not_for_noops() {
        let s = vod();
        assert_eq!(s.version, 1);
        let s = ok(s.apply(Command::Play, T0));
        assert_eq!(s.version, 2);
        let again = s.apply(Command::Play, T0 + 1).unwrap();
        assert!(!again.changed);
        assert_eq!(again.state.version, 2);
        let s = ok(s.apply(Command::Seek(5), T0 + 2));
        assert_eq!(s.version, 3);
        let s = ok(s.apply(Command::Pause, T0 + 3));
        assert_eq!(s.version, 4);
        assert!(!s.apply(Command::Pause, T0 + 4).unwrap().changed);
    }

    #[test]
    fn rate_is_bounded_and_equal_rate_is_a_noop() {
        assert_eq!(
            vod().apply(Command::Rate(499), T0),
            Err(StateError::BadRate)
        );
        assert_eq!(
            vod().apply(Command::Rate(2001), T0),
            Err(StateError::BadRate)
        );
        assert!(!vod().apply(Command::Rate(1000), T0).unwrap().changed);
        assert!(vod().apply(Command::Rate(1500), T0).unwrap().changed);
    }

    #[test]
    fn play_from_the_end_restarts() {
        let end = ok(vod().apply(Command::Seek(100_000), T0));
        let s = ok(end.apply(Command::Play, T0 + 1));
        assert_eq!(s.position_ms, 0);
        assert!(s.playing);
    }

    #[test]
    fn auto_pause_and_resume_keep_the_position_continuous() {
        let s = ok(vod().apply(Command::Play, T0));
        let paused = ok(s.apply(Command::AutoPause, T0 + 4_000));
        assert!(paused.auto_paused && !paused.playing);
        assert_eq!(paused.position_at(T0 + 9_000), 4_000);
        let resumed = ok(paused.apply(Command::AutoResume, T0 + 9_000));
        assert!(resumed.playing && !resumed.auto_paused);
        assert_eq!(resumed.position_at(T0 + 10_000), 5_000);
    }

    #[test]
    fn auto_resume_only_undoes_an_auto_pause() {
        let manual = ok(vod().apply(Command::Pause, T0));
        assert!(!manual.apply(Command::AutoResume, T0 + 1).unwrap().changed);
        let playing = ok(vod().apply(Command::Play, T0));
        assert!(!playing.apply(Command::AutoResume, T0 + 1).unwrap().changed);
    }

    #[test]
    fn a_controller_pause_cancels_an_auto_pause() {
        let s = ok(vod().apply(Command::Play, T0));
        let paused = ok(s.apply(Command::AutoPause, T0 + 1));
        let manual = ok(paused.apply(Command::Pause, T0 + 2));
        assert!(!manual.auto_paused);
        assert!(!manual.apply(Command::AutoResume, T0 + 3).unwrap().changed);
    }

    #[test]
    fn a_controller_play_cancels_an_auto_pause() {
        let s = ok(vod().apply(Command::Play, T0));
        let paused = ok(s.apply(Command::AutoPause, T0 + 1));
        let played = ok(paused.apply(Command::Play, T0 + 2));
        assert!(played.playing && !played.auto_paused);
    }

    #[test]
    fn ended_needs_the_position_near_the_end() {
        let s = ok(vod().apply(Command::Play, T0));
        assert_eq!(
            s.apply(Command::Ended, T0 + 10_000),
            Err(StateError::NotAtEnd)
        );
        let done = ok(s.apply(Command::Ended, T0 + 99_000));
        assert!(!done.playing);
        assert_eq!(done.position_ms, 100_000);
        assert!(!done.apply(Command::Ended, T0 + 99_500).unwrap().changed);
    }

    #[test]
    fn live_parties_follow_the_live_edge() {
        let live = Playback::new(T0, 0, None, true);
        assert!(live.playing);
        assert_eq!(live.apply(Command::Seek(5), T0), Err(StateError::Live));
        assert_eq!(live.apply(Command::Pause, T0), Err(StateError::Live));
        assert_eq!(live.apply(Command::Rate(1500), T0), Err(StateError::Live));
        assert!(!live.apply(Command::Play, T0).unwrap().changed);
    }

    #[test]
    fn clock_going_backwards_does_not_move_the_position_back() {
        let s = ok(vod().apply(Command::Play, T0));
        assert_eq!(s.position_at(T0 - 5_000), 0);
        let p = ok(s.apply(Command::Pause, T0 - 5_000));
        assert_eq!(p.position_ms, 0);
    }

    #[test]
    fn unknown_duration_never_clamps_the_end() {
        let s = Playback::new(T0, 0, None, false);
        let s = ok(s.apply(Command::Play, T0));
        assert_eq!(s.position_at(T0 + 10_000_000), 10_000_000);
        assert!(!s.at_end(T0 + 10_000_000));
    }

    #[test]
    fn version_check_accepts_matching_and_missing_tokens_and_reports_conflicts() {
        let s = ok(vod().apply(Command::Play, T0));
        assert!(s.check_version(None, T0).is_ok());
        assert!(s.check_version(Some(2), T0).is_ok());
        match s.check_version(Some(1), T0) {
            Err(PartyError::VersionConflict { current }) => assert_eq!(current.version, 2),
            other => panic!("expected a conflict, got {other:?}"),
        }
    }

    #[test]
    fn racing_commands_resolve_by_arrival_order_and_versions_stay_monotonic() {
        // Two controllers act on the same base version: the second one is refused when it carries the
        // version it saw, and applied (last writer wins) when it carries none.
        let base = ok(vod().apply(Command::Play, T0));
        let first = ok(base.apply(Command::Seek(30_000), T0 + 10));
        assert!(first.version > base.version);
        assert!(first.check_version(Some(base.version), T0).is_err());
        let second = ok(first.apply(Command::Seek(70_000), T0 + 20));
        assert!(second.version > first.version);
        assert_eq!(second.position_ms, 70_000);
    }
}
