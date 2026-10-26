//! A bot that joins a voice channel, takes part in the call's end-to-end encryption, sends test frames and logs
//! who talks (DEV-BOT-007, RDY-DEV-002).
//!
//! ```text
//! SR_BOT_TOKEN=... SR_API_URL=https://chat.example/api/v1 SR_GUILD_ID=... SR_VOICE_CHANNEL_ID=... \
//!   SR_BIND_IP=192.168.1.20 SR_GATEWAY_URL=wss://chat.example/gateway \
//!   cargo run -p sr-sdk --example voice_bot
//! ```
//!
//! The flow: gateway op 4 declaring `sr_e2ee` ([`E2eeCall::join_state`]), the `VOICE_SERVER_UPDATE` dispatch, then
//! the MLS handshake (D-249, D-263): `hello` as gateway op 101 within ten seconds, every `E2EE_MLS` dispatch
//! into [`E2eeCall::handle`], its actions back out as op 101, the epoch keys into [`E2eeMedia`] and the server's
//! `execute` to switch encryptors. Meanwhile [`VoiceLink`] holds the media connection: a dropped SFU WebSocket
//! is resumed and a forgotten session joined again with the same transform, and the shard itself resumes the
//! gateway session and re-sends the join after a fresh one.
//!
//! Knobs that make the example usable as a smoke test (`e2e.yml`, job `live`):
//!
//! * `SR_RUN_SECONDS=60` leaves after that long (exit code 0 only if the call was joined, encrypted when the
//!   call is, and resumed after the drop); default: run forever;
//! * `SR_DROP_AFTER_SECONDS=20` drops the SFU WebSocket once after that long, to watch the resume;
//! * what it hears is decoded and printed as `<user> spoke <bytes> bytes (<level> dBov)`;
//! * `SR_IDENTITY_SECRET=<64 hex digits>` keeps the bot's identity key between runs (default: a new one).
//!
//! The bot speaks a 440 Hz tone: each 20 ms of PCM goes through [`OpusEncoder`] (feature `voice-opus`, DEV-FU-002)
//! and the packet, with its real level from [`audio_level_dbov`], to `send_opus`. A bot that already has Opus
//! (a file, another SDK) passes those packets to `send_opus` as they are.

use std::collections::HashSet;
use std::hash::{BuildHasher, Hasher};
use std::net::IpAddr;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde_json::Value;
use sr_sdk::audio::{
    Application, Channels, FRAME_SAMPLES, OpusDecoder, OpusEncoder, audio_level_dbov,
};
use sr_sdk::voice::{Action, E2eeCall, E2eeMedia, Identity};
use sr_sdk::voice_transport::{
    ReconnectPolicy, SharedMedia, VoiceEvent, VoiceLink, VoiceServerUpdate, voice_state_body,
};
use sr_sdk::{Client, Context, Error, Event, EventHandler, GatewayHandle, Result, intents};
use tokio::sync::mpsc;

/// What the gateway tells `main`.
enum Msg {
    Ready { user: u64 },
    Server(VoiceServerUpdate),
    Mls(Value),
}

struct Voice(mpsc::UnboundedSender<Msg>);

#[async_trait]
impl EventHandler for Voice {
    async fn on_event(&self, _ctx: Context, _shard: u32, event: Event) {
        let msg = match event {
            Event::Ready(r) => match r.user.id.parse() {
                Ok(user) => Msg::Ready { user },
                Err(_) => return,
            },
            Event::Other { name, data } if name == "VOICE_SERVER_UPDATE" => {
                match VoiceServerUpdate::from_dispatch(&data) {
                    Some(u) => Msg::Server(u),
                    None => return,
                }
            }
            Event::Other { name, data } if name == "E2EE_MLS" => Msg::Mls(data),
            _ => return,
        };
        let _ = self.0.send(msg);
    }
}

/// 20 ms of a 440 Hz sine at a modest level, frame number `n` onwards (the phase continues across frames).
fn tone_frame(n: u64) -> Vec<i16> {
    (0..FRAME_SAMPLES as u64)
        .map(|i| {
            let t = (n * FRAME_SAMPLES as u64 + i) as f64 / 48_000.0;
            (f64::sin(2.0 * std::f64::consts::PI * 440.0 * t) * 8_000.0) as i16
        })
        .collect()
}

fn seconds(var: &str) -> Option<Duration> {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs)
}

/// Random bytes without a new dependency: the std hasher is seeded from the operating system. A bot that
/// matters should use a real CSPRNG and store the identity secret.
fn random_bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    for chunk in out.chunks_mut(8) {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        let b = h.finish().to_le_bytes();
        chunk.copy_from_slice(&b[..chunk.len()]);
    }
    out
}

fn identity() -> Identity {
    if let Ok(hex) = std::env::var("SR_IDENTITY_SECRET") {
        let bytes: Vec<u8> = (0..hex.len() / 2)
            .filter_map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok())
            .collect();
        return Identity::from_secret(&bytes).expect("SR_IDENTITY_SECRET is 64 hex digits");
    }
    let mut secret = random_bytes::<32>();
    // A P-256 scalar must be below the group order: a leading zero byte is always safe.
    secret[0] = 0;
    Identity::from_secret(&secret).expect("a fresh identity key")
}

/// Send the actions of the key agreement: op 101 payloads out, keys and `execute` into the media.
async fn apply(
    actions: Vec<Action>,
    gateway: &GatewayHandle,
    guild: &str,
    media: &SharedMedia,
    epochs: &mut Vec<u64>,
) -> Result<()> {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    for a in actions {
        match a {
            Action::Send(d) => gateway.e2ee_mls(guild, d).await?,
            Action::Keys(k) => {
                if let Err(e) = media.lock().prepare(&k, now_ms) {
                    println!("keys of epoch {} refused: {e}", k.epoch);
                }
            }
            Action::Execute(epoch) => {
                if media.lock().execute(epoch) {
                    println!("encrypting under epoch {epoch}");
                    epochs.push(epoch);
                }
            }
            Action::Failed(why) => {
                println!("key agreement failed, asking to be added again: {why}")
            }
        }
    }
    Ok(())
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

    let (tx, mut inbox) = mpsc::unbounded_channel();
    let mut builder = Client::builder(token, api).intents(intents::DEFAULT);
    // The gateway URL that `GET /gateway/bot` gives is built from the server's public URL; name another one
    // (optional) when the bot reaches the server some other way.
    if let Ok(url) = std::env::var("SR_GATEWAY_URL") {
        builder = builder.gateway_url(url);
    }
    let client = builder.handler(Voice(tx)).build()?;
    let gateway = client.gateway();
    tokio::spawn(client.start());

    // Ask to join once the shard is identified; the user id comes with READY.
    let Some(Msg::Ready { user: me }) = inbox.recv().await else {
        panic!("expected READY first");
    };
    let identity = identity();
    let mut call =
        E2eeCall::new(me, &channel, &identity).map_err(|e| Error::Invalid(e.to_string()))?;
    gateway
        .voice_state_update(
            &guild,
            voice_state_body(Some(&guild), Some(&channel), false, false, true),
        )
        .await?;

    let media = SharedMedia::new(E2eeMedia::new(me, random_bytes::<4>()));
    let mut epochs = Vec::new();
    let mut roster: HashSet<u64> = HashSet::from([me]);
    let mut link: Option<VoiceLink> = None;

    let started = Instant::now();
    let stop_after = seconds("SR_RUN_SECONDS");
    let mut drop_after = seconds("SR_DROP_AFTER_SECONDS");
    let (mut joined, mut resumed, mut frames) = (false, false, 0u32);
    let mut encoder = OpusEncoder::new(Channels::Mono, Application::Voip)
        .map_err(|e| Error::Invalid(e.to_string()))?;
    let mut pcm_clock = 0u64;
    // One decoder per speaker: each keeps the state of its own stream.
    let mut decoders: std::collections::HashMap<u64, OpusDecoder> =
        std::collections::HashMap::new();
    let mut e2ee_call = false;
    loop {
        if stop_after.is_some_and(|s| started.elapsed() >= s) {
            println!(
                "done after {:?}: connected={joined} resumed={resumed} encrypted_epochs={} frames={frames}",
                started.elapsed(),
                epochs.len()
            );
            let ok = joined
                && (drop_after.is_none() || resumed)
                && (!e2ee_call || (!epochs.is_empty() && frames > 0));
            return if ok {
                Ok(())
            } else {
                Err(Error::Transport(
                    "the call was not joined, encrypted or resumed".into(),
                ))
            };
        }
        // The gateway side: the SFU's address, then the MLS handshake.
        while let Ok(msg) = inbox.try_recv() {
            match msg {
                Msg::Ready { .. } => {}
                Msg::Server(update) => {
                    e2ee_call = update.e2ee;
                    if update.e2ee {
                        // Within ten seconds of VOICE_SERVER_UPDATE or the server removes the bot (D-263).
                        apply(vec![call.hello()], &gateway, &guild, &media, &mut epochs).await?;
                    }
                    match link.as_mut() {
                        Some(l) => {
                            println!("rejoining {}", update.endpoint);
                            l.rejoin(update)
                                .await
                                .map_err(|e| Error::Transport(e.to_string()))?;
                        }
                        None => {
                            println!("joining {} (e2ee: {})", update.endpoint, update.e2ee);
                            let mut l =
                                VoiceLink::connect(update, bind, ReconnectPolicy::default())
                                    .await
                                    .map_err(|e| Error::Transport(e.to_string()))?;
                            roster.extend(l.session().initial_participants.iter().copied());
                            l.session().set_transform(media.clone());
                            link = Some(l);
                        }
                    }
                }
                Msg::Mls(d) => {
                    let actions = call.handle(&d, &roster, Instant::now());
                    apply(actions, &gateway, &guild, &media, &mut epochs).await?;
                }
            }
        }
        if call.deadline().is_some_and(|t| Instant::now() >= t) {
            let actions = call.poll(&roster, Instant::now());
            apply(actions, &gateway, &guild, &media, &mut epochs).await?;
        }
        let Some(l) = link.as_mut() else {
            tokio::time::sleep(Duration::from_millis(20)).await;
            continue;
        };
        if drop_after.is_some_and(|d| started.elapsed() >= d) {
            println!("dropping the SFU WebSocket");
            l.session().abandon_signalling().await;
            drop_after = None;
        }
        // One 20 ms frame of the tone once the call is encrypted under a key (or at once in a call in the clear).
        if l.session().is_connected() && (!e2ee_call || media.lock().sending_epoch().is_some()) {
            let pcm = tone_frame(pcm_clock);
            let packet = encoder
                .encode_frame(&pcm)
                .map_err(|e| Error::Invalid(e.to_string()))?;
            if l.session()
                .send_opus(&packet, audio_level_dbov(&pcm))
                .is_ok()
            {
                frames += 1;
                pcm_clock += 1;
            }
        }
        let events = l
            .pump(Duration::from_millis(20))
            .await
            .map_err(|e| Error::Transport(e.to_string()))?;
        let mut changed = false;
        for e in events {
            match e {
                VoiceEvent::Connected => {
                    joined = true;
                    println!("connected");
                }
                VoiceEvent::Audio { user, frame } => {
                    // Hear what was said: decode and report the level (the live harness checks it is not silence).
                    let decoder = match decoders.entry(user) {
                        std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                        std::collections::hash_map::Entry::Vacant(e) => e.insert(
                            OpusDecoder::new(Channels::Mono)
                                .map_err(|e| Error::Invalid(e.to_string()))?,
                        ),
                    };
                    let level = audio_level_dbov(&decoder.decode_frame(&frame, false));
                    println!("{user} spoke {} bytes ({level} dBov)", frame.len());
                }
                VoiceEvent::Joined(u) => changed |= roster.insert(u),
                VoiceEvent::Left(u) => changed |= roster.remove(&u),
                VoiceEvent::Resumed { participants } => {
                    resumed = true;
                    println!("resumed ({} in the channel)", participants.len());
                    roster = participants.into_iter().chain([me]).collect();
                    changed = true;
                }
                VoiceEvent::Reconnected { participants } => {
                    println!("joined again ({} in the channel)", participants.len());
                    roster = participants.into_iter().chain([me]).collect();
                    changed = true;
                }
                VoiceEvent::Closed(code) => {
                    println!("closed ({code})");
                    return Ok(());
                }
                other => println!("{other:?}"),
            }
        }
        if changed {
            let actions = call.roster_changed(&roster, Instant::now());
            apply(actions, &gateway, &guild, &media, &mut epochs).await?;
        }
    }
}
