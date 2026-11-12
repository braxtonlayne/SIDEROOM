//! Per-track ingress caps (SEC-AUD-142, SEC-AUD-143, D-631).
//!
//! The SFU never decodes, so nothing used to stop a modified client from sending a track at any rate:
//! every listener's downlink and the node's egress paid for it. Each (participant, source) now has a
//! byte token bucket (the shape of [`crate::datachannel::Budget`]): a rate and one second's burst. A
//! frame that does not fit is dropped for everyone (not queued), nobody is disconnected and nothing
//! is announced; the sender only loses its own excess.
//!
//! The caps are fixed constants and sit above anything an unmodified SIDE ROOM client sends at any
//! setting. The rates the client can produce are media payload; headers, the E2EE frame overhead and
//! retransmissions come on top, which is what the headroom is for:
//!
//! | track | highest client rate | source | cap |
//! |---|---|---|---|
//! | microphone, screen audio | 384 kbit/s (`BITRATE_MAX`, `sr-chat/src/channel_rules.rs`) | `engine.ts` `applyBitrate` | 640 kbit/s |
//! | music | the room's `music_bitrate_kbps`, 32..=320 | `engine.ts` `applyBitrate` | that x 1.5 + 64 kbit/s |
//! | screen video | 7 Mbit/s (`screenBitrate`: 5000 x 1.4 above 720 lines and 30 fps) | `engine.ts` | 14 Mbit/s |
//! | camera video | 8.12 Mbit/s: 1080p60 simulcast, 4900 + 2380 + 840 kbit/s (`simulcastEncodings`, `video.ts`) | `video.ts` | 16.24 Mbit/s |

use std::time::{Duration, Instant};

/// Microphone and screen audio: `BITRATE_MAX` (384) x 1.5 + 64.
pub const AUDIO_CAP_KBPS: u32 = 640;
/// A screen share's video: twice the client's highest (7 Mbit/s).
pub const SCREEN_CAP_KBPS: u32 = 14_000;
/// A camera's video, all simulcast layers together: twice the client's highest (8.12 Mbit/s).
pub const CAMERA_CAP_KBPS: u32 = 16_240;
/// The bucket always holds at least this much, so one large keyframe fits a full bucket.
pub const VIDEO_BURST_FLOOR: usize = 512 * 1024;
/// The bucket always holds at least this much, so one audio frame fits a full bucket.
pub const AUDIO_BURST_FLOOR: usize = 2048;
/// At most one warning per participant and track in this time.
pub const WARN_EVERY: Duration = Duration::from_secs(60);

/// The cap for a music track in a room whose music bitrate is `kbps`.
pub fn music_cap_kbps(kbps: u16) -> u32 {
    u32::from(kbps) * 3 / 2 + 64
}

/// What to do with one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Forward it.
    Pass,
    /// Forward it; an earlier frame of this layer was dropped, so ask for a keyframe.
    Resumed,
    /// Drop it. `warn` is true at most once per [`WARN_EVERY`].
    Drop { warn: bool },
}

/// A token bucket in bytes: refills at the cap, holds one second's worth (at least `floor`).
#[derive(Debug, Clone)]
pub struct Ingress {
    rate_bps: f64,
    floor: usize,
    capacity: f64,
    tokens: f64,
    last: Option<Instant>,
    /// Layers (a bit per layer index) that lost a frame and have not been forwarded since.
    lost: u8,
    last_warn: Option<Instant>,
}

impl Ingress {
    pub fn new(kbps: u32, floor: usize) -> Self {
        let rate_bps = f64::from(kbps) * 1000.0 / 8.0;
        let capacity = rate_bps.max(floor as f64);
        Ingress {
            rate_bps,
            floor,
            capacity,
            tokens: capacity,
            last: None,
            lost: 0,
            last_warn: None,
        }
    }

    pub fn audio(kbps: u32) -> Self {
        Self::new(kbps, AUDIO_BURST_FLOOR)
    }

    pub fn video(kbps: u32) -> Self {
        Self::new(kbps, VIDEO_BURST_FLOOR)
    }

    /// Change the cap (the room's music bitrate changed); the tokens already held stay.
    pub fn set_kbps(&mut self, kbps: u32) {
        self.rate_bps = f64::from(kbps) * 1000.0 / 8.0;
        self.capacity = self.rate_bps.max(self.floor as f64);
        self.tokens = self.tokens.min(self.capacity);
    }

    pub fn rate_kbps(&self) -> u32 {
        (self.rate_bps * 8.0 / 1000.0).round() as u32
    }

    /// Take a frame of `bytes` on `layer` (0 for audio) from the bucket.
    pub fn take(&mut self, now: Instant, layer: usize, bytes: usize) -> Verdict {
        if let Some(last) = self.last {
            let dt = now.saturating_duration_since(last).as_secs_f64();
            self.tokens = (self.tokens + dt * self.rate_bps).min(self.capacity);
        }
        self.last = Some(now);
        let bit = 1u8 << layer.min(7);
        let n = bytes as f64;
        if n > self.tokens {
            self.lost |= bit;
            let warn = self
                .last_warn
                .is_none_or(|w| now.saturating_duration_since(w) >= WARN_EVERY);
            if warn {
                self.last_warn = Some(now);
            }
            return Verdict::Drop { warn };
        }
        self.tokens -= n;
        if self.lost & bit != 0 {
            self.lost &= !bit;
            return Verdict::Resumed;
        }
        Verdict::Pass
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Frames of `frame` bytes every `every_ms`, for `secs`: how many were dropped.
    fn dropped(cap: u32, frame: usize, every_ms: u64, secs: u64) -> (u64, u64) {
        let mut b = Ingress::audio(cap);
        let t0 = Instant::now();
        let (mut ok, mut lost) = (0, 0);
        for i in 0..secs * 1000 / every_ms {
            match b.take(t0 + Duration::from_millis(i * every_ms), 0, frame) {
                Verdict::Drop { .. } => lost += 1,
                _ => ok += 1,
            }
        }
        (ok, lost)
    }

    #[test]
    fn a_track_within_its_cap_loses_nothing() {
        // A 20 ms Opus frame at 384 kbit/s is 960 bytes; at 320 kbit/s (music) 800 bytes.
        assert_eq!(dropped(AUDIO_CAP_KBPS, 960, 20, 10).1, 0);
        assert_eq!(dropped(music_cap_kbps(320), 800, 20, 10).1, 0);
        // Even a quarter more than the maximum (headers and overhead on top).
        assert_eq!(dropped(AUDIO_CAP_KBPS, 1200, 20, 10).1, 0);
    }

    #[test]
    fn a_track_over_its_cap_is_dropped_to_the_cap() {
        // 3x the 256 kbit/s music setting: the cap is 448 kbit/s = 56 000 B/s plus a 56 000 B burst.
        let cap = music_cap_kbps(256);
        assert_eq!(cap, 448);
        // 768 kbit/s = 1920 B per 20 ms frame, for 10 s.
        let (ok, lost) = dropped(cap, 1920, 20, 10);
        assert!(lost > 0);
        let passed = ok * 1920;
        assert!(passed <= 56_000 * 10 + 56_000, "passed {passed} bytes");
        assert!(passed >= 56_000 * 10 * 9 / 10, "passed {passed} bytes");
    }

    #[test]
    fn the_bucket_has_a_one_second_burst_and_refills() {
        let t0 = Instant::now();
        let mut b = Ingress::audio(80); // 10 000 B/s
        assert_eq!(b.take(t0, 0, 10_000), Verdict::Pass);
        assert!(matches!(b.take(t0, 0, 1), Verdict::Drop { .. }));
        // Half a second refills 5000 bytes.
        let t1 = t0 + Duration::from_millis(500);
        assert!(matches!(b.take(t1, 0, 5001), Verdict::Drop { .. }));
        assert_eq!(b.take(t1, 0, 5000), Verdict::Resumed);
        // A long pause never banks more than one second.
        let t2 = t1 + Duration::from_secs(60);
        assert_eq!(b.take(t2, 0, 10_000), Verdict::Pass);
        assert!(matches!(b.take(t2, 0, 1), Verdict::Drop { .. }));
    }

    #[test]
    fn a_keyframe_always_fits_a_full_bucket() {
        // A cap far below one keyframe still holds a keyframe-sized burst.
        let mut b = Ingress::video(100);
        assert_eq!(b.take(Instant::now(), 0, VIDEO_BURST_FLOOR), Verdict::Pass);
        // And the real caps hold far more than the floor.
        assert!(Ingress::video(SCREEN_CAP_KBPS).capacity >= VIDEO_BURST_FLOOR as f64);
        assert!(Ingress::video(CAMERA_CAP_KBPS).capacity >= VIDEO_BURST_FLOOR as f64);
    }

    #[test]
    fn a_screen_share_at_the_clients_maximum_loses_nothing() {
        // 7 Mbit/s at 60 fps with a 600 KB keyframe every 3 seconds (a generous one).
        let mut b = Ingress::video(SCREEN_CAP_KBPS);
        let t0 = Instant::now();
        let mut lost = 0;
        for i in 0..600u64 {
            let size = if i % 180 == 0 {
                600_000
            } else {
                7_000_000 / 8 / 60
            };
            if matches!(
                b.take(t0 + Duration::from_micros(i * 16_667), 0, size),
                Verdict::Drop { .. }
            ) {
                lost += 1;
            }
        }
        assert_eq!(lost, 0);
    }

    #[test]
    fn simulcast_layers_share_one_bucket() {
        // The camera's three layers at the highest ladder sum to 8.12 Mbit/s: fine. At 3x that the
        // layers together run out although the largest alone (3x 4.9 = 14.7) is near the cap.
        let run_layers = |factor: f64| {
            let mut b = Ingress::video(CAMERA_CAP_KBPS);
            let t0 = Instant::now();
            let mut lost = 0;
            for i in 0..600u64 {
                let now = t0 + Duration::from_micros(i * 16_667);
                for (l, kbps) in [(0usize, 4900.0), (1, 2380.0), (2, 840.0)] {
                    let size = (kbps * factor * 1000.0 / 8.0 / 60.0) as usize;
                    if matches!(b.take(now, l, size), Verdict::Drop { .. }) {
                        lost += 1;
                    }
                }
            }
            lost
        };
        assert_eq!(run_layers(1.0), 0);
        assert!(run_layers(3.0) > 0);
    }

    #[test]
    fn the_warning_is_rate_limited() {
        let t0 = Instant::now();
        let mut b = Ingress::audio(8);
        assert_eq!(b.take(t0, 0, 1000), Verdict::Pass);
        let warns = (0..100u64)
            .filter(|i| {
                matches!(
                    b.take(t0 + Duration::from_millis(*i), 0, 3000),
                    Verdict::Drop { warn: true }
                )
            })
            .count();
        assert_eq!(warns, 1);
        assert!(matches!(
            b.take(t0 + WARN_EVERY + Duration::from_millis(1), 0, 3000),
            Verdict::Drop { warn: true }
        ));
    }

    #[test]
    fn set_kbps_follows_the_music_bitrate() {
        let mut b = Ingress::audio(music_cap_kbps(320));
        assert_eq!(b.rate_kbps(), 544);
        b.set_kbps(music_cap_kbps(32));
        assert_eq!(b.rate_kbps(), 112);
    }
}
