//! Comments on series and chapters (LIB-SOC-003, LIB-RDR-014) and the curators' report queue.
//! The logic lives in `sr_library_social::comments`.

use super::social::{CountBody, social_err};
use super::{MemberCtx, PublisherCtx, id};
use crate::{
    error::{ApiResult, HttpError},
    openapi::ErrorBody,
};
use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::json;
use sr_core::{ApiError, ErrorCode, PlatformPermissions as P};
use sr_library_social::model::*;
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CommentParams {
    /// Cursor from the previous page's `next_before`.
    pub before: Option<String>,
    /// 1 to 100, default 24.
    pub limit: Option<u32>,
}

/// Top-level comments on a series, newest first.
#[utoipa::path(
    get, path = "/library/series/{series_id}/comments", operation_id = "library_series_comments",
    params(CommentParams, ("series_id" = String, Path, description = "Series snowflake")),
    responses((status = 200, body = CommentPage), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn series_comments(
    ctx: MemberCtx,
    Path(series_id): Path<String>,
    Query(p): Query<CommentParams>,
) -> ApiResult<Json<CommentPage>> {
    ctx.social()?
        .comments(
            ctx.sviewer(),
            id(&series_id, "series")?,
            0,
            p.before.as_deref(),
            p.limit,
        )
        .await
        .map(Json)
        .map_err(social_err)
}

/// Comment on a series, or reply to one of its top-level comments (`parent_id`).
#[utoipa::path(
    post, path = "/library/series/{series_id}/comments", operation_id = "library_post_series_comment",
    params(("series_id" = String, Path, description = "Series snowflake")), request_body = CommentInput,
    responses((status = 201, body = Comment), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody, description = "Age confirmation required (900011)."),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn post_series_comment(
    ctx: MemberCtx,
    Path(series_id): Path<String>,
    Json(b): Json<CommentInput>,
) -> ApiResult<(StatusCode, Json<Comment>)> {
    let c = ctx
        .social()?
        .post_comment(ctx.sviewer(), id(&series_id, "series")?, 0, b)
        .await
        .map_err(social_err)?;
    Ok((StatusCode::CREATED, Json(c)))
}

/// Top-level comments on a chapter, newest first. `total` counts comments and replies.
#[utoipa::path(
    get, path = "/library/chapters/{chapter_id}/comments", operation_id = "library_chapter_comments",
    params(CommentParams, ("chapter_id" = String, Path, description = "Chapter snowflake")),
    responses((status = 200, body = CommentPage), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn chapter_comments(
    ctx: MemberCtx,
    Path(chapter_id): Path<String>,
    Query(p): Query<CommentParams>,
) -> ApiResult<Json<CommentPage>> {
    ctx.social()?
        .comments(
            ctx.sviewer(),
            0,
            id(&chapter_id, "chapter")?,
            p.before.as_deref(),
            p.limit,
        )
        .await
        .map(Json)
        .map_err(social_err)
}

/// Comment on a chapter, or reply to one of its top-level comments (`parent_id`).
#[utoipa::path(
    post, path = "/library/chapters/{chapter_id}/comments", operation_id = "library_post_chapter_comment",
    params(("chapter_id" = String, Path, description = "Chapter snowflake")), request_body = CommentInput,
    responses((status = 201, body = Comment), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody, description = "Age confirmation required (900011)."),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn post_chapter_comment(
    ctx: MemberCtx,
    Path(chapter_id): Path<String>,
    Json(b): Json<CommentInput>,
) -> ApiResult<(StatusCode, Json<Comment>)> {
    let c = ctx
        .social()?
        .post_comment(ctx.sviewer(), 0, id(&chapter_id, "chapter")?, b)
        .await
        .map_err(social_err)?;
    Ok((StatusCode::CREATED, Json(c)))
}

/// Replies to a top-level comment, oldest first.
#[utoipa::path(
    get, path = "/library/comments/{comment_id}/replies", operation_id = "library_comment_replies",
    params(CommentParams, ("comment_id" = String, Path, description = "Comment snowflake")),
    responses((status = 200, body = CommentPage), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn replies(
    ctx: MemberCtx,
    Path(comment_id): Path<String>,
    Query(p): Query<CommentParams>,
) -> ApiResult<Json<CommentPage>> {
    ctx.social()?
        .comment_replies(
            ctx.sviewer(),
            id(&comment_id, "comment")?,
            p.before.as_deref(),
            p.limit,
        )
        .await
        .map(Json)
        .map_err(social_err)
}

/// Edit one's own comment.
#[utoipa::path(
    patch, path = "/library/comments/{comment_id}", operation_id = "library_edit_comment", request_body = CommentPatch,
    params(("comment_id" = String, Path, description = "Comment snowflake")),
    responses((status = 200, body = Comment), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn edit(
    ctx: MemberCtx,
    Path(comment_id): Path<String>,
    Json(b): Json<CommentPatch>,
) -> ApiResult<Json<Comment>> {
    ctx.social()?
        .edit_comment(ctx.sviewer(), id(&comment_id, "comment")?, b)
        .await
        .map(Json)
        .map_err(social_err)
}

/// Delete one's own comment (its replies go with it).
#[utoipa::path(
    delete, path = "/library/comments/{comment_id}", operation_id = "library_delete_comment",
    params(("comment_id" = String, Path, description = "Comment snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete(ctx: MemberCtx, Path(comment_id): Path<String>) -> ApiResult<StatusCode> {
    ctx.social()?
        .delete_comment(ctx.sviewer(), id(&comment_id, "comment")?)
        .await
        .map_err(social_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Like a comment (idempotent).
#[utoipa::path(
    put, path = "/library/comments/{comment_id}/like", operation_id = "library_like_comment",
    params(("comment_id" = String, Path, description = "Comment snowflake")),
    responses((status = 200, body = CountBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn like(ctx: MemberCtx, Path(comment_id): Path<String>) -> ApiResult<Json<CountBody>> {
    let count = ctx
        .social()?
        .set_comment_like(ctx.sviewer(), id(&comment_id, "comment")?, true)
        .await
        .map_err(social_err)?;
    Ok(Json(CountBody { count }))
}

/// Take back a like.
#[utoipa::path(
    delete, path = "/library/comments/{comment_id}/like", operation_id = "library_unlike_comment",
    params(("comment_id" = String, Path, description = "Comment snowflake")),
    responses((status = 200, body = CountBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn unlike(ctx: MemberCtx, Path(comment_id): Path<String>) -> ApiResult<Json<CountBody>> {
    let count = ctx
        .social()?
        .set_comment_like(ctx.sviewer(), id(&comment_id, "comment")?, false)
        .await
        .map_err(social_err)?;
    Ok(Json(CountBody { count }))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = LibraryReportBody)]
pub struct ReportBody {
    /// 1 to 500 characters.
    pub reason: String,
}

/// Report a comment for the curators.
#[utoipa::path(
    post, path = "/library/comments/{comment_id}/report", operation_id = "library_report_comment", request_body = ReportBody,
    params(("comment_id" = String, Path, description = "Comment snowflake")),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn report(
    ctx: MemberCtx,
    Path(comment_id): Path<String>,
    Json(b): Json<ReportBody>,
) -> ApiResult<StatusCode> {
    ctx.social()?
        .report_comment(ctx.sviewer(), id(&comment_id, "comment")?, &b.reason)
        .await
        .map_err(social_err)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- curators ------------------------------------------------------------------

fn curator(ctx: &PublisherCtx) -> Result<&sr_library_social::LibrarySocial, HttpError> {
    if !ctx.staff.caller.permissions.has(P::REVIEW_READING) {
        return Err(ApiError::with_message(
            ErrorCode::MissingPermissions,
            "Missing platform permission REVIEW_READING".to_owned(),
        )
        .into());
    }
    ctx.library.social.as_ref().ok_or_else(|| {
        tracing::error!("library social service is not configured");
        ErrorCode::SrInternal.into()
    })
}

#[derive(Debug, serde::Serialize, ToSchema)]
#[schema(as = LibraryCommentReports)]
pub struct ReportList {
    pub items: Vec<CommentReport>,
}

/// Open comment reports, oldest first (`REVIEW_READING`).
#[utoipa::path(
    get, path = "/admin/library/comment-reports", operation_id = "admin_library_comment_reports",
    params(CommentParams),
    responses((status = 200, body = ReportList), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn reports(
    ctx: PublisherCtx,
    Query(p): Query<CommentParams>,
) -> ApiResult<Json<ReportList>> {
    let items = curator(&ctx)?
        .comment_reports(p.limit)
        .await
        .map_err(social_err)?;
    Ok(Json(ReportList { items }))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = LibraryResolveReportBody)]
pub struct ResolveBody {
    /// Hide the comment (true) or dismiss the report (false).
    pub hide: bool,
}

/// Resolve a report: hide the comment or dismiss the report (`REVIEW_READING`).
#[utoipa::path(
    post, path = "/admin/library/comment-reports/{report_id}/resolve", operation_id = "admin_library_resolve_comment_report",
    request_body = ResolveBody, params(("report_id" = String, Path, description = "Report snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn resolve(
    ctx: PublisherCtx,
    Path(report_id): Path<String>,
    Json(b): Json<ResolveBody>,
) -> ApiResult<StatusCode> {
    let rid = id(&report_id, "report")?;
    let cid = curator(&ctx)?
        .resolve_comment_report(ctx.staff.caller.user_id, rid, b.hide)
        .await
        .map_err(social_err)?;
    ctx.audit(
        "library.comment.report.resolve",
        ("comment", cid),
        json!({ "report": rid.to_string(), "hide": b.hide }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}
