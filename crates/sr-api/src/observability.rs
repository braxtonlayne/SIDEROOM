//! Logging and metrics (FND-OBS-001, FND-OBS-002).

use axum::{
    extract::{MatchedPath, Request},
    middleware::Next,
    response::Response,
};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use sr_core::config::LogConfig;
use std::{sync::OnceLock, time::Instant};
use tracing_subscriber::{EnvFilter, util::SubscriberInitExt};

/// Metric names, so other crates (gateway, jobs, db) record under the same keys.
pub mod names {
    /// Counter: `method`, `route`, `status`.
    pub const HTTP_REQUESTS_TOTAL: &str = "http_requests_total";
    /// Histogram (seconds): `method`, `route`.
    pub const HTTP_REQUEST_DURATION: &str = "http_request_duration_seconds";
    /// Bucket bounds (seconds) for [`HTTP_REQUEST_DURATION`].
    pub const HTTP_LATENCY_BUCKETS: &[f64] = &[
        0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
    ];
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
    /// Histogram (bytes): size of each gateway frame sent to a client, before
    /// compression; `compression` is `none` or `zstd`. The "p95 above 64 KiB"
    /// trigger for browser zstd (Q122) is read from this. Recorded by `sr-gateway`
    /// under the same key.
    pub const GATEWAY_DISPATCH_BYTES: &str = "sr_gateway_dispatch_bytes";
    /// Bucket bounds (bytes) for [`GATEWAY_DISPATCH_BYTES`]; 64 KiB is a bound.
    pub const GATEWAY_DISPATCH_BUCKETS: &[f64] = &[
        256.0, 1024.0, 4096.0, 16384.0, 32768.0, 65536.0, 131072.0, 262144.0, 1048576.0,
    ];
    /// Gauge: jobs waiting to run (`state = 'queued'`), per `kind`. A kind with none is set to 0.
    /// Set by the health job (`admin.health.check`), never on scrape.
    pub const JOBS_QUEUED: &str = "sr_jobs_queued";
    /// Gauge (seconds): how long the oldest due queued job has been waiting; 0 when nothing is due.
    /// The `JobBacklog` rule reads it. Set by the health job.
    pub const JOBS_OLDEST_QUEUED_AGE: &str = "sr_jobs_oldest_queued_age_seconds";
    /// Gauge (unix seconds): when the newest backup file in the backup folder was written; 0 when
    /// there is none or no folder is configured (so the `BackupMissingOrStale` rule fires).
    pub const BACKUP_LAST_SUCCESS: &str = "sr_backup_last_success_timestamp_seconds";
    /// Gauge (unix seconds): when the restore drill last left its `restore-drill.ok` marker; 0 when
    /// it never did.
    pub const RESTORE_CHECK_LAST_SUCCESS: &str = "sr_restore_check_last_success_timestamp_seconds";
    /// Gauge (unix seconds): `pg_stat_archiver.last_archived_time`. Absent while `archive_mode` is
    /// off, so its rule cannot fire on a server that does not archive.
    pub const WAL_ARCHIVE_LAST_SUCCESS: &str = "sr_wal_archive_last_success_timestamp_seconds";
    /// Counter: `pg_stat_archiver.failed_count` (cumulative since the last statistics reset).
    /// Absent while `archive_mode` is off.
    pub const WAL_ARCHIVE_FAILED_TOTAL: &str = "sr_wal_archive_failed_total";
    /// Gauge (ratio 0 to 1): free space of the disk holding `path`. Set by the health job from the
    /// same figures as Admin > System; the live threshold is the `alerts.disk_percent` setting.
    pub const DISK_FREE_RATIO: &str = "sr_disk_free_ratio";
    /// Gauge: invoices waiting for payment (`open`, `seen` or `confirming`). Set by the invoice
    /// watcher on each pass.
    pub const INVOICES_PENDING: &str = "sr_invoices_pending";
    /// Gauge (seconds): age of the oldest invoice still waiting for payment; 0 when there is none.
    /// Set by the invoice watcher on each pass.
    pub const INVOICE_OLDEST_PENDING_AGE: &str = "sr_invoice_oldest_pending_age_seconds";
    /// Gauge (bytes): stored bytes per `bucket` (`attachments`). For Grafana; no alert rule reads it.
    pub const STORAGE_BYTES: &str = "sr_storage_bytes";
    /// Counter: Content-Security-Policy violation reports taken, per `directive`.
    pub const CSP_REPORTS_TOTAL: &str = "sr_csp_reports_total";
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
                let recorder = PrometheusBuilder::new()
                    .set_buckets_for_metric(
                        Matcher::Full(names::GATEWAY_DISPATCH_BYTES.to_owned()),
                        names::GATEWAY_DISPATCH_BUCKETS,
                    )
                    .and_then(|b| {
                        // Real histograms (not summaries) so Grafana can compute
                        // p95 across instances; 100 ms is the REST p95 budget.
                        b.set_buckets_for_metric(
                            Matcher::Full(names::HTTP_REQUEST_DURATION.to_owned()),
                            names::HTTP_LATENCY_BUCKETS,
                        )
                    })
                    .map_or_else(
                        |_| PrometheusBuilder::new().build_recorder(),
                        |b| b.build_recorder(),
                    );
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
    describe_histogram!(
        names::GATEWAY_DISPATCH_BYTES,
        Unit::Bytes,
        "Size of gateway frames sent to clients, before compression"
    );
    describe_counter!(names::JOBS_TOTAL, "Background jobs processed");
    describe_counter!(
        names::RATE_LIMITED_TOTAL,
        "Requests rejected by rate limits"
    );
    describe_gauge!(names::JOBS_QUEUED, "Jobs waiting to run, per kind");
    describe_gauge!(
        names::JOBS_OLDEST_QUEUED_AGE,
        Unit::Seconds,
        "Age of the oldest due queued job"
    );
    describe_gauge!(
        names::BACKUP_LAST_SUCCESS,
        Unit::Seconds,
        "Unix time of the newest backup file (0: none)"
    );
    describe_gauge!(
        names::RESTORE_CHECK_LAST_SUCCESS,
        Unit::Seconds,
        "Unix time of the last restore drill marker (0: never)"
    );
    describe_gauge!(
        names::WAL_ARCHIVE_LAST_SUCCESS,
        Unit::Seconds,
        "Unix time of the last archived WAL segment"
    );
    describe_counter!(
        names::WAL_ARCHIVE_FAILED_TOTAL,
        "Failed WAL archive attempts"
    );
    describe_gauge!(names::DISK_FREE_RATIO, "Free space ratio of a watched disk");
    describe_gauge!(names::INVOICES_PENDING, "Invoices waiting for payment");
    describe_gauge!(
        names::INVOICE_OLDEST_PENDING_AGE,
        Unit::Seconds,
        "Age of the oldest invoice waiting for payment"
    );
    describe_gauge!(names::STORAGE_BYTES, Unit::Bytes, "Stored bytes per bucket");
    describe_counter!(
        names::CSP_REPORTS_TOTAL,
        "Content-Security-Policy violation reports, per directive"
    );
}

/// Kinds [`record_jobs_queued`] last reported, so a kind that drains is set back to 0 instead of
/// keeping its last figure.
static QUEUED_KINDS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Set the queue gauges: `per_kind` the queued job counts, `oldest_age_secs` the age of the oldest
/// due job (0 when none). Called by the health job.
pub fn record_jobs_queued(per_kind: &[(String, i64)], oldest_age_secs: f64) {
    let mut seen = QUEUED_KINDS.lock().unwrap_or_else(|e| e.into_inner());
    for (kind, n) in per_kind {
        metrics::gauge!(names::JOBS_QUEUED, "kind" => kind.clone()).set(*n as f64);
    }
    for gone in seen
        .iter()
        .filter(|k| !per_kind.iter().any(|(p, _)| p == *k))
    {
        metrics::gauge!(names::JOBS_QUEUED, "kind" => gone.clone()).set(0.0);
    }
    *seen = per_kind.iter().map(|(k, _)| k.clone()).collect();
    metrics::gauge!(names::JOBS_OLDEST_QUEUED_AGE).set(oldest_age_secs);
}

/// Set the backup and restore-drill timestamps (unix seconds; `None` is 0, which reads as "never").
pub fn record_backup_times(backup_secs: Option<f64>, restore_check_secs: Option<f64>) {
    metrics::gauge!(names::BACKUP_LAST_SUCCESS).set(backup_secs.unwrap_or(0.0));
    metrics::gauge!(names::RESTORE_CHECK_LAST_SUCCESS).set(restore_check_secs.unwrap_or(0.0));
}

/// Set the WAL archiver series from `pg_stat_archiver`. Call only when `archive_mode` is on, so the
/// series stays absent otherwise and its alert cannot fire. `failed_total` is cumulative.
pub fn record_wal_archive(last_archived_secs: Option<f64>, failed_total: u64) {
    if let Some(t) = last_archived_secs {
        metrics::gauge!(names::WAL_ARCHIVE_LAST_SUCCESS).set(t);
    }
    metrics::counter!(names::WAL_ARCHIVE_FAILED_TOTAL).absolute(failed_total);
}

/// Set the free-space ratio of the disk holding `path`.
pub fn record_disk_free(path: &str, free_bytes: u64, total_bytes: u64) {
    if total_bytes == 0 {
        return;
    }
    metrics::gauge!(names::DISK_FREE_RATIO, "path" => path.to_owned())
        .set(free_bytes as f64 / total_bytes as f64);
}

/// Set the invoice gauges (the invoice watcher, each pass).
pub fn record_invoices(pending: i64, oldest_pending_age_secs: f64) {
    metrics::gauge!(names::INVOICES_PENDING).set(pending as f64);
    metrics::gauge!(names::INVOICE_OLDEST_PENDING_AGE).set(oldest_pending_age_secs);
}

/// Set the stored bytes of one bucket.
pub fn record_storage_bytes(bucket: &'static str, bytes: u64) {
    metrics::gauge!(names::STORAGE_BYTES, "bucket" => bucket).set(bytes as f64);
}

/// Count one CSP violation report (the `directive` is already reduced to a directive name).
pub fn record_csp_report(directive: &str) {
    metrics::counter!(names::CSP_REPORTS_TOTAL, "directive" => directive.to_owned()).increment(1);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// RDY-OBS-001: every gauge and counter the playbooks read is registered under its documented name.
    #[test]
    fn the_playbook_series_are_exported_under_their_names() {
        let m = Metrics::global();
        record_jobs_queued(&[("mail.send".to_owned(), 3)], 42.0);
        record_backup_times(Some(1_700_000_000.0), None);
        record_wal_archive(Some(1_700_000_001.0), 2);
        record_disk_free("/data", 25, 100);
        record_invoices(4, 90.0);
        record_storage_bytes("attachments", 1234);
        record_csp_report("img-src");
        let text = m.render();
        for series in [
            "sr_jobs_queued{kind=\"mail.send\"} 3",
            "sr_jobs_oldest_queued_age_seconds 42",
            "sr_backup_last_success_timestamp_seconds 1700000000",
            "sr_restore_check_last_success_timestamp_seconds 0",
            "sr_wal_archive_last_success_timestamp_seconds 1700000001",
            "sr_wal_archive_failed_total 2",
            "sr_disk_free_ratio{path=\"/data\"} 0.25",
            "sr_invoices_pending 4",
            "sr_invoice_oldest_pending_age_seconds 90",
            "sr_storage_bytes{bucket=\"attachments\"} 1234",
            "sr_csp_reports_total{directive=\"img-src\"} 1",
        ] {
            assert!(
                text.contains(series),
                "missing {series} in:
{text}"
            );
        }
        // A kind that drains reads 0 instead of its last figure.
        record_jobs_queued(&[], 0.0);
        assert!(m.render().contains("sr_jobs_queued{kind=\"mail.send\"} 0"));
    }
}
