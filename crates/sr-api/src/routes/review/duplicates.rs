//! Possible duplicates of a submission (REV-ED-006) and the reviewers' resolution of them
//! (REV-RV-003).

use super::{MemberCtx, ReviewerCtx, id, map_err};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{Json, extract::Path};
use serde_json::json;
use sr_review::model::*;

/// Existing catalog entries my submission may duplicate (title, year, cover artwork).
#[utoipa::path(
    get, path = "/submissions/{submission_id}/duplicates", operation_id = "submissions_duplicates",
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = DuplicateReport), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn mine(
    ctx: MemberCtx,
    Path(submission_id): Path<String>,
) -> ApiResult<Json<DuplicateReport>> {
    ctx.svc()
        .duplicates_mine(ctx.user(), id(&submission_id, "submission")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Possible duplicates with the reviewers' resolution (reviewers of the kind).
#[utoipa::path(
    get, path = "/admin/review/submissions/{submission_id}/duplicates", operation_id = "admin_review_duplicates",
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = DuplicateReport), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn review(
    ctx: ReviewerCtx,
    Path(submission_id): Path<String>,
) -> ApiResult<Json<DuplicateReport>> {
    ctx.svc()
        .duplicates_for_review(ctx.caller(), id(&submission_id, "submission")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Resolve the duplicate panel: `merge` into an existing entry, mark as a `replacement`, or
/// `not_duplicate`. Needs the review claim. A merge or replacement earns no contribution credit.
#[utoipa::path(
    put, path = "/admin/review/submissions/{submission_id}/duplicate", operation_id = "admin_review_resolve_duplicate",
    request_body = DuplicateResolutionInput,
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = DuplicateReport), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn resolve(
    ctx: ReviewerCtx,
    Path(submission_id): Path<String>,
    Json(b): Json<DuplicateResolutionInput>,
) -> ApiResult<Json<DuplicateReport>> {
    let sid = id(&submission_id, "submission")?;
    let action = b.action;
    let target = b.target;
    let r = ctx
        .svc()
        .resolve_duplicate(ctx.caller(), sid, b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "review.duplicate",
        sid,
        json!({ "action": action.as_str(), "target": target.map(|t| t.to_string()) }),
    )
    .await;
    Ok(Json(r))
}
