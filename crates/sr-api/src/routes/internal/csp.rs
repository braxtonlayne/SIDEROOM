//! `POST /internal/csp-report`: count Content-Security-Policy violations without keeping anything personal.

use crate::{AppState, HttpError, rate_limit::client_ip, routes::Ext};
use async_trait::async_trait;
use axum::{
    extract::{Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use governor::{Quota, RateLimiter, clock::DefaultClock, state::keyed::DefaultKeyedStateStore};
use serde::Serialize;
use serde_json::Value;
use sr_core::ErrorCode;
use std::{
    collections::BTreeMap,
    num::NonZeroU32,
    sync::{Arc, Mutex},
};
use utoipa::ToSchema;

/// Largest body read; a real report is well under 2 KiB.
pub const MAX_BODY: usize = 8 * 1024;
/// Reports taken from one `application/reports+json` list.
const MAX_PER_BODY: usize = 20;
/// Longest key kept for an origin or a path.
const MAX_KEY: usize = 120;

/// What one report is reduced to. Nothing else of the report survives parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// The violated directive name, for example `script-src-elem`; `unknown` when the report names none.
    pub directive: String,
    /// The blocked origin (`https://cdn.example`), or a keyword (`inline`, `eval`, `data`, `blob`, `wasm-eval`,
    /// `other`).
    pub origin: String,
    /// The path of the document the violation happened on: no host, no query, no fragment, ids folded.
    pub path: String,
}

/// Where the counters live (Postgres in the server, memory in tests).
#[async_trait]
pub trait CspReportStore: Send + Sync {
    async fn record(&self, v: &Violation);
    /// The top `limit` keys of each kind.
    async fn top(&self, limit: usize) -> Result<CspSnapshot, String>;
}

/// One counted key.
#[derive(Debug, Clone, Serialize, ToSchema, PartialEq, Eq)]
pub struct CspCounter {
    pub key: String,
    pub count: u64,
    /// Unix milliseconds of the latest report with this key.
    pub last_seen_ms: i64,
}

/// What the Admin > System card shows.
#[derive(Debug, Clone, Default, Serialize, ToSchema, PartialEq, Eq)]
pub struct CspSnapshot {
    /// Reports per violated directive.
    pub directives: Vec<CspCounter>,
    /// Reports per blocked origin.
    pub origins: Vec<CspCounter>,
    /// Reports per document path.
    pub paths: Vec<CspCounter>,
}

/// (kind, key) to (count, last seen).
type Rows = BTreeMap<(&'static str, String), (u64, i64)>;

/// In-memory counters for tests and a server without a database.
#[derive(Default)]
pub struct MemoryCspReports {
    rows: Mutex<Rows>,
}

impl MemoryCspReports {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl CspReportStore for MemoryCspReports {
    async fn record(&self, v: &Violation) {
        let mut rows = self.rows.lock().unwrap_or_else(|e| e.into_inner());
        for (kind, key) in [
            ("directive", &v.directive),
            ("origin", &v.origin),
            ("path", &v.path),
        ] {
            if key.is_empty() {
                continue;
            }
            let e = rows.entry((kind, key.clone())).or_insert((0, 0));
            e.0 += 1;
            e.1 += 1;
        }
    }

    async fn top(&self, limit: usize) -> Result<CspSnapshot, String> {
        let rows = self.rows.lock().unwrap_or_else(|e| e.into_inner());
        let pick = |kind: &str| {
            let mut v: Vec<CspCounter> = rows
                .iter()
                .filter(|((k, _), _)| *k == kind)
                .map(|((_, key), (count, seen))| CspCounter {
                    key: key.clone(),
                    count: *count,
                    last_seen_ms: *seen,
                })
                .collect();
            v.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.key.cmp(&b.key)));
            v.truncate(limit);
            v
        };
        Ok(CspSnapshot {
            directives: pick("directive"),
            origins: pick("origin"),
            paths: pick("path"),
        })
    }
}

type Keyed = RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>;

/// The service behind the route: the store and its limits (30 reports a minute per address with a burst of 10,
/// 600 a minute in all).
pub struct CspReports {
    pub store: Arc<dyn CspReportStore>,
    per_ip: Keyed,
    total: RateLimiter<governor::state::NotKeyed, governor::state::InMemoryState, DefaultClock>,
}

impl CspReports {
    pub fn new(store: Arc<dyn CspReportStore>) -> Self {
        let per_ip = Quota::per_minute(NonZeroU32::new(30).unwrap_or(NonZeroU32::MIN))
            .allow_burst(NonZeroU32::new(10).unwrap_or(NonZeroU32::MIN));
        let total = Quota::per_minute(NonZeroU32::new(600).unwrap_or(NonZeroU32::MIN));
        Self {
            store,
            per_ip: RateLimiter::keyed(per_ip),
            total: RateLimiter::direct(total),
        }
    }

    fn allow(&self, who: &str) -> bool {
        if self.per_ip.len() > 10_000 {
            self.per_ip.retain_recent();
            self.per_ip.shrink_to_fit();
        }
        self.total.check().is_ok() && self.per_ip.check_key(&who.to_owned()).is_ok()
    }
}

/// Browsers report here (public, rate limited, body capped, counters only). Answers 204 for anything it could
/// read, 400 for a body that is not a report, 413 over the cap and 429 over the limits.
#[utoipa::path(post, path = "/internal/csp-report", responses((status = 204)))]
pub async fn csp_report(
    State(state): State<AppState>,
    Ext(reports): Ext<CspReports>,
    req: Request,
) -> Response {
    let who = client_ip(&req, state.settings().trust_forwarded_for)
        .map_or_else(|| "unknown".to_owned(), |ip| ip.to_string());
    if !reports.allow(&who) {
        return HttpError::from(ErrorCode::TooManyRequests).into_response();
    }
    if req
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
        .is_some_and(|n| n > MAX_BODY)
    {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !(content_type.starts_with("application/csp-report")
        || content_type.starts_with("application/reports+json")
        || content_type.starts_with("application/json"))
    {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }
    let Ok(body) = axum::body::to_bytes(req.into_body(), MAX_BODY).await else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };
    let found = parse_reports(&body);
    if found.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    for v in &found {
        crate::observability::record_csp_report(&v.directive);
        reports.store.record(v).await;
    }
    StatusCode::NO_CONTENT.into_response()
}

/// The violations in a report body, in either format. An unreadable body gives an empty list.
pub fn parse_reports(body: &[u8]) -> Vec<Violation> {
    let Ok(json) = serde_json::from_slice::<Value>(body) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    match &json {
        Value::Array(items) => {
            for item in items.iter().take(MAX_PER_BODY) {
                if item.get("type").and_then(Value::as_str) != Some("csp-violation") {
                    continue;
                }
                let b = item.get("body").unwrap_or(&Value::Null);
                let text = |k: &str| b.get(k).and_then(Value::as_str).unwrap_or("");
                let directive = [text("effectiveDirective"), text("violatedDirective")]
                    .into_iter()
                    .find(|s| !s.is_empty())
                    .unwrap_or("");
                let doc = [
                    text("documentURL"),
                    item.get("url").and_then(Value::as_str).unwrap_or(""),
                ]
                .into_iter()
                .find(|s| !s.is_empty())
                .unwrap_or("");
                out.push(violation(directive, text("blockedURL"), doc));
            }
        }
        Value::Object(map) => {
            if let Some(r) = map.get("csp-report") {
                let text = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or("");
                let directive = [text("effective-directive"), text("violated-directive")]
                    .into_iter()
                    .find(|s| !s.is_empty())
                    .unwrap_or("");
                out.push(violation(
                    directive,
                    text("blocked-uri"),
                    text("document-uri"),
                ));
            }
        }
        _ => {}
    }
    out
}

fn violation(directive: &str, blocked: &str, document: &str) -> Violation {
    Violation {
        directive: directive_name(directive),
        origin: blocked_origin(blocked),
        path: document_path(document),
    }
}

/// `script-src-elem 'self'` -> `script-src-elem`; anything that is not a directive name -> `unknown`.
pub fn directive_name(raw: &str) -> String {
    let first = raw
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if !first.is_empty()
        && first.len() <= 40
        && first.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
    {
        first
    } else {
        "unknown".to_owned()
    }
}

/// The origin of a blocked URL (scheme, host and port; no user info, path or query), or a keyword.
pub fn blocked_origin(raw: &str) -> String {
    let raw = raw.trim();
    let lower = raw.to_ascii_lowercase();
    for kw in ["inline", "eval", "wasm-eval", "data", "blob", "self"] {
        if lower == kw || lower.starts_with(&format!("{kw}:")) {
            return kw.to_owned();
        }
    }
    let Some((scheme, rest)) = lower.split_once("://") else {
        return "other".to_owned();
    };
    if scheme.is_empty()
        || !scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'-' || b == b'.')
    {
        return "other".to_owned();
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    if host.is_empty()
        || !host.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':' | b'[' | b']' | b'*')
        })
    {
        return "other".to_owned();
    }
    let mut s = format!("{scheme}://{host}");
    s.truncate(MAX_KEY);
    s
}

/// The path of a document URL: host, query and fragment dropped; numeric and long opaque segments become `:id`.
pub fn document_path(raw: &str) -> String {
    let raw = raw.trim();
    let path = match raw.split_once("://") {
        Some((_, rest)) => rest.find('/').map_or("/", |i| &rest[i..]),
        None => raw,
    };
    let path = path.split(['?', '#']).next().unwrap_or("");
    if !path.starts_with('/') {
        return String::new();
    }
    let folded: Vec<String> = path
        .split('/')
        .map(|seg| {
            let numeric = !seg.is_empty() && seg.bytes().all(|b| b.is_ascii_digit());
            let opaque = seg.len() >= 16 && seg.bytes().any(|b| b.is_ascii_digit());
            if numeric || opaque {
                ":id".to_owned()
            } else {
                seg.chars()
                    .filter(|c| c.is_ascii_alphanumeric() || "-_.~@%".contains(*c))
                    .collect()
            }
        })
        .collect();
    let mut s = folded.join("/");
    s.truncate(MAX_KEY);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_and_reporting_api_bodies_reduce_to_three_fields() {
        let legacy = br#"{"csp-report":{"document-uri":"https://chat.example.org/library/series/123456789012345678/read?token=secret#frag","referrer":"https://evil.test/?q=me","violated-directive":"script-src-elem","effective-directive":"script-src-elem","original-policy":"default-src 'self'","blocked-uri":"https://cdn.bad.test/x.js?u=1","source-file":"https://chat.example.org/a.js","line-number":3,"script-sample":"alert(1)"}}"#;
        let v = parse_reports(legacy);
        assert_eq!(
            v,
            vec![Violation {
                directive: "script-src-elem".into(),
                origin: "https://cdn.bad.test".into(),
                path: "/library/series/:id/read".into(),
            }]
        );
        let api = br#"[{"type":"csp-violation","age":1,"url":"https://chat.example.org/x","body":{"documentURL":"https://chat.example.org/lounge/42?x=1","blockedURL":"inline","effectiveDirective":"style-src-elem","sample":"private"}},{"type":"deprecation","body":{}}]"#;
        let v = parse_reports(api);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].origin, "inline");
        assert_eq!(v[0].path, "/lounge/:id");
        assert!(parse_reports(b"not json").is_empty());
        assert!(parse_reports(b"{}").is_empty());
        assert!(parse_reports(b"[1,2,3]").is_empty());
    }

    #[test]
    fn nothing_personal_survives_the_reduction() {
        for (blocked, document) in [
            (
                "https://user:pass@evil.test:8443/p?q=1#f",
                "https://chat.example.org/u/someone@example.org?email=a@b.c",
            ),
            (
                "data:image/png;base64,AAAA",
                "http://localhost:5173/?code=abc",
            ),
            ("", ""),
        ] {
            let v = violation("img-src 'self'", blocked, document);
            let all = format!("{} {} {}", v.directive, v.origin, v.path);
            for banned in [
                "pass", "user:", "q=1", "#", "?", "code=", "base64", "email=",
            ] {
                assert!(!all.contains(banned), "{banned} in {all}");
            }
        }
        assert_eq!(
            blocked_origin("https://user:pass@evil.test:8443/p"),
            "https://evil.test:8443"
        );
        assert_eq!(blocked_origin("data:image/png;base64,AAAA"), "data");
        assert_eq!(blocked_origin("javascript:alert(1)"), "other");
        assert_eq!(document_path("http://localhost:5173/?code=abc"), "/");
        assert_eq!(directive_name("script-src 'self'"), "script-src");
        assert_eq!(directive_name("<script>"), "unknown");
    }

    #[tokio::test]
    async fn memory_store_counts_and_sorts() {
        let s = MemoryCspReports::new();
        for _ in 0..3 {
            s.record(&violation("img-src", "https://a.test", "/x"))
                .await;
        }
        s.record(&violation("font-src", "https://a.test", "/y"))
            .await;
        let top = s.top(10).await.unwrap();
        assert_eq!(top.directives[0].key, "img-src");
        assert_eq!(top.directives[0].count, 3);
        assert_eq!(top.origins[0].count, 4);
        assert_eq!(s.top(1).await.unwrap().paths.len(), 1);
    }
}
