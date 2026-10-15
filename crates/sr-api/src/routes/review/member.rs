//! Submitter endpoints: create, edit, submit, withdraw, follow the timeline.

use super::{ListParams, MemberCtx, id, map_err, now_ms};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use serde::Serialize;
use sr_review::model::*;
use utoipa::ToSchema;

/// The submission kinds that can be created.
#[utoipa::path(
    get, path = "/submissions/kinds", operation_id = "submissions_kinds",
    responses((status = 200, body = Vec<KindInfo>), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn kinds(ctx: MemberCtx) -> Json<Vec<KindInfo>> {
    Json(ctx.svc().kind_infos())
}

/// My submissions, newest activity first by default (`sort=updated&order=desc`), all states.
#[utoipa::path(
    get, path = "/submissions", operation_id = "submissions_list", params(ListParams),
    responses((status = 200, body = SubmissionPage), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(ctx: MemberCtx, Query(p): Query<ListParams>) -> ApiResult<Json<SubmissionPage>> {
    let mut q = p.into_query(ctx.user(), now_ms())?;
    q.claimed_by = None;
    q.unclaimed = false;
    ctx.svc()
        .list_mine(ctx.user(), q)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Start a submission (a draft). For `library.series` the payload is the series
/// (`SeriesInput` plus optional `language` and `edition_label`); for `library.items` it is
/// `{ "series_id": "...", "edition_id": "..." }` naming a published series. A member may have
/// 5 open submissions; staff are exempt.
#[utoipa::path(
    post, path = "/submissions", operation_id = "submissions_create", request_body = CreateSubmission,
    responses((status = 201, body = SubmissionDetail), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 409, body = ErrorBody, description = "Too many open submissions.")),
    security(("session" = []))
)]
pub async fn create(
    ctx: MemberCtx,
    Json(b): Json<CreateSubmission>,
) -> ApiResult<(StatusCode, Json<SubmissionDetail>)> {
    let d = ctx.svc().create(ctx.caller, b).await.map_err(map_err)?;
    Ok((StatusCode::CREATED, Json(d)))
}

/// One of my submissions with its items and timeline (reviewer-only notes are left out).
#[utoipa::path(
    get, path = "/submissions/{submission_id}", operation_id = "submissions_get",
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = SubmissionDetail), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get(
    ctx: MemberCtx,
    Path(submission_id): Path<String>,
) -> ApiResult<Json<SubmissionDetail>> {
    ctx.svc()
        .get_mine(ctx.user(), id(&submission_id, "submission")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Change the title or summary while the submission is a draft or has changes requested.
#[utoipa::path(
    patch, path = "/submissions/{submission_id}", operation_id = "submissions_update", request_body = UpdateSubmission,
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = SubmissionDetail), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn update(
    ctx: MemberCtx,
    Path(submission_id): Path<String>,
    Json(b): Json<UpdateSubmission>,
) -> ApiResult<Json<SubmissionDetail>> {
    ctx.svc()
        .update(ctx.user(), id(&submission_id, "submission")?, b)
        .await
        .map(Json)
        .map_err(map_err)
}

/// What the kind shows reviewers: for the Library kinds the series facts, editions and chapters.
#[derive(Debug, Serialize, ToSchema)]
pub struct PreviewBody {
    #[schema(value_type = Object)]
    pub preview: serde_json::Value,
}

/// The preview of my submission, as a reviewer will see it.
#[utoipa::path(
    get, path = "/submissions/{submission_id}/preview", operation_id = "submissions_preview",
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = PreviewBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn preview(
    ctx: MemberCtx,
    Path(submission_id): Path<String>,
) -> ApiResult<Json<PreviewBody>> {
    let preview = ctx
        .svc()
        .preview_mine(ctx.user(), id(&submission_id, "submission")?)
        .await
        .map_err(map_err)?;
    Ok(Json(PreviewBody { preview }))
}

/// Submit a draft for review, or resubmit one with changes requested. The kind checks the
/// content first; problems come back as field errors `problems.N`.
#[utoipa::path(
    post, path = "/submissions/{submission_id}/submit", operation_id = "submissions_submit",
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = SubmissionDetail), (status = 400, body = ErrorBody, description = "The submission is not ready."),
              (status = 401, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn submit(
    ctx: MemberCtx,
    Path(submission_id): Path<String>,
) -> ApiResult<Json<SubmissionDetail>> {
    ctx.svc()
        .submit(ctx.user(), id(&submission_id, "submission")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Withdraw a submission that is not yet approved.
#[utoipa::path(
    post, path = "/submissions/{submission_id}/withdraw", operation_id = "submissions_withdraw",
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = SubmissionDetail), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn withdraw(
    ctx: MemberCtx,
    Path(submission_id): Path<String>,
) -> ApiResult<Json<SubmissionDetail>> {
    ctx.svc()
        .withdraw(ctx.user(), id(&submission_id, "submission")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Add a note the reviewers can read.
#[utoipa::path(
    post, path = "/submissions/{submission_id}/comments", operation_id = "submissions_comment", request_body = CommentInput,
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = SubmissionDetail), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn comment(
    ctx: MemberCtx,
    Path(submission_id): Path<String>,
    Json(b): Json<CommentInput>,
) -> ApiResult<Json<SubmissionDetail>> {
    let sid = id(&submission_id, "submission")?;
    // The submitter's own notes are always public.
    ctx.svc().own(ctx.user(), sid).await.map_err(map_err)?;
    ctx.svc()
        .comment(
            sr_review::Caller::member(ctx.user()),
            sid,
            CommentInput {
                internal: false,
                ..b
            },
        )
        .await
        .map(Json)
        .map_err(map_err)
}

/// What I have left of today's upload allowance (50 GB per UTC day for members; staff have no
/// limit and get `null` limits).
#[utoipa::path(
    get, path = "/submissions/quota", operation_id = "submissions_quota",
    responses((status = 200, body = UploadAllowance), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn quota(ctx: MemberCtx) -> ApiResult<Json<UploadAllowance>> {
    ctx.svc()
        .upload_allowance(ctx.caller)
        .await
        .map(Json)
        .map_err(map_err)
}

/// The contribution credits one of my submissions earned (one per new published item;
/// replacements earn none). Revoked credits stay listed with their reason.
#[utoipa::path(
    get, path = "/submissions/{submission_id}/credits", operation_id = "submissions_credits",
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = Vec<Credit>), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn credits(
    ctx: MemberCtx,
    Path(submission_id): Path<String>,
) -> ApiResult<Json<Vec<Credit>>> {
    let sid = id(&submission_id, "submission")?;
    // Only the submitter reads these here; reviewers use the admin route.
    ctx.svc().own(ctx.user(), sid).await.map_err(map_err)?;
    ctx.svc()
        .credits(ctx.caller, sid)
        .await
        .map(Json)
        .map_err(map_err)
}
