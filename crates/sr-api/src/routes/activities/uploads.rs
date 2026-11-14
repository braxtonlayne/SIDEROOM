//! Resumable upload of an Activity package (SEC-AUD-176, D-654): the five owner routes. The logic is
//! `sr_activities::package_upload::PackageUploads`; this is the HTTP layer. All routes are session only and the
//! caller must own the application (as `upload_bundle`). An upload id of another application answers 404.

use super::{ActivitiesDeps, act};
use crate::{
    auth::SessionOnly,
    openapi::ErrorBody,
    routes::{
        Ext,
        developer::DevDeps,
        messages::error::{ApiJson, MsgError, MsgResult},
    },
};
use axum::{Json, extract::Path, http::StatusCode};
use sr_activities::package_upload::{
    CODE_NO_PARTS, CODE_PARTS_MISSING, PackageLimitsView, PackageUploadBody, PackageUploadError,
    PackageUploadSlot, PackageUploadStatus, PackageUploads,
};
use sr_core::{ApiError, ErrorCode};

fn id(raw: &str, unknown: ErrorCode) -> MsgResult<i64> {
    raw.parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| ApiError::new(unknown).into())
}

/// The upload service, or the answer of a server that has none (the client then falls back to the single request).
fn service(deps: &ActivitiesDeps) -> MsgResult<&PackageUploads> {
    deps.uploads.as_ref().ok_or_else(|| {
        ApiError::with_message(
            ErrorCode::SrNotFound,
            "Package uploads are not available on this server.",
        )
        .into()
    })
}

pub(super) fn up_err(e: PackageUploadError) -> MsgError {
    match e {
        PackageUploadError::TooLarge(m) => {
            ApiError::with_message(ErrorCode::RequestEntityTooLarge, m).into()
        }
        PackageUploadError::RateLimited => MsgError::RetryAfter {
            api: ApiError::with_message(
                ErrorCode::TooManyRequests,
                "Too many package uploads. Try again in an hour.",
            ),
            retry_after_ms: 3_600_000,
        },
        PackageUploadError::Busy => MsgError::RetryAfter {
            api: ApiError::with_message(
                ErrorCode::TooManyRequests,
                "Another upload for this application is starting. Try again in a moment.",
            ),
            retry_after_ms: 5_000,
        },
        PackageUploadError::NoParts => {
            let m = "This server's storage cannot take resumable uploads.";
            ApiError::with_message(ErrorCode::SrConflict, m)
                .field("upload", CODE_NO_PARTS, m)
                .into()
        }
        PackageUploadError::PartsMissing(m) => {
            ApiError::with_message(ErrorCode::SrConflict, m.clone())
                .field("upload", CODE_PARTS_MISSING, m)
                .into()
        }
        PackageUploadError::Activity(e) => act(e),
    }
}

async fn own(dev: &DevDeps, user: i64, app: &str) -> MsgResult<i64> {
    let app = id(app, ErrorCode::UnknownApplication)?;
    dev.apps
        .owned(user, app)
        .await
        .map_err(crate::routes::developer::app_err)?;
    Ok(app)
}

/// The package limits of my application: the size limit with its tier, and whether uploads can be resumed.
#[utoipa::path(
    get, path = "/applications/{application_id}/activity/package-limits", tag = "activities",
    params(("application_id" = String, Path, description = "Application snowflake")),
    responses((status = 200, body = PackageLimitsView), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_limits(
    SessionOnly(p): SessionOnly,
    Ext(dev): Ext<DevDeps>,
    Ext(deps): Ext<ActivitiesDeps>,
    Path(app): Path<String>,
) -> MsgResult<Json<PackageLimitsView>> {
    let app = own(&dev, p.user_id, &app).await?;
    Ok(Json(service(&deps)?.limits(app).await.map_err(act)?))
}

/// Start an upload of a package in parts. The answer holds the presigned part URLs; send each part with a `PUT`,
/// then call `complete`. `413` over the application's limit, `429` after six uploads in an hour, `409`
/// (`PACKAGE_PARTS_UNAVAILABLE`) when this server's storage cannot take parts.
#[utoipa::path(
    post, path = "/applications/{application_id}/activity/uploads", tag = "activities",
    params(("application_id" = String, Path, description = "Application snowflake")),
    request_body = PackageUploadBody,
    responses((status = 201, body = PackageUploadSlot), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody), (status = 413, body = ErrorBody),
              (status = 429, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn begin_upload(
    SessionOnly(p): SessionOnly,
    Ext(dev): Ext<DevDeps>,
    Ext(deps): Ext<ActivitiesDeps>,
    Path(app): Path<String>,
    ApiJson(body): ApiJson<PackageUploadBody>,
) -> MsgResult<(StatusCode, Json<PackageUploadSlot>)> {
    let app = own(&dev, p.user_id, &app).await?;
    let slot = service(&deps)?
        .begin(app, p.user_id, body.size, &body.filename)
        .await
        .map_err(up_err)?;
    Ok((StatusCode::CREATED, Json(slot)))
}

/// Where an upload stands: the parts the store holds and fresh URLs for the rest while uploading, the checked
/// share while checking, then the bundle or the reason it was refused.
#[utoipa::path(
    get, path = "/applications/{application_id}/activity/uploads/{upload_id}", tag = "activities",
    params(("application_id" = String, Path, description = "Application snowflake"),
           ("upload_id" = String, Path, description = "Upload snowflake")),
    responses((status = 200, body = PackageUploadStatus), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_upload(
    SessionOnly(p): SessionOnly,
    Ext(dev): Ext<DevDeps>,
    Ext(deps): Ext<ActivitiesDeps>,
    Path((app, upload)): Path<(String, String)>,
) -> MsgResult<Json<PackageUploadStatus>> {
    let app = own(&dev, p.user_id, &app).await?;
    let upload = id(&upload, ErrorCode::SrNotFound)?;
    Ok(Json(
        service(&deps)?.status(app, upload).await.map_err(act)?,
    ))
}

/// All parts are sent: the server completes the object from the parts the store holds and queues the check.
/// `409` (`PACKAGE_PARTS_MISSING`) names the parts that are missing.
#[utoipa::path(
    post, path = "/applications/{application_id}/activity/uploads/{upload_id}/complete", tag = "activities",
    params(("application_id" = String, Path, description = "Application snowflake"),
           ("upload_id" = String, Path, description = "Upload snowflake")),
    responses((status = 202, body = PackageUploadStatus), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn complete_upload(
    SessionOnly(p): SessionOnly,
    Ext(dev): Ext<DevDeps>,
    Ext(deps): Ext<ActivitiesDeps>,
    Path((app, upload)): Path<(String, String)>,
) -> MsgResult<(StatusCode, Json<PackageUploadStatus>)> {
    let app = own(&dev, p.user_id, &app).await?;
    let upload = id(&upload, ErrorCode::SrNotFound)?;
    let status = service(&deps)?
        .complete(app, upload)
        .await
        .map_err(up_err)?;
    Ok((StatusCode::ACCEPTED, Json(status)))
}

/// Give an upload up: its parts are aborted at the store and a check in progress is cancelled.
#[utoipa::path(
    delete, path = "/applications/{application_id}/activity/uploads/{upload_id}", tag = "activities",
    params(("application_id" = String, Path, description = "Application snowflake"),
           ("upload_id" = String, Path, description = "Upload snowflake")),
    responses((status = 204), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn abort_upload(
    SessionOnly(p): SessionOnly,
    Ext(dev): Ext<DevDeps>,
    Ext(deps): Ext<ActivitiesDeps>,
    Path((app, upload)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let app = own(&dev, p.user_id, &app).await?;
    let upload = id(&upload, ErrorCode::SrNotFound)?;
    service(&deps)?.abort(app, upload).await.map_err(act)?;
    Ok(StatusCode::NO_CONTENT)
}
