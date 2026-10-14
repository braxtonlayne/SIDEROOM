//! Ratings, reviews, comments and reports (CIN-SOC-001).

use super::{CinemaCtx, CountBody, id, map_err};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use serde::Deserialize;
use sr_cinema_social::model::*;
use sr_cinema_social::{ContentKind, SocialError};
use utoipa::IntoParams;

fn episode(raw: &str) -> ApiResult<i64> {
    id(raw, "episode")
}

// ---- ratings ---------------------------------------------------------------

async fn ratings(ctx: &CinemaCtx, title: i64, ep: i64) -> ApiResult<Json<Ratings>> {
    ctx.svc()
        .ratings(ctx.viewer, title, ep)
        .await
        .map(Json)
        .map_err(map_err)
}

async fn rate(ctx: &CinemaCtx, title: i64, ep: i64, b: RatingInput) -> ApiResult<StatusCode> {
    ctx.svc()
        .rate(ctx.viewer, title, ep, b.score)
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn unrate(ctx: &CinemaCtx, title: i64, ep: i64) -> ApiResult<StatusCode> {
    if ctx
        .svc()
        .unrate(ctx.viewer, title, ep)
        .await
        .map_err(map_err)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(map_err(SocialError::NotFound("rating")))
    }
}

/// The caller's rating of a title and the average per scope (global, friends, people who share a server).
#[utoipa::path(
    get, path = "/cinema/titles/{title_id}/ratings", operation_id = "cinema_title_ratings",
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 200, body = Ratings), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn title_ratings(
    ctx: CinemaCtx,
    Path(title_id): Path<String>,
) -> ApiResult<Json<Ratings>> {
    ratings(&ctx, id(&title_id, "title")?, 0).await
}

/// Rate a title 1 to 10.
#[utoipa::path(
    put, path = "/cinema/titles/{title_id}/rating", operation_id = "cinema_rate_title", request_body = RatingInput,
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn rate_title(
    ctx: CinemaCtx,
    Path(title_id): Path<String>,
    Json(b): Json<RatingInput>,
) -> ApiResult<StatusCode> {
    rate(&ctx, id(&title_id, "title")?, 0, b).await
}

/// Remove the caller's rating of a title.
#[utoipa::path(
    delete, path = "/cinema/titles/{title_id}/rating", operation_id = "cinema_unrate_title",
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn unrate_title(ctx: CinemaCtx, Path(title_id): Path<String>) -> ApiResult<StatusCode> {
    unrate(&ctx, id(&title_id, "title")?, 0).await
}

/// Ratings of an episode.
#[utoipa::path(
    get, path = "/cinema/titles/{title_id}/episodes/{episode_id}/ratings", operation_id = "cinema_episode_ratings",
    params(("title_id" = String, Path, description = "Title snowflake"), ("episode_id" = String, Path, description = "Episode snowflake")),
    responses((status = 200, body = Ratings), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn episode_ratings(
    ctx: CinemaCtx,
    Path((title_id, episode_id)): Path<(String, String)>,
) -> ApiResult<Json<Ratings>> {
    ratings(&ctx, id(&title_id, "title")?, episode(&episode_id)?).await
}

/// Rate an episode 1 to 10.
#[utoipa::path(
    put, path = "/cinema/titles/{title_id}/episodes/{episode_id}/rating", operation_id = "cinema_rate_episode", request_body = RatingInput,
    params(("title_id" = String, Path, description = "Title snowflake"), ("episode_id" = String, Path, description = "Episode snowflake")),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn rate_episode(
    ctx: CinemaCtx,
    Path((title_id, episode_id)): Path<(String, String)>,
    Json(b): Json<RatingInput>,
) -> ApiResult<StatusCode> {
    rate(&ctx, id(&title_id, "title")?, episode(&episode_id)?, b).await
}

/// Remove the caller's rating of an episode.
#[utoipa::path(
    delete, path = "/cinema/titles/{title_id}/episodes/{episode_id}/rating", operation_id = "cinema_unrate_episode",
    params(("title_id" = String, Path, description = "Title snowflake"), ("episode_id" = String, Path, description = "Episode snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn unrate_episode(
    ctx: CinemaCtx,
    Path((title_id, episode_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    unrate(&ctx, id(&title_id, "title")?, episode(&episode_id)?).await
}

// ---- reviews ---------------------------------------------------------------

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReviewParams {
    /// `helpful` (default) or `recent`.
    pub sort: Option<ReviewSort>,
    /// Cursor from the previous page's `next_before`.
    pub before: Option<String>,
    /// 1 to 100, default 24.
    pub limit: Option<u32>,
}

/// Reviews of a title.
#[utoipa::path(
    get, path = "/cinema/titles/{title_id}/reviews", operation_id = "cinema_reviews", params(ReviewParams, ("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 200, body = ReviewPage), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn reviews(
    ctx: CinemaCtx,
    Path(title_id): Path<String>,
    Query(p): Query<ReviewParams>,
) -> ApiResult<Json<ReviewPage>> {
    ctx.svc()
        .reviews(
            ctx.viewer,
            id(&title_id, "title")?,
            p.sort.unwrap_or(ReviewSort::Helpful),
            p.before.as_deref(),
            p.limit,
        )
        .await
        .map(Json)
        .map_err(map_err)
}

/// Write or replace the caller's review of a title (one per account). A `rating` also sets the rating.
#[utoipa::path(
    put, path = "/cinema/titles/{title_id}/review", operation_id = "cinema_put_review", request_body = ReviewInput,
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 200, body = Review), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn put_review(
    ctx: CinemaCtx,
    Path(title_id): Path<String>,
    Json(b): Json<ReviewInput>,
) -> ApiResult<Json<Review>> {
    ctx.svc()
        .put_review(ctx.viewer, id(&title_id, "title")?, b)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Delete the caller's review of a title.
#[utoipa::path(
    delete, path = "/cinema/titles/{title_id}/review", operation_id = "cinema_delete_review",
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_review(ctx: CinemaCtx, Path(title_id): Path<String>) -> ApiResult<StatusCode> {
    if ctx
        .svc()
        .delete_review(ctx.viewer, id(&title_id, "title")?)
        .await
        .map_err(map_err)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(map_err(SocialError::NotFound("review")))
    }
}

/// Mark a review helpful (idempotent; not on one's own).
#[utoipa::path(
    put, path = "/cinema/reviews/{review_id}/helpful", operation_id = "cinema_review_helpful",
    params(("review_id" = String, Path, description = "Review snowflake")),
    responses((status = 200, body = CountBody), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn helpful(ctx: CinemaCtx, Path(review_id): Path<String>) -> ApiResult<Json<CountBody>> {
    let count = ctx
        .svc()
        .set_helpful(ctx.viewer, id(&review_id, "review")?, true)
        .await
        .map_err(map_err)?;
    Ok(Json(CountBody { count }))
}

/// Take back a helpful mark.
#[utoipa::path(
    delete, path = "/cinema/reviews/{review_id}/helpful", operation_id = "cinema_review_unhelpful",
    params(("review_id" = String, Path, description = "Review snowflake")),
    responses((status = 200, body = CountBody), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn unhelpful(
    ctx: CinemaCtx,
    Path(review_id): Path<String>,
) -> ApiResult<Json<CountBody>> {
    let count = ctx
        .svc()
        .set_helpful(ctx.viewer, id(&review_id, "review")?, false)
        .await
        .map_err(map_err)?;
    Ok(Json(CountBody { count }))
}

async fn report(
    ctx: &CinemaCtx,
    kind: ContentKind,
    target: i64,
    b: ReportInput,
) -> ApiResult<StatusCode> {
    ctx.svc()
        .report(ctx.viewer, kind, target, b)
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Report a review to the moderators (a second report by the same account is accepted silently).
#[utoipa::path(
    post, path = "/cinema/reviews/{review_id}/report", operation_id = "cinema_report_review", request_body = ReportInput,
    params(("review_id" = String, Path, description = "Review snowflake")),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn report_review(
    ctx: CinemaCtx,
    Path(review_id): Path<String>,
    Json(b): Json<ReportInput>,
) -> ApiResult<StatusCode> {
    report(&ctx, ContentKind::Review, id(&review_id, "review")?, b).await
}

// ---- comments --------------------------------------------------------------

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageParams {
    /// Cursor from the previous page's `next_before`.
    pub before: Option<String>,
    /// 1 to 100, default 24.
    pub limit: Option<u32>,
}

async fn comments(
    ctx: &CinemaCtx,
    title: i64,
    ep: i64,
    p: PageParams,
) -> ApiResult<Json<CommentPage>> {
    ctx.svc()
        .comments(ctx.viewer, title, ep, p.before.as_deref(), p.limit)
        .await
        .map(Json)
        .map_err(map_err)
}

async fn post_comment(
    ctx: &CinemaCtx,
    title: i64,
    ep: i64,
    b: CommentInput,
) -> ApiResult<Json<Comment>> {
    ctx.svc()
        .post_comment(ctx.viewer, title, ep, b)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Top-level comments on a title, newest first.
#[utoipa::path(
    get, path = "/cinema/titles/{title_id}/comments", operation_id = "cinema_title_comments", params(PageParams, ("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 200, body = CommentPage), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn title_comments(
    ctx: CinemaCtx,
    Path(title_id): Path<String>,
    Query(p): Query<PageParams>,
) -> ApiResult<Json<CommentPage>> {
    comments(&ctx, id(&title_id, "title")?, 0, p).await
}

/// Comment on a title, or reply to a top-level comment (`parent_id`).
#[utoipa::path(
    post, path = "/cinema/titles/{title_id}/comments", operation_id = "cinema_post_title_comment", request_body = CommentInput,
    params(("title_id" = String, Path, description = "Title snowflake")),
    responses((status = 200, body = Comment), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn post_title_comment(
    ctx: CinemaCtx,
    Path(title_id): Path<String>,
    Json(b): Json<CommentInput>,
) -> ApiResult<Json<Comment>> {
    post_comment(&ctx, id(&title_id, "title")?, 0, b).await
}

/// Top-level comments on an episode, newest first.
#[utoipa::path(
    get, path = "/cinema/titles/{title_id}/episodes/{episode_id}/comments", operation_id = "cinema_episode_comments",
    params(PageParams, ("title_id" = String, Path, description = "Title snowflake"), ("episode_id" = String, Path, description = "Episode snowflake")),
    responses((status = 200, body = CommentPage), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn episode_comments(
    ctx: CinemaCtx,
    Path((title_id, episode_id)): Path<(String, String)>,
    Query(p): Query<PageParams>,
) -> ApiResult<Json<CommentPage>> {
    comments(&ctx, id(&title_id, "title")?, episode(&episode_id)?, p).await
}

/// Comment on an episode, or reply to a top-level comment.
#[utoipa::path(
    post, path = "/cinema/titles/{title_id}/episodes/{episode_id}/comments", operation_id = "cinema_post_episode_comment", request_body = CommentInput,
    params(("title_id" = String, Path, description = "Title snowflake"), ("episode_id" = String, Path, description = "Episode snowflake")),
    responses((status = 200, body = Comment), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn post_episode_comment(
    ctx: CinemaCtx,
    Path((title_id, episode_id)): Path<(String, String)>,
    Json(b): Json<CommentInput>,
) -> ApiResult<Json<Comment>> {
    post_comment(&ctx, id(&title_id, "title")?, episode(&episode_id)?, b).await
}

/// Replies to a top-level comment, oldest first.
#[utoipa::path(
    get, path = "/cinema/comments/{comment_id}/replies", operation_id = "cinema_comment_replies", params(PageParams, ("comment_id" = String, Path, description = "Comment snowflake")),
    responses((status = 200, body = CommentPage), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn replies(
    ctx: CinemaCtx,
    Path(comment_id): Path<String>,
    Query(p): Query<PageParams>,
) -> ApiResult<Json<CommentPage>> {
    ctx.svc()
        .replies(
            ctx.viewer,
            id(&comment_id, "comment")?,
            p.before.as_deref(),
            p.limit,
        )
        .await
        .map(Json)
        .map_err(map_err)
}

/// Edit the caller's own comment.
#[utoipa::path(
    patch, path = "/cinema/comments/{comment_id}", operation_id = "cinema_edit_comment", request_body = CommentPatch,
    params(("comment_id" = String, Path, description = "Comment snowflake")),
    responses((status = 200, body = Comment), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn edit_comment(
    ctx: CinemaCtx,
    Path(comment_id): Path<String>,
    Json(b): Json<CommentPatch>,
) -> ApiResult<Json<Comment>> {
    ctx.svc()
        .edit_comment(ctx.viewer, id(&comment_id, "comment")?, b)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Delete the caller's own comment (its replies go with it).
#[utoipa::path(
    delete, path = "/cinema/comments/{comment_id}", operation_id = "cinema_delete_comment",
    params(("comment_id" = String, Path, description = "Comment snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_comment(
    ctx: CinemaCtx,
    Path(comment_id): Path<String>,
) -> ApiResult<StatusCode> {
    if ctx
        .svc()
        .delete_comment(ctx.viewer, id(&comment_id, "comment")?)
        .await
        .map_err(map_err)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(map_err(SocialError::NotFound("comment")))
    }
}

/// Like a comment (idempotent). Answers the new count.
#[utoipa::path(
    put, path = "/cinema/comments/{comment_id}/like", operation_id = "cinema_like_comment",
    params(("comment_id" = String, Path, description = "Comment snowflake")),
    responses((status = 200, body = CountBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn like(ctx: CinemaCtx, Path(comment_id): Path<String>) -> ApiResult<Json<CountBody>> {
    let count = ctx
        .svc()
        .set_like(ctx.viewer, id(&comment_id, "comment")?, true)
        .await
        .map_err(map_err)?;
    Ok(Json(CountBody { count }))
}

/// Take back a like. Answers the new count.
#[utoipa::path(
    delete, path = "/cinema/comments/{comment_id}/like", operation_id = "cinema_unlike_comment",
    params(("comment_id" = String, Path, description = "Comment snowflake")),
    responses((status = 200, body = CountBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn unlike(ctx: CinemaCtx, Path(comment_id): Path<String>) -> ApiResult<Json<CountBody>> {
    let count = ctx
        .svc()
        .set_like(ctx.viewer, id(&comment_id, "comment")?, false)
        .await
        .map_err(map_err)?;
    Ok(Json(CountBody { count }))
}

/// Report a comment to the moderators.
#[utoipa::path(
    post, path = "/cinema/comments/{comment_id}/report", operation_id = "cinema_report_comment", request_body = ReportInput,
    params(("comment_id" = String, Path, description = "Comment snowflake")),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn report_comment(
    ctx: CinemaCtx,
    Path(comment_id): Path<String>,
    Json(b): Json<ReportInput>,
) -> ApiResult<StatusCode> {
    report(&ctx, ContentKind::Comment, id(&comment_id, "comment")?, b).await
}
