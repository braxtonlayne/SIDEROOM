//! SEC-AUD-010: `/gateway` limits for clients that have not identified yet: a message-size cap, an
//! IDENTIFY deadline that heartbeats do not extend, and a per-address cap on such sockets.

use std::net::IpAddr;
use std::time::Duration;

use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::test_support::{TestClient, TestWorld, token_for};
use crate::{ConnectParams, GatewayConfig, MAX_MESSAGE, close, intents, routes};

fn from(ip: &str) -> ConnectParams {
    ConnectParams {
        peer: Some(ip.parse::<IpAddr>().unwrap()),
        ..ConnectParams::default()
    }
}

#[tokio::test]
async fn heartbeats_do_not_keep_an_unidentified_socket_open() {
    let w = TestWorld::new(GatewayConfig {
        identify_timeout: Duration::from_millis(300),
        ..GatewayConfig::default()
    });
    let mut c = w.connect();
    c.recv().await; // HELLO
    assert_eq!(w.gw.unidentified_count(), 1);
    let closed = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let _ = c.try_send(json!({"op": 1, "d": null})).await;
            let r = tokio::time::timeout(Duration::from_millis(50), c.frame()).await;
            if let Ok(crate::Frame::Close { code, .. }) = r {
                return code;
            }
            // One heartbeat per 50 ms (well inside the 120-ops-a-minute budget).
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("an unidentified socket must be closed");
    assert_eq!(closed, close::SESSION_TIMED_OUT);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(w.gw.unidentified_count(), 0);
}

#[tokio::test]
async fn an_identified_socket_outlives_the_identify_deadline() {
    let w = TestWorld::new(GatewayConfig {
        identify_timeout: Duration::from_millis(200),
        ..GatewayConfig::default()
    });
    let mut c = w.connect();
    c.identify(&token_for(1), intents::GUILDS, 0).await;
    assert_eq!(w.gw.unidentified_count(), 0);
    tokio::time::sleep(Duration::from_millis(400)).await;
    c.send(json!({"op": 1, "d": null})).await;
    assert_eq!(c.recv().await["op"], 11);
}

#[tokio::test]
async fn unidentified_sockets_are_capped_per_address() {
    let w = TestWorld::new(GatewayConfig {
        max_unidentified_per_ip: 2,
        ..GatewayConfig::default()
    });
    let mut a1 = TestClient::connect_with(&w.gw, from("203.0.113.5"));
    let mut a2 = TestClient::connect_with(&w.gw, from("203.0.113.5"));
    a1.recv().await;
    a2.recv().await;
    // A third socket from the same address is refused; another address is not.
    let mut a3 = TestClient::connect_with(&w.gw, from("203.0.113.5"));
    assert_eq!(a3.recv_close().await, close::RATE_LIMITED);
    let mut b = TestClient::connect_with(&w.gw, from("198.51.100.7"));
    assert_eq!(b.recv().await["op"], 10);
    assert_eq!(w.gw.unidentified_count(), 3);
    // Once one of them identifies, its address may open another.
    a1.send(json!({"op": 2, "d": {"token": token_for(1), "intents": intents::GUILDS, "properties": {}}}))
        .await;
    a1.expect_event("READY").await;
    let mut a4 = TestClient::connect_with(&w.gw, from("203.0.113.5"));
    assert_eq!(a4.recv().await["op"], 10);
}

// ---- the size cap, over a real socket -------------------------------------------------------

/// A client text frame (masked with a zero key, so the payload goes as is).
fn text_frame(payload: &[u8]) -> Vec<u8> {
    let mut f = vec![0x81];
    let n = payload.len();
    if n < 126 {
        f.push(0x80 | u8::try_from(n).unwrap());
    } else if let Ok(n) = u16::try_from(n) {
        f.push(0x80 | 126);
        f.extend_from_slice(&n.to_be_bytes());
    } else {
        f.push(0x80 | 127);
        f.extend_from_slice(&(n as u64).to_be_bytes());
    }
    f.extend_from_slice(&[0, 0, 0, 0]);
    f.extend_from_slice(payload);
    f
}

/// Read one server frame; `None` once the socket closed or a close frame came.
async fn read_frame(s: &mut TcpStream) -> Option<String> {
    let mut h = [0u8; 2];
    s.read_exact(&mut h).await.ok()?;
    let mut len = u64::from(h[1] & 0x7f);
    if len == 126 {
        let mut b = [0u8; 2];
        s.read_exact(&mut b).await.ok()?;
        len = u64::from(u16::from_be_bytes(b));
    } else if len == 127 {
        let mut b = [0u8; 8];
        s.read_exact(&mut b).await.ok()?;
        len = u64::from_be_bytes(b);
    }
    let mut body = vec![0u8; usize::try_from(len).ok()?];
    s.read_exact(&mut body).await.ok()?;
    if h[0] & 0x0f == 0x8 {
        return None;
    }
    Some(String::from_utf8_lossy(&body).into_owned())
}

/// Open `/gateway` on a real listener and read HELLO.
async fn open(addr: std::net::SocketAddr) -> TcpStream {
    let mut s = TcpStream::connect(addr).await.unwrap();
    s.write_all(
        b"GET /gateway?v=1&encoding=json HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\n\
          Connection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
          Sec-WebSocket-Version: 13\r\n\r\n",
    )
    .await
    .unwrap();
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        let mut b = [0u8; 1];
        s.read_exact(&mut b).await.unwrap();
        head.push(b[0]);
    }
    assert!(
        head.starts_with(b"HTTP/1.1 101"),
        "{}",
        String::from_utf8_lossy(&head)
    );
    let hello = read_frame(&mut s).await.expect("HELLO");
    assert!(hello.contains("\"op\":10"), "{hello}");
    s
}

/// A heartbeat padded to `size` bytes; any size the server accepts is answered with op 11.
fn heartbeat(size: usize) -> Vec<u8> {
    let head = r#"{"op":1,"d":null,"pad":""#;
    let pad = size - head.len() - 2;
    format!("{head}{}\"}}", "a".repeat(pad)).into_bytes()
}

#[tokio::test]
async fn messages_over_the_cap_close_the_socket_unanswered() {
    let w = TestWorld::new(GatewayConfig::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = routes().with_state(w.gw.clone());
    tokio::spawn(async move { axum::serve(listener, app).await });

    // The largest legitimate client op (an E2EE commit, under 200 KiB) still goes through.
    let mut s = open(addr).await;
    s.write_all(&text_frame(&heartbeat(200 * 1024)))
        .await
        .unwrap();
    let ack = tokio::time::timeout(Duration::from_secs(10), read_frame(&mut s))
        .await
        .unwrap()
        .expect("an answer");
    assert!(ack.contains("\"op\":11"), "{ack}");

    // One byte over the cap: no answer, the socket is closed.
    let mut s = open(addr).await;
    let _ = s.write_all(&text_frame(&heartbeat(MAX_MESSAGE + 1))).await;
    let answer = tokio::time::timeout(Duration::from_secs(10), read_frame(&mut s))
        .await
        .expect("the server must close the socket");
    assert_eq!(answer, None, "an oversized message was processed");
}
