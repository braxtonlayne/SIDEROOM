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
}

/// Counter increments since the last [`publish`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Deltas {
    pub packets: u64,
    pub frames: u64,
    pub keyframes: u64,
    pub unkeyed: u64,
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
    metrics::gauge!(GOVERNOR_SEVERITY).set(f64::from(s.governor_severity));
}
