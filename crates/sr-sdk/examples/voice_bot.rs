//! A bot that joins a voice channel, says hello with a test tone and logs who talks (DEV-BOT-007).
//!
//! ```text
//! SR_BOT_TOKEN=... SR_API_URL=https://chat.example/api/v1 SR_GUILD_ID=... SR_VOICE_CHANNEL_ID=... \
//!   SR_BIND_IP=192.168.1.20 \
//!   cargo run -p sr-sdk --features voice-transport,voice-e2ee --example voice_bot
//! ```
//!
//! The flow: gateway op 4 declaring `sr_e2ee`, the `VOICE_SERVER_UPDATE` dispatch, then
//! [`VoiceSession`] for the media. Calls are end-to-end encrypted by default (D-249, D-263), so the bot must
//! also be an MLS group member: feed the `E2EE_MLS` dispatches and the roster to `E2eeCall`, send its actions
//! as gateway op 101, and hand the epoch keys to `E2eeMedia` (see `sr_sdk::voice` and
//! `tests/voice_e2ee.rs`). This example wires the media side only and sends nothing into an encrypted call
//! until a transform is installed, which is the behaviour the transport enforces.
//!
//! Reconnects are handled by [`VoiceLink`] (RDY-DEV-002): a dropped SFU WebSocket is resumed and a forgotten
//! session joined again, and the shard itself resumes the gateway session and re-sends the join after a fresh
//! one. Two knobs make the example usable as a smoke test (`e2e.yml`, job `live`):
//!
//! * `SR_RUN_SECONDS=60` leaves after that long (exit code 0 only if the call was joined); default: run forever;
//! * `SR_DROP_AFTER_SECONDS=20` drops the SFU WebSocket once after that long, to watch the resume.
//!
//! The "tone" is not Opus: a real bot encodes PCM with an Opus encoder (the SDK ships none) and passes each
//! 20 ms packet to `send_opus`.

use std::net::IpAddr;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use sr_sdk::voice_transport::{
    ReconnectPolicy, VoiceEvent, VoiceLink, VoiceServerUpdate, voice_state_body,
};
use sr_sdk::{Client, Context, Event, EventHandler, Result, intents};
use tokio::sync::mpsc;

/// Hands the `VOICE_SERVER_UPDATE` dispatches to `main`.
struct Voice {
    updates: mpsc::UnboundedSender<VoiceServerUpdate>,
    ready: mpsc::UnboundedSender<()>,
}

#[async_trait]
impl EventHandler for Voice {
    async fn on_event(&self, _ctx: Context, _shard: u32, event: Event) {
        match event {
            Event::Ready(_) => {
                let _ = self.ready.send(());
            }
            Event::Other { name, data } if name == "VOICE_SERVER_UPDATE" => {
                if let Some(update) = VoiceServerUpdate::from_dispatch(&data) {
                    let _ = self.updates.send(update);
                }
            }
            _ => {}
        }
    }
}

fn seconds(var: &str) -> Option<Duration> {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
}

#[tokio::main]
async fn main() -> Result<()> {
    let token = std::env::var("SR_BOT_TOKEN").expect("set SR_BOT_TOKEN");
    let api = std::env::var("SR_API_URL").expect("set SR_API_URL, e.g. https://host/api/v1");
    let guild = std::env::var("SR_GUILD_ID").expect("set SR_GUILD_ID");
    let channel = std::env::var("SR_VOICE_CHANNEL_ID").expect("set SR_VOICE_CHANNEL_ID");
    let bind: IpAddr = std::env::var("SR_BIND_IP")
        .expect("set SR_BIND_IP to the local address the SFU can reach")
        .parse()
        .expect("SR_BIND_IP is an IP address");

    let (updates_tx, mut updates) = mpsc::unbounded_channel();
    let (ready_tx, mut ready) = mpsc::unbounded_channel();
    let client = Client::builder(token, api)
        .intents(intents::DEFAULT)
        .handler(Voice {
            updates: updates_tx,
            ready: ready_tx,
        })
        .build()?;
    let gateway = client.gateway();
    tokio::spawn(client.start());

    // Ask to join once the shard is identified.
    ready.recv().await.expect("READY");
    gateway
        .voice_state_update(
            &guild,
            voice_state_body(Some(&guild), Some(&channel), false, false, true),
        )
        .await?;
    let update = updates.recv().await.expect("a VOICE_SERVER_UPDATE");
    println!("joining {} (e2ee: {})", update.endpoint, update.e2ee);
    let mut link = VoiceLink::connect(update, bind, ReconnectPolicy::default())
        .await
        .map_err(|e| sr_sdk::Error::Transport(e.to_string()))?;

    let started = Instant::now();
    let stop_after = seconds("SR_RUN_SECONDS");
    let mut drop_after = seconds("SR_DROP_AFTER_SECONDS");
    let (mut joined, mut resumed) = (false, false);
    loop {
        if stop_after.is_some_and(|s| started.elapsed() >= s) {
            println!("done after {:?}: connected={joined} resumed={resumed}", started.elapsed());
            link.session().set_speaking(false);
            return if joined && (drop_after.is_none() || resumed) {
                Ok(())
            } else {
                Err(sr_sdk::Error::Transport("the call was not joined or resumed".into()))
            };
        }
        if drop_after.is_some_and(|d| started.elapsed() >= d) {
            println!("dropping the SFU WebSocket");
            link.session().abandon_signalling().await;
            drop_after = None;
        }
        // A newer VOICE_SERVER_UPDATE (a moved call, a refreshed token) replaces the join.
        while let Ok(newer) = updates.try_recv() {
            println!("rejoining {}", newer.endpoint);
            link.rejoin(newer)
                .await
                .map_err(|e| sr_sdk::Error::Transport(e.to_string()))?;
        }
        let events = link
            .pump(Duration::from_millis(20))
            .await
            .map_err(|e| sr_sdk::Error::Transport(e.to_string()))?;
        for e in events {
            match e {
                VoiceEvent::Connected => {
                    joined = true;
                    println!("connected");
                }
                VoiceEvent::Audio { user, frame } => println!("{user} spoke {} bytes", frame.len()),
                VoiceEvent::Resumed { participants } => {
                    resumed = true;
                    println!("resumed ({} in the channel)", participants.len());
                }
                VoiceEvent::Reconnected { participants } => {
                    println!("joined again ({} in the channel)", participants.len());
                }
                VoiceEvent::Closed(code) => {
                    println!("closed ({code})");
                    return Ok(());
                }
                other => println!("{other:?}"),
            }
        }
        // `link.session().send_opus(&packet, -30)` publishes one 20 ms Opus packet once a transform is installed.
    }
}
