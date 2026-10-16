//! Load test for the SFU core (VOX-SFU-011). See `sr_voice::loadtest` for what is measured.
//!
//! ```text
//! cargo run --release -p sr-voice --example loadtest -- \
//!     --publishers 5 --viewers 40 --seconds 10 --layer high
//! # the sharded node: four actors, each carrying that load
//! cargo run --release -p sr-voice --example loadtest -- --shards 4 --publishers 2 --viewers 20
//! ```
//!
//! Options: `--publishers N`, `--viewers N`, `--seconds N` (measured), `--warmup N`,
//! `--layer low|mid|high` (what every viewer asks for), `--no-audio`, `--shards N` (run N
//! actors at once, each with the given publishers and viewers).

use std::time::Duration;

use sr_voice::loadtest::{LoadConfig, run, run_sharded};
use sr_voice_proto::video::Layer;

fn main() {
    let mut cfg = LoadConfig::default();
    let mut shards = 1usize;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut num = |name: &str| -> u64 {
            args.next()
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| panic!("{name} needs a number"))
        };
        match a.as_str() {
            "--publishers" => cfg.publishers = num("--publishers") as u32,
            "--viewers" => cfg.viewers = num("--viewers") as u32,
            "--seconds" => cfg.measure = Duration::from_secs(num("--seconds")),
            "--warmup" => cfg.warmup = Duration::from_secs(num("--warmup")),
            "--shards" => shards = num("--shards") as usize,
            "--no-audio" => cfg.audio = false,
            "--layer" => {
                cfg.viewer_layer = match args.next().as_deref() {
                    Some("low") => Layer::Low,
                    Some("mid") => Layer::Mid,
                    Some("high") => Layer::High,
                    _ => panic!("--layer is low, mid or high"),
                }
            }
            other => panic!("unknown option {other}"),
        }
    }
    println!(
        "{} publishers, {} viewers per shard, {} shard(s), layer {:?}, {} s measured after {} s warm-up",
        cfg.publishers,
        cfg.viewers,
        shards,
        cfg.viewer_layer,
        cfg.measure.as_secs(),
        cfg.warmup.as_secs()
    );
    let wall = std::time::Instant::now();
    if shards > 1 {
        let r = run_sharded(&cfg, shards);
        for (i, s) in r.shards.iter().enumerate() {
            println!(
                "shard {i}: {:.1} Mbit/s egress, {:.1} % of its core",
                s.egress_mbps,
                s.core_cpu_fraction * 100.0
            );
        }
        println!("participants          {}", r.participants);
        println!("egress                {:.1} Mbit/s", r.egress_mbps);
        println!("ingress               {:.1} Mbit/s", r.ingress_mbps);
        println!(
            "cores busy            {:.2} (all shards together)",
            r.cores_busy
        );
        println!(
            "busiest shard         {:.1} % of one core",
            r.busiest_shard_fraction * 100.0
        );
        println!(
            "at 300 Mbit/s egress  {:.2} cores in total, {:.1} % of each of {} shard cores",
            r.cores_at_target,
            r.cores_at_target / shards as f64 * 100.0,
            shards
        );
        println!("(harness wall time {:.1} s)", wall.elapsed().as_secs_f64());
        return;
    }
    let r = run(&cfg);
    println!("participants          {}", r.participants);
    println!("egress                {:.1} Mbit/s", r.egress_mbps);
    println!("ingress               {:.1} Mbit/s", r.ingress_mbps);
    println!(
        "core time             {:.2} s over {:.0} s simulated",
        r.core_seconds, r.seconds
    );
    println!(
        "one core busy         {:.1} %  (the core is single threaded)",
        r.core_cpu_fraction * 100.0
    );
    println!(
        "at 300 Mbit/s egress  {:.1} % of one core",
        r.cpu_fraction_at_target * 100.0
    );
    println!("video frames forwarded {}", r.frames_forwarded);
    println!("video frames received  {}", r.frames_received);
    println!("keyframe requests      {}", r.keyframe_requests);
    println!("(harness wall time {:.1} s)", wall.elapsed().as_secs_f64());
}
