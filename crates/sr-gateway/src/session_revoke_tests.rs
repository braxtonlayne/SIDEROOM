//! SEC-AUD-002: a revoked sign-in session must not keep a live gateway socket.

use std::time::Duration;

use sr_core::UserId;
use sr_events::DomainEvent;

use crate::test_support::{TestClient, TestWorld};
use crate::{GatewayConfig, close, intents};

/// Connect and identify with `token` (no guilds).
async fn connect(w: &TestWorld, token: &str) -> TestClient {
    let mut c = w.connect();
    c.identify(token, intents::GUILDS, 0).await;
    c
}

/// A long periodic re-check, so only the bus event can close a socket within the test.
fn event_only() -> GatewayConfig {
    GatewayConfig {
        session_recheck: Duration::from_secs(3600),
        ..GatewayConfig::default()
    }
}

#[tokio::test]
async fn a_revoked_session_closes_its_socket_and_keeps_the_others() {
    let w = TestWorld::new(event_only());
    let mut phone = connect(&w, "user:7:0:phone").await;
    let mut laptop = connect(&w, "user:7:0:laptop").await;
    let mut other = connect(&w, "user:8:0:x").await;
    // Logout on the phone: its row is revoked, then the API publishes the event.
    w.auth.revoke("user:7:0:phone");
    w.emit(DomainEvent::SessionsRevoked {
        user: UserId::new(7),
    })
    .await;
    assert_eq!(phone.recv_close().await, close::AUTHENTICATION_FAILED);
    laptop.assert_silent().await;
    other.assert_silent().await;
}

#[tokio::test]
async fn logout_all_closes_every_socket_of_the_account_only() {
    let w = TestWorld::new(event_only());
    let mut a = connect(&w, "user:7:0:a").await;
    let mut b = connect(&w, "user:7:0:b").await;
    let mut other = connect(&w, "user:8:0:x").await;
    w.auth.revoke("user:7:0:a");
    w.auth.revoke("user:7:0:b");
    w.emit(DomainEvent::SessionsRevoked {
        user: UserId::new(7),
    })
    .await;
    assert_eq!(a.recv_close().await, close::AUTHENTICATION_FAILED);
    assert_eq!(b.recv_close().await, close::AUTHENTICATION_FAILED);
    other.assert_silent().await;
    // Another account's revocation event does not touch user 8's live session either.
    w.emit(DomainEvent::SessionsRevoked {
        user: UserId::new(8),
    })
    .await;
    other.assert_silent().await;
}

#[tokio::test]
async fn the_periodic_recheck_closes_a_revoked_session_without_an_event() {
    let w = TestWorld::new(GatewayConfig {
        session_recheck: Duration::from_millis(200),
        ..GatewayConfig::default()
    });
    let mut gone = connect(&w, "user:7:0:a").await;
    let mut kept = connect(&w, "user:8:0:x").await;
    // A revocation that published nothing (a Person ban, the moderation sweep).
    w.auth.revoke("user:7:0:a");
    let code = tokio::time::timeout(Duration::from_secs(5), gone.recv_close())
        .await
        .expect("the re-check should close the socket");
    assert_eq!(code, close::AUTHENTICATION_FAILED);
    // The live session survives several re-checks.
    assert!(
        tokio::time::timeout(Duration::from_millis(700), kept.recv_close())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_revoked_session_cannot_resume() {
    let w = TestWorld::new(event_only());
    let mut c = w.connect();
    let ready = c.identify("user:7:0:a", intents::GUILDS, 0).await;
    let sid = ready["d"]["session_id"].as_str().unwrap().to_owned();
    w.auth.revoke("user:7:0:a");
    w.emit(DomainEvent::SessionsRevoked {
        user: UserId::new(7),
    })
    .await;
    assert_eq!(c.recv_close().await, close::AUTHENTICATION_FAILED);
    let mut again = w.connect();
    again.recv().await;
    again
        .send(
            serde_json::json!({"op": 6, "d": {"token": "user:7:0:a", "session_id": sid, "seq": 0}}),
        )
        .await;
    assert_eq!(again.recv_close().await, close::AUTHENTICATION_FAILED);
}
