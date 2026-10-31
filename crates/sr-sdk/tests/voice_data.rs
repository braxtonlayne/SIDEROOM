//! ACT-CH-001: two bots exchange activity data through the real SFU (in process, loopback): the SCTP data
//! channel is opened with `ConnectOptions::data`, frames are sealed with `voice::DataChannel` under the call's
//! epoch key and relayed only within the instance both joined. The SFU's side of the relay (scope, budget) is
//! tested in crates/sr-voice/tests/datachannel.rs; this checks the bot's half end to end.
//!
//! Run with `cargo test -p sr-sdk --features voice-transport,voice-e2ee --test voice_data`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sr_core::{ChannelId, GuildId, UserId};
use sr_e2ee::SenderSecret;
use sr_sdk::voice::{DataChannel, EpochKeys};
use sr_sdk::voice_transport::{ConnectOptions, VoiceEvent, VoiceServerUpdate, VoiceSession};
use sr_voice::SfuConfig;
use sr_voice_proto::{JoinClaims, TokenSigner, VoiceRelay};

const SECRET: &[u8] = b"test-secret-test-secret-test-secret";
const CHANNEL: u64 = 710;

async fn start() -> (sr_voice::SfuHandle, String) {
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
    let keyer = handle.clone();
    tokio::spawn(async move {
        loop {
            for user in 1..=3u64 {
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
    (handle, format!("ws://{addr}/voice/ws"))
}

fn update(url: &str, user: u64) -> VoiceServerUpdate {
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
        "endpoint": url,
        "guild_id": "9",
        "channel_id": CHANNEL.to_string(),
        "sr_e2ee": true,
    }))
    .unwrap()
}

fn keys(epoch: u64) -> EpochKeys {
    EpochKeys {
        epoch,
        own_leaf: 0,
        senders: (1..=3u64)
            .map(|u| (u, SenderSecret::new([u as u8; 16])))
            .collect(),
    }
}

async fn pump2(
    a: &mut VoiceSession,
    b: &mut VoiceSession,
    ms: u64,
) -> (Vec<VoiceEvent>, Vec<VoiceEvent>) {
    let (ea, eb) = tokio::join!(
        a.pump(Duration::from_millis(ms)),
        b.pump(Duration::from_millis(ms))
    );
    (ea.unwrap(), eb.unwrap())
}

#[tokio::test]
async fn bots_exchange_sealed_data_within_an_instance() {
    let (_sfu, url) = start().await;
    let opts = ConnectOptions {
        data: true,
        ..ConnectOptions::default()
    };
    let mut a = VoiceSession::connect_with(&update(&url, 1), [127, 0, 0, 1].into(), opts)
        .await
        .unwrap();
    let mut b = VoiceSession::connect_with(&update(&url, 2), [127, 0, 0, 1].into(), opts)
        .await
        .unwrap();
    let mut da = DataChannel::new(1, [1, 2, 3, 4]);
    let mut db = DataChannel::new(2, [5, 6, 7, 8]);
    for d in [&mut da, &mut db] {
        d.prepare(&keys(1), 0).unwrap();
        assert!(d.execute(1));
    }

    // Wait for both data channels to open, then both join instance 42; only `a` joins 43.
    let (mut a_open, mut b_open) = (false, false);
    for _ in 0..200 {
        let (ea, eb) = pump2(&mut a, &mut b, 50).await;
        a_open |= ea.iter().any(|e| matches!(e, VoiceEvent::DataOpen));
        b_open |= eb.iter().any(|e| matches!(e, VoiceEvent::DataOpen));
        if a_open && b_open {
            break;
        }
    }
    assert!(a_open && b_open, "data channels did not open");
    assert!(a.send_data(&da.join(42)).unwrap());
    assert!(a.send_data(&da.join(43)).unwrap());
    assert!(b.send_data(&db.join(42)).unwrap());
    for _ in 0..4 {
        pump2(&mut a, &mut b, 30).await;
    }

    // `a` sends into 42 and 43; `b` hears only 42, opened under a's key, and the SFU saw sealed bytes.
    let sealed = da.seal(42, b"hello from a").unwrap();
    assert!(!sealed.windows(5).any(|w| w == b"hello"));
    assert!(a.send_data(&sealed).unwrap());
    assert!(
        a.send_data(&da.seal(43, b"other instance").unwrap())
            .unwrap()
    );
    let mut heard = Vec::new();
    for _ in 0..10 {
        let (_, eb) = pump2(&mut a, &mut b, 30).await;
        for e in eb {
            if let VoiceEvent::Data { frame } = e {
                heard.push(db.open(&frame, 1).expect("opens"));
            }
        }
    }
    assert_eq!(heard.len(), 1, "{heard:?}");
    assert_eq!((heard[0].instance, heard[0].from), (42, 1));
    assert_eq!(heard[0].payload, b"hello from a");

    // Before the first `execute` nothing is sealed.
    let mut fresh = DataChannel::new(1, [9, 9, 9, 9]);
    assert!(fresh.seal(42, b"x").is_err());
}
