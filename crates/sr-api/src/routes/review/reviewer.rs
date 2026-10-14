//! Reviewer endpoints (`/admin/review/*`): queue, claim, preview, notes, decisions.

use super::{ListParams, ReviewerCtx, id, map_err, now_ms};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{
    Json,
    extract::{Path, Query},
};
use serde::Deserialize;
use serde_json::json;
use sr_review::model::*;
use utoipa::ToSchema;

use super::member::PreviewBody;

/// The review queue: submitted and in review by default, for the kinds the caller may review
/// (`REVIEW_READING` for the Library kinds), never the caller's own submissions. Filters: kind,
/// state, `claimed=mine|unclaimed`, submitter, `min_age_hours`, title; sort and pagination.
#[utoipa::path(
    get, path = "/admin/review/queue", operation_id = "admin_review_queue", params(ListParams),
    responses((status = 200, body = SubmissionPage), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn queue(
    ctx: ReviewerCtx,
    Query(p): Query<ListParams>,
) -> ApiResult<Json<SubmissionPage>> {
    let q = p.into_query(ctx.caller().user_id, now_ms())?;
    ctx.svc()
        .queue(ctx.caller(), q)
        .await
        .map(Json)
        .map_err(map_err)
}

/// A submission with items and the full timeline, internal notes included.
#[utoipa::path(
    get, path = "/admin/review/submissions/{submission_id}", operation_id = "admin_review_get",
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = SubmissionDetail), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get(
    ctx: ReviewerCtx,
    Path(submission_id): Path<String>,
) -> ApiResult<Json<SubmissionDetail>> {
    ctx.svc()
        .get_for_review(ctx.caller(), id(&submission_id, "submission")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// The kind's preview (Library: series facts, editions and the chapters under review).
#[utoipa::path(
    get, path = "/admin/review/submissions/{submission_id}/preview", operation_id = "admin_review_preview",
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = PreviewBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn preview(
    ctx: ReviewerCtx,
    Path(submission_id): Path<String>,
) -> ApiResult<Json<PreviewBody>> {
    let preview = ctx
        .svc()
        .preview_for_review(ctx.caller(), id(&submission_id, "submission")?)
        .await
        .map_err(map_err)?;
    Ok(Json(PreviewBody { preview }))
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ClaimBody {
    /// Take the claim from another reviewer whose lease is still running (needs the kind's
    /// publish permission, e.g. `PUBLISH_READING`).
    #[serde(default)]
    pub takeover: bool,
}

/// Claim a submission for 30 minutes (renews your own claim).
#[utoipa::path(
    post, path = "/admin/review/submissions/{submission_id}/claim", operation_id = "admin_review_claim",
    request_body(content = ClaimBody, description = "Optional"),
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = SubmissionDetail), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody, description = "Not in the queue, or another reviewer holds the claim.")),
    security(("session" = []))
)]
pub async fn claim(
    ctx: ReviewerCtx,
    Path(submission_id): Path<String>,
    body: Option<Json<ClaimBody>>,
) -> ApiResult<Json<SubmissionDetail>> {
    let sid = id(&submission_id, "submission")?;
    let takeover = body.is_some_and(|b| b.0.takeover);
    let d = ctx
        .svc()
        .claim(ctx.caller(), sid, takeover)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "review.claim",
        sid,
        json!({ "kind": d.submission.kind, "takeover": takeover }),
    )
    .await;
    Ok(Json(d))
}

/// Give the claim back; the submission returns to the queue.
#[utoipa::path(
    post, path = "/admin/review/submissions/{submission_id}/release", operation_id = "admin_review_release",
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = SubmissionDetail), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn release(
    ctx: ReviewerCtx,
    Path(submission_id): Path<String>,
) -> ApiResult<Json<SubmissionDetail>> {
    let sid = id(&submission_id, "submission")?;
    let d = ctx
        .svc()
        .release(ctx.caller(), sid)
        .await
        .map_err(map_err)?;
    ctx.audit("review.release", sid, json!({ "kind": d.submission.kind }))
        .await;
    Ok(Json(d))
}

/// A note on the timeline: public (the submitter sees it) or `internal`.
#[utoipa::path(
    post, path = "/admin/review/submissions/{submission_id}/comments", operation_id = "admin_review_comment", request_body = CommentInput,
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = SubmissionDetail), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn comment(
    ctx: ReviewerCtx,
    Path(submission_id): Path<String>,
    Json(b): Json<CommentInput>,
) -> ApiResult<Json<SubmissionDetail>> {
    ctx.svc()
        .comment(ctx.caller(), id(&submission_id, "submission")?, b)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Approve, reject (note required) or reset one item. Needs your live claim.
#[utoipa::path(
    put, path = "/admin/review/submissions/{submission_id}/items/{item_id}", operation_id = "admin_review_decide_item",
    request_body = ItemDecisionInput,
    params(("submission_id" = String, Path, description = "Submission snowflake"),
           ("item_id" = String, Path, description = "Item snowflake")),
    responses((status = 200, body = SubmissionDetail), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn decide_item(
    ctx: ReviewerCtx,
    Path((submission_id, item_id)): Path<(String, String)>,
    Json(b): Json<ItemDecisionInput>,
) -> ApiResult<Json<SubmissionDetail>> {
    let sid = id(&submission_id, "submission")?;
    let item = id(&item_id, "item")?;
    let state = b.state;
    let d = ctx
        .svc()
        .decide_item(ctx.caller(), sid, item, b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "review.item",
        sid,
        json!({ "item": item.to_string(), "state": state.as_str() }),
    )
    .await;
    Ok(Json(d))
}

/// Send the submission back to the submitter; `reason` is required and visible to them.
#[utoipa::path(
    post, path = "/admin/review/submissions/{submission_id}/request-changes", operation_id = "admin_review_request_changes", request_body = DecisionInput,
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = SubmissionDetail), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn request_changes(
    ctx: ReviewerCtx,
    Path(submission_id): Path<String>,
    Json(b): Json<DecisionInput>,
) -> ApiResult<Json<SubmissionDetail>> {
    let sid = id(&submission_id, "submission")?;
    let d = ctx
        .svc()
        .request_changes(ctx.caller(), sid, b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "review.request_changes",
        sid,
        json!({ "kind": d.submission.kind, "reason": d.submission.decision_reason }),
    )
    .await;
    Ok(Json(d))
}

/// Reject; `reason` is required and visible to the submitter.
#[utoipa::path(
    post, path = "/admin/review/submissions/{submission_id}/reject", operation_id = "admin_review_reject", request_body = DecisionInput,
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = SubmissionDetail), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn reject(
    ctx: ReviewerCtx,
    Path(submission_id): Path<String>,
    Json(b): Json<DecisionInput>,
) -> ApiResult<Json<SubmissionDetail>> {
    let sid = id(&submission_id, "submission")?;
    let d = ctx
        .svc()
        .reject(ctx.caller(), sid, b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "review.reject",
        sid,
        json!({ "kind": d.submission.kind, "reason": d.submission.decision_reason }),
    )
    .await;
    Ok(Json(d))
}

/// Approve: pending items are approved (rejected ones stay out) and the kind publishes. When
/// publishing fails the submission stays `approved` with a `publish_failed` note; call
/// `publish` to retry.
#[utoipa::path(
    post, path = "/admin/review/submissions/{submission_id}/approve", operation_id = "admin_review_approve",
    request_body(content = DecisionInput, description = "Optional note"),
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = SubmissionDetail), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn approve(
    ctx: ReviewerCtx,
    Path(submission_id): Path<String>,
    body: Option<Json<DecisionInput>>,
) -> ApiResult<Json<SubmissionDetail>> {
    let sid = id(&submission_id, "submission")?;
    let d = ctx
        .svc()
        .approve(ctx.caller(), sid, body.map(|b| b.0).unwrap_or_default())
        .await
        .map_err(map_err)?;
    ctx.audit(
        "review.approve",
        sid,
        json!({
            "kind": d.submission.kind,
            "state": d.submission.state.as_str(),
            "published": d.submission.state == SubmissionState::Published,
        }),
    )
    .await;
    Ok(Json(d))
}

/// Publish an approved submission (the retry after a failed publish).
#[utoipa::path(
    post, path = "/admin/review/submissions/{submission_id}/publish", operation_id = "admin_review_publish",
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = SubmissionDetail), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn publish(
    ctx: ReviewerCtx,
    Path(submission_id): Path<String>,
) -> ApiResult<Json<SubmissionDetail>> {
    let sid = id(&submission_id, "submission")?;
    let d = ctx
        .svc()
        .publish(ctx.caller(), sid)
        .await
        .map_err(map_err)?;
    ctx.audit("review.publish", sid, json!({ "kind": d.submission.kind }))
        .await;
    Ok(Json(d))
}

/// Publish your own draft without review (the kind's publish permission, e.g.
/// `PUBLISH_READING`).
#[utoipa::path(
    post, path = "/admin/review/submissions/{submission_id}/publish-now", operation_id = "admin_review_publish_now",
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = SubmissionDetail), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn publish_now(
    ctx: ReviewerCtx,
    Path(submission_id): Path<String>,
) -> ApiResult<Json<SubmissionDetail>> {
    let sid = id(&submission_id, "submission")?;
    let d = ctx
        .svc()
        .publish_now(ctx.caller(), sid)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "review.publish_now",
        sid,
        json!({ "kind": d.submission.kind, "direct": true }),
    )
    .await;
    Ok(Json(d))
}
