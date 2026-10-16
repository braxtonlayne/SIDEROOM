//! Publisher uploads (`/admin/library/.../uploads`): presigned upload slots, completion and
//! status polling for chapter files (CBZ/CB7/CBR/EPUB/PDF). The conversion itself runs in the
//! `library.ingest` job. Begin and complete are written to the staff access log; the job writes
//! the outcome (`library.chapter.ingest`).

use super::{PublisherCtx, id, map_err};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{Json, extract::Path, http::StatusCode};
use serde_json::json;
use sr_library::model::*;

/// Reserve an upload slot for a chapter's file and get the presigned `PUT` URL (`PUBLISH_READING`).
///
/// The file's extension must fit the chapter's format (`.cbz .zip .cb7 .7z .cbr .rar` for
/// images, `.epub`, `.pdf`). Send the bytes to `upload_url` with `PUT`, then call the
/// `complete` endpoint. A slot that is never completed expires after a day.
#[utoipa::path(
    post, path = "/admin/library/chapters/{chapter_id}/uploads", operation_id = "admin_library_begin_upload",
    request_body = UploadRequest,
    params(("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 201, body = UploadSlot), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody, description = "The chapter already has an upload being converted, or uploads are not available.")),
    security(("session" = []))
)]
pub async fn begin(
    ctx: PublisherCtx,
    Path(chapter_id): Path<String>,
    Json(b): Json<UploadRequest>,
) -> ApiResult<(StatusCode, Json<UploadSlot>)> {
    let slot = ctx
        .svc()
        .begin_upload(ctx.actor(), id(&chapter_id, "chapter")?, b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "library.upload.begin",
        ("chapter", slot.upload.chapter_id),
        json!({
            "upload": slot.upload.id.to_string(),
            "filename": slot.upload.filename,
            "size_bytes": slot.upload.size_bytes,
        }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(slot)))
}

/// The file is uploaded: check it and queue the conversion (`PUBLISH_READING`). Repeating the call
/// for a queued or finished upload returns it unchanged.
#[utoipa::path(
    post, path = "/admin/library/uploads/{upload_id}/complete", operation_id = "admin_library_complete_upload",
    params(("upload_id" = String, Path, description = "Upload snowflake")),
    responses((status = 202, body = Upload), (status = 400, body = ErrorBody, description = "The file is missing or over the size limit."),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn complete(
    ctx: PublisherCtx,
    Path(upload_id): Path<String>,
) -> ApiResult<(StatusCode, Json<Upload>)> {
    let up = ctx
        .svc()
        .complete_upload(ctx.actor(), id(&upload_id, "upload")?)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "library.upload.complete",
        ("chapter", up.chapter_id),
        json!({ "upload": up.id.to_string(), "status": up.status.as_str() }),
    )
    .await;
    Ok((StatusCode::ACCEPTED, Json(up)))
}

/// Poll an upload: `awaiting_upload`, `queued`, `running`, `done`, `failed` or `expired`
/// (`PUBLISH_READING`).
#[utoipa::path(
    get, path = "/admin/library/uploads/{upload_id}", operation_id = "admin_library_upload",
    params(("upload_id" = String, Path, description = "Upload snowflake")),
    responses((status = 200, body = Upload), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn status(ctx: PublisherCtx, Path(upload_id): Path<String>) -> ApiResult<Json<Upload>> {
    ctx.svc()
        .upload_status(ctx.actor(), id(&upload_id, "upload")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// The latest uploads of a chapter, newest first (`PUBLISH_READING`).
#[utoipa::path(
    get, path = "/admin/library/chapters/{chapter_id}/uploads", operation_id = "admin_library_chapter_uploads",
    params(("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 200, body = UploadList), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(
    ctx: PublisherCtx,
    Path(chapter_id): Path<String>,
) -> ApiResult<Json<UploadList>> {
    ctx.svc()
        .chapter_uploads(ctx.actor(), id(&chapter_id, "chapter")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Resume a multipart upload: the parts that arrived and fresh URLs for the missing ones
/// (`PUBLISH_READING`).
#[utoipa::path(
    get, path = "/admin/library/uploads/{upload_id}/parts", operation_id = "admin_library_upload_parts",
    params(("upload_id" = String, Path, description = "Upload snowflake")),
    responses((status = 200, body = sr_library::multipart::UploadParts), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody, description = "Not a multipart upload, or no longer awaiting its file.")),
    security(("session" = []))
)]
pub async fn parts(
    ctx: PublisherCtx,
    Path(upload_id): Path<String>,
) -> ApiResult<Json<sr_library::multipart::UploadParts>> {
    ctx.svc()
        .upload_parts(ctx.actor(), id(&upload_id, "upload")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Cancel an upload that was not completed and free its parts (`PUBLISH_READING`).
#[utoipa::path(
    delete, path = "/admin/library/uploads/{upload_id}", operation_id = "admin_library_abort_upload",
    params(("upload_id" = String, Path, description = "Upload snowflake")),
    responses((status = 200, body = Upload), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn abort(ctx: PublisherCtx, Path(upload_id): Path<String>) -> ApiResult<Json<Upload>> {
    let up = ctx
        .svc()
        .abort_upload(ctx.actor(), id(&upload_id, "upload")?)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "library.upload.abort",
        ("chapter", up.chapter_id),
        json!({ "upload": up.id.to_string() }),
    )
    .await;
    Ok(Json(up))
}
