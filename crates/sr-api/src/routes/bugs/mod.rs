//! The beta bug reporter (SEC-AUD-166 and 167, wave 64, D-647 to D-650). Modelled on crash telemetry
//! (`routes::crash`), with two differences that matter: a report belongs to a person (their account id is stored, so
//! staff can ask a follow-up question) and it may hold a picture of that person's screen. So it is personal data.
//!
//! * `POST /bug-reports`: a signed-in session sends a description, a debug bundle and optionally a JPEG the member
//!   previewed. Nothing is accepted before sign-in ([`SessionOnly`]), the body is capped at
//!   [`allow_list::MAX_BODY_BYTES`], each account and the platform may send only so many, and the route answers like
//!   any switched-off module while `platform.beta_reports` is off (`module_gate`). The server does not store the body:
//!   [`allow_list::sanitize`] builds the stored description, bundle and picture from named fields.
//! * `GET /admin/bug-reports`, `GET /admin/bug-reports/{id}`, `GET /admin/bug-reports/{id}/screenshot`,
//!   `POST /admin/bug-reports/{id}/close`: any appointed staff member (`STAFF_BASIC`). The list carries no
//!   description and no picture. Opening a report, its picture and closing one each write a platform audit entry BEFORE
//!   the data is returned; a failure to write it fails the request, and nothing switches these entries off. The staff
//!   routes are not behind the switch: staff can read and close reports after the beta has ended.
//! * Retention (a daily job, `sr-server`): the picture goes when the report is closed or after 30 days, the report after
//!   90 days. Erasing an account deletes its reports; the data export lists them.

pub mod allow_list;

#[cfg(test)]
mod tests;

use crate::auth::SessionOnly;
use crate::error::{ApiResult, HttpError};
use crate::openapi::ErrorBody;
use crate::routes::Ext;
use crate::routes::admin::{StaffCtx, map_err};
use crate::state::AppState;
use axum::{
    Json,
    extract::{DefaultBodyLimit, Path, Query},
    http::{HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sr_core::{ApiError, ErrorCode};
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
            "/bug-reports",
            post(submit).layer(DefaultBodyLimit::max(allow_list::MAX_BODY_BYTES)),
        )
        .route("/admin/bug-reports", get(list))
        .route("/admin/bug-reports/{bug_id}", get(detail))
        .route("/admin/bug-reports/{bug_id}/screenshot", get(screenshot))
        .route("/admin/bug-reports/{bug_id}/close", post(close))
}

/// What goes into the store: only what [`allow_list::sanitize`] rebuilt.
#[derive(Debug, Clone)]
pub struct NewBug {
    pub reporter_id: i64,
    pub commit_sha: String,
    pub route: String,
    pub description: String,
    pub bundle: Value,
    pub screenshot: Option<Vec<u8>>,
}

/// A person as the list shows them.
#[derive(Debug, Clone, PartialEq)]
pub struct BugPerson {
    pub id: i64,
    pub username: String,
}

/// One stored report (see `sr_db::bug_reports::BugRow`). `description` and `bundle` are `None` in a list; the picture
/// bytes are never part of a row.
#[derive(Debug, Clone, PartialEq)]
pub struct BugRow {
    pub id: i64,
    pub created_ms: i64,
    pub status: String,
    pub route: String,
    pub commit_sha: String,
    pub reporter: BugPerson,
    pub has_screenshot: bool,
    pub description: Option<String>,
    pub bundle: Option<Value>,
    pub closed_ms: Option<i64>,
    pub closed_by: Option<BugPerson>,
}

/// Where bug reports are kept (implemented in `sr-server` over `sr_db::bug_reports::PgBugReports`).
#[async_trait::async_trait]
pub trait BugStore: Send + Sync {
    async fn insert(&self, new: &NewBug) -> Result<i64, String>;
    /// Newest first, without description or bundle; and the number of open reports.
    async fn list(&self, status: Option<&str>, limit: i64) -> Result<(Vec<BugRow>, i64), String>;
    async fn get(&self, id: i64) -> Result<Option<BugRow>, String>;
    async fn screenshot(&self, id: i64) -> Result<Option<Vec<u8>>, String>;
    /// Close the report and delete its picture. Idempotent. `false` when there is no such report.
    async fn close(&self, id: i64, by: i64) -> Result<bool, String>;
}

/// How much one account and the platform may send.
#[derive(Debug, Clone, Copy)]
pub struct BugLimits {
    /// Reports per account per `per_user_window`.
    pub per_user: usize,
    pub per_user_window: Duration,
    /// Reports from everybody per `global_window`.
    pub global: usize,
    pub global_window: Duration,
}

impl Default for BugLimits {
    fn default() -> Self {
        Self {
            per_user: 5,
            per_user_window: Duration::from_secs(3600),
            global: 100,
            global_window: Duration::from_secs(24 * 3600),
        }
    }
}

#[derive(Default)]
struct Windows {
    users: HashMap<i64, VecDeque<Instant>>,
    global: VecDeque<Instant>,
}

/// In-memory sliding windows, per process like every limiter here.
pub struct BugLimiter {
    limits: BugLimits,
    windows: Mutex<Windows>,
}

fn prune(q: &mut VecDeque<Instant>, now: Instant, window: Duration) {
    while q.front().is_some_and(|t| now.duration_since(*t) >= window) {
        q.pop_front();
    }
}

impl BugLimiter {
    pub fn new(limits: BugLimits) -> Self {
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
        prune(&mut w.global, now, self.limits.global_window);
        if w.global.len() >= self.limits.global {
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
}

/// Collaborators of the bug report routes.
pub struct BugDeps {
    pub repo: Arc<dyn BugStore>,
    pub limiter: BugLimiter,
}

impl BugDeps {
    pub fn new(repo: Arc<dyn BugStore>) -> Self {
        Self {
            repo,
            limiter: BugLimiter::new(BugLimits::default()),
        }
    }

    pub fn with_limits(mut self, limits: BugLimits) -> Self {
        self.limiter = BugLimiter::new(limits);
        self
    }
}

fn internal(e: &dyn std::fmt::Display) -> HttpError {
    tracing::error!(error = %e, "bug report store failure");
    ApiError::new(ErrorCode::SrInternal).into()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn unknown() -> HttpError {
    ApiError::with_message(ErrorCode::SrNotFound, "Unknown bug report").into()
}

fn parse_id(raw: &str) -> Result<i64, HttpError> {
    raw.parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(unknown)
}

// ---- POST /bug-reports --------------------------------------------------------------------------------------------

/// What a browser may send. This type only documents the wire contract for the generated client: the handler reads a
/// plain JSON value and takes nothing from it that [`allow_list::sanitize`] does not name, so an extra field is
/// ignored, not rejected.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BugReportBody {
    /// The member's own words, 1 to 2000 characters after trimming.
    pub description: String,
    pub bundle: Option<BugBundleBody>,
    /// Optional: a JPEG the member previewed (and could black out).
    pub screenshot: Option<BugScreenshotBody>,
}

/// The debug bundle. Every field is optional: the server keeps only what its allow-list names and whose shape checks.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BugBundleBody {
    /// The build's commit, 7 to 40 lowercase hex characters.
    pub commit: Option<String>,
    /// The route NAME (for example `lounge.:guildId.:channelId`), never a URL.
    pub route: Option<String>,
    /// Client time, Unix ms.
    pub at: Option<i64>,
    pub viewport: Option<BugViewportBody>,
    /// Capability flags (the crash allow-list's booleans) and `engine`.
    #[schema(value_type = Option<Object>)]
    pub caps: Option<Value>,
    pub locale: Option<String>,
    /// `editorial`, `lounge-light`, `lounge-dark` or `other`.
    pub theme: Option<String>,
    pub online: Option<bool>,
    pub gateway: Option<BugGatewayBody>,
    /// `none`, `installing`, `waiting` or `active`.
    pub sw: Option<String>,
    pub voice: Option<BugVoiceBody>,
    /// At most 20 are kept.
    pub failed_requests: Option<Vec<BugFailedRequestBody>>,
    /// Crash fingerprints (64 lowercase hex characters) this page load already reported; at most 5 are kept.
    pub crashes: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BugViewportBody {
    pub w: u32,
    pub h: u32,
    pub dpr: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BugGatewayBody {
    /// `connected`, `connecting`, `reconnecting` or `closed`.
    pub status: String,
    pub reconnects: Option<u32>,
    pub heartbeat_ms: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BugVoiceBody {
    pub connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BugFailedRequestBody {
    /// `GET`, `POST`, `PUT`, `PATCH`, `DELETE` or `?` (the browser's resource timing does not tell the method).
    pub method: String,
    /// A path template matching `^/[a-z0-9_@:./-]{1,120}$` with ids replaced by `:id`: no query, no run of 6 digits.
    pub path: String,
    pub status: u16,
    pub ms: Option<u32>,
    pub ago_s: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BugScreenshotBody {
    /// Must be `image/jpeg`.
    #[serde(rename = "type")]
    pub kind: String,
    /// The JPEG, base64, at most 2 MiB decoded.
    pub data: String,
    /// How many regions the member blacked out (0 to 64).
    pub masked: Option<u32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BugCreated {
    pub id: String,
}

/// Send a bug report. Session only (nothing before sign-in, D-385); at most 3 MiB; 5 per account per hour and 100 per
/// day for the platform; the server rebuilds the stored report from the allow-list. 400 for a missing or too long
/// description or a picture that is not a JPEG, 404 while `platform.beta_reports` is off, 413 for a picture over 2 MiB,
/// 429 past the limits.
#[utoipa::path(
    post, path = "/bug-reports", tag = "bugs",
    request_body = BugReportBody,
    responses((status = 201, body = BugCreated), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 413, body = ErrorBody), (status = 429, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn submit(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<BugDeps>,
    Json(raw): Json<Value>,
) -> ApiResult<(StatusCode, Json<BugCreated>)> {
    let clean = allow_list::sanitize(&raw, now_ms()).map_err(|r| {
        if r == allow_list::Reason::PictureTooLarge {
            return HttpError::from(ApiError::with_message(
                ErrorCode::RequestEntityTooLarge,
                r.message(),
            ));
        }
        HttpError::from(ApiError::invalid_form().field(
            r.field(),
            "INVALID_VALUE",
            r.message().to_owned(),
        ))
    })?;
    if !deps.limiter.allow_report(p.user_id) {
        return Err(ApiError::with_message(
            ErrorCode::TooManyRequests,
            "Too many bug reports. Try again later.",
        )
        .into());
    }
    let id = deps
        .repo
        .insert(&NewBug {
            reporter_id: p.user_id,
            commit_sha: clean.commit,
            route: clean.route,
            description: clean.description,
            bundle: clean.bundle,
            screenshot: clean.screenshot,
        })
        .await
        .map_err(|e| internal(&e))?;
    Ok((StatusCode::CREATED, Json(BugCreated { id: id.to_string() })))
}

// ---- the staff routes ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BugPersonView {
    pub id: String,
    pub username: String,
}

fn person(p: &BugPerson) -> BugPersonView {
    BugPersonView {
        id: p.id.to_string(),
        username: p.username.clone(),
    }
}

/// A report as the list shows it: no description, no bundle, no picture.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BugSummary {
    pub id: String,
    pub created_ms: i64,
    /// `open` or `closed`.
    pub status: String,
    pub route: String,
    pub commit: String,
    pub reporter: BugPersonView,
    pub has_screenshot: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BugList {
    pub reports: Vec<BugSummary>,
    /// Open reports in all, for the badge (not only those in `reports`).
    pub open_count: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BugDetail {
    #[serde(flatten)]
    pub summary: BugSummary,
    pub description: String,
    /// The allow-listed bundle exactly as stored.
    #[schema(value_type = Object)]
    pub bundle: Value,
    pub closed_ms: Option<i64>,
    pub closed_by: Option<BugPersonView>,
}

fn summary(r: &BugRow) -> BugSummary {
    BugSummary {
        id: r.id.to_string(),
        created_ms: r.created_ms,
        status: r.status.clone(),
        route: r.route.clone(),
        commit: r.commit_sha.clone(),
        reporter: person(&r.reporter),
        has_screenshot: r.has_screenshot,
    }
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub status: Option<String>,
    pub limit: Option<i64>,
}

/// The reports, newest first (any appointed staff member). No audit entry: the list holds no description or picture.
#[utoipa::path(
    get, path = "/admin/bug-reports", tag = "bugs",
    params(("status" = Option<String>, Query, description = "open or closed; omit for both"),
           ("limit" = Option<i64>, Query, description = "1 to 200, default 50")),
    responses((status = 200, body = BugList), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(
    ctx: StaffCtx,
    Ext(deps): Ext<BugDeps>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<BugList>> {
    ctx.svc()
        .authorize_bug_list(&ctx.caller)
        .await
        .map_err(map_err)?;
    let status = match q.status.as_deref() {
        None | Some("") => None,
        Some(s @ ("open" | "closed")) => Some(s),
        Some(_) => {
            return Err(ApiError::invalid_form()
                .field("status", "INVALID_VALUE", "status is open or closed.")
                .into());
        }
    };
    let (rows, open_count) = deps
        .repo
        .list(status, q.limit.unwrap_or(50).clamp(1, 200))
        .await
        .map_err(|e| internal(&e))?;
    Ok(Json(BugList {
        reports: rows.iter().map(summary).collect(),
        open_count,
    }))
}

/// One report with its description and bundle (any appointed staff member). Leaves a platform audit entry
/// (`bug.report.read`) before the data is returned.
#[utoipa::path(
    get, path = "/admin/bug-reports/{bug_id}", tag = "bugs",
    params(("bug_id" = String, Path, description = "Bug report id")),
    responses((status = 200, body = BugDetail), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn detail(
    ctx: StaffCtx,
    Ext(deps): Ext<BugDeps>,
    Path(bug_id): Path<String>,
) -> ApiResult<Json<BugDetail>> {
    let id = parse_id(&bug_id)?;
    ctx.svc()
        .authorize_bug_read(&ctx.caller, id)
        .await
        .map_err(map_err)?;
    let row = deps
        .repo
        .get(id)
        .await
        .map_err(|e| internal(&e))?
        .ok_or_else(unknown)?;
    Ok(Json(BugDetail {
        summary: summary(&row),
        description: row.description.clone().unwrap_or_default(),
        bundle: row.bundle.clone().unwrap_or(Value::Null),
        closed_ms: row.closed_ms,
        closed_by: row.closed_by.as_ref().map(person),
    }))
}

/// The picture, as a download (any appointed staff member). Leaves a platform audit entry (`bug.screenshot.read`)
/// before the bytes are returned. 404 when the report has no picture (never sent, closed, or older than 30 days).
#[utoipa::path(
    get, path = "/admin/bug-reports/{bug_id}/screenshot", tag = "bugs",
    params(("bug_id" = String, Path, description = "Bug report id")),
    responses((status = 200, description = "The JPEG bytes", content_type = "image/jpeg"),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn screenshot(
    ctx: StaffCtx,
    Ext(deps): Ext<BugDeps>,
    Path(bug_id): Path<String>,
) -> ApiResult<Response> {
    let id = parse_id(&bug_id)?;
    ctx.svc()
        .authorize_bug_screenshot(&ctx.caller, id)
        .await
        .map_err(map_err)?;
    let bytes = deps
        .repo
        .screenshot(id)
        .await
        .map_err(|e| internal(&e))?
        .ok_or_else(|| {
            HttpError::from(ApiError::with_message(
                ErrorCode::SrNotFound,
                "This report has no picture",
            ))
        })?;
    Ok(picture_response(id, bytes))
}

/// The picture is an inert download: a fixed type, an attachment, no sniffing, no caching.
pub fn picture_response(id: i64, bytes: Vec<u8>) -> Response {
    let disposition = format!("attachment; filename=\"bug-{id}.jpg\"");
    let mut res = (StatusCode::OK, bytes).into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
    if let Ok(v) = HeaderValue::from_str(&disposition) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    h.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

/// Close a report and delete its picture (any appointed staff member). Idempotent. Leaves a platform audit entry
/// (`bug.report.close`) before anything changes.
#[utoipa::path(
    post, path = "/admin/bug-reports/{bug_id}/close", tag = "bugs",
    params(("bug_id" = String, Path, description = "Bug report id")),
    responses((status = 204, description = "Closed"), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn close(
    ctx: StaffCtx,
    Ext(deps): Ext<BugDeps>,
    Path(bug_id): Path<String>,
) -> ApiResult<StatusCode> {
    let id = parse_id(&bug_id)?;
    ctx.svc()
        .authorize_bug_close(&ctx.caller, id)
        .await
        .map_err(map_err)?;
    let found = deps
        .repo
        .close(id, ctx.caller.user_id)
        .await
        .map_err(|e| internal(&e))?;
    if !found {
        return Err(unknown());
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(OpenApi)]
#[openapi(
    paths(submit, list, detail, screenshot, close),
    components(schemas(
        BugReportBody,
        BugBundleBody,
        BugViewportBody,
        BugGatewayBody,
        BugVoiceBody,
        BugFailedRequestBody,
        BugScreenshotBody,
        BugCreated,
        BugPersonView,
        BugSummary,
        BugList,
        BugDetail
    ))
)]
pub struct BugApiDoc;

/// The OpenAPI document fragment for the bug report endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    BugApiDoc::openapi()
}
