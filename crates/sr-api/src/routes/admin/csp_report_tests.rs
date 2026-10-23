//! RDY-SEC-009: `POST /internal/csp-report` is public, capped and rate limited, counts without keeping anything
//! personal, and the counters reach `GET /admin/system/csp-reports` (`OPERATOR`).

use super::{BASIC, Env, OWNER, env};
use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

async fn post(e: &Env, content_type: &str, body: Vec<u8>) -> StatusCode {
    let req = Request::builder()
        .method(Method::POST)
        .uri("/internal/csp-report")
        .header("content-type", content_type)
        .body(Body::from(body))
        .unwrap();
    e.app.clone().oneshot(req).await.unwrap().status()
}

const LEGACY: &str = r#"{"csp-report":{"document-uri":"https://chat.example.org/library/series/123456789012345678/read?token=SECRET#frag","referrer":"https://elsewhere.test/?me=1","violated-directive":"script-src-elem 'self'","effective-directive":"script-src-elem","blocked-uri":"https://cdn.bad.test/x.js?u=SECRET","source-file":"https://chat.example.org/a.js","script-sample":"SECRET"}}"#;

#[tokio::test]
async fn a_report_is_counted_without_credentials_and_read_by_an_operator() {
    let e = env().await;
    assert_eq!(
        post(&e, "application/csp-report", LEGACY.as_bytes().to_vec()).await,
        StatusCode::NO_CONTENT
    );
    let api = r#"[{"type":"csp-violation","url":"https://chat.example.org/lounge","body":{"documentURL":"https://chat.example.org/lounge/42","blockedURL":"inline","effectiveDirective":"style-src-elem"}}]"#;
    assert_eq!(
        post(&e, "application/reports+json", api.as_bytes().to_vec()).await,
        StatusCode::NO_CONTENT
    );
    let (s, b) = e
        .call(Some(OWNER), Method::GET, "/admin/system/csp-reports", None)
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let keys = |k: &str| -> Vec<String> {
        b[k].as_array()
            .unwrap()
            .iter()
            .map(|r| r["key"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(keys("directives"), ["script-src-elem", "style-src-elem"]);
    assert_eq!(keys("origins"), ["https://cdn.bad.test", "inline"]);
    assert_eq!(keys("paths"), ["/library/series/:id/read", "/lounge/:id"]);
    assert_eq!(b["directives"][0]["count"], 1);
    let text = b.to_string();
    for banned in ["SECRET", "token", "frag", "elsewhere", "a.js", "me=1"] {
        assert!(!text.contains(banned), "{banned} was stored: {text}");
    }
}

#[tokio::test]
async fn the_counters_are_for_operators_only() {
    let e = env().await;
    let (s, _) = e
        .call(None, Method::GET, "/admin/system/csp-reports", None)
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = e
        .call(Some(BASIC), Method::GET, "/admin/system/csp-reports", None)
        .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn oversize_unreadable_and_wrong_type_bodies_are_refused() {
    let e = env().await;
    let big = vec![b' '; 9 * 1024];
    assert_eq!(
        post(&e, "application/csp-report", big).await,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(
        post(&e, "application/csp-report", b"not json".to_vec()).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        post(&e, "text/html", LEGACY.as_bytes().to_vec()).await,
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    let (_, b) = e
        .call(Some(OWNER), Method::GET, "/admin/system/csp-reports", None)
        .await;
    assert!(b["directives"].as_array().unwrap().is_empty(), "{b}");
}

#[tokio::test]
async fn a_flood_from_one_address_is_limited() {
    let e = env().await;
    let mut seen = Vec::new();
    for _ in 0..14 {
        seen.push(post(&e, "application/csp-report", LEGACY.as_bytes().to_vec()).await);
    }
    assert_eq!(seen[..10], [StatusCode::NO_CONTENT; 10], "{seen:?}");
    assert!(
        seen[10..].contains(&StatusCode::TOO_MANY_REQUESTS),
        "{seen:?}"
    );
    let resp = {
        let req = Request::builder()
            .method(Method::POST)
            .uri("/internal/csp-report")
            .header("content-type", "application/csp-report")
            .body(Body::from(LEGACY))
            .unwrap();
        e.app.clone().oneshot(req).await.unwrap()
    };
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    assert!(v.is_null() || v["code"].is_number());
}
