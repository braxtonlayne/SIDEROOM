//! Deterministic crash telemetry (OPS-CRASH-001, wave 45-A, threat model T45, D-385).
//!
//! * `POST /crash-reports`: a signed-in session sends what the browser caught (an unhandled error or rejection, a React
//!   boundary, a WASM or WebGPU panic) with the last 60 deterministic ticks. Nothing is accepted before sign-in
//!   ([`SessionOnly`]), the body is capped at [`allow_list::MAX_BODY_BYTES`], and each account, and the platform as a
//!   whole, may send only so many. The server does not store or forward the body: it builds a new card from the
//!   allow-list in [`allow_list`] and drops the rest, so the client is never trusted to have stripped anything.
//! * The card is stored once per `(commit, normalized top frames)` fingerprint with a count. The first report posts a
//!   structured card in `#crash-triage` of the Staff Server through [`CrashSink`]; repeats only raise the count (and
//!   refresh the card at a few milestones).
//! * `GET /admin/crash-reports` and `GET /admin/crash-reports/{id}`: any appointed staff member (and an agent run by
//!   one) reads the cards and the replay buffer. Opening a replay leaves a platform audit entry.
//!
//! An agent proposes, a human merges: nothing in this module deploys, merges or runs code. A card is a bug report; a
//! fix is a pull request that staff review like any other.

pub mod allow_list;

use crate::auth::SessionOnly;
use crate::error::{ApiResult, HttpError};
use crate::openapi::ErrorBody;
use crate::routes::Ext;
use crate::routes::admin::{StaffCtx, map_err};
use crate::state::AppState;
use axum::{
    Json,
    extract::{DefaultBodyLimit, Path, Query},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sr_core::{ApiError, ErrorCode};
use sr_db::crash_reports::{CrashRow, PgCrashReports};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use utoipa::{OpenApi, ToSchema};

/// The routes, relative to `/api/v1`.
pub fn router() -> axum::Router<AppState> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route(
            "/crash-reports",
            post(submit).layer(DefaultBodyLimit::max(allow_list::MAX_BODY_BYTES)),
        )
        .route("/admin/crash-reports", get(list))
        .route("/admin/crash-reports/{report_id}", get(detail))
}

/// A crash as the Staff Server card poster sees it.
#[derive(Debug, Clone)]
pub struct CrashView {
    pub id: i64,
    pub count: i32,
    /// The allow-listed card.
    pub card: Value,
    /// The allow-listed replay buffer.
    pub replay: Value,
}

/// Where a crash card goes: `#crash-triage` of the Staff Server (implemented in `sr-server`).
#[async_trait::async_trait]
pub trait CrashSink: Send + Sync {
    /// Post the card of a new crash. Returns the channel and message ids, or `None` when there is no Staff Server.
    async fn post(&self, view: &CrashView) -> Option<(i64, i64)>;
    /// Rewrite the card posted earlier (the count moved).
    async fn refresh(&self, channel: i64, message: i64, view: &CrashView) -> bool;
}

/// How much one account and the platform may send.
#[derive(Debug, Clone, Copy)]
pub struct CrashLimits {
    /// Reports per account per `per_user_window`.
    pub per_user: usize,
    pub per_user_window: Duration,
    /// Reports from everybody per hour.
    pub global_per_hour: usize,
    /// New crash groups (new cards in the channel) per hour.
    pub new_groups_per_hour: usize,
}

impl Default for CrashLimits {
    fn default() -> Self {
        Self {
            per_user: 6,
            per_user_window: Duration::from_secs(10 * 60),
            global_per_hour: 600,
            new_groups_per_hour: 60,
        }
    }
}

#[derive(Default)]
struct Windows {
    users: HashMap<i64, VecDeque<Instant>>,
    global: VecDeque<Instant>,
    groups: VecDeque<Instant>,
}

/// In-memory sliding windows, per process like every limiter here.
pub struct CrashLimiter {
    limits: CrashLimits,
    windows: Mutex<Windows>,
}

fn prune(q: &mut VecDeque<Instant>, now: Instant, window: Duration) {
    while q.front().is_some_and(|t| now.duration_since(*t) >= window) {
        q.pop_front();
    }
}

const HOUR: Duration = Duration::from_secs(3600);

impl CrashLimiter {
    pub fn new(limits: CrashLimits) -> Self {
        Self {
            limits,
            windows: Mutex::new(Windows::default()),
        }
    }

    /// Count one report of `user`; `false` when the account or the platform is over its figure.
    pub fn allow_report(&self, user: i64) -> bool {
        let now = Instant::now();
        let Ok(mut w) = self.windows.lock() else {
            return false;
        };
        prune(&mut w.global, now, HOUR);
        if w.global.len() >= self.limits.global_per_hour {
            return false;
        }
        let window = self.limits.per_user_window;
        if w.users.len() > 10_000 {
            w.users
                .retain(|_, q| q.back().is_some_and(|t| now.duration_since(*t) < window));
        }
        let q = w.users.entry(user).or_default();
        prune(q, now, window);
        if q.len() >= self.limits.per_user {
            return false;
        }
        q.push_back(now);
        w.global.push_back(now);
        true
    }

    /// Count one new crash group; `false` past the hourly figure.
    pub fn allow_new_group(&self) -> bool {
        let now = Instant::now();
        let Ok(mut w) = self.windows.lock() else {
            return false;
        };
        prune(&mut w.groups, now, HOUR);
        if w.groups.len() >= self.limits.new_groups_per_hour {
            return false;
        }
        w.groups.push_back(now);
        true
    }
}

/// Collaborators of the crash routes.
pub struct CrashDeps {
    pub repo: PgCrashReports,
    pub sink: Option<Arc<dyn CrashSink>>,
    pub limiter: CrashLimiter,
}

impl CrashDeps {
    pub fn new(repo: PgCrashReports, sink: Option<Arc<dyn CrashSink>>) -> Self {
        Self {
            repo,
            sink,
            limiter: CrashLimiter::new(CrashLimits::default()),
        }
    }

    pub fn with_limits(mut self, limits: CrashLimits) -> Self {
        self.limiter = CrashLimiter::new(limits);
        self
    }
}

/// A repeat refreshes the card only at these counts, then every hundredth: a crash loop must not turn the channel
/// into a feed of edits.
pub fn refresh_due(count: i32) -> bool {
    matches!(count, 2 | 3 | 5 | 10 | 25 | 50) || (count >= 100 && count % 100 == 0)
}

fn internal(e: &dyn std::fmt::Display) -> HttpError {
    tracing::error!(error = %e, "crash telemetry failure");
    ApiError::new(ErrorCode::SrInternal).into()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ---- POST /crash-reports ------------------------------------------------------------------------------------------

/// What a browser may send. This type only documents the allow-list for the generated client: the handler reads a
/// plain JSON value and takes nothing from it that [`allow_list::sanitize`] does not name, so an extra field is
/// ignored, not rejected.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CrashReportBody {
    /// The build's commit, 7 to 40 lowercase hex characters.
    pub commit: String,
    /// `error`, `unhandled_rejection`, `react_boundary`, `wasm_panic` or `webgpu_panic`.
    pub kind: String,
    /// The route NAME (for example `lounge.:guildId`), never a URL.
    pub route: Option<String>,
    /// Client time, Unix ms.
    pub at: Option<i64>,
    /// Stack frames, newest first (at most 24 are kept).
    pub frames: Option<Vec<CrashFrameBody>>,
    /// The last 60 deterministic ticks, oldest first.
    pub ticks: Option<Vec<CrashTickBody>>,
    /// Capability flags: booleans named in the allow-list, and `engine`.
    #[schema(value_type = Option<Object>)]
    pub caps: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CrashFrameBody {
    pub file: String,
    pub line: Option<u32>,
    pub col: Option<u32>,
    #[serde(rename = "fn")]
    pub function: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CrashTickBody {
    pub t: Option<u64>,
    /// `activity`, `game` or `emulator`.
    pub src: String,
    /// An opaque input enum; anything outside the list is recorded as `other`.
    pub input: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CrashAccepted {
    pub id: String,
    /// Reports with this fingerprint so far.
    pub count: i32,
    /// An earlier report had the same fingerprint.
    pub deduplicated: bool,
}

/// Report a crash. Session only (nothing before sign-in, D-385); at most 32 KiB; the server rebuilds the card from the
/// allow-list and drops every other field. 400 for a report without a valid commit and kind, 429 past the limits.
#[utoipa::path(
    post, path = "/crash-reports", tag = "crash",
    request_body = CrashReportBody,
    responses((status = 202, body = CrashAccepted), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 413, body = ErrorBody),
              (status = 429, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn submit(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<CrashDeps>,
    Json(raw): Json<Value>,
) -> ApiResult<(StatusCode, Json<CrashAccepted>)> {
    if !deps.limiter.allow_report(p.user_id) {
        return Err(ApiError::with_message(
            ErrorCode::TooManyRequests,
            "Too many crash reports. Try again later.",
        )
        .into());
    }
    let s = allow_list::sanitize(&raw, now_ms()).map_err(|r| {
        HttpError::from(ApiError::invalid_form().field(
            "report",
            "INVALID_VALUE",
            r.message().to_owned(),
        ))
    })?;
    let (card, replay) = (s.card_json(), s.replay_json());
    let rec = deps
        .repo
        .record(
            &s.fingerprint,
            &s.card.commit,
            s.card.kind,
            &s.card.route,
            &card,
            &replay,
        )
        .await
        .map_err(|e| internal(&e))?;
    if let Some(sink) = &deps.sink {
        let view = CrashView {
            id: rec.id,
            count: rec.count,
            card,
            replay,
        };
        if rec.is_new {
            if deps.limiter.allow_new_group() {
                if let Some((channel, message)) = sink.post(&view).await {
                    deps.repo
                        .set_message(rec.id, channel, message)
                        .await
                        .map_err(|e| internal(&e))?;
                }
            }
        } else if let (Some(channel), Some(message)) = (rec.channel_id, rec.message_id) {
            if refresh_due(rec.count) {
                // The stored first card and replay are the reproduction; only the count is news.
                if let Ok(Some(row)) = deps.repo.get(rec.id).await {
                    let view = CrashView {
                        id: row.id,
                        count: row.count,
                        card: row.card,
                        replay: row.replay,
                    };
                    sink.refresh(channel, message, &view).await;
                }
            }
        }
    }
    Ok((
        StatusCode::ACCEPTED,
        Json(CrashAccepted {
            id: rec.id.to_string(),
            count: rec.count,
            deduplicated: !rec.is_new,
        }),
    ))
}

// ---- GET /admin/crash-reports -------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CrashSummary {
    pub id: String,
    pub fingerprint: String,
    pub commit: String,
    pub kind: String,
    pub route: String,
    pub count: i32,
    pub first_seen_ms: i64,
    pub last_seen_ms: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CrashDetail {
    #[serde(flatten)]
    pub summary: CrashSummary,
    /// The allow-listed card: commit, kind, route, frames, capability flags.
    #[schema(value_type = Object)]
    pub card: Value,
    /// The last 60 ticks, oldest first: `{t, src, input}` with `input` an opaque enum.
    #[schema(value_type = Vec<Object>)]
    pub replay: Value,
}

fn summary(r: &CrashRow) -> CrashSummary {
    CrashSummary {
        id: r.id.to_string(),
        fingerprint: r.fingerprint.clone(),
        commit: r.commit_sha.clone(),
        kind: r.kind.clone(),
        route: r.route.clone(),
        count: r.count,
        first_seen_ms: r.first_seen_ms,
        last_seen_ms: r.last_seen_ms,
    }
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub limit: Option<i64>,
}

/// The crashes, most recently seen first (any appointed staff member).
#[utoipa::path(
    get, path = "/admin/crash-reports", tag = "crash",
    params(("limit" = Option<i64>, Query, description = "1 to 200, default 50")),
    responses((status = 200, body = Vec<CrashSummary>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(
    ctx: StaffCtx,
    Ext(deps): Ext<CrashDeps>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<CrashSummary>>> {
    ctx.svc()
        .authorize_crash_read(&ctx.caller, None)
        .await
        .map_err(map_err)?;
    let rows = deps
        .repo
        .list(q.limit.unwrap_or(50))
        .await
        .map_err(|e| internal(&e))?;
    Ok(Json(rows.iter().map(summary).collect()))
}

/// One crash with its replay buffer (any appointed staff member). Leaves a platform audit entry.
#[utoipa::path(
    get, path = "/admin/crash-reports/{report_id}", tag = "crash",
    params(("report_id" = String, Path, description = "Crash report id")),
    responses((status = 200, body = CrashDetail), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn detail(
    ctx: StaffCtx,
    Ext(deps): Ext<CrashDeps>,
    Path(report_id): Path<String>,
) -> ApiResult<Json<CrashDetail>> {
    let id = report_id
        .parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| {
            HttpError::from(ApiError::with_message(
                ErrorCode::SrNotFound,
                "Unknown crash report",
            ))
        })?;
    ctx.svc()
        .authorize_crash_read(&ctx.caller, Some(id))
        .await
        .map_err(map_err)?;
    let row = deps
        .repo
        .get(id)
        .await
        .map_err(|e| internal(&e))?
        .ok_or_else(|| {
            HttpError::from(ApiError::with_message(
                ErrorCode::SrNotFound,
                "Unknown crash report",
            ))
        })?;
    Ok(Json(CrashDetail {
        summary: summary(&row),
        card: row.card,
        replay: row.replay,
    }))
}

#[derive(OpenApi)]
#[openapi(
    paths(submit, list, detail),
    components(schemas(
        CrashReportBody,
        CrashFrameBody,
        CrashTickBody,
        CrashAccepted,
        CrashSummary,
        CrashDetail
    ))
)]
pub struct CrashApiDoc;

/// The OpenAPI document fragment for the crash endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    CrashApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repeat_refreshes_the_card_at_milestones_only() {
        let due: Vec<i32> = (1..=300).filter(|c| refresh_due(*c)).collect();
        assert_eq!(due, [2, 3, 5, 10, 25, 50, 100, 200, 300]);
    }

    #[test]
    fn each_account_and_the_platform_have_a_figure() {
        let l = CrashLimiter::new(CrashLimits {
            per_user: 2,
            per_user_window: Duration::from_secs(60),
            global_per_hour: 3,
            new_groups_per_hour: 1,
        });
        assert!(l.allow_report(1));
        assert!(l.allow_report(1));
        // The account is over its figure; another account is not.
        assert!(!l.allow_report(1));
        assert!(l.allow_report(2));
        // The platform is over its figure.
        assert!(!l.allow_report(3));
        assert!(l.allow_new_group());
        assert!(!l.allow_new_group());
    }
}
