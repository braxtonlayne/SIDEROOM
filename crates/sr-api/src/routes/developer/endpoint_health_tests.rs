//! Endpoint auto-disable (G-05, D-186) through the real router.

use super::endpoint_tests::{Setup, URL, setup_default};
use super::tests::{BOB, OWNER, Who, next_named};
use axum::http::StatusCode;
use serde_json::{Value, json};
use std::{sync::atomic::Ordering, time::Duration};

#[tokio::test]
async fn twenty_five_failures_disable_the_endpoint_until_the_owner_verifies_it_again() {
    let s: Setup = setup_default().await;
    let _ = s.key().await;
    assert_eq!(s.set_url(json!(URL)).await.0, StatusCode::OK);
    assert_eq!(
        s.app_json().await["interactions_endpoint_state"],
        json!("enabled")
    );
    s.dev.fail.store(true, Ordering::SeqCst);
    for _ in 0..25 {
        assert_eq!(s.invoke().await.0, StatusCode::NO_CONTENT);
    }
    // deliveries are counted in the background; wait for the 25th
    let mut state = Value::Null;
    for _ in 0..150 {
        state = s.app_json().await["interactions_endpoint_state"].clone();
        if state == json!("disabled") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(state, json!("disabled"));
    // the owner got exactly one system DM, naming the way back
    let dms = s.e.dms.0.lock().unwrap().clone();
    assert_eq!(dms.len(), 1);
    assert_eq!(dms[0].0, OWNER);
    assert!(dms[0].1.description.contains("Verify and re-enable"));

    // the URL stays, but interactions use the gateway again
    let mut bot_events = s.e.user_events(s.bot.parse().unwrap());
    assert_eq!(s.invoke().await.0, StatusCode::NO_CONTENT);
    next_named(&mut bot_events, "INTERACTION_CREATE").await;
    let seen = s.dev.seen.lock().unwrap().clone();
    assert_eq!(seen.iter().filter(|v| v["type"] == 2).count(), 25);

    // only the owner verifies; a healthy PING check re-enables
    let verify = format!("/applications/{}/endpoint/verify", s.app);
    let (st, v) = s.e.post(Who::Session(BOB), &verify, json!({})).await;
    assert_eq!(st, StatusCode::NOT_FOUND, "{v}");
    let (st, v) = s.e.post(Who::Session(OWNER), &verify, json!({})).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["interactions_endpoint_state"], json!("enabled"));
    assert_eq!(
        s.app_json().await["interactions_endpoint_state"],
        json!("enabled")
    );
}

#[tokio::test]
async fn a_success_resets_the_failure_streak() {
    let s: Setup = setup_default().await;
    let _ = s.key().await;
    assert_eq!(s.set_url(json!(URL)).await.0, StatusCode::OK);
    s.dev.fail.store(true, Ordering::SeqCst);
    for _ in 0..24 {
        assert_eq!(s.invoke().await.0, StatusCode::NO_CONTENT);
    }
    tokio::time::sleep(Duration::from_millis(400)).await;
    s.dev.fail.store(false, Ordering::SeqCst);
    let mut invoker = s.e.user_events(BOB);
    assert_eq!(s.invoke().await.0, StatusCode::NO_CONTENT);
    next_named(&mut invoker, "INTERACTION_SUCCESS").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    s.dev.fail.store(true, Ordering::SeqCst);
    for _ in 0..24 {
        assert_eq!(s.invoke().await.0, StatusCode::NO_CONTENT);
    }
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        s.app_json().await["interactions_endpoint_state"],
        json!("enabled")
    );
    assert!(s.e.dms.0.lock().unwrap().is_empty());
}
