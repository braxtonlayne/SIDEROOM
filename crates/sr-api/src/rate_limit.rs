//! Rate limiting with Discord-style buckets and headers (FND-API-003).
//!
//! Every route belongs to a bucket (`METHOD pattern`, overridable per route).
//! A request is counted against a per-identity global limiter (default 50/s,
//! like Discord) and against its bucket, keyed by identity and the route's
//! "major parameter" (the channel, guild or webhook id). Enforcement and the
//! `Remaining`/`Reset` headers both come from `governor` (GCRA): the state
//! snapshot reports the remaining burst, and the time to a full bucket follows
//! from the emission interval.

use crate::{observability::names, state::AppState};
use axum::{
    Json,
    extract::{ConnectInfo, MatchedPath, Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use governor::{
    Quota, RateLimiter as Governor,
    clock::{Clock, DefaultClock},
    middleware::StateInformationMiddleware,
    state::keyed::DefaultKeyedStateStore,
};
use serde_json::json;
use sr_core::ErrorCode;
use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    net::{IpAddr, SocketAddr},
    num::NonZeroU32,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

type KeyedLimiter =
    Governor<String, DefaultKeyedStateStore<String>, DefaultClock, StateInformationMiddleware>;

/// A keyed GCRA limiter whose successful checks return a state snapshot.
fn keyed(quota: Quota) -> KeyedLimiter {
    Governor::keyed(quota).with_middleware::<StateInformationMiddleware>()
}

/// A bucket's allowance: `limit` requests per `window`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BucketSpec {
    /// Requests allowed per window.
    pub limit: u32,
    /// Window length.
    pub window: Duration,
}

impl BucketSpec {
    /// `limit` requests per `secs` seconds.
    pub const fn new(limit: u32, secs: u64) -> Self {
        Self {
            limit,
            window: Duration::from_secs(secs),
        }
    }

    fn quota(&self) -> Quota {
        let burst = NonZeroU32::new(self.limit.max(1)).unwrap_or(NonZeroU32::MIN);
        Quota::with_period(self.window / burst.get())
            .unwrap_or_else(|| Quota::per_second(NonZeroU32::MIN))
            .allow_burst(burst)
    }
}

/// Rate limit configuration.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Master switch.
    pub enabled: bool,
    /// Per-identity requests per second across all routes.
    pub global_per_second: u32,
    /// Bucket for routes without an override.
    pub default_bucket: BucketSpec,
    /// Overrides keyed by `"METHOD /pattern"` (pattern without the
    /// `/api/v1` prefix, e.g. `"POST /channels/{channel_id}/messages"`).
    pub routes: HashMap<String, BucketSpec>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        let routes = [
            (
                "POST /channels/{channel_id}/messages",
                BucketSpec::new(5, 5),
            ),
            (
                "DELETE /channels/{channel_id}/messages/{message_id}",
                BucketSpec::new(5, 1),
            ),
            ("POST /guilds", BucketSpec::new(2, 60)),
            ("POST /sr/register", BucketSpec::new(5, 60)),
            // Signed media links: a page of messages fans out into many image fetches.
            ("GET /cdn/{*path}", BucketSpec::new(300, 10)),
            // Third-party image proxy (D-176): every request per client address, cache
            // misses per user and in total (each one fetches from the internet).
            (
                "GET /cdn/external/{hmac}/{url_b64}",
                BucketSpec::new(300, 10),
            ),
            ("GET /cdn/external (fetch)", BucketSpec::new(30, 60)),
            ("GET /cdn/external (global)", BucketSpec::new(240, 60)),
            // Developer platform (docs/specs/developer-platform.md section 3.8). The token endpoints
            // are limited per client id and per address inside the handlers.
            ("POST /oauth2/token (client)", BucketSpec::new(30, 60)),
            ("POST /oauth2/token (address)", BucketSpec::new(30, 60)),
            (
                "POST /oauth2/token/revoke (client)",
                BucketSpec::new(30, 60),
            ),
            (
                "POST /oauth2/token/revoke (address)",
                BucketSpec::new(30, 60),
            ),
            ("POST /oauth2/introspect (client)", BucketSpec::new(60, 60)),
            ("POST /oauth2/introspect (address)", BucketSpec::new(60, 60)),
            ("POST /oauth2/authorize", BucketSpec::new(20, 60)),
            ("POST /interactions", BucketSpec::new(20, 10)),
            (
                "POST /interactions/{interaction_id}/{token}/callback",
                BucketSpec::new(5, 5),
            ),
            ("POST /applications", BucketSpec::new(5, 3600)),
            ("POST /developers/apply", BucketSpec::new(3, 86_400)),
            (
                "POST /applications/{application_id}/reset-secret",
                BucketSpec::new(5, 3600),
            ),
            (
                "POST /applications/{application_id}/bot/reset-token",
                BucketSpec::new(5, 3600),
            ),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        Self {
            enabled: true,
            global_per_second: 50,
            default_bucket: BucketSpec::new(40, 10),
            routes,
        }
    }
}

struct Bucket {
    id: String,
    spec: BucketSpec,
    gov: KeyedLimiter,
}

/// Header values describing a bucket after a request was counted.
#[derive(Debug, Clone)]
pub struct RateInfo {
    /// `X-RateLimit-Limit`.
    pub limit: u32,
    /// `X-RateLimit-Remaining`.
    pub remaining: u32,
    /// `X-RateLimit-Reset-After` in seconds.
    pub reset_after: f64,
    /// `X-RateLimit-Reset` as epoch seconds.
    pub reset: f64,
    /// `X-RateLimit-Bucket`.
    pub bucket: String,
}

/// Outcome of [`RateLimiter::check`].
#[derive(Debug, Clone)]
pub enum Decision {
    /// Proceed; add these headers.
    Allowed(RateInfo),
    /// Reject with 429.
    Limited {
        /// Seconds until a retry can succeed.
        retry_after: f64,
        /// True when the global limit tripped.
        global: bool,
        /// Bucket info (absent for global limits).
        info: Option<RateInfo>,
    },
}

/// The limiter. One per process, held in `AppState`.
pub struct RateLimiter {
    cfg: RateLimitConfig,
    global: KeyedLimiter,
    buckets: Mutex<HashMap<String, Arc<Bucket>>>,
    calls: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn hash_hex(s: &str) -> String {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

impl RateLimiter {
    /// Build from config.
    pub fn new(cfg: RateLimitConfig) -> Self {
        let per_sec = NonZeroU32::new(cfg.global_per_second.max(1)).unwrap_or(NonZeroU32::MIN);
        Self {
            global: keyed(Quota::per_second(per_sec)),
            cfg,
            buckets: Mutex::new(HashMap::new()),
            calls: AtomicU64::new(0),
        }
    }

    fn bucket(&self, route_key: &str) -> Arc<Bucket> {
        let mut map = lock(&self.buckets);
        if let Some(b) = map.get(route_key) {
            return b.clone();
        }
        let spec = self
            .cfg
            .routes
            .get(route_key)
            .copied()
            .unwrap_or(self.cfg.default_bucket);
        let b = Arc::new(Bucket {
            id: hash_hex(route_key),
            spec,
            gov: keyed(spec.quota()),
        });
        map.insert(route_key.to_owned(), b.clone());
        b
    }

    /// Count one request. `route_key` is `"METHOD /pattern"`, `resource` the
    /// major-parameter value (or empty), `identity` the caller.
    pub fn check(&self, route_key: &str, resource: &str, identity: &str) -> Decision {
        if !self.cfg.enabled {
            return Decision::Allowed(RateInfo {
                limit: u32::MAX,
                remaining: u32::MAX,
                reset_after: 0.0,
                reset: 0.0,
                bucket: String::new(),
            });
        }
        if self.calls.fetch_add(1, Ordering::Relaxed) % 4096 == 4095 {
            self.sweep();
        }
        let clock = DefaultClock::default();
        if let Err(nu) = self.global.check_key(&identity.to_owned()) {
            return Decision::Limited {
                retry_after: nu.wait_time_from(clock.now()).as_secs_f64(),
                global: true,
                info: None,
            };
        }
        let bucket = self.bucket(route_key);
        let key = format!("{identity}|{resource}");
        let limit = bucket.spec.limit.max(1);
        // Emission interval is window / limit, so a bucket with `used` spent
        // cells refills completely after `used` intervals.
        let refill = |used: u32| (bucket.spec.window / limit) * used;
        let (denied, remaining, reset_after) = match bucket.gov.check_key(&key) {
            Ok(snapshot) => {
                let remaining = snapshot.remaining_burst_capacity().min(limit);
                (None, remaining, refill(limit - remaining))
            }
            Err(nu) => {
                let wait = nu.wait_time_from(clock.now());
                (Some(wait), 0, wait)
            }
        };
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64());
        let reset_after = reset_after.as_secs_f64();
        let info = RateInfo {
            limit: bucket.spec.limit,
            remaining,
            reset_after,
            reset: epoch + reset_after,
            bucket: bucket.id.clone(),
        };
        match denied {
            None => Decision::Allowed(info),
            Some(wait) => Decision::Limited {
                retry_after: wait.as_secs_f64(),
                global: false,
                info: Some(info),
            },
        }
    }

    /// Drop idle keys so memory stays bounded.
    fn sweep(&self) {
        self.global.retain_recent();
        for b in lock(&self.buckets).values() {
            b.gov.retain_recent();
        }
    }
}

fn set(h: &mut HeaderMap, name: &'static str, v: impl ToString) {
    if let Ok(v) = HeaderValue::from_str(&v.to_string()) {
        h.insert(HeaderName::from_static(name), v);
    }
}

fn apply_info(h: &mut HeaderMap, i: &RateInfo) {
    set(h, "x-ratelimit-limit", i.limit);
    set(h, "x-ratelimit-remaining", i.remaining);
    set(h, "x-ratelimit-reset", format!("{:.3}", i.reset));
    set(
        h,
        "x-ratelimit-reset-after",
        format!("{:.3}", i.reset_after),
    );
    set(h, "x-ratelimit-bucket", &i.bucket);
    set(h, "x-ratelimit-scope", "user");
}

/// Strip the API version prefix so `/api/v1` and `/api/v10` share buckets.
fn normalise(pattern: &str) -> &str {
    for p in ["/api/v10", "/api/v1"] {
        if let Some(rest) = pattern.strip_prefix(p) {
            return if rest.is_empty() { "/" } else { rest };
        }
    }
    pattern
}

/// First path-parameter value for `/channels/{..}`, `/guilds/{..}`,
/// `/webhooks/{..}` routes (Discord's "major parameters").
fn major_resource<'a>(pattern: &str, path: &'a str) -> &'a str {
    let first = pattern.trim_start_matches('/').split('/').next();
    if matches!(first, Some("channels" | "guilds" | "webhooks")) {
        // path is /api/vN/<kind>/<id>/...
        let mut segs = path.trim_start_matches('/').split('/');
        if segs.next() == Some("api") {
            segs.next();
        }
        segs.nth(1).unwrap_or("")
    } else {
        ""
    }
}

/// The caller's address: the socket peer, or, when the proxy is trusted, the
/// last `X-Forwarded-For` entry (the one Caddy appended). Open question 57.
pub fn client_ip(req: &Request, trust_forwarded_for: bool) -> Option<IpAddr> {
    if trust_forwarded_for {
        let forwarded = req
            .headers()
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.rsplit(',').next())
            .and_then(|s| s.trim().parse().ok());
        if forwarded.is_some() {
            return forwarded;
        }
    }
    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip())
}

fn identity(req: &Request, trust_forwarded_for: bool) -> String {
    if let Some(a) = req.headers().get(header::AUTHORIZATION) {
        return format!("auth:{}", hash_hex(a.to_str().unwrap_or("")));
    }
    if let Some(c) = crate::auth::parse_cookie(req.headers(), crate::auth::SESSION_COOKIE) {
        return format!("sess:{}", hash_hex(c));
    }
    client_ip(req, trust_forwarded_for)
        .map_or_else(|| "ip:unknown".to_owned(), |ip| format!("ip:{ip}"))
}

/// Middleware; install with `route_layer` so the matched route is known.
pub async fn rate_limit(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let Some(matched) = req.extensions().get::<MatchedPath>() else {
        return next.run(req).await;
    };
    let pattern = normalise(matched.as_str()).to_owned();
    let route_key = format!("{} {}", req.method(), pattern);
    let resource = major_resource(&pattern, req.uri().path()).to_owned();
    let who = identity(&req, state.settings().trust_forwarded_for);
    match state.rate_limiter().check(&route_key, &resource, &who) {
        Decision::Allowed(info) => {
            let mut resp = next.run(req).await;
            if info.limit != u32::MAX {
                apply_info(resp.headers_mut(), &info);
            }
            resp
        }
        Decision::Limited {
            retry_after,
            global,
            info,
        } => {
            metrics::counter!(names::RATE_LIMITED_TOTAL, "scope" => if global { "global" } else { "user" })
                .increment(1);
            let secs = retry_after.max(0.0);
            let mut resp = (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({
                    "code": ErrorCode::TooManyRequests.code(),
                    "message": "You are being rate limited.",
                    "retry_after": secs,
                    "global": global,
                })),
            )
                .into_response();
            let h = resp.headers_mut();
            if let Some(i) = &info {
                apply_info(h, i);
            }
            if global {
                set(h, "x-ratelimit-global", "true");
                set(h, "x-ratelimit-scope", "global");
            }
            set(h, "retry-after", secs.ceil() as u64);
            resp
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_counts_down_then_limits() {
        let rl = RateLimiter::new(RateLimitConfig {
            default_bucket: BucketSpec::new(3, 60),
            ..RateLimitConfig::default()
        });
        for expect in [2, 1, 0] {
            match rl.check("GET /x", "", "u") {
                Decision::Allowed(i) => {
                    assert_eq!(i.limit, 3);
                    assert_eq!(i.remaining, expect);
                }
                Decision::Limited { .. } => panic!("limited early"),
            }
        }
        match rl.check("GET /x", "", "u") {
            Decision::Limited {
                retry_after,
                global,
                info,
            } => {
                assert!(!global);
                assert!(retry_after > 0.0);
                assert_eq!(info.map(|i| i.remaining), Some(0));
            }
            Decision::Allowed(_) => panic!("should limit"),
        }
        // other identity and other resource are independent
        assert!(matches!(rl.check("GET /x", "", "v"), Decision::Allowed(_)));
        assert!(matches!(rl.check("GET /x", "7", "u"), Decision::Allowed(_)));
    }

    #[test]
    fn global_limit_trips_first() {
        let rl = RateLimiter::new(RateLimitConfig {
            global_per_second: 2,
            ..RateLimitConfig::default()
        });
        let mut limited = None;
        for _ in 0..5 {
            if let Decision::Limited { global, .. } = rl.check("GET /y", "", "u") {
                limited = Some(global);
                break;
            }
        }
        assert_eq!(limited, Some(true));
    }

    #[test]
    fn prefix_and_major_param() {
        assert_eq!(normalise("/api/v10/channels/{id}"), "/channels/{id}");
        assert_eq!(normalise("/api/v1/users/@me"), "/users/@me");
        assert_eq!(
            major_resource("/channels/{id}/messages", "/api/v10/channels/42/messages"),
            "42"
        );
        assert_eq!(major_resource("/users/@me", "/api/v1/users/@me"), "");
    }
}
