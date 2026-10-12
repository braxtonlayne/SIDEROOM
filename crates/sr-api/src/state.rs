//! Shared application state (FND-API-001).

use crate::{
    auth::{
        BearerBackend, BotBackend, SessionBackend,
        memory::{MemoryBearers, MemoryBots, MemorySessions},
    },
    health::ReadinessCheck,
    observability::Metrics,
    openapi,
    rate_limit::{RateLimitConfig, RateLimiter},
};
use sr_auth::session::SessionPolicy;
use sr_core::{Clock, SystemClock};
use std::{any::Any, any::TypeId, collections::HashMap, sync::Arc, time::Duration};
use utoipa::openapi::OpenApi;

/// HTTP-layer settings.
#[derive(Debug, Clone)]
pub struct ApiSettings {
    /// Origins allowed by CORS (with credentials). Exact match.
    pub allowed_origins: Vec<String>,
    /// Per-request timeout (408 on expiry).
    pub request_timeout: Duration,
    /// Per-check timeout for `/readyz`.
    pub readiness_timeout: Duration,
    /// Set the `Secure` cookie attribute.
    pub secure_cookies: bool,
    /// Rate limit configuration.
    pub rate_limit: RateLimitConfig,
    /// Trust the proxy's `X-Forwarded-For` (last entry) for the client IP.
    /// Only enable behind a reverse proxy that appends to the header (Caddy).
    pub trust_forwarded_for: bool,
}

impl Default for ApiSettings {
    fn default() -> Self {
        Self::for_public_url("http://localhost:8080")
    }
}

impl ApiSettings {
    /// Defaults derived from the server's public URL (its origin is the only
    /// allowed CORS origin).
    pub fn for_public_url(public_url: &str) -> Self {
        Self {
            allowed_origins: vec![public_url.trim_end_matches('/').to_owned()],
            request_timeout: Duration::from_secs(30),
            readiness_timeout: Duration::from_secs(2),
            secure_cookies: public_url.starts_with("https://"),
            rate_limit: RateLimitConfig::default(),
            trust_forwarded_for: false,
        }
    }
}

struct Inner {
    sessions: Arc<dyn SessionBackend>,
    bots: Arc<dyn BotBackend>,
    bearers: Arc<dyn BearerBackend>,
    clock: Arc<dyn Clock>,
    session_policy: SessionPolicy,
    readiness: Vec<Arc<dyn ReadinessCheck>>,
    metrics: Metrics,
    openapi: OpenApi,
    settings: ApiSettings,
    rate_limiter: RateLimiter,
    extensions: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

/// Cheaply cloneable application state.
#[derive(Clone)]
pub struct AppState(Arc<Inner>);

impl AppState {
    /// Start building; unset backends default to empty in-memory stubs.
    pub fn builder() -> AppStateBuilder {
        AppStateBuilder::default()
    }

    /// Session backend.
    pub fn sessions(&self) -> &dyn SessionBackend {
        &*self.0.sessions
    }
    /// Bot token backend.
    pub fn bots(&self) -> &dyn BotBackend {
        &*self.0.bots
    }
    /// OAuth bearer backend.
    pub fn bearers(&self) -> &dyn BearerBackend {
        &*self.0.bearers
    }
    /// Session expiry policy.
    pub fn session_policy(&self) -> &SessionPolicy {
        &self.0.session_policy
    }
    /// Current Unix time in seconds from the injected clock.
    pub fn now_secs(&self) -> i64 {
        self.0.clock.now_unix_ms().div_euclid(1000)
    }
    /// HTTP settings.
    pub fn settings(&self) -> &ApiSettings {
        &self.0.settings
    }
    /// Registered readiness checks.
    pub fn readiness(&self) -> &[Arc<dyn ReadinessCheck>] {
        &self.0.readiness
    }
    /// Metrics handle.
    pub fn metrics(&self) -> &Metrics {
        &self.0.metrics
    }
    /// The assembled OpenAPI document.
    pub fn openapi(&self) -> &OpenApi {
        &self.0.openapi
    }
    /// The rate limiter.
    pub fn rate_limiter(&self) -> &RateLimiter {
        &self.0.rate_limiter
    }
    /// A service registered with [`AppStateBuilder::extension`], by type.
    /// Handlers reach it through the [`crate::routes::Ext`] extractor.
    pub fn extension<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.0
            .extensions
            .get(&TypeId::of::<T>())?
            .clone()
            .downcast::<T>()
            .ok()
    }
}

/// Builder for [`AppState`].
#[derive(Default)]
pub struct AppStateBuilder {
    sessions: Option<Arc<dyn SessionBackend>>,
    bots: Option<Arc<dyn BotBackend>>,
    bearers: Option<Arc<dyn BearerBackend>>,
    clock: Option<Arc<dyn Clock>>,
    session_policy: Option<SessionPolicy>,
    readiness: Vec<Arc<dyn ReadinessCheck>>,
    openapi: Option<OpenApi>,
    settings: Option<ApiSettings>,
    extensions: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl AppStateBuilder {
    /// Session backend (any `SessionRepository` works).
    pub fn sessions(mut self, v: Arc<dyn SessionBackend>) -> Self {
        self.sessions = Some(v);
        self
    }
    /// Bot token backend.
    pub fn bots(mut self, v: Arc<dyn BotBackend>) -> Self {
        self.bots = Some(v);
        self
    }
    /// OAuth bearer backend.
    pub fn bearers(mut self, v: Arc<dyn BearerBackend>) -> Self {
        self.bearers = Some(v);
        self
    }
    /// Injected clock.
    pub fn clock(mut self, v: Arc<dyn Clock>) -> Self {
        self.clock = Some(v);
        self
    }
    /// Session policy.
    pub fn session_policy(mut self, v: SessionPolicy) -> Self {
        self.session_policy = Some(v);
        self
    }
    /// Add a readiness check.
    pub fn readiness(mut self, v: Arc<dyn ReadinessCheck>) -> Self {
        self.readiness.push(v);
        self
    }
    /// Replace the OpenAPI document (feature crates merge theirs into
    /// [`openapi::base`]).
    pub fn openapi(mut self, v: OpenApi) -> Self {
        self.openapi = Some(v);
        self
    }
    /// HTTP settings.
    pub fn settings(mut self, v: ApiSettings) -> Self {
        self.settings = Some(v);
        self
    }

    /// Register a typed service (one value per type; registering the same
    /// type again replaces it). Retrieved with [`AppState::extension`].
    pub fn extension<T: Send + Sync + 'static>(mut self, v: Arc<T>) -> Self {
        self.extensions.insert(TypeId::of::<T>(), v);
        self
    }

    /// Finish.
    pub fn build(self) -> AppState {
        let settings = self.settings.unwrap_or_default();
        AppState(Arc::new(Inner {
            sessions: self
                .sessions
                .unwrap_or_else(|| Arc::new(MemorySessions::new())),
            bots: self.bots.unwrap_or_else(|| Arc::new(MemoryBots::new())),
            bearers: self
                .bearers
                .unwrap_or_else(|| Arc::new(MemoryBearers::new())),
            clock: self.clock.unwrap_or_else(|| Arc::new(SystemClock)),
            session_policy: self.session_policy.unwrap_or_default(),
            readiness: self.readiness,
            metrics: Metrics::global(),
            openapi: self.openapi.unwrap_or_else(openapi::base),
            rate_limiter: RateLimiter::new(settings.rate_limit.clone()),
            settings,
            extensions: self.extensions,
        }))
    }
}
