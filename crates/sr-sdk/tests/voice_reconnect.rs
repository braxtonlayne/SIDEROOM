//! RDY-DEV-002: `VoiceLink` against the real SFU, in process on loopback. A dropped signalling WebSocket is
//! re-attached with RESUME and audio keeps flowing; a session the SFU forgot is joined again with the frame
//! transform kept; a removal from the call is final.
//!
//! Run with `cargo test -p sr-sdk --features voice-transport --test voice_reconnect`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sr_core::{ChannelId, GuildId, UserId};
use sr_sdk::voice_transport::{
    FrameTransform, ReconnectPolicy, VoiceEvent, VoiceLink, VoiceServerUpdate,
};
use sr_voice::SfuConfig;
use sr_voice_proto::{JoinClaims, TokenSigner, VoiceCommand, VoiceRelay};

const SECRET: &[u8] = b"test-secret-test-secret-test-secret";
const CHANNEL: u64 = 700;
const LOCAL: std::net::IpAddr = std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);

struct Sfu {
    handle: sr_voice::SfuHandle,
    url: String,
}

async fn start(resume_window: Duration) -> Sfu {
    let handle = sr_voice::start(SfuConfig {
        bind_ip: [127, 0, 0, 1].into(),
        udp_port_min: 0,
        udp_port_max: 0,
        tcp_port: None,
        token_secret: SECRET.to_vec(),
        resume_window,
        ..SfuConfig::default()
    })
    .await
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = sr_voice::router(handle.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
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
        + 120;
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

/// A toy transform, as in `voice_transport.rs`.
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

fn policy(min_backoff: Duration) -> ReconnectPolicy {
    ReconnectPolicy {
        max_attempts: 4,
        min_backoff,
        max_backoff: Duration::from_secs(4),
    }
}

async fn join(sfu: &Sfu, user: u64, e2ee: bool, backoff: Duration) -> VoiceLink {
    let mut link = VoiceLink::connect(update(sfu, user, e2ee), LOCAL, policy(backoff))
        .await
        .unwrap();
    if e2ee {
        link.session().set_transform(Flip);
    }
    link
}

/// Pump both links for `rounds` rounds of 30 ms and collect what each saw.
async fn run(
    a: &mut VoiceLink,
    b: &mut VoiceLink,
    rounds: u32,
) -> (Vec<VoiceEvent>, Vec<VoiceEvent>) {
    let (mut sa, mut sb) = (Vec::new(), Vec::new());
    for _ in 0..rounds {
        let (ea, eb) = tokio::join!(
            a.pump(Duration::from_millis(30)),
            b.pump(Duration::from_millis(30))
        );
        sa.extend(ea.unwrap());
        sb.extend(eb.unwrap());
    }
    (sa, sb)
}

async fn connect_both(a: &mut VoiceLink, b: &mut VoiceLink) {
    for _ in 0..200 {
        run(a, b, 1).await;
        if a.session().is_connected() && b.session().is_connected() {
            break;
        }
    }
    assert!(a.session().is_connected() && b.session().is_connected());
    // Let the offers for the new audio lines settle.
    run(a, b, 20).await;
}

/// `a` speaks `frames` frames; return how many of them `b` heard from user `from`.
async fn talk(a: &mut VoiceLink, b: &mut VoiceLink, from: u64, frames: u32) -> usize {
    a.session().set_speaking(true);
    let mut heard = 0;
    for n in 0..frames {
        let mut f = vec![0x5A; 60];
        f[..4].copy_from_slice(&n.to_be_bytes());
        a.session().send_opus(&f, -28).unwrap();
        let (_, eb) = run(a, b, 1).await;
        heard += count_audio(&eb, from);
    }
    // The tail still in flight.
    let (_, eb) = run(a, b, 10).await;
    heard + count_audio(&eb, from)
}

fn count_audio(events: &[VoiceEvent], from: u64) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, VoiceEvent::Audio { user, frame } if *user == from && frame.len() == 60))
        .count()
}

#[tokio::test]
async fn a_dropped_websocket_resumes_and_the_audio_keeps_flowing() {
    let sfu = start(Duration::from_secs(20)).await;
    let mut a = join(&sfu, 1, false, Duration::from_millis(100)).await;
    let mut b = join(&sfu, 2, false, Duration::from_millis(100)).await;
    connect_both(&mut a, &mut b).await;
    assert!(
        talk(&mut a, &mut b, 1, 30).await >= 15,
        "audio before the drop"
    );

    a.session().abandon_signalling().await;
    let mut resumed = None;
    for _ in 0..100 {
        let (ea, _) = run(&mut a, &mut b, 1).await;
        if let Some(VoiceEvent::Resumed { participants }) = ea
            .into_iter()
            .find(|e| matches!(e, VoiceEvent::Resumed { .. }))
        {
            resumed = Some(participants);
            break;
        }
    }
    assert_eq!(
        resumed,
        Some(vec![2]),
        "RESUMED lists who is in the channel"
    );
    assert!(
        a.session().is_connected(),
        "the media connection never went down"
    );
    assert!(
        talk(&mut a, &mut b, 1, 30).await >= 15,
        "audio after the resume"
    );
    sfu.handle.shutdown().await;
}

#[tokio::test]
async fn a_session_the_sfu_forgot_is_joined_again_with_the_transform_kept() {
    // The SFU drops a participant whose socket stays away for a second; the link waits three before trying.
    let sfu = start(Duration::from_secs(1)).await;
    let mut a = join(&sfu, 1, true, Duration::from_secs(3)).await;
    let mut b = join(&sfu, 2, true, Duration::from_millis(100)).await;
    connect_both(&mut a, &mut b).await;
    assert!(
        talk(&mut a, &mut b, 1, 30).await >= 15,
        "encrypted audio before"
    );

    a.session().abandon_signalling().await;
    let mut rejoined = false;
    for _ in 0..400 {
        let (ea, _) = run(&mut a, &mut b, 1).await;
        assert!(
            !ea.iter()
                .any(|e| matches!(e, VoiceEvent::Resumed { .. })),
            "the session was gone, a resume must not succeed"
        );
        if ea
            .iter()
            .any(|e| matches!(e, VoiceEvent::Reconnected { .. }))
        {
            rejoined = true;
            break;
        }
    }
    assert!(rejoined, "the link joined again");
    // The new media connection comes up, and the Flip transform is still installed (the call is encrypted,
    // so a missing transform would refuse to send, and B would drop undecryptable frames).
    for _ in 0..200 {
        run(&mut a, &mut b, 1).await;
        if a.session().is_connected() {
            break;
        }
    }
    assert!(a.session().is_connected());
    run(&mut a, &mut b, 20).await;
    assert!(
        talk(&mut a, &mut b, 1, 30).await >= 15,
        "encrypted audio after"
    );
    sfu.handle.shutdown().await;
}

#[tokio::test]
async fn a_removal_from_the_call_is_final() {
    let sfu = start(Duration::from_secs(20)).await;
    let mut a = join(&sfu, 1, false, Duration::from_millis(100)).await;
    let mut b = join(&sfu, 2, false, Duration::from_millis(100)).await;
    connect_both(&mut a, &mut b).await;
    sfu.handle
        .apply(VoiceCommand::Disconnect {
            channel: ChannelId::new(CHANNEL),
            user: UserId::new(1),
        })
        .await;
    let mut closed = None;
    for _ in 0..100 {
        let (ea, _) = run(&mut a, &mut b, 1).await;
        assert!(
            !ea.iter().any(|e| matches!(
                e,
                VoiceEvent::Resumed { .. } | VoiceEvent::Reconnected { .. }
            )),
            "a moderator removal is not retried"
        );
        if let Some(VoiceEvent::Closed(code)) =
            ea.into_iter().find(|e| matches!(e, VoiceEvent::Closed(_)))
        {
            closed = Some(code);
            break;
        }
    }
    assert_eq!(closed, Some(4014));
    sfu.handle.shutdown().await;
}
