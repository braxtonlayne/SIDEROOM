//! Logging and metrics (FND-OBS-001, FND-OBS-002).

use axum::{
    extract::{MatchedPath, Request},
    middleware::Next,
    response::Response,
};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use sr_core::config::LogConfig;
use std::{sync::OnceLock, time::Instant};
use tracing_subscriber::{EnvFilter, util::SubscriberInitExt};

/// Metric names, so other crates (gateway, jobs, db) record under the same keys.
pub mod names {
    /// Counter: `method`, `route`, `status`.
    pub const HTTP_REQUESTS_TOTAL: &str = "http_requests_total";
    /// Histogram (seconds): `method`, `route`.
    pub const HTTP_REQUEST_DURATION: &str = "http_request_duration_seconds";
    /// Gauge.
    pub const HTTP_IN_FLIGHT: &str = "http_requests_in_flight";
    /// Gauge: connections currently open in the pool.
    pub const DB_POOL_SIZE: &str = "db_pool_connections";
    /// Gauge: idle connections in the pool.
    pub const DB_POOL_IDLE: &str = "db_pool_idle_connections";
    /// Gauge: live gateway sessions.
    pub const GATEWAY_SESSIONS: &str = "gateway_sessions";
    /// Counter: `kind`, `outcome` (`ok`, `failed`, `dead`).
    pub const JOBS_TOTAL: &str = "jobs_processed_total";
    /// Counter: rejected by the rate limiter, `scope`.
    pub const RATE_LIMITED_TOTAL: &str = "http_rate_limited_total";
}

/// Initialise `tracing` from config: JSON lines (or human text when
/// `json = false`). Safe to call more than once; later calls are no-ops.
pub fn init_tracing(cfg: &LogConfig) {
    let filter = EnvFilter::try_new(&cfg.filter).unwrap_or_else(|_| EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    let _ = if cfg.json {
        builder
            .json()
            .with_current_span(true)
            .with_span_list(false)
            .finish()
            .try_init()
    } else {
        builder.finish().try_init()
    };
}

/// Span for one gateway session; enter it for the session's lifetime.
pub fn gateway_session_span(session_id: &str) -> tracing::Span {
    tracing::info_span!("gateway_session", session_id)
}

/// Span for one background job execution.
pub fn job_span(kind: &str, job_id: i64) -> tracing::Span {
    tracing::info_span!("job", kind, job_id)
}

/// Process-wide Prometheus recorder handle.
#[derive(Clone)]
pub struct Metrics(PrometheusHandle);

static GLOBAL: OnceLock<Metrics> = OnceLock::new();

impl Metrics {
    /// The process-wide registry, installed on first use.
    pub fn global() -> Self {
        GLOBAL
            .get_or_init(|| {
                let recorder = PrometheusBuilder::new().build_recorder();
                let handle = recorder.handle();
                if metrics::set_global_recorder(recorder).is_err() {
                    tracing::warn!("a global metrics recorder was already installed");
                }
                describe();
                Metrics(handle)
            })
            .clone()
    }

    /// Prometheus text format.
    pub fn render(&self) -> String {
        self.0.render()
    }
}

fn describe() {
    use metrics::{Unit, describe_counter, describe_gauge, describe_histogram};
    describe_counter!(names::HTTP_REQUESTS_TOTAL, "HTTP requests handled");
    describe_histogram!(
        names::HTTP_REQUEST_DURATION,
        Unit::Seconds,
        "HTTP request latency"
    );
    describe_gauge!(names::HTTP_IN_FLIGHT, "HTTP requests being processed");
    describe_gauge!(names::DB_POOL_SIZE, "Open database connections");
    describe_gauge!(names::DB_POOL_IDLE, "Idle database connections");
    describe_gauge!(names::GATEWAY_SESSIONS, "Live gateway sessions");
    describe_counter!(names::JOBS_TOTAL, "Background jobs processed");
    describe_counter!(
        names::RATE_LIMITED_TOTAL,
        "Requests rejected by rate limits"
    );
}

/// Record the database pool gauges (call periodically with `PgPool::size()`
/// and `num_idle()`).
pub fn record_db_pool(size: u32, idle: usize) {
    metrics::gauge!(names::DB_POOL_SIZE).set(f64::from(size));
    metrics::gauge!(names::DB_POOL_IDLE).set(idle as f64);
}

/// Set the live gateway session gauge.
pub fn record_gateway_sessions(n: usize) {
    metrics::gauge!(names::GATEWAY_SESSIONS).set(n as f64);
}

/// Count a finished job.
pub fn record_job(kind: &'static str, outcome: &'static str) {
    metrics::counter!(names::JOBS_TOTAL, "kind" => kind, "outcome" => outcome).increment(1);
}

/// Middleware recording the standard HTTP metrics. The `route` label is the
/// matched route pattern (low cardinality), never the raw path.
pub async fn http_metrics(req: Request, next: Next) -> Response {
    let method = req.method().as_str().to_owned();
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map_or_else(|| "unmatched".to_owned(), |m| m.as_str().to_owned());
    let start = Instant::now();
    let in_flight = metrics::gauge!(names::HTTP_IN_FLIGHT);
    in_flight.increment(1.0);
    let resp = next.run(req).await;
    in_flight.decrement(1.0);
    let status = resp.status().as_u16().to_string();
    metrics::counter!(names::HTTP_REQUESTS_TOTAL, "method" => method.clone(), "route" => route.clone(), "status" => status)
        .increment(1);
    metrics::histogram!(names::HTTP_REQUEST_DURATION, "method" => method, "route" => route)
        .record(start.elapsed().as_secs_f64());
    resp
}
