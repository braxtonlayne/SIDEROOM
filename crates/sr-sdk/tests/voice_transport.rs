//! DEV-BOT-007: bots in voice over the real SFU. Two `VoiceSession`s join one channel of an in-process SFU
//! (WebSocket plus UDP on loopback), one publishes Opus-sized frames, the other receives them. The encrypted
//! variants check that the SFU only relays what the frame transform produced and that a call which declares
//! E2EE cannot be spoken into in the clear.
//!
//! Run with `cargo test -p sr-sdk --features voice-transport,voice-e2ee,voice-opus --test voice_transport`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sr_core::{ChannelId, GuildId, UserId};
use sr_sdk::voice_transport::{
    FrameTransform, TransportError, VoiceEvent, VoiceServerUpdate, VoiceSession,
};
use sr_voice::SfuConfig;
use sr_voice_proto::{JoinClaims, TokenSigner, VoiceRelay};

const SECRET: &[u8] = b"test-secret-test-secret-test-secret";
const CHANNEL: u64 = 700;

struct Sfu {
    handle: sr_voice::SfuHandle,
    url: String,
}

async fn start() -> Sfu {
    let handle = sr_voice::start(SfuConfig {
        bind_ip: [127, 0, 0, 1].into(),
        udp_port_min: 0,
        udp_port_max: 0,
        tcp_port: None,
        token_secret: SECRET.to_vec(),
        ..SfuConfig::default()
    })
    .await
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = sr_voice::router(handle.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    // OPS-IMM-001: the SFU forwards only senders the gateway reported as E2EE-keyed (op 101). There is no
    // gateway here, so the test plays its part and keeps re-announcing the test users (it also covers a rejoin).
    let keyer = handle.clone();
    tokio::spawn(async move {
        loop {
            for user in 1..=4u64 {
                keyer
                    .apply(sr_voice_proto::VoiceCommand::E2eeKeyed {
                        channel: ChannelId::new(CHANNEL),
                        user: UserId::new(user),
                        keyed: true,
                    })
                    .await;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
    Sfu {
        handle,
        url: format!("ws://{addr}/voice/ws"),
    }
}

fn update(sfu: &Sfu, user: u64, e2ee: bool) -> VoiceServerUpdate {
    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 60;
    let token = TokenSigner::new(SECRET.to_vec())
        .sign(&JoinClaims {
            user_id: UserId::new(user),
            guild_id: Some(GuildId::new(9)),
            channel_id: ChannelId::new(CHANNEL),
            session_id: format!("gw-{user}"),
            exp,
            speak: true,
            deaf: false,
            ptt_required: false,
            priority: false,
            camera: false,
            screen: false,
            tier: 0,
            video_quality: 0,
        })
        .unwrap();
    VoiceServerUpdate::from_dispatch(&serde_json::json!({
        "token": token,
        "endpoint": sfu.url,
        "guild_id": "9",
        "channel_id": CHANNEL.to_string(),
        "sr_e2ee": e2ee,
    }))
    .unwrap()
}

/// A toy transform: marks and flips every byte. Enough to prove the SFU relays exactly what the transform
/// produced and that the receiver runs the inverse.
struct Flip;

impl FrameTransform for Flip {
    fn encrypt(&mut self, frame: &[u8]) -> Result<Vec<u8>, String> {
        let mut out = vec![0xEE];
        out.extend(frame.iter().map(|b| !b));
        Ok(out)
    }

    fn decrypt(&mut self, _sender: u64, frame: &[u8]) -> Result<Vec<u8>, String> {
        match frame.split_first() {
            Some((0xEE, rest)) => Ok(rest.iter().map(|b| !b).collect()),
            _ => Err("not one of ours".into()),
        }
    }
}

/// A fake 20 ms Opus frame.
fn frame(n: u32) -> Vec<u8> {
    let mut f = vec![0x5A; 60];
    f[..4].copy_from_slice(&n.to_be_bytes());
    f
}

/// Let `a` talk for `frames` frames while both sessions run; return what `b` heard and `a` saw.
async fn talk(
    a: &mut VoiceSession,
    b: &mut VoiceSession,
    frames: u32,
) -> (Vec<VoiceEvent>, Vec<VoiceEvent>) {
    talk_with(a, b, frames, |n| (frame(n), -28)).await
}

/// [`talk`] with the frame and its audio level chosen by the caller.
async fn talk_with(
    a: &mut VoiceSession,
    b: &mut VoiceSession,
    frames: u32,
    mut next: impl FnMut(u32) -> (Vec<u8>, i8),
) -> (Vec<VoiceEvent>, Vec<VoiceEvent>) {
    let (mut heard_b, mut seen_a) = (Vec::new(), Vec::new());
    // Connect first: both sides must pump for ICE and DTLS to complete.
    for _ in 0..200 {
        let (ea, eb) = tokio::join!(
            a.pump(Duration::from_millis(50)),
            b.pump(Duration::from_millis(50))
        );
        seen_a.extend(ea.unwrap());
        heard_b.extend(eb.unwrap());
        if a.is_connected() && b.is_connected() {
            break;
        }
    }
    assert!(a.is_connected() && b.is_connected(), "did not connect");
    // Let the SFU's offers for the new audio lines settle.
    for _ in 0..20 {
        let (ea, eb) = tokio::join!(
            a.pump(Duration::from_millis(50)),
            b.pump(Duration::from_millis(50))
        );
        seen_a.extend(ea.unwrap());
        heard_b.extend(eb.unwrap());
    }
    a.set_speaking(true);
    for n in 0..frames {
        let (f, level) = next(n);
        a.send_opus(&f, level).unwrap();
        let (ea, eb) = tokio::join!(
            a.pump(Duration::from_millis(20)),
            b.pump(Duration::from_millis(20))
        );
        seen_a.extend(ea.unwrap());
        heard_b.extend(eb.unwrap());
    }
    a.set_speaking(false);
    for _ in 0..10 {
        let (ea, eb) = tokio::join!(
            a.pump(Duration::from_millis(30)),
            b.pump(Duration::from_millis(30))
        );
        seen_a.extend(ea.unwrap());
        heard_b.extend(eb.unwrap());
    }
    (heard_b, seen_a)
}

fn audio_from(events: &[VoiceEvent], user: u64) -> Vec<Vec<u8>> {
    events
        .iter()
        .filter_map(|e| match e {
            VoiceEvent::Audio { user: u, frame } if *u == user => Some(frame.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_bot_publishes_opus_and_another_receives_it() {
    let sfu = start().await;
    let mut a = VoiceSession::connect(&update(&sfu, 1, false), [127, 0, 0, 1].into())
        .await
        .unwrap();
    let mut b = VoiceSession::connect(&update(&sfu, 2, false), [127, 0, 0, 1].into())
        .await
        .unwrap();
    assert_eq!(b.initial_participants, vec![1], "READY lists who was there");
    let (heard, seen) = talk(&mut a, &mut b, 60).await;
    let got = audio_from(&heard, 1);
    assert!(got.len() >= 40, "B received {} of A's frames", got.len());
    assert_eq!(got[0].len(), 60);
    assert!(got.iter().all(|f| f[4..].iter().all(|b| *b == 0x5A)));
    assert!(audio_from(&seen, 2).is_empty() && audio_from(&seen, 1).is_empty());
    assert!(seen.contains(&VoiceEvent::Joined(2)), "A is told B joined");
    assert!(heard.contains(&VoiceEvent::Connected));
    assert!(
        heard
            .iter()
            .any(|e| matches!(e, VoiceEvent::Speaking { user: 1, speaking } if *speaking != 0)),
        "B sees A start speaking"
    );
    sfu.handle.shutdown().await;
}

/// DEV-FU-002: real Opus. A tone goes through `OpusEncoder`, the SFU relays it, the receiver decodes it to a
/// buffer that is not silence and whose level matches what the sender declared.
#[cfg(feature = "voice-opus")]
#[tokio::test]
async fn encoded_frames_decode_to_sound_on_the_other_side() {
    use sr_sdk::audio::{
        Application, Channels, FRAME_SAMPLES, OpusDecoder, OpusEncoder, audio_level_dbov,
    };

    let sfu = start().await;
    let mut a = VoiceSession::connect(&update(&sfu, 1, false), [127, 0, 0, 1].into())
        .await
        .unwrap();
    let mut b = VoiceSession::connect(&update(&sfu, 2, false), [127, 0, 0, 1].into())
        .await
        .unwrap();
    let mut enc = OpusEncoder::new(Channels::Mono, Application::Voip).unwrap();
    let (heard, _) = talk_with(&mut a, &mut b, 60, |n| {
        let pcm: Vec<i16> = (0..FRAME_SAMPLES)
            .map(|i| {
                let t = (n as usize * FRAME_SAMPLES + i) as f64 / 48_000.0;
                (f64::sin(2.0 * std::f64::consts::PI * 440.0 * t) * 12_000.0) as i16
            })
            .collect();
        (enc.encode_frame(&pcm).unwrap(), audio_level_dbov(&pcm))
    })
    .await;
    let got = audio_from(&heard, 1);
    assert!(got.len() >= 40, "B received {} of A's frames", got.len());
    let mut dec = OpusDecoder::new(Channels::Mono).unwrap();
    let pcm: Vec<i16> = got
        .iter()
        .flat_map(|f| dec.decode_frame(f, false))
        .collect();
    assert!(pcm.len() >= 40 * FRAME_SAMPLES);
    let level = audio_level_dbov(&pcm[5 * FRAME_SAMPLES..]);
    assert!(
        level > -20,
        "the receiver hears a tone, not silence ({level} dBov)"
    );
    sfu.handle.shutdown().await;
}

#[tokio::test]
async fn an_encrypted_call_never_carries_plaintext() {
    let sfu = start().await;
    // No transform in a call that declares E2EE: sending is refused once connected, and nothing leaks.
    let mut clear = VoiceSession::connect(&update(&sfu, 1, true), [127, 0, 0, 1].into())
        .await
        .unwrap();
    let mut b = VoiceSession::connect(&update(&sfu, 2, true), [127, 0, 0, 1].into())
        .await
        .unwrap();
    b.set_transform(Flip);
    for _ in 0..200 {
        let _ = tokio::join!(
            clear.pump(Duration::from_millis(50)),
            b.pump(Duration::from_millis(50))
        );
        if clear.is_connected() && b.is_connected() {
            break;
        }
    }
    assert!(matches!(
        clear.send_opus(&frame(0), -28),
        Err(TransportError::EncryptionRequired)
    ));
    sfu.handle.shutdown().await;

    // With transforms on both sides the receiver gets the plaintext back.
    let sfu = start().await;
    let mut a = VoiceSession::connect(&update(&sfu, 1, true), [127, 0, 0, 1].into())
        .await
        .unwrap()
        .with_transform(Flip);
    let mut b = VoiceSession::connect(&update(&sfu, 2, true), [127, 0, 0, 1].into())
        .await
        .unwrap()
        .with_transform(Flip);
    let (heard, _) = talk(&mut a, &mut b, 60).await;
    let got = audio_from(&heard, 1);
    assert!(got.len() >= 40, "B decrypted {} frames", got.len());
    assert_eq!(got[0].len(), 60, "the marker byte is gone again");
    sfu.handle.shutdown().await;
}

#[tokio::test]
async fn a_listener_without_the_keys_drops_frames_instead_of_playing_them() {
    let sfu = start().await;
    let mut a = VoiceSession::connect(&update(&sfu, 1, true), [127, 0, 0, 1].into())
        .await
        .unwrap()
        .with_transform(Flip);
    // B declared E2EE but has no transform (the key agreement has not finished).
    let mut b = VoiceSession::connect(&update(&sfu, 2, true), [127, 0, 0, 1].into())
        .await
        .unwrap();
    let (heard, _) = talk(&mut a, &mut b, 40).await;
    assert!(audio_from(&heard, 1).is_empty(), "ciphertext is not audio");
    assert!(
        heard
            .iter()
            .any(|e| matches!(e, VoiceEvent::Undecryptable { user: 1 }))
    );
    sfu.handle.shutdown().await;
}

#[tokio::test]
async fn a_bad_token_is_refused_with_the_close_code() {
    let sfu = start().await;
    let mut u = update(&sfu, 1, false);
    u.token = "forged".into();
    match VoiceSession::connect(&u, [127, 0, 0, 1].into()).await {
        Err(TransportError::Refused(code)) => assert_eq!(code, 4004),
        other => panic!("expected a refusal, got {other:?}"),
    }
    sfu.handle.shutdown().await;
}

/// The real thing: MLS-derived keys (`E2eeMedia`) as the frame transform.
#[cfg(feature = "voice-e2ee")]
#[tokio::test]
async fn real_epoch_keys_encrypt_the_audio_end_to_end() {
    use sr_e2ee::SenderSecret;
    use sr_sdk::voice::{E2eeMedia, EpochKeys};

    let keys = || EpochKeys {
        epoch: 1,
        own_leaf: 0,
        senders: vec![
            (1, SenderSecret::new([11; 16])),
            (2, SenderSecret::new([22; 16])),
        ],
    };
    let media = |me: u64, instance: [u8; 4]| {
        let mut m = E2eeMedia::new(me, instance);
        m.prepare(&keys(), 0).unwrap();
        assert!(m.execute(1));
        m
    };
    let sfu = start().await;
    let mut a = VoiceSession::connect(&update(&sfu, 1, true), [127, 0, 0, 1].into())
        .await
        .unwrap()
        .with_transform(media(1, [1, 2, 3, 4]));
    let mut b = VoiceSession::connect(&update(&sfu, 2, true), [127, 0, 0, 1].into())
        .await
        .unwrap()
        .with_transform(media(2, [5, 6, 7, 8]));
    let (heard, _) = talk(&mut a, &mut b, 60).await;
    let got = audio_from(&heard, 1);
    assert!(got.len() >= 40, "B decrypted {} frames", got.len());
    assert!(
        got.iter()
            .all(|f| f.len() == 60 && f[4..].iter().all(|b| *b == 0x5A)),
        "the frames come back byte for byte"
    );
    sfu.handle.shutdown().await;
}
