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
//! The "tone" is not Opus: a real bot encodes PCM with an Opus encoder (the SDK ships none) and passes each
//! 20 ms packet to `send_opus`.

use std::net::IpAddr;
use std::time::Duration;

use async_trait::async_trait;
use sr_sdk::voice_transport::{VoiceEvent, VoiceServerUpdate, VoiceSession, voice_state_body};
use sr_sdk::{Client, Context, Event, EventHandler, Result, intents};
use tokio::sync::mpsc;

struct Voice(mpsc::UnboundedSender<VoiceServerUpdate>);

#[async_trait]
impl EventHandler for Voice {
    async fn on_event(&self, _ctx: Context, _shard: u32, event: Event) {
        if let Event::Other { name, data } = event
            && name == "VOICE_SERVER_UPDATE"
            && let Some(update) = VoiceServerUpdate::from_dispatch(&data)
        {
            let _ = self.0.send(update);
        }
    }
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

    let (tx, mut updates) = mpsc::unbounded_channel();
    let client = Client::builder(token, api)
        .intents(intents::DEFAULT)
        .handler(Voice(tx))
        .build()?;
    let gateway = client.gateway();
    tokio::spawn(client.start());

    // Give the shard a moment to identify, then ask to join.
    tokio::time::sleep(Duration::from_secs(3)).await;
    gateway
        .voice_state_update(
            &guild,
            voice_state_body(Some(&guild), Some(&channel), false, false, true),
        )
        .await?;
    let update = updates.recv().await.expect("a VOICE_SERVER_UPDATE");
    println!("joining {} (e2ee: {})", update.endpoint, update.e2ee);
    let mut session = VoiceSession::connect(&update, bind)
        .await
        .map_err(|e| sr_sdk::Error::Transport(e.to_string()))?;
    loop {
        let events = session
            .pump(Duration::from_millis(20))
            .await
            .map_err(|e| sr_sdk::Error::Transport(e.to_string()))?;
        for e in events {
            match e {
                VoiceEvent::Connected => println!("connected"),
                VoiceEvent::Audio { user, frame } => println!("{user} spoke {} bytes", frame.len()),
                VoiceEvent::Closed(code) => {
                    println!("closed ({code})");
                    return Ok(());
                }
                other => println!("{other:?}"),
            }
        }
        // `session.send_opus(&packet, -30)` publishes one 20 ms Opus packet once a transform is installed.
    }
}
