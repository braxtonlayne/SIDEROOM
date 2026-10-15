//! Data export routes (LNG-USET-020, doc 09 section 4): "Request my data".
//!
//! * `POST /users/@me/data-export` queues a build job and answers `202` with the request.
//!   One open request at a time, one request per 24 hours.
//! * `GET /users/@me/data-export` is the status of the newest request (`state: "none"` when
//!   there never was one): `pending`, `building`, `ready`, `failed` or `expired`.
//! * `GET /users/@me/data-export/{id}/download` streams the zip while it is `ready` and
//!   younger than 7 days. The link is the route itself: it needs the owner's session.
//!
//! The store behind the routes is [`DataExportStore`] (Postgres in `sr-server`); the zip is
//! built by the `user.data_export` job and lives in object storage.

use crate::routes::messages::error::{MsgError, MsgResult};
use crate::routes::messages::model::iso;
use crate::routes::social::util::actor;
use crate::routes::Ext;
use crate::{auth::UserOnly, openapi::ErrorBody, state::AppState};
use async_trait::async_trait;
use axum::{
    Json, Router,
    body::Body,
    extract::Path,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Serialize;
use sr_core::{ApiError, Clock, ErrorCode};
use sr_storage::ObjectStorage;
use std::sync::Arc;
use utoipa::{OpenApi, ToSchema};

/// One export request as the store keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportRecord {
    pub id: i64,
    pub state: String,
    pub requested_ms: i64,
    pub ready_ms: Option<i64>,
    pub expires_ms: Option<i64>,
    pub storage_key: Option<String>,
    pub size_bytes: Option<i64>,
}

/// Why a request was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// A package is already being built.
    InProgress(ExportRecord),
    /// The last request was less than 24 hours ago.
    TooSoon { retry_after_secs: i64 },
}

/// Persistence behind the routes.
#[async_trait]
pub trait DataExportStore: Send + Sync {
    async fn latest(&self, user: i64) -> Result<Option<ExportRecord>, String>;
    async fn get(&self, user: i64, id: i64) -> Result<Option<ExportRecord>, String>;
    /// Record a request and queue its build job.
    async fn request(&self, user: i64) -> Result<Result<ExportRecord, Refusal>, String>;
}

/// Collaborators of the routes, registered on the composition root.
pub struct DataExportDeps {
    pub store: Arc<dyn DataExportStore>,
    pub storage: Arc<dyn ObjectStorage>,
    pub clock: Arc<dyn Clock>,
}

/// An export request.
#[derive(Debug, Serialize, ToSchema)]
pub struct ExportObject {
    /// Absent when the account never asked.
    pub id: Option<String>,
    /// `none`, `pending`, `building`, `ready`, `failed` or `expired`.
    pub state: String,
    pub requested_at: Option<String>,
    pub ready_at: Option<String>,
    /// The download link works until this time (7 days after it was built).
    pub expires_at: Option<String>,
    pub size_bytes: Option<i64>,
    /// Relative API path of the zip, present while `ready`.
    pub download_url: Option<String>,
}

impl ExportObject {
    fn none() -> Self {
        Self {
            id: None,
            state: "none".into(),
            requested_at: None,
            ready_at: None,
            expires_at: None,
            size_bytes: None,
            download_url: None,
        }
    }

    fn from_record(r: &ExportRecord, now_ms: i64) -> Self {
        // A lapsed link reads as expired even before the hourly sweep deletes the file.
        let lapsed = r.state == "ready" && r.expires_ms.is_some_and(|e| e <= now_ms);
        let ready = r.state == "ready" && !lapsed;
        Self {
            id: Some(r.id.to_string()),
            state: if lapsed { "expired".into() } else { r.state.clone() },
            requested_at: Some(iso(r.requested_ms)),
            ready_at: r.ready_ms.map(iso),
            expires_at: r.expires_ms.map(iso),
            size_bytes: r.size_bytes,
            download_url: ready.then(|| format!("/users/@me/data-export/{}/download", r.id)),
        }
    }
}

fn internal(e: String) -> MsgError {
    tracing::error!(error = %e, "data export store failure");
    ErrorCode::SrInternal.into()
}

/// Ask for a package of the caller's data. Built in the background; poll
/// `GET /users/@me/data-export`.
#[utoipa::path(
    post,
    path = "/users/@me/data-export",
    responses(
        (status = 202, body = ExportObject),
        (status = 400, body = ErrorBody, description = "A package is already being built"),
        (status = 429, body = ErrorBody, description = "Asked less than 24 hours ago"),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn request(
    UserOnly(p): UserOnly,
    Ext(deps): Ext<DataExportDeps>,
) -> MsgResult<(StatusCode, Json<ExportObject>)> {
    let me = actor(&p)?;
    let now = deps.clock.now_unix_ms();
    match deps
        .store
        .request(i64::try_from(me.get()).unwrap_or(0))
        .await
        .map_err(internal)?
    {
        Ok(r) => Ok((StatusCode::ACCEPTED, Json(ExportObject::from_record(&r, now)))),
        Err(Refusal::InProgress(_)) => Err(ApiError::invalid_form()
            .field(
                "export",
                "in_progress",
                "Your data package is already being prepared.",
            )
            .into()),
        Err(Refusal::TooSoon { retry_after_secs }) => Err(MsgError::RetryAfter {
            api: ApiError::with_message(
                ErrorCode::TooManyRequests,
                "You can request your data once a day.",
            ),
            retry_after_ms: retry_after_secs.saturating_mul(1000),
        }),
    }
}

/// The newest request of the caller.
#[utoipa::path(
    get,
    path = "/users/@me/data-export",
    responses((status = 200, body = ExportObject)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn status(UserOnly(p): UserOnly, Ext(deps): Ext<DataExportDeps>) -> MsgResult<Json<ExportObject>> {
    let me = actor(&p)?;
    let now = deps.clock.now_unix_ms();
    let latest = deps
        .store
        .latest(i64::try_from(me.get()).unwrap_or(0))
        .await
        .map_err(internal)?;
    Ok(Json(
        latest.map_or_else(ExportObject::none, |r| ExportObject::from_record(&r, now)),
    ))
}

/// Download the finished package (a zip). 404 once it expired or when it is not ready.
#[utoipa::path(
    get,
    path = "/users/@me/data-export/{id}/download",
    params(("id" = String, Path, description = "Export request id")),
    responses(
        (status = 200, description = "The zip package", content_type = "application/zip"),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn download(
    UserOnly(p): UserOnly,
    Ext(deps): Ext<DataExportDeps>,
    Path(id): Path<String>,
) -> MsgResult<Response> {
    let me = actor(&p)?;
    let not_found = || {
        MsgError::from(ApiError::with_message(
            ErrorCode::UnknownSession,
            "Unknown data export.",
        ))
    };
    let id: i64 = id.parse().map_err(|_| not_found())?;
    let rec = deps
        .store
        .get(i64::try_from(me.get()).unwrap_or(0), id)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    let now = deps.clock.now_unix_ms();
    if rec.state != "ready" || rec.expires_ms.is_none_or(|e| e <= now) {
        return Err(not_found());
    }
    let key = rec.storage_key.ok_or_else(not_found)?;
    let stream = deps.storage.get_stream(&key, None).await?;
    let mut res = Response::new(Body::from_stream(stream.body));
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/zip"));
    h.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"sideroom-data-package.zip\""),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if let Ok(len) = HeaderValue::from_str(&stream.info.size.to_string()) {
        h.insert(header::CONTENT_LENGTH, len);
    }
    Ok(res.into_response())
}

/// The data export routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/users/@me/data-export",
            get(status).post(request),
        )
        .route("/users/@me/data-export/{id}/download", get(download))
}

/// OpenAPI fragment.
#[derive(OpenApi)]
#[openapi(
    paths(request, status, download),
    components(schemas(ExportObject))
)]
pub struct DataExportApiDoc;

