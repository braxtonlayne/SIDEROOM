//! Prometheus metrics for the SFU (VOX-SFU-009). Recorded through the `metrics` facade, so
//! they appear on `/internal/metrics` when the SFU runs inside `sr-server` and on the
//! standalone binary's own exporter otherwise.

use crate::core::Snapshot;

pub const ROOMS: &str = "sr_voice_rooms";
pub const PARTICIPANTS: &str = "sr_voice_participants";
pub const EGRESS_BPS: &str = "sr_voice_egress_bits_per_second";
pub const INGRESS_BPS: &str = "sr_voice_ingress_bits_per_second";
pub const PACKET_LOSS: &str = "sr_voice_packet_loss_ratio";
pub const FORWARDED: &str = "sr_voice_forwarded_packets_total";

/// Describe the metrics once at startup.
pub fn describe() {
    metrics::describe_gauge!(ROOMS, "Voice rooms with at least one participant");
    metrics::describe_gauge!(PARTICIPANTS, "Participants connected to this SFU node");
    metrics::describe_gauge!(EGRESS_BPS, "Bits per second the SFU sends to participants");
    metrics::describe_gauge!(INGRESS_BPS, "Bits per second participants send to the SFU");
    metrics::describe_gauge!(
        PACKET_LOSS,
        "Mean packet loss fraction (0..1) participants report for audio received from the SFU"
    );
    metrics::describe_counter!(FORWARDED, "Audio packets forwarded to subscribers");
}

/// Publish a snapshot; `forwarded_delta` is the packets forwarded since the last call.
pub fn publish(s: &Snapshot, forwarded_delta: u64) {
    metrics::gauge!(ROOMS).set(s.rooms as f64);
    metrics::gauge!(PARTICIPANTS).set(s.participants as f64);
    metrics::gauge!(EGRESS_BPS).set(s.egress_bps);
    metrics::gauge!(INGRESS_BPS).set(s.ingress_bps);
    metrics::gauge!(PACKET_LOSS).set(s.loss_ratio);
    metrics::counter!(FORWARDED).increment(forwarded_delta);
}
