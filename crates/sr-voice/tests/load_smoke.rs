//! The load harness (VOX-SFU-011) at a size that runs in a debug build in a few seconds. The
//! 300 Mbit/s run is an operator tool: `cargo run --release -p sr-voice --example loadtest`.

use std::time::Duration;

use sr_voice::loadtest::{LoadConfig, run};
use sr_voice_proto::video::Layer;

#[test]
fn the_harness_moves_video_and_reports_sane_numbers() {
    let cfg = LoadConfig {
        publishers: 2,
        viewers: 4,
        viewer_layer: Layer::High,
        warmup: Duration::from_secs(7),
        measure: Duration::from_secs(3),
        ..LoadConfig::default()
    };
    let r = run(&cfg);
    assert_eq!(r.participants, 6);
    // Each viewer watches two 1.5 Mbit/s cameras, plus audio and RTCP overhead.
    let want = 4.0 * 2.0 * 1.5;
    assert!(
        r.egress_mbps > want * 0.7 && r.egress_mbps < want * 1.6,
        "egress {:.1} Mbit/s, expected about {want}",
        r.egress_mbps
    );
    // Publishers send three layers of two cameras.
    assert!(
        r.ingress_mbps > 2.0 * 2.1 * 0.7,
        "ingress {:.1}",
        r.ingress_mbps
    );
    assert!(
        r.frames_received > 4 * 2 * 3 * 20,
        "frames {}",
        r.frames_received
    );
    assert!(r.core_seconds > 0.0 && r.core_cpu_fraction > 0.0);
    assert!(r.cpu_fraction_at_target > 0.0);
    assert!(r.keyframe_requests >= 1);
}

#[test]
fn thumbnails_cost_a_fraction_of_the_full_layer() {
    let base = LoadConfig {
        publishers: 2,
        viewers: 3,
        warmup: Duration::from_secs(5),
        measure: Duration::from_secs(2),
        audio: false,
        ..LoadConfig::default()
    };
    let low = run(&LoadConfig {
        viewer_layer: Layer::Low,
        ..base.clone()
    });
    let high = run(&LoadConfig {
        viewer_layer: Layer::High,
        warmup: Duration::from_secs(8),
        ..base
    });
    assert!(
        low.egress_mbps * 4.0 < high.egress_mbps,
        "low {:.2} Mbit/s, high {:.2} Mbit/s",
        low.egress_mbps,
        high.egress_mbps
    );
}
