//! Library draft content of a submission (`/submissions/{id}/library/*`): the member builds a
//! series and its chapters as unpublished drafts and uploads the files through the same
//! presigned-PUT and `library.ingest` path the publishers use, but only inside the one series
//! of their own submission, and only while the submission is a draft or has changes requested
//! (uploads can still be polled afterwards).

use super::{MemberCtx, ReviewerCtx, id, map_err};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{Json, extract::Path, http::StatusCode};
use sr_core::{ApiError, ErrorCode};
use sr_library::model::*;
use sr_library::submissions::LibraryDraftOps;
use sr_review::model::Submission;
use sr_review::repo::NewItem;

use crate::error::HttpError;

fn ops(ctx: &MemberCtx) -> Result<&LibraryDraftOps, HttpError> {
    ctx.deps.library.as_ref().ok_or_else(|| {
        ApiError::with_message(ErrorCode::SrConflict, "The Library is not available").into()
    })
}

fn label(number: f64, title: Option<&str>) -> String {
    let n = if number.fract() == 0.0 {
        format!("{number:.0}")
    } else {
        number.to_string()
    };
    match title.map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => format!("Chapter {n}: {t}"),
        None => format!("Chapter {n}"),
    }
}

/// An editable submission of a Library kind.
async fn editable(ctx: &MemberCtx, raw: &str) -> Result<Submission, HttpError> {
    ops(ctx)?;
    let s = ctx
        .svc()
        .editable_mine(ctx.user(), id(raw, "submission")?)
        .await
        .map_err(map_err)?;
    library_kind(s)
}

/// Any state, for reads and polling.
async fn own(ctx: &MemberCtx, raw: &str) -> Result<Submission, HttpError> {
    ops(ctx)?;
    let s = ctx
        .svc()
        .own(ctx.user(), id(raw, "submission")?)
        .await
        .map_err(map_err)?;
    library_kind(s)
}

fn library_kind(s: Submission) -> Result<Submission, HttpError> {
    if s.kind.starts_with("library.") {
        Ok(s)
    } else {
        Err(ApiError::with_message(
            ErrorCode::SrConflict,
            "This submission is not a Library submission",
        )
        .into())
    }
}

/// Edit the metadata of the draft series (`library.series` submissions). A cover cannot be
/// set by a member; reviewers can.
#[utoipa::path(
    put, path = "/submissions/{submission_id}/library/series", operation_id = "submissions_library_update_series", request_body = SeriesInput,
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = Series), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn update_series(
    ctx: MemberCtx,
    Path(submission_id): Path<String>,
    Json(b): Json<SeriesInput>,
) -> ApiResult<Json<Series>> {
    let s = editable(&ctx, &submission_id).await?;
    let series = ops(&ctx)?
        .update_series(&s, ctx.user(), b)
        .await
        .map_err(map_err)?;
    Ok(Json(series))
}

/// The chapters of the submission's series, drafts included, with their upload state through
/// `.../chapters/{id}/uploads`.
#[utoipa::path(
    get, path = "/submissions/{submission_id}/library/chapters", operation_id = "submissions_library_chapters",
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = ChapterList), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn chapters(
    ctx: MemberCtx,
    Path(submission_id): Path<String>,
) -> ApiResult<Json<ChapterList>> {
    let s = own(&ctx, &submission_id).await?;
    ops(&ctx)?
        .chapters(&s, ctx.user())
        .await
        .map(Json)
        .map_err(map_err)
}

/// Add a draft chapter to the series (the submission's edition). It is registered as an item
/// of the submission. Upload its file next.
#[utoipa::path(
    post, path = "/submissions/{submission_id}/library/chapters", operation_id = "submissions_library_create_chapter", request_body = ChapterInput,
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 201, body = Chapter), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_chapter(
    ctx: MemberCtx,
    Path(submission_id): Path<String>,
    Json(b): Json<ChapterInput>,
) -> ApiResult<(StatusCode, Json<Chapter>)> {
    let s = editable(&ctx, &submission_id).await?;
    let ch = ops(&ctx)?
        .create_chapter(&s, ctx.user(), None, b)
        .await
        .map_err(map_err)?;
    ctx.svc()
        .register_item(
            ctx.user(),
            s.id,
            NewItem {
                ref_id: ch.id,
                label: label(ch.number, ch.title.as_deref()),
                position: (ch.number.clamp(0.0, 1.0e6) * 100.0).round() as i32,
            },
        )
        .await
        .map_err(map_err)?;
    Ok((StatusCode::CREATED, Json(ch)))
}

/// Edit a draft chapter of the submission.
#[utoipa::path(
    put, path = "/submissions/{submission_id}/library/chapters/{chapter_id}", operation_id = "submissions_library_update_chapter", request_body = ChapterInput,
    params(("submission_id" = String, Path, description = "Submission snowflake"),
           ("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 200, body = Chapter), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn update_chapter(
    ctx: MemberCtx,
    Path((submission_id, chapter_id)): Path<(String, String)>,
    Json(b): Json<ChapterInput>,
) -> ApiResult<Json<Chapter>> {
    let s = editable(&ctx, &submission_id).await?;
    ops(&ctx)?
        .update_chapter(&s, ctx.user(), id(&chapter_id, "chapter")?, b)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Remove a draft chapter from the submission.
#[utoipa::path(
    delete, path = "/submissions/{submission_id}/library/chapters/{chapter_id}", operation_id = "submissions_library_delete_chapter",
    params(("submission_id" = String, Path, description = "Submission snowflake"),
           ("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_chapter(
    ctx: MemberCtx,
    Path((submission_id, chapter_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let s = editable(&ctx, &submission_id).await?;
    let cid = id(&chapter_id, "chapter")?;
    ops(&ctx)?
        .delete_chapter(&s, ctx.user(), cid)
        .await
        .map_err(map_err)?;
    ctx.svc()
        .unregister_item(ctx.user(), s.id, cid)
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Signed page URLs of a draft chapter of my submission.
#[utoipa::path(
    get, path = "/submissions/{submission_id}/library/chapters/{chapter_id}/pages", operation_id = "submissions_library_pages",
    params(("submission_id" = String, Path, description = "Submission snowflake"),
           ("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 200, body = PageManifest), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn pages(
    ctx: MemberCtx,
    Path((submission_id, chapter_id)): Path<(String, String)>,
) -> ApiResult<Json<PageManifest>> {
    let s = own(&ctx, &submission_id).await?;
    ops(&ctx)?
        .pages(&s, ctx.user(), id(&chapter_id, "chapter")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Reserve an upload slot for a chapter's file and get the presigned `PUT` URL (see
/// `POST /admin/library/chapters/{id}/uploads` for the file rules). Then `PUT` the bytes and
/// call `complete`.
#[utoipa::path(
    post, path = "/submissions/{submission_id}/library/chapters/{chapter_id}/uploads", operation_id = "submissions_library_begin_upload", request_body = UploadRequest,
    params(("submission_id" = String, Path, description = "Submission snowflake"),
           ("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 201, body = UploadSlot), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn begin_upload(
    ctx: MemberCtx,
    Path((submission_id, chapter_id)): Path<(String, String)>,
    Json(b): Json<UploadRequest>,
) -> ApiResult<(StatusCode, Json<UploadSlot>)> {
    let s = editable(&ctx, &submission_id).await?;
    let size = b.size_bytes;
    let slot = ops(&ctx)?
        .begin_upload(&s, ctx.user(), id(&chapter_id, "chapter")?, b)
        .await
        .map_err(map_err)?;
    // The file passed the Library checks; now count it against the daily allowance (50 GB
    // for members, staff exempt). Refused here, the slot is never handed out and its
    // reservation lapses with the upload expiry.
    ctx.svc()
        .charge_upload(ctx.caller, size)
        .await
        .map_err(map_err)?;
    Ok((StatusCode::CREATED, Json(slot)))
}

/// The latest uploads of a chapter of my submission.
#[utoipa::path(
    get, path = "/submissions/{submission_id}/library/chapters/{chapter_id}/uploads", operation_id = "submissions_library_chapter_uploads",
    params(("submission_id" = String, Path, description = "Submission snowflake"),
           ("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 200, body = UploadList), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn uploads(
    ctx: MemberCtx,
    Path((submission_id, chapter_id)): Path<(String, String)>,
) -> ApiResult<Json<UploadList>> {
    let s = own(&ctx, &submission_id).await?;
    ops(&ctx)?
        .chapter_uploads(&s, ctx.user(), id(&chapter_id, "chapter")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// The file is uploaded: check it and queue the conversion.
#[utoipa::path(
    post, path = "/submissions/{submission_id}/library/uploads/{upload_id}/complete", operation_id = "submissions_library_complete_upload",
    params(("submission_id" = String, Path, description = "Submission snowflake"),
           ("upload_id" = String, Path, description = "Upload snowflake")),
    responses((status = 202, body = Upload), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn complete_upload(
    ctx: MemberCtx,
    Path((submission_id, upload_id)): Path<(String, String)>,
) -> ApiResult<(StatusCode, Json<Upload>)> {
    let s = editable(&ctx, &submission_id).await?;
    let up = ops(&ctx)?
        .complete_upload(&s, ctx.user(), id(&upload_id, "upload")?)
        .await
        .map_err(map_err)?;
    Ok((StatusCode::ACCEPTED, Json(up)))
}

/// Poll an upload of my submission.
#[utoipa::path(
    get, path = "/submissions/{submission_id}/library/uploads/{upload_id}", operation_id = "submissions_library_upload",
    params(("submission_id" = String, Path, description = "Submission snowflake"),
           ("upload_id" = String, Path, description = "Upload snowflake")),
    responses((status = 200, body = Upload), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn upload_status(
    ctx: MemberCtx,
    Path((submission_id, upload_id)): Path<(String, String)>,
) -> ApiResult<Json<Upload>> {
    let s = own(&ctx, &submission_id).await?;
    ops(&ctx)?
        .upload_status(&s, ctx.user(), id(&upload_id, "upload")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Signed page URLs of a chapter under review (reviewers of the kind).
#[utoipa::path(
    get, path = "/admin/review/submissions/{submission_id}/library/chapters/{chapter_id}/pages", operation_id = "admin_review_library_pages",
    params(("submission_id" = String, Path, description = "Submission snowflake"),
           ("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 200, body = PageManifest), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn review_pages(
    ctx: ReviewerCtx,
    Path((submission_id, chapter_id)): Path<(String, String)>,
) -> ApiResult<Json<PageManifest>> {
    let ops = ctx.review.library.as_ref().ok_or_else(|| -> HttpError {
        ApiError::with_message(ErrorCode::SrConflict, "The Library is not available").into()
    })?;
    let s = ctx
        .svc()
        .reviewable(ctx.caller(), id(&submission_id, "submission")?)
        .await
        .map_err(map_err)?;
    let s = library_kind(s)?;
    ops.pages(&s, ctx.caller().user_id, id(&chapter_id, "chapter")?)
        .await
        .map(Json)
        .map_err(map_err)
}
