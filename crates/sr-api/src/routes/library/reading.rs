//! Member reading state: progress and cross-device sync (LIB-RDR-012), read marks, history and
//! bookmarks (LIB-RDR-013). Every change is also dispatched as `SR_LIBRARY_UPDATE` to the
//! caller's other sessions.

use super::{MemberCtx, id, map_err};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use serde::Deserialize;
use sr_library::LibraryError;
use sr_library::reading::*;
use utoipa::IntoParams;

/// The caller's saved progress in a chapter.
#[utoipa::path(
    get, path = "/library/chapters/{chapter_id}/progress", operation_id = "library_get_progress",
    params(("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 200, body = Progress), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody, description = "Age confirmation required (900011)."),
              (status = 404, body = ErrorBody, description = "Unknown chapter, or no progress saved yet.")),
    security(("session" = []))
)]
pub async fn get_progress(
    ctx: MemberCtx,
    Path(chapter_id): Path<String>,
) -> ApiResult<Json<Progress>> {
    ctx.deps
        .service
        .progress(ctx.viewer, id(&chapter_id, "chapter")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Save reading progress. With `base_updated_at_ms` a newer stored position is kept and returned
/// with `applied: false` so the client can ask "continue where?"; repeat with `force` to overwrite.
#[utoipa::path(
    put, path = "/library/chapters/{chapter_id}/progress", operation_id = "library_put_progress",
    request_body = ProgressInput,
    params(("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 200, body = ProgressSaved), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody, description = "Age confirmation required (900011)."),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn put_progress(
    ctx: MemberCtx,
    Path(chapter_id): Path<String>,
    Json(b): Json<ProgressInput>,
) -> ApiResult<Json<ProgressSaved>> {
    ctx.deps
        .service
        .save_progress(ctx.viewer, id(&chapter_id, "chapter")?, b)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Mark a chapter unread (forget its progress).
#[utoipa::path(
    delete, path = "/library/chapters/{chapter_id}/progress", operation_id = "library_delete_progress",
    params(("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_progress(
    ctx: MemberCtx,
    Path(chapter_id): Path<String>,
) -> ApiResult<StatusCode> {
    let removed = ctx
        .deps
        .service
        .clear_progress(ctx.viewer, id(&chapter_id, "chapter")?)
        .await
        .map_err(map_err)?;
    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(map_err(LibraryError::NotFound("progress")))
    }
}

/// Mark a chapter read or unread, and with `up_to` every earlier chapter of its edition.
#[utoipa::path(
    post, path = "/library/chapters/{chapter_id}/read", operation_id = "library_mark_read",
    request_body = ReadMarkInput,
    params(("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 200, body = ReadMarkResult), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn mark_read(
    ctx: MemberCtx,
    Path(chapter_id): Path<String>,
    Json(b): Json<ReadMarkInput>,
) -> ApiResult<Json<ReadMarkResult>> {
    ctx.deps
        .service
        .mark_read(ctx.viewer, id(&chapter_id, "chapter")?, b)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct HistoryParams {
    /// Keep only the latest chapter of each series (default true).
    pub per_series: Option<bool>,
    /// Cursor from the previous page\'s `next_before`.
    pub before: Option<String>,
    /// 1 to 100, default 24.
    pub limit: Option<u32>,
}

/// Reading history, newest first ("pick up where you left off", the history page).
#[utoipa::path(
    get, path = "/library/me/progress", operation_id = "library_my_progress", params(HistoryParams),
    responses((status = 200, body = HistoryPage), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn history(
    ctx: MemberCtx,
    Query(p): Query<HistoryParams>,
) -> ApiResult<Json<HistoryPage>> {
    ctx.deps
        .service
        .history(
            ctx.viewer,
            p.per_series.unwrap_or(true),
            p.before.as_deref(),
            p.limit,
        )
        .await
        .map(Json)
        .map_err(map_err)
}

/// The caller's bookmarks in a chapter.
#[utoipa::path(
    get, path = "/library/chapters/{chapter_id}/bookmarks", operation_id = "library_chapter_bookmarks",
    params(("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 200, body = BookmarkList), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn chapter_bookmarks(
    ctx: MemberCtx,
    Path(chapter_id): Path<String>,
) -> ApiResult<Json<BookmarkList>> {
    ctx.deps
        .service
        .chapter_bookmarks(ctx.viewer, id(&chapter_id, "chapter")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Bookmark a page, or change the note of the bookmark already on it.
#[utoipa::path(
    put, path = "/library/chapters/{chapter_id}/bookmarks", operation_id = "library_put_bookmark",
    request_body = BookmarkInput,
    params(("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 200, body = Bookmark), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody, description = "The chapter already holds 200 bookmarks.")),
    security(("session" = []))
)]
pub async fn put_bookmark(
    ctx: MemberCtx,
    Path(chapter_id): Path<String>,
    Json(b): Json<BookmarkInput>,
) -> ApiResult<Json<Bookmark>> {
    ctx.deps
        .service
        .save_bookmark(ctx.viewer, id(&chapter_id, "chapter")?, b)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Remove one of the caller's bookmarks.
#[utoipa::path(
    delete, path = "/library/bookmarks/{bookmark_id}", operation_id = "library_delete_bookmark",
    params(("bookmark_id" = String, Path, description = "Bookmark snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_bookmark(
    ctx: MemberCtx,
    Path(bookmark_id): Path<String>,
) -> ApiResult<StatusCode> {
    let removed = ctx
        .deps
        .service
        .delete_bookmark(ctx.viewer, id(&bookmark_id, "bookmark")?)
        .await
        .map_err(map_err)?;
    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(map_err(LibraryError::NotFound("bookmark")))
    }
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct BookmarksParams {
    /// Cursor from the previous page\'s `next_before`.
    pub before: Option<String>,
    /// 1 to 100, default 24.
    pub limit: Option<u32>,
}

/// All of the caller's bookmarks with series and chapter titles, newest first.
#[utoipa::path(
    get, path = "/library/me/bookmarks", operation_id = "library_my_bookmarks", params(BookmarksParams),
    responses((status = 200, body = BookmarkPage), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn my_bookmarks(
    ctx: MemberCtx,
    Query(p): Query<BookmarksParams>,
) -> ApiResult<Json<BookmarkPage>> {
    ctx.deps
        .service
        .my_bookmarks(ctx.viewer, p.before.as_deref(), p.limit)
        .await
        .map(Json)
        .map_err(map_err)
}
