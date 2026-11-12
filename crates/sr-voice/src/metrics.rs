//! Prometheus metrics for the SFU (VOX-SFU-009). Recorded through the `metrics` facade, so
//! they appear on `/internal/metrics` when the SFU runs inside `sr-server` and on the
//! standalone binary's own exporter otherwise.

use crate::core::Snapshot;

pub const ROOMS: &str = "sr_voice_rooms";
pub const PARTICIPANTS: &str = "sr_voice_participants";
pub const EGRESS_BPS: &str = "sr_voice_egress_bits_per_second";
pub const INGRESS_BPS: &str = "sr_voice_ingress_bits_per_second";
/// The music track's share of [`EGRESS_BPS`] (MUS-JB-001; already counted there, never added to it).
pub const MUSIC_EGRESS_BPS: &str = "sr_voice_music_egress_bits_per_second";
pub const PACKET_LOSS: &str = "sr_voice_packet_loss_ratio";
pub const FORWARDED: &str = "sr_voice_forwarded_packets_total";
pub const FORWARDED_FRAMES: &str = "sr_voice_forwarded_video_frames_total";
pub const GOVERNOR_SEVERITY: &str = "sr_voice_governor_severity";
pub const KEYFRAME_REQUESTS: &str = "sr_voice_keyframe_requests_total";
/// Frames dropped because the sender had not completed the E2EE handshake (OPS-IMM-001).
pub const UNKEYED_FRAMES: &str = "sr_voice_unkeyed_frames_total";
/// Frames dropped by the ingress caps (D-631, D-639), label `kind` = `audio` or `video`.
pub const INGRESS_DROPPED: &str = "sr_voice_ingress_dropped_frames_total";

/// Describe the metrics once at startup.
pub fn describe() {
    metrics::describe_gauge!(ROOMS, "Voice rooms with at least one participant");
    metrics::describe_gauge!(PARTICIPANTS, "Participants connected to this SFU node");
    metrics::describe_gauge!(EGRESS_BPS, "Bits per second the SFU sends to participants");
    metrics::describe_gauge!(INGRESS_BPS, "Bits per second participants send to the SFU");
    metrics::describe_gauge!(
        MUSIC_EGRESS_BPS,
        "Bits per second of music tracks the SFU sends to participants (part of the egress)"
    );
    metrics::describe_gauge!(
        PACKET_LOSS,
        "Mean packet loss fraction (0..1) participants report for audio received from the SFU"
    );
    metrics::describe_counter!(FORWARDED, "Audio packets forwarded to subscribers");
    metrics::describe_counter!(FORWARDED_FRAMES, "Video frames forwarded to subscribers");
    metrics::describe_gauge!(
        GOVERNOR_SEVERITY,
        "Egress governor severity, 0 when not engaged (doc 08 section 3)"
    );
    metrics::describe_counter!(
        KEYFRAME_REQUESTS,
        "Keyframe requests sent to publishers (after coalescing)"
    );
    metrics::describe_counter!(
        UNKEYED_FRAMES,
        "Frames dropped because their sender had not sent an MLS key package (E2EE is the only voice mode)"
    );
    metrics::describe_counter!(
        INGRESS_DROPPED,
        "Frames dropped because a participant sent more than the ingress cap of the track (D-631)"
    );
}

/// Counter increments since the last [`publish`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Deltas {
    pub packets: u64,
    pub frames: u64,
    pub keyframes: u64,
    pub unkeyed: u64,
    pub ingress_dropped_audio: u64,
    pub ingress_dropped_video: u64,
}

/// The running counter totals one SFU shard has published up to now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub packets: u64,
    pub frames: u64,
    pub keyframes: u64,
    pub unkeyed: u64,
    pub ingress_dropped_audio: u64,
    pub ingress_dropped_video: u64,
}

impl Counts {
    /// The totals in a snapshot (the shard's own, not the node merge).
    pub fn of(s: &Snapshot) -> Self {
        Self {
            packets: s.forwarded_packets,
            frames: s.forwarded_frames,
            keyframes: s.keyframe_requests,
            unkeyed: s.unkeyed_frames,
            ingress_dropped_audio: s.ingress_dropped_audio,
            ingress_dropped_video: s.ingress_dropped_video,
        }
    }

    /// The increase since `last`; a counter that went backwards (a restarted core) counts as zero.
    pub fn since(&self, last: &Self) -> Deltas {
        Deltas {
            packets: self.packets.saturating_sub(last.packets),
            frames: self.frames.saturating_sub(last.frames),
            keyframes: self.keyframes.saturating_sub(last.keyframes),
            unkeyed: self.unkeyed.saturating_sub(last.unkeyed),
            ingress_dropped_audio: self
                .ingress_dropped_audio
                .saturating_sub(last.ingress_dropped_audio),
            ingress_dropped_video: self
                .ingress_dropped_video
                .saturating_sub(last.ingress_dropped_video),
        }
    }
}

/// Publish a snapshot and the counter increments since the last call.
pub fn publish(s: &Snapshot, deltas: Deltas) {
    metrics::gauge!(ROOMS).set(s.rooms as f64);
    metrics::gauge!(PARTICIPANTS).set(s.participants as f64);
    metrics::gauge!(EGRESS_BPS).set(s.egress_bps);
    metrics::gauge!(INGRESS_BPS).set(s.ingress_bps);
    metrics::gauge!(MUSIC_EGRESS_BPS).set(s.music_egress_bps);
    metrics::gauge!(PACKET_LOSS).set(s.loss_ratio);
    metrics::counter!(FORWARDED).increment(deltas.packets);
    metrics::counter!(FORWARDED_FRAMES).increment(deltas.frames);
    metrics::counter!(KEYFRAME_REQUESTS).increment(deltas.keyframes);
    metrics::counter!(UNKEYED_FRAMES).increment(deltas.unkeyed);
    metrics::counter!(INGRESS_DROPPED, "kind" => "audio").increment(deltas.ingress_dropped_audio);
    metrics::counter!(INGRESS_DROPPED, "kind" => "video").increment(deltas.ingress_dropped_video);
    metrics::gauge!(GOVERNOR_SEVERITY).set(f64::from(s.governor_severity));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(audio: u64, video: u64) -> Counts {
        Counts {
            ingress_dropped_audio: audio,
            ingress_dropped_video: video,
            ..Counts::default()
        }
    }

    #[test]
    fn the_dropped_frame_delta_is_the_difference_since_the_last_publish() {
        let first = counts(7, 3).since(&Counts::default());
        assert_eq!(
            (first.ingress_dropped_audio, first.ingress_dropped_video),
            (7, 3)
        );
        let second = counts(10, 3).since(&counts(7, 3));
        assert_eq!(
            (second.ingress_dropped_audio, second.ingress_dropped_video),
            (3, 0)
        );
        let none = counts(10, 3).since(&counts(10, 3));
        assert_eq!(
            (none.ingress_dropped_audio, none.ingress_dropped_video),
            (0, 0)
        );
    }

    #[test]
    fn a_restarted_counter_does_not_underflow() {
        let d = counts(1, 0).since(&counts(50, 20));
        assert_eq!((d.ingress_dropped_audio, d.ingress_dropped_video), (0, 0));
    }

    #[test]
    fn the_counts_come_from_the_snapshot_fields() {
        let snap = Snapshot {
            ingress_dropped_audio: 4,
            ingress_dropped_video: 9,
            ..Snapshot::default()
        };
        let c = Counts::of(&snap);
        assert_eq!((c.ingress_dropped_audio, c.ingress_dropped_video), (4, 9));
    }
}
