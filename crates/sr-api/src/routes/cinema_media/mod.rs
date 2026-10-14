//! Cinema media REST API (CIN-ING, CIN-DL): the staff upload path for originals
//! (`/admin/cinema/assets/*`), the transcoding admin (`/admin/transcode/*`: worker tokens, workers,
//! queue, logs, re-transcode, retention sweep) and the Patron downloads (`/cinema/titles/{id}/downloads`,
//! `/cinema/downloads/*`). The worker-facing routes (`/api/v1/transcode/worker/*`) are built by
//! `sr_media::http::router` and mounted by the composition root, not here: they authenticate with
//! worker tokens, not sessions.
//!
//! Permissions: `PUBLISH_MEDIA` for uploads and asset actions, `MANAGE_TRANSCODE` for the
//! transcoding admin; downloads need the Patron perk (403 otherwise) and the same age gate as
//! playback. Every staff change is written to the access log.

mod downloads;
mod staff;
mod transcode;

#[cfg(test)]
mod tests;

use crate::{
    error::HttpError,
    openapi::ErrorBody,
    routes::{
        Ext,
        admin::{AdminDeps, StaffCtx},
        cinema::CinemaDeps,
    },
    state::AppState,
};
use axum::{
    Router,
    extract::FromRequestParts,
    http::request::Parts,
    routing::{delete, get, post, put},
};
use sr_core::{ApiError, ErrorCode, PlatformPermissions as P};
use sr_media::assets::MediaError;
use sr_media::downloads::DownloadService;
use sr_media::service::MediaService;
use std::sync::Arc;
use utoipa::OpenApi;

/// Collaborators of the media endpoints.
pub struct MediaDeps {
    pub media: MediaService,
    pub downloads: DownloadService,
}

impl MediaDeps {
    pub fn new(media: MediaService, downloads: DownloadService) -> Self {
        Self { media, downloads }
    }
}

/// A staff session with the media services (and the catalog, for linking) next to it.
pub(crate) struct MediaCtx {
    pub staff: StaffCtx,
    pub deps: Arc<MediaDeps>,
    pub cinema: Arc<CinemaDeps>,
}

impl MediaCtx {
    pub fn media(&self) -> &MediaService {
        &self.deps.media
    }

    pub fn user(&self) -> i64 {
        self.staff.caller.user_id
    }

    pub fn require(&self, perm: P, name: &'static str) -> Result<(), HttpError> {
        if self.staff.caller.permissions.has(perm) {
            Ok(())
        } else {
            Err(ApiError::with_message(
                ErrorCode::MissingPermissions,
                format!("Missing platform permission {name}"),
            )
            .into())
        }
    }

    /// Record a successful change in the staff access log; a logging failure is reported but
    /// does not fail the request, the change already happened.
    pub async fn audit(&self, action: &str, target: (&str, i64), detail: serde_json::Value) {
        let admin: &AdminDeps = &self.staff.deps;
        if let Err(e) = admin
            .service
            .record_action(&self.staff.caller, action, Some(target), detail)
            .await
        {
            tracing::error!(error = %e, action, "cinema media change was not written to the access log");
        }
    }
}

impl FromRequestParts<AppState> for MediaCtx {
    type Rejection = HttpError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let staff = StaffCtx::from_request_parts(parts, state).await?;
        let Ext(deps) = Ext::<MediaDeps>::from_request_parts(parts, state).await?;
        let Ext(cinema) = Ext::<CinemaDeps>::from_request_parts(parts, state).await?;
        Ok(Self {
            staff,
            deps,
            cinema,
        })
    }
}

/// Parse a snowflake path segment; a malformed one names nothing.
pub(crate) fn id(raw: &str, what: &'static str) -> Result<i64, HttpError> {
    raw.parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| not_found(what))
}

fn not_found(what: &str) -> HttpError {
    ApiError::with_message(ErrorCode::SrNotFound, format!("Unknown {what}")).into()
}

pub(crate) fn map_err(e: MediaError) -> HttpError {
    match e {
        MediaError::NotFound(w) => not_found(w),
        MediaError::Invalid(m) => ApiError::with_message(ErrorCode::InvalidFormBody, m).into(),
        MediaError::Conflict(m) => ApiError::with_message(ErrorCode::SrConflict, m).into(),
        MediaError::Forbidden(m) => ApiError::with_message(ErrorCode::MissingAccess, m).into(),
        MediaError::RateLimited(m) => ApiError::with_message(ErrorCode::TooManyRequests, m).into(),
        MediaError::Unavailable(m) => {
            ApiError::with_message(ErrorCode::ApiResourceOverloaded, m).into()
        }
        MediaError::Storage(m) | MediaError::Backend(m) => {
            tracing::error!(error = %m, "cinema media failure");
            ErrorCode::SrInternal.into()
        }
    }
}

/// Routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        // Staff: originals.
        .route(
            "/admin/cinema/assets",
            get(staff::list).post(staff::begin_upload),
        )
        .route("/admin/cinema/assets/{asset_id}", get(staff::status))
        .route(
            "/admin/cinema/assets/{asset_id}/complete",
            post(staff::complete_upload),
        )
        .route(
            "/admin/cinema/assets/{asset_id}/probe",
            post(staff::reprobe),
        )
        .route(
            "/admin/cinema/assets/{asset_id}/transcode",
            post(staff::transcode),
        )
        .route("/admin/cinema/assets/{asset_id}/link", put(staff::link))
        .route(
            "/admin/cinema/assets/{asset_id}/original-downloads",
            put(staff::original_downloads),
        )
        // Staff: transcoding.
        .route(
            "/admin/transcode/tokens",
            get(transcode::tokens).post(transcode::create_token),
        )
        .route(
            "/admin/transcode/tokens/{token_id}",
            delete(transcode::revoke_token),
        )
        .route("/admin/transcode/workers", get(transcode::workers))
        .route(
            "/admin/transcode/workers/{worker_id}/drain",
            put(transcode::drain),
        )
        .route("/admin/transcode/queue", get(transcode::queue))
        .route("/admin/transcode/jobs", get(transcode::jobs))
        .route("/admin/transcode/jobs/{job_id}", get(transcode::job))
        .route(
            "/admin/transcode/jobs/{job_id}/cancel",
            post(transcode::cancel),
        )
        .route(
            "/admin/transcode/jobs/{job_id}/retry",
            post(transcode::retry),
        )
        .route("/admin/transcode/retranscode", post(transcode::retranscode))
        .route("/admin/transcode/sweep", post(transcode::sweep))
        // Members: Patron downloads.
        .route(
            "/cinema/titles/{title_id}/downloads",
            get(downloads::options).post(downloads::start),
        )
        .route("/cinema/downloads/{job_id}", get(downloads::job))
        .route("/cinema/downloads/{job_id}/file", get(downloads::file))
}

/// OpenAPI fragment for the media endpoints.
#[derive(OpenApi)]
#[openapi(
    paths(
        staff::list,
        staff::begin_upload,
        staff::status,
        staff::complete_upload,
        staff::reprobe,
        staff::transcode,
        staff::link,
        staff::original_downloads,
        transcode::tokens,
        transcode::create_token,
        transcode::revoke_token,
        transcode::workers,
        transcode::drain,
        transcode::queue,
        transcode::jobs,
        transcode::job,
        transcode::cancel,
        transcode::retry,
        transcode::retranscode,
        transcode::sweep,
        downloads::options,
        downloads::start,
        downloads::job,
        downloads::file,
    ),
    components(schemas(
        ErrorBody,
        staff::AssetList,
        staff::LinkBody,
        staff::OriginalDownloadsBody,
        transcode::CreateTokenBody,
        transcode::DrainBody,
        transcode::RetranscodeBody,
        transcode::RetranscodeScopeName,
        downloads::StartBody,
        sr_media::assets::Asset,
        sr_media::assets::AssetState,
        sr_media::assets::JobSummary,
        sr_media::assets::JobDetail,
        sr_media::assets::QueueCounts,
        sr_media::assets::WorkerToken,
        sr_media::assets::WorkerView,
        sr_media::worker_api::JobKind,
        sr_media::worker_api::JobState,
        sr_media::worker_api::Stage,
        sr_media::worker_api::ProgressRequest,
        sr_media::worker_api::WorkerCapabilities,
        sr_media::worker_api::PartUrl,
        sr_media::worker_api::PartDone,
        sr_media::service::UploadRequest,
        sr_media::service::UploadSlot,
        sr_media::service::CompleteUploadRequest,
        sr_media::service::CreatedToken,
        sr_media::service::AssetStatus,
        sr_media::service::ProbeSummary,
        sr_media::service::RetranscodeOutcome,
        sr_media::service::RetranscodeSkip,
        sr_media::service::SweepReport,
        sr_media::downloads::DownloadRendition,
        sr_media::downloads::DownloadKind,
        sr_media::downloads::DownloadOptions,
        sr_media::downloads::DownloadState,
        sr_media::downloads::DownloadJob,
    ))
)]
pub struct MediaApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    MediaApiDoc::openapi()
}
