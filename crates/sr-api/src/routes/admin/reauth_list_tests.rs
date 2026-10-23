//! RDY-SEC-010: every route in `SENSITIVE_ACTIONS` enforces the second-factor window. One test walks the list
//! through the real router with the window demanded, so a row added to the list without a working gate (or a
//! route renamed under it) fails here.

use super::{Env, MODER, OWNER, env_with, totp_now};
use crate::sensitive::{Gate, SENSITIVE_ACTIONS};
use axum::http::{Method, StatusCode};
use serde_json::json;

fn concrete(template: &str) -> String {
    template
        .split('/')
        .map(|seg| match seg {
            "{key}" => "k".to_owned(),
            s if s.starts_with('{') => "1".to_owned(),
            s => s.to_owned(),
        })
        .collect::<Vec<_>>()
        .join("/")
}

async fn code_of(e: &Env, who: i64, method: &Method, path: &str) -> (StatusCode, Option<u64>) {
    let (s, b) = e
        .call(
            Some(who),
            method.clone(),
            path,
            Some(json!({ "reason": "drill" })),
        )
        .await;
    (s, b["code"].as_u64())
}

#[tokio::test]
async fn every_listed_staff_route_demands_the_second_factor() {
    let e = env_with(true).await;
    let rows: Vec<_> = SENSITIVE_ACTIONS
        .iter()
        .filter(|a| a.gate == Gate::StaffSecondFactor)
        .collect();
    assert!(rows.len() >= 20);
    // Closed window: the enrolled moderator (stale) and the Owner (nothing enrolled) are both turned back
    // before any handler or permission check runs.
    for a in &rows {
        let method = Method::from_bytes(a.method.as_bytes()).unwrap();
        let path = concrete(a.path);
        for who in [MODER, OWNER] {
            assert_eq!(
                code_of(&e, who, &method, &path).await,
                (StatusCode::FORBIDDEN, Some(900_004)),
                "{} {} for {who} ({})",
                a.method,
                a.path,
                a.id
            );
        }
    }
    // Open the window for the moderator: no listed route answers 900004 any more (what they answer
    // instead belongs to their own tests).
    let (s, st) = e
        .call(
            Some(MODER),
            Method::POST,
            "/admin/reauth",
            Some(json!({ "method": "totp", "code": totp_now(&e) })),
        )
        .await;
    assert_eq!((s, st["fresh"].as_bool()), (StatusCode::OK, Some(true)));
    for a in &rows {
        let method = Method::from_bytes(a.method.as_bytes()).unwrap();
        let (_, code) = code_of(&e, MODER, &method, &concrete(a.path)).await;
        assert_ne!(code, Some(900_004), "{} {} stays closed", a.method, a.path);
    }
    // Fifteen minutes on, the window closes again for every row.
    e.clock.advance_ms(16 * 60_000);
    for a in &rows {
        let method = Method::from_bytes(a.method.as_bytes()).unwrap();
        assert_eq!(
            code_of(&e, MODER, &method, &concrete(a.path)).await,
            (StatusCode::FORBIDDEN, Some(900_004)),
            "{} {} after the window",
            a.method,
            a.path
        );
    }
}

#[tokio::test]
async fn an_unlisted_route_is_not_gated() {
    let e = env_with(true).await;
    let (s, _) = e.call(Some(MODER), Method::GET, "/admin/me", None).await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test]
async fn the_gate_is_off_when_the_window_is_not_demanded() {
    let e = env_with(false).await;
    let (_, code) = code_of(&e, MODER, &Method::POST, "/admin/accounts/20/ban").await;
    assert_ne!(code, Some(900_004));
}
