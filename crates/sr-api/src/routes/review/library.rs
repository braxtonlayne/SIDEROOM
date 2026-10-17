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
use sr_library::multipart::UploadParts;
use sr_library::submissions::LibraryDraftOps;
use sr_review::EditAs;
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

/// A Library submission whose draft metadata the caller may write: the submitter while it is a draft or
/// has changes requested, or a reviewer of the kind who holds the live claim while it is in review (D-255).
async fn editable_any(ctx: &MemberCtx, raw: &str) -> Result<(Submission, EditAs), HttpError> {
    ops(ctx)?;
    let (s, who) = ctx
        .svc()
        .editable_for(ctx.caller, id(raw, "submission")?)
        .await
        .map_err(map_err)?;
    Ok((library_kind(s)?, who))
}

/// A reviewer's write shows on the submission's timeline for the submitter (`metadata_edited`).
async fn note(ctx: &MemberCtx, s: &Submission, who: EditAs, field: &str) -> Result<(), HttpError> {
    if who == EditAs::Reviewer {
        ctx.svc()
            .note_reviewer_edit(ctx.caller, s.id, &[field])
            .await
            .map_err(map_err)?;
    }
    Ok(())
}

/// Any state, for the submitter or a reviewer of the kind, to read the draft chapters.
async fn readable(ctx: &MemberCtx, raw: &str) -> Result<Submission, HttpError> {
    ops(ctx)?;
    let s = ctx
        .svc()
        .readable_for(ctx.caller, id(raw, "submission")?)
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
    let (s, who) = editable_any(&ctx, &submission_id).await?;
    let series = ops(&ctx)?
        .update_series(&s, ctx.user(), b)
        .await
        .map_err(map_err)?;
    note(&ctx, &s, who, "series").await?;
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
    let s = readable(&ctx, &submission_id).await?;
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
    let (s, who) = editable_any(&ctx, &submission_id).await?;
    let ch = ops(&ctx)?
        .create_chapter(&s, ctx.user(), None, b)
        .await
        .map_err(map_err)?;
    ctx.svc()
        .register_item_for(
            ctx.caller,
            s.id,
            NewItem {
                ref_id: ch.id,
                label: label(ch.number, ch.title.as_deref()),
                position: (ch.number.clamp(0.0, 1.0e6) * 100.0).round() as i32,
            },
        )
        .await
        .map_err(map_err)?;
    note(&ctx, &s, who, "chapters").await?;
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
    let (s, who) = editable_any(&ctx, &submission_id).await?;
    let ch = ops(&ctx)?
        .update_chapter(&s, ctx.user(), id(&chapter_id, "chapter")?, b)
        .await
        .map_err(map_err)?;
    // The review item carries the chapter's number and title: a rename (REV-ED-001) refreshes its label.
    ctx.svc()
        .register_item_for(
            ctx.caller,
            s.id,
            NewItem {
                ref_id: ch.id,
                label: label(ch.number, ch.title.as_deref()),
                position: (ch.number.clamp(0.0, 1.0e6) * 100.0).round() as i32,
            },
        )
        .await
        .map_err(map_err)?;
    note(&ctx, &s, who, "chapters").await?;
    Ok(Json(ch))
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
    let (s, who) = editable_any(&ctx, &submission_id).await?;
    let cid = id(&chapter_id, "chapter")?;
    ops(&ctx)?
        .delete_chapter(&s, ctx.user(), cid)
        .await
        .map_err(map_err)?;
    ctx.svc()
        .unregister_item_for(ctx.caller, s.id, cid)
        .await
        .map_err(map_err)?;
    note(&ctx, &s, who, "chapters").await?;
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

/// Resume a multipart upload: which parts arrived and fresh URLs for the missing ones.
#[utoipa::path(
    get, path = "/submissions/{submission_id}/library/uploads/{upload_id}/parts", operation_id = "submissions_library_upload_parts",
    params(("submission_id" = String, Path, description = "Submission snowflake"),
           ("upload_id" = String, Path, description = "Upload snowflake")),
    responses((status = 200, body = UploadParts), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn upload_parts(
    ctx: MemberCtx,
    Path((submission_id, upload_id)): Path<(String, String)>,
) -> ApiResult<Json<UploadParts>> {
    let s = editable(&ctx, &submission_id).await?;
    ops(&ctx)?
        .upload_parts(&s, ctx.user(), id(&upload_id, "upload")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Cancel an upload that was not completed: its parts are freed.
#[utoipa::path(
    delete, path = "/submissions/{submission_id}/library/uploads/{upload_id}", operation_id = "submissions_library_abort_upload",
    params(("submission_id" = String, Path, description = "Submission snowflake"),
           ("upload_id" = String, Path, description = "Upload snowflake")),
    responses((status = 200, body = Upload), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn abort_upload(
    ctx: MemberCtx,
    Path((submission_id, upload_id)): Path<(String, String)>,
) -> ApiResult<Json<Upload>> {
    let s = editable(&ctx, &submission_id).await?;
    ops(&ctx)?
        .abort_upload(&s, ctx.user(), id(&upload_id, "upload")?)
        .await
        .map(Json)
        .map_err(map_err)
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

/// Body of `POST /library/submissions/{id}/cover`.
#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct CoverBody {
    /// `data:image/jpeg;base64,...` or `data:image/png;base64,...`: one image of at most 2 MiB.
    pub image: String,
}

/// Largest request body of the cover route: a 2 MiB image as base64 plus the JSON around it.
pub const COVER_BODY_LIMIT: usize = 3 * 1024 * 1024 + 512 * 1024;

/// Set the cover of my draft series (G-78): one JPEG or PNG up to 2 MiB, sniffed from its content
/// and re-encoded (metadata stripped, at most 600 x 900). Only the submitter, and only while the
/// submission is a draft or has changes requested. The web app sends the first page of an uploaded
/// PDF here, rendered in the browser. New series (`library.series`) only.
#[utoipa::path(
    post, path = "/library/submissions/{submission_id}/cover", operation_id = "submissions_library_set_cover", request_body = CoverBody,
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = Series), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn set_cover(
    ctx: MemberCtx,
    Path(submission_id): Path<String>,
    Json(b): Json<CoverBody>,
) -> ApiResult<Json<Series>> {
    let (s, who) = editable_any(&ctx, &submission_id).await?;
    let bytes = crate::routes::settings::expressions::decode_data_uri("image", &b.image)?;
    let series = ops(&ctx)?
        .set_cover(&s, ctx.user(), &bytes)
        .await
        .map_err(map_err)?;
    note(&ctx, &s, who, "cover").await?;
    Ok(Json(series))
}

/// List the contents of an uploaded archive of my submission without converting it: pages per
/// chapter folder, ignored files, `ComicInfo.xml` facts and why it would be refused.
#[utoipa::path(
    get, path = "/submissions/{submission_id}/library/uploads/{upload_id}/inspect", operation_id = "submissions_library_inspect_upload",
    params(("submission_id" = String, Path, description = "Submission snowflake"),
           ("upload_id" = String, Path, description = "Upload snowflake")),
    responses((status = 200, body = sr_library::ingest::inspect::ArchiveReport), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn inspect_upload(
    ctx: MemberCtx,
    Path((submission_id, upload_id)): Path<(String, String)>,
) -> ApiResult<Json<sr_library::ingest::inspect::ArchiveReport>> {
    let s = own(&ctx, &submission_id).await?;
    ops(&ctx)?
        .inspect_upload(&s, ctx.user(), id(&upload_id, "upload")?)
        .await
        .map(Json)
        .map_err(map_err)
}
