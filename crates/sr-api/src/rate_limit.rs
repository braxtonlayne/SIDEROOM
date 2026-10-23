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
    /// Live HTTP limits (H-39): when set, the limiter reads the current [`RateConfig`] on every
    /// request, so a change made in Admin applies on the next request without a restart.
    pub live: Option<tokio::sync::watch::Receiver<RateConfig>>,
}

/// The route key of a message send.
const MESSAGES_ROUTE: &str = "POST /channels/{channel_id}/messages";
/// The route key of an upload slot request.
const UPLOADS_ROUTE: &str = "POST /channels/{channel_id}/attachments";

/// The abuse limits that ride the HTTP limiter (RDY-SEC-006). Each one is a platform setting in the
/// `abuse` group of the Admin > Limits catalog (`sr_admin::settings`), read live like the message
/// limit, so an edit applies on the next request. Scope is per process, like every limiter here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Abuse {
    /// Friend requests and blocks, per credential per hour.
    FriendRequests,
    /// Opening direct or group conversations (Message Requests), per credential per hour.
    DmOpens,
    /// Executions of one webhook token, per minute, across every caller.
    WebhookExecute,
    /// OAuth2 authorize approvals, per credential or address per minute.
    OauthAuthorize,
    /// Interaction callbacks, per address per minute.
    InteractionCallbacks,
    /// Watch-party joins, per credential per minute.
    WatchPartyJoins,
    /// Custom emoji uploads, per credential per server per hour.
    EmojiUploads,
    /// Sticker uploads, per credential per server per hour.
    StickerUploads,
    /// Ban appeals, per address per hour.
    AppealAttempts,
}

impl Abuse {
    /// Every limit, in the order of [`RateConfig::abuse`].
    pub const ALL: [Abuse; 9] = [
        Abuse::FriendRequests,
        Abuse::DmOpens,
        Abuse::WebhookExecute,
        Abuse::OauthAuthorize,
        Abuse::InteractionCallbacks,
        Abuse::WatchPartyJoins,
        Abuse::EmojiUploads,
        Abuse::StickerUploads,
        Abuse::AppealAttempts,
    ];

    /// The index into [`RateConfig::abuse`].
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The platform setting that holds the figure.
    pub const fn key(self) -> &'static str {
        use sr_admin::settings as k;
        match self {
            Abuse::FriendRequests => k::ABUSE_FRIEND_REQUESTS,
            Abuse::DmOpens => k::ABUSE_DM_OPENS,
            Abuse::WebhookExecute => k::ABUSE_WEBHOOK_EXECUTE,
            Abuse::OauthAuthorize => k::ABUSE_OAUTH_AUTHORIZE,
            Abuse::InteractionCallbacks => k::ABUSE_INTERACTION_CALLBACKS,
            Abuse::WatchPartyJoins => k::ABUSE_WATCH_PARTY_JOINS,
            Abuse::EmojiUploads => k::ABUSE_EMOJI_UPLOADS,
            Abuse::StickerUploads => k::ABUSE_STICKER_UPLOADS,
            Abuse::AppealAttempts => k::ABUSE_APPEAL_ATTEMPTS,
        }
    }

    /// The window the setting is counted in, in seconds (per hour or per minute).
    pub const fn window_secs(self) -> u64 {
        match self {
            Abuse::FriendRequests
            | Abuse::DmOpens
            | Abuse::EmojiUploads
            | Abuse::StickerUploads
            | Abuse::AppealAttempts => 3600,
            Abuse::WebhookExecute
            | Abuse::OauthAuthorize
            | Abuse::InteractionCallbacks
            | Abuse::WatchPartyJoins => 60,
        }
    }

    /// How much of the window may be spent at once: webhooks and interaction callbacks allow a burst
    /// of five seconds' worth (Discord's shape), the rest the whole window's figure.
    const fn burst_secs(self) -> u64 {
        match self {
            Abuse::WebhookExecute | Abuse::InteractionCallbacks => 5,
            other => other.window_secs(),
        }
    }

    /// The catalog default (the one source of the figure; doc 05 repeats it and a test compares).
    pub fn default_figure(self) -> u32 {
        sr_admin::settings::find(self.key())
            .and_then(|d| u32::try_from(d.default_int).ok())
            .unwrap_or(1)
    }

    /// The bucket for `figure` events per window.
    fn spec(self, figure: u32) -> BucketSpec {
        let burst = (u64::from(figure.max(1)) * self.burst_secs() / self.window_secs()).max(1);
        BucketSpec::new(u32::try_from(burst).unwrap_or(u32::MAX), self.burst_secs())
    }
}

/// The routes each abuse limit guards: `"METHOD /pattern"` as the limiter keys them (the same
/// strings as `route_matrix.tsv`, which a test checks).
pub const ABUSE_ROUTES: &[(&str, Abuse)] = &[
    ("POST /users/@me/relationships", Abuse::FriendRequests),
    (
        "PUT /users/@me/relationships/{user_id}",
        Abuse::FriendRequests,
    ),
    ("POST /users/@me/channels", Abuse::DmOpens),
    ("POST /webhooks/{webhook_id}/{token}", Abuse::WebhookExecute),
    ("POST /oauth2/authorize", Abuse::OauthAuthorize),
    (
        "POST /interactions/{interaction_id}/{token}/callback",
        Abuse::InteractionCallbacks,
    ),
    (
        "POST /watch-parties/{party_id}/join",
        Abuse::WatchPartyJoins,
    ),
    ("POST /guilds/{guild_id}/emojis", Abuse::EmojiUploads),
    (
        "POST /applications/{application_id}/emojis",
        Abuse::EmojiUploads,
    ),
    ("POST /guilds/{guild_id}/stickers", Abuse::StickerUploads),
    ("POST /moderation/appeals", Abuse::AppealAttempts),
];

/// The limit that guards `route_key`, if any.
pub fn abuse_for(route_key: &str) -> Option<Abuse> {
    ABUSE_ROUTES
        .iter()
        .find(|(k, _)| *k == route_key)
        .map(|(_, a)| *a)
}

/// The live HTTP limits, all requests (or sends) per minute. Gateway and voice limits stay static.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateConfig {
    /// Requests per minute from one address that is not signed in.
    pub ip: u32,
    /// Requests per minute from one browser session.
    pub session: u32,
    /// Requests per minute from one bot or API credential.
    pub bot: u32,
    /// Message sends per minute in one channel by one caller.
    pub messages: u32,
    /// Upload slot requests per minute in one channel by one caller.
    pub uploads: u32,
    /// The abuse limits, indexed by [`Abuse::index`] (RDY-SEC-006); each in its own unit (see [`Abuse`]).
    pub abuse: [u32; Abuse::ALL.len()],
}

impl Default for RateConfig {
    fn default() -> Self {
        Self {
            ip: 3000,
            session: 3000,
            bot: 3000,
            messages: 60,
            uploads: 240,
            abuse: Abuse::ALL.map(Abuse::default_figure),
        }
    }
}

impl RateConfig {
    /// The per-minute figures that a static [`RateLimitConfig`] stands for.
    pub fn of(cfg: &RateLimitConfig) -> Self {
        let per_minute = |s: &BucketSpec| {
            let secs = s.window.as_secs().max(1);
            u32::try_from(u64::from(s.limit) * 60 / secs).unwrap_or(u32::MAX)
        };
        let global = cfg.global_per_second.saturating_mul(60);
        Self {
            ip: global,
            session: global,
            bot: global,
            messages: cfg.routes.get(MESSAGES_ROUTE).map_or(60, per_minute),
            uploads: cfg
                .routes
                .get(UPLOADS_ROUTE)
                .map_or_else(|| per_minute(&cfg.default_bucket), per_minute),
            abuse: Self::default().abuse,
        }
    }

    /// The figures in a list of platform settings (`GET /admin/settings`); a missing or out of
    /// range entry keeps the default.
    pub fn from_settings(views: &[sr_admin::SettingView]) -> Self {
        use sr_admin::settings as k;
        let d = Self::default();
        let get = |key: &str, default: u32| {
            views
                .iter()
                .find(|v| v.key == key)
                .and_then(|v| v.value.as_u64())
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| *n > 0)
                .unwrap_or(default)
        };
        Self {
            ip: get(k::RATE_IP, d.ip),
            session: get(k::RATE_SESSION, d.session),
            bot: get(k::RATE_BOT, d.bot),
            messages: get(k::RATE_MESSAGES, d.messages),
            uploads: get(k::RATE_UPLOADS, d.uploads),
            abuse: Abuse::ALL.map(|a| get(a.key(), d.abuse[a.index()])),
        }
    }

    /// Per-identity limiter: `n` requests a minute with a burst of one second's worth.
    fn global_quota(n: u32) -> Quota {
        let n = n.max(1);
        let burst = NonZeroU32::new((n / 60).max(1)).unwrap_or(NonZeroU32::MIN);
        Quota::with_period(Duration::from_secs(60) / n)
            .unwrap_or_else(|| Quota::per_second(NonZeroU32::MIN))
            .allow_burst(burst)
    }

    /// Message bucket: a burst of five seconds' worth.
    fn messages_spec(&self) -> BucketSpec {
        BucketSpec::new((self.messages / 12).max(1), 5)
    }

    /// Upload slot bucket: a burst of ten seconds' worth.
    fn uploads_spec(&self) -> BucketSpec {
        BucketSpec::new((self.uploads / 6).max(1), 10)
    }
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
            // `POST /oauth2/authorize` and the interaction callback follow the live `abuse.*` settings.
            ("POST /interactions", BucketSpec::new(20, 10)),
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
            live: None,
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
    live: Mutex<Live>,
    buckets: Mutex<HashMap<String, Arc<Bucket>>>,
    calls: AtomicU64,
}

/// The limiters that follow the live [`RateConfig`]: one per identity kind.
struct Live {
    current: RateConfig,
    ip: Arc<KeyedLimiter>,
    session: Arc<KeyedLimiter>,
    bot: Arc<KeyedLimiter>,
}

impl Live {
    fn new(current: RateConfig) -> Self {
        Self {
            ip: Arc::new(keyed(RateConfig::global_quota(current.ip))),
            session: Arc::new(keyed(RateConfig::global_quota(current.session))),
            bot: Arc::new(keyed(RateConfig::global_quota(current.bot))),
            current,
        }
    }
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
        let start = cfg
            .live
            .as_ref()
            .map_or_else(|| RateConfig::of(&cfg), |rx| *rx.borrow());
        Self {
            live: Mutex::new(Live::new(start)),
            cfg,
            buckets: Mutex::new(HashMap::new()),
            calls: AtomicU64::new(0),
        }
    }

    /// The per-identity limiter for `identity` under the current live figures, rebuilt when an
    /// admin changed that figure since the last request (its counters start afresh, which is fine
    /// for a setting that is edited rarely).
    fn global_for(&self, identity: &str) -> (Arc<KeyedLimiter>, RateConfig) {
        let now = self
            .cfg
            .live
            .as_ref()
            .map(|rx| *rx.borrow())
            .unwrap_or_else(|| lock(&self.live).current);
        let mut live = lock(&self.live);
        if live.current.ip != now.ip {
            live.ip = Arc::new(keyed(RateConfig::global_quota(now.ip)));
        }
        if live.current.session != now.session {
            live.session = Arc::new(keyed(RateConfig::global_quota(now.session)));
        }
        if live.current.bot != now.bot {
            live.bot = Arc::new(keyed(RateConfig::global_quota(now.bot)));
        }
        live.current = now;
        let g = if identity.starts_with("auth:") {
            live.bot.clone()
        } else if identity.starts_with("sess:") {
            live.session.clone()
        } else {
            live.ip.clone()
        };
        (g, now)
    }

    fn bucket(&self, route_key: &str, now: RateConfig) -> Arc<Bucket> {
        let mut map = lock(&self.buckets);
        let spec = match route_key {
            MESSAGES_ROUTE if self.cfg.live.is_some() => now.messages_spec(),
            UPLOADS_ROUTE if self.cfg.live.is_some() => now.uploads_spec(),
            // An explicit static override wins (tests); otherwise the abuse figure, live.
            _ if abuse_for(route_key).is_some() && !self.cfg.routes.contains_key(route_key) => {
                abuse_for(route_key)
                    .map_or(self.cfg.default_bucket, |a| a.spec(now.abuse[a.index()]))
            }
            _ => self
                .cfg
                .routes
                .get(route_key)
                .copied()
                .unwrap_or(self.cfg.default_bucket),
        };
        if let Some(b) = map.get(route_key)
            && b.spec == spec
        {
            return b.clone();
        }
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
        self.check_scoped(route_key, resource, identity, identity)
    }

    /// Like [`check`](Self::check), but the bucket is keyed by `bucket_identity` while the global
    /// limiter still sees `identity`: a limit that belongs to a resource (one webhook token) and not
    /// to whoever calls it.
    pub fn check_scoped(
        &self,
        route_key: &str,
        resource: &str,
        identity: &str,
        bucket_identity: &str,
    ) -> Decision {
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
        let (global, now) = self.global_for(identity);
        if let Err(nu) = global.check_key(&identity.to_owned()) {
            return Decision::Limited {
                retry_after: nu.wait_time_from(clock.now()).as_secs_f64(),
                global: true,
                info: None,
            };
        }
        let bucket = self.bucket(route_key, now);
        let key = format!("{bucket_identity}|{resource}");
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
        {
            let live = lock(&self.live);
            live.ip.retain_recent();
            live.session.retain_recent();
            live.bot.retain_recent();
        }
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

/// `<webhook id>/<hash of the token>` for `/api/vN/webhooks/{id}/{token}`: the key of a webhook's own
/// bucket. The token is hashed so it never sits in the limiter's memory in the clear.
fn webhook_token_resource(path: &str) -> String {
    let mut segs = path.trim_start_matches('/').split('/');
    if segs.next() == Some("api") {
        segs.next();
    }
    let _webhooks = segs.next();
    let id = segs.next().unwrap_or("");
    let token = segs.next().unwrap_or("");
    format!("{id}/{}", hash_hex(token))
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
    let who = identity(&req, state.settings().trust_forwarded_for);
    // A webhook token's limit belongs to the token, whoever calls it (RDY-SEC-006).
    let (resource, bucket_who) = if abuse_for(&route_key) == Some(Abuse::WebhookExecute) {
        (
            webhook_token_resource(req.uri().path()),
            "webhook-token".to_owned(),
        )
    } else {
        (
            major_resource(&pattern, req.uri().path()).to_owned(),
            who.clone(),
        )
    };
    match state
        .rate_limiter()
        .check_scoped(&route_key, &resource, &who, &bucket_who)
    {
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
    fn a_live_change_applies_on_the_next_request() {
        let (tx, rx) = tokio::sync::watch::channel(RateConfig::default());
        let rl = RateLimiter::new(RateLimitConfig {
            live: Some(rx),
            ..RateLimitConfig::default()
        });
        let send = || rl.check(MESSAGES_ROUTE, "7", "sess:a");
        // Defaults: 60 a minute is a burst of five.
        for _ in 0..5 {
            assert!(matches!(send(), Decision::Allowed(_)));
        }
        assert!(matches!(send(), Decision::Limited { global: false, .. }));
        // Raised to 240 a minute (a burst of twenty): the very next request is allowed.
        tx.send_modify(|c| c.messages = 240);
        match send() {
            Decision::Allowed(i) => assert_eq!(i.limit, 20),
            Decision::Limited { .. } => panic!("the new limit did not apply"),
        }
        // Lowered to 12 a minute (a burst of one) on a fresh channel: second send is refused.
        tx.send_modify(|c| c.messages = 12);
        assert!(matches!(
            rl.check(MESSAGES_ROUTE, "8", "sess:a"),
            Decision::Allowed(_)
        ));
        assert!(matches!(
            rl.check(MESSAGES_ROUTE, "8", "sess:a"),
            Decision::Limited { global: false, .. }
        ));
    }

    #[test]
    fn identity_kinds_follow_their_own_live_limit() {
        let (tx, rx) = tokio::sync::watch::channel(RateConfig::default());
        let rl = RateLimiter::new(RateLimitConfig {
            live: Some(rx),
            ..RateLimitConfig::default()
        });
        // 60 a minute for addresses: a burst of one, so the second request is a global refusal.
        tx.send_modify(|c| c.ip = 60);
        assert!(matches!(
            rl.check("GET /a", "", "ip:1.2.3.4"),
            Decision::Allowed(_)
        ));
        assert!(matches!(
            rl.check("GET /b", "", "ip:1.2.3.4"),
            Decision::Limited { global: true, .. }
        ));
        // Sessions and bots keep their own figures.
        for who in ["sess:x", "auth:y"] {
            assert!(matches!(rl.check("GET /a", "", who), Decision::Allowed(_)));
            assert!(matches!(rl.check("GET /b", "", who), Decision::Allowed(_)));
        }
    }

    #[test]
    fn static_config_maps_to_the_same_figures() {
        let c = RateConfig::of(&RateLimitConfig::default());
        assert_eq!(c, RateConfig::default());
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

    // ---- abuse limits (RDY-SEC-006) -------------------------------------------------------------

    fn live_limiter() -> (tokio::sync::watch::Sender<RateConfig>, RateLimiter) {
        let (tx, rx) = tokio::sync::watch::channel(RateConfig::default());
        let rl = RateLimiter::new(RateLimitConfig {
            live: Some(rx),
            ..RateLimitConfig::default()
        });
        (tx, rl)
    }

    #[test]
    fn abuse_defaults_come_from_the_catalog_and_every_limit_has_a_route() {
        let d = RateConfig::default();
        let mut keys = std::collections::HashSet::new();
        for a in Abuse::ALL {
            let def = sr_admin::settings::find(a.key()).expect("limit is in the catalog");
            assert_eq!(def.group, "abuse", "{}", a.key());
            assert_eq!(d.abuse[a.index()], a.default_figure());
            assert_eq!(i64::from(a.default_figure()), def.default_int);
            assert!(keys.insert(a.key()), "duplicate {}", a.key());
            assert!(
                ABUSE_ROUTES.iter().any(|(_, x)| *x == a),
                "{a:?} guards no route"
            );
        }
        assert_eq!(Abuse::ALL.len(), 9);
        assert_eq!(RateConfig::of(&RateLimitConfig::default()), d);
    }

    #[test]
    fn abuse_routes_exist_in_the_route_matrix() {
        let matrix = include_str!("../tests/route_matrix.tsv");
        let routes: std::collections::HashSet<String> = matrix
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .filter_map(|l| {
                let mut c = l.split('\t');
                Some(format!("{} {}", c.next()?, c.next()?))
            })
            .collect();
        for (key, _) in ABUSE_ROUTES {
            assert!(routes.contains(*key), "{key} is not an API route");
        }
    }

    #[test]
    fn every_abuse_route_limits_at_its_figure_and_follows_a_live_change() {
        for (route, abuse) in ABUSE_ROUTES {
            let (tx, rl) = live_limiter();
            let def = sr_admin::settings::find(abuse.key()).unwrap();
            let floor = def.min as u32;
            tx.send_modify(|c| c.abuse[abuse.index()] = floor);
            let burst = abuse.spec(floor).limit;
            // The webhook limit is per token; every other one is per caller.
            let go = |who: &str| rl.check_scoped(route, "7", "sess:a", who);
            for i in 0..burst {
                assert!(matches!(go("sess:a"), Decision::Allowed(_)), "{route} #{i}");
            }
            assert!(
                matches!(go("sess:a"), Decision::Limited { global: false, .. }),
                "{route} did not limit after {burst}"
            );
            // Another caller (or another resource) is unaffected.
            assert!(matches!(
                rl.check_scoped(route, "8", "sess:a", "sess:a"),
                Decision::Allowed(_)
            ));
            // Raising the setting applies on the next request.
            tx.send_modify(|c| c.abuse[abuse.index()] = def.max as u32);
            assert!(matches!(go("sess:a"), Decision::Allowed(_)), "{route} live");
        }
    }

    #[test]
    fn the_figures_mean_what_the_catalog_says() {
        // Per hour: the whole figure may be spent at once.
        assert_eq!(Abuse::FriendRequests.spec(60), BucketSpec::new(60, 3600));
        assert_eq!(Abuse::EmojiUploads.spec(50), BucketSpec::new(50, 3600));
        // Per minute: the whole figure for authorize and watch parties.
        assert_eq!(Abuse::OauthAuthorize.spec(20), BucketSpec::new(20, 60));
        assert_eq!(Abuse::WatchPartyJoins.spec(20), BucketSpec::new(20, 60));
        // Webhooks and callbacks: 60 a minute is a burst of five (the former callback bucket, 5 per 5 s).
        assert_eq!(Abuse::WebhookExecute.spec(60), BucketSpec::new(5, 5));
        assert_eq!(Abuse::InteractionCallbacks.spec(60), BucketSpec::new(5, 5));
    }

    #[test]
    fn a_webhook_limit_belongs_to_the_token_not_the_caller() {
        let (tx, rl) = live_limiter();
        tx.send_modify(|c| c.abuse[Abuse::WebhookExecute.index()] = 12); // a burst of one
        let route = "POST /webhooks/{webhook_id}/{token}";
        let a = webhook_token_resource("/api/v1/webhooks/5/tok-a");
        let b = webhook_token_resource("/api/v10/webhooks/5/tok-b");
        assert_ne!(a, b);
        assert!(!a.contains("tok-a"), "the token is hashed");
        assert!(matches!(
            rl.check_scoped(route, &a, "ip:1.1.1.1", "webhook-token"),
            Decision::Allowed(_)
        ));
        // Same token from another address: the same bucket.
        assert!(matches!(
            rl.check_scoped(route, &a, "ip:2.2.2.2", "webhook-token"),
            Decision::Limited { global: false, .. }
        ));
        // Another token of the same webhook id is untouched.
        assert!(matches!(
            rl.check_scoped(route, &b, "ip:2.2.2.2", "webhook-token"),
            Decision::Allowed(_)
        ));
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
