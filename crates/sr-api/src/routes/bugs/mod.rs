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
    extract::{DefaultBodyLimit, Path, Query, State},
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
    /// Newest first (highest id first), without description or bundle; and the number of open reports. `before` is
    /// the id of the last report the caller already has (SEC-AUD-267): only reports with a smaller id are returned.
    async fn list(
        &self,
        status: Option<&str>,
        before: Option<i64>,
        limit: i64,
    ) -> Result<(Vec<BugRow>, i64), String>;
    /// Active accounts of the platform (the count the Admin dashboard shows as `accounts_active`: bots excluded).
    async fn active_accounts(&self) -> Result<i64, String>;
    async fn get(&self, id: i64) -> Result<Option<BugRow>, String>;
    /// Whether a report with this id exists (SEC-AUD-269: the audit entry of a close is written only for one that does).
    async fn exists(&self, id: i64) -> Result<bool, String> {
        Ok(self.get(id).await?.is_some())
    }
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

    /// Give back the slot [`Self::allow_report`] just took, for a report that was refused after it (a description that
    /// is not valid, a picture that is not a JPEG): such a body stores nothing and must not use up the hour or the
    /// day. Only counts matter, so the newest stamp of the account and of the platform goes.
    pub fn release_report(&self, user: i64) {
        let Ok(mut w) = self.windows.lock() else {
            return;
        };
        if let Some(q) = w.users.get_mut(&user) {
            q.pop_back();
        }
        w.global.pop_back();
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
/// day for the platform (counted BEFORE the body is decoded, so an account over its figure never makes the server
/// decode a 2 MiB picture; a body the allow-list then refuses gives its slot back); the server rebuilds the stored
/// report from the allow-list. 400 for a missing or too long description or a picture that is not a JPEG, 404 while
/// `platform.beta_reports` is off, 413 for a picture over 2 MiB, 429 past the limits.
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
    // SEC-AUD-269: the limiter first, so an account over its figure never makes the server decode and check a picture.
    if !deps.limiter.allow_report(p.user_id) {
        return Err(ApiError::with_message(
            ErrorCode::TooManyRequests,
            "Too many bug reports. Try again later.",
        )
        .into());
    }
    let clean = match allow_list::sanitize(&raw, now_ms()) {
        Ok(c) => c,
        Err(r) => {
            // Nothing is stored for a refused body, so it does not use up the hour (the slot goes back).
            deps.limiter.release_report(p.user_id);
            if r == allow_list::Reason::PictureTooLarge {
                return Err(HttpError::from(ApiError::with_message(
                    ErrorCode::RequestEntityTooLarge,
                    r.message(),
                )));
            }
            return Err(HttpError::from(ApiError::invalid_form().field(
                r.field(),
                "INVALID_VALUE",
                r.message().to_owned(),
            )));
        }
    };
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
    /// More reports exist after the last one of `reports` (SEC-AUD-267): ask again with `before` set to its id.
    pub has_more: bool,
    /// D-647: `platform.beta_reports` is on and the platform has 100 or more active accounts, so the page tells staff
    /// the beta tool has outgrown its purpose. No number is given: `STAFF_BASIC` staff need not learn the account count.
    pub beta_over_100: bool,
}

/// The active-account count at which the beta bug reporter is reported as outgrown (D-647).
pub const BETA_NOTICE_ACCOUNTS: i64 = 100;

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
    pub before: Option<String>,
    pub limit: Option<i64>,
}

/// The reports, newest first (any appointed staff member), at most `limit` of them; `before` (a report id) starts the
/// page after that report and `has_more` says whether another page exists. No audit entry: the list holds no
/// description or picture.
#[utoipa::path(
    get, path = "/admin/bug-reports", tag = "bugs",
    params(("status" = Option<String>, Query, description = "open or closed; omit for both"),
           ("before" = Option<String>, Query, description = "A report id: only older reports are returned"),
           ("limit" = Option<i64>, Query, description = "1 to 200, default 50")),
    responses((status = 200, body = BugList), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(
    State(state): State<AppState>,
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
    let before = match q.before.as_deref() {
        None | Some("") => None,
        Some(b) => Some(b.parse::<i64>().ok().filter(|v| *v > 0).ok_or_else(|| {
            HttpError::from(ApiError::invalid_form().field(
                "before",
                "INVALID_VALUE",
                "before is a report id.".to_owned(),
            ))
        })?),
    };
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    // One more than the page, to know whether another page exists.
    let (mut rows, open_count) = deps
        .repo
        .list(status, before, limit + 1)
        .await
        .map_err(|e| internal(&e))?;
    let has_more = i64::try_from(rows.len()).is_ok_and(|n| n > limit);
    rows.truncate(usize::try_from(limit).unwrap_or(50));
    // One cheap count per call, and only while the switch is on.
    let beta_over_100 =
        if crate::module_gate::platform_on(&state, sr_features::BETA_REPORTS_ID).await {
            deps.repo
                .active_accounts()
                .await
                .map_err(|e| internal(&e))?
                >= BETA_NOTICE_ACCOUNTS
        } else {
            false
        };
    Ok(Json(BugList {
        reports: rows.iter().map(summary).collect(),
        open_count,
        has_more,
        beta_over_100,
    }))
}

/// One report with its description and bundle (any appointed staff member). Leaves a platform audit entry
/// (`bug.report.read`) once the report is found and before the data is returned; an id that is not there leaves none.
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
        .require_bug_access(&ctx.caller, "bug.report.read")
        .await
        .map_err(map_err)?;
    let row = deps
        .repo
        .get(id)
        .await
        .map_err(|e| internal(&e))?
        .ok_or_else(unknown)?;
    // The audit log's entry is never optional: if it cannot be written the read fails and nothing is returned.
    ctx.svc()
        .record_bug_access(&ctx.caller, "bug.report.read", id)
        .await
        .map_err(map_err)?;
    Ok(Json(BugDetail {
        summary: summary(&row),
        description: row.description.clone().unwrap_or_default(),
        bundle: row.bundle.clone().unwrap_or(Value::Null),
        closed_ms: row.closed_ms,
        closed_by: row.closed_by.as_ref().map(person),
    }))
}

/// The picture, as a download (any appointed staff member). Leaves a platform audit entry (`bug.screenshot.read`)
/// once the picture is found and before the bytes are returned. 404 (and no entry) when the report has no picture
/// (never sent, closed, or older than 30 days) or there is no such report.
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
        .require_bug_access(&ctx.caller, "bug.screenshot.read")
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
    ctx.svc()
        .record_bug_access(&ctx.caller, "bug.screenshot.read", id)
        .await
        .map_err(map_err)?;
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
/// (`bug.report.close`) before anything changes, for a report that exists (an id that is not there leaves none).
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
        .require_bug_access(&ctx.caller, "bug.report.close")
        .await
        .map_err(map_err)?;
    if !deps.repo.exists(id).await.map_err(|e| internal(&e))? {
        return Err(unknown());
    }
    ctx.svc()
        .record_bug_access(&ctx.caller, "bug.report.close", id)
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
