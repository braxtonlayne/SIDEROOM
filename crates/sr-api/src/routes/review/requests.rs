//! Content requests (non-bounty, REV-REQ): a board per section where members ask for titles,
//! upvote, and where a published submission fulfils a request.

use super::{MemberCtx, ReviewerCtx, id, map_err};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::json;
use sr_core::{ApiError, ErrorCode};
use sr_review::model::*;
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RequestParams {
    /// `library` (default) or `cinema`.
    pub section: Option<String>,
    /// `open`, `fulfilled` or `declined`.
    pub status: Option<String>,
    /// `votes` (default) or `new`.
    pub sort: Option<String>,
    /// 1 to 100, default 25.
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

fn invalid(m: &str) -> crate::error::HttpError {
    ApiError::with_message(ErrorCode::InvalidFormBody, m.to_owned()).into()
}

/// The request board of a section.
#[utoipa::path(
    get, path = "/requests", operation_id = "requests_list", params(RequestParams),
    responses((status = 200, body = RequestPage), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(ctx: MemberCtx, Query(p): Query<RequestParams>) -> ApiResult<Json<RequestPage>> {
    let status = match p.status.as_deref() {
        None => None,
        Some(s) => Some(RequestStatus::parse(s).ok_or_else(|| invalid("unknown status"))?),
    };
    let newest = match p.sort.as_deref() {
        None | Some("votes") => false,
        Some("new") => true,
        Some(_) => return Err(invalid("sort must be votes or new")),
    };
    ctx.svc()
        .list_requests(
            ctx.user(),
            RequestQuery {
                section: p.section.unwrap_or_else(|| "library".into()),
                status,
                newest,
                limit: p.limit.unwrap_or(25),
                offset: p.offset.unwrap_or(0),
            },
        )
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateRequestBody {
    /// `library` or `cinema`.
    pub section: String,
    pub title: String,
    pub creator: Option<String>,
    pub year: Option<i16>,
    pub link: Option<String>,
    #[serde(default)]
    pub note: String,
}

/// Ask for a title. You vote for your own request; 10 open requests at a time.
#[utoipa::path(
    post, path = "/requests", operation_id = "requests_create", request_body = CreateRequestBody,
    responses((status = 201, body = ContentRequest), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create(
    ctx: MemberCtx,
    Json(b): Json<CreateRequestBody>,
) -> ApiResult<(StatusCode, Json<ContentRequest>)> {
    let r = ctx
        .svc()
        .create_request(
            ctx.user(),
            &b.section,
            RequestInput {
                title: b.title,
                creator: b.creator,
                year: b.year,
                link: b.link,
                note: b.note,
            },
        )
        .await
        .map_err(map_err)?;
    Ok((StatusCode::CREATED, Json(r)))
}

/// Upvote an open request (idempotent).
#[utoipa::path(
    put, path = "/requests/{request_id}/vote", operation_id = "requests_vote",
    params(("request_id" = String, Path, description = "Request snowflake")),
    responses((status = 200, body = ContentRequest), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn vote(
    ctx: MemberCtx,
    Path(request_id): Path<String>,
) -> ApiResult<Json<ContentRequest>> {
    ctx.svc()
        .vote_request(ctx.user(), id(&request_id, "request")?, true)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Take back my upvote.
#[utoipa::path(
    delete, path = "/requests/{request_id}/vote", operation_id = "requests_unvote",
    params(("request_id" = String, Path, description = "Request snowflake")),
    responses((status = 200, body = ContentRequest), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn unvote(
    ctx: MemberCtx,
    Path(request_id): Path<String>,
) -> ApiResult<Json<ContentRequest>> {
    ctx.svc()
        .vote_request(ctx.user(), id(&request_id, "request")?, false)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct FulfilBody {
    /// A published submission of the request's section.
    pub submission_id: String,
}

/// Link a published submission to an open request and notify everybody who voted
/// (`SR_SUBMISSION_UPDATE` `request_fulfilled`). The submitter of that submission or a
/// reviewer of the section may do it.
#[utoipa::path(
    post, path = "/requests/{request_id}/fulfil", operation_id = "requests_fulfil", request_body = FulfilBody,
    params(("request_id" = String, Path, description = "Request snowflake")),
    responses((status = 200, body = ContentRequest), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn fulfil(
    ctx: MemberCtx,
    Path(request_id): Path<String>,
    Json(b): Json<FulfilBody>,
) -> ApiResult<Json<ContentRequest>> {
    ctx.svc()
        .fulfil_request(
            ctx.caller,
            id(&request_id, "request")?,
            id(&b.submission_id, "submission")?,
        )
        .await
        .map(Json)
        .map_err(map_err)
}

/// "I'll upload this": tell the others you are working on an open request, with a short note.
/// One member at a time; claiming again updates the note.
#[utoipa::path(
    put, path = "/requests/{request_id}/claim", operation_id = "requests_claim", request_body = ClaimRequestInput,
    params(("request_id" = String, Path, description = "Request snowflake")),
    responses((status = 200, body = ContentRequest), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn claim(
    ctx: MemberCtx,
    Path(request_id): Path<String>,
    body: Option<Json<ClaimRequestInput>>,
) -> ApiResult<Json<ContentRequest>> {
    let input = body.map(|Json(b)| b).unwrap_or(ClaimRequestInput {
        note: String::new(),
    });
    ctx.svc()
        .claim_request(ctx.user(), id(&request_id, "request")?, input)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Give up my claim (reviewers of the section may release anybody's).
#[utoipa::path(
    delete, path = "/requests/{request_id}/claim", operation_id = "requests_release",
    params(("request_id" = String, Path, description = "Request snowflake")),
    responses((status = 200, body = ContentRequest), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn release(
    ctx: MemberCtx,
    Path(request_id): Path<String>,
) -> ApiResult<Json<ContentRequest>> {
    ctx.svc()
        .release_request(ctx.caller, id(&request_id, "request")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Close a request without fulfilling it (reviewers of the section).
#[utoipa::path(
    post, path = "/admin/review/requests/{request_id}/decline", operation_id = "admin_review_decline_request",
    params(("request_id" = String, Path, description = "Request snowflake")),
    responses((status = 200, body = ContentRequest), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn decline(
    ctx: ReviewerCtx,
    Path(request_id): Path<String>,
) -> ApiResult<Json<ContentRequest>> {
    let rid = id(&request_id, "request")?;
    let r = ctx
        .svc()
        .decline_request(ctx.caller(), rid)
        .await
        .map_err(map_err)?;
    ctx.staff
        .deps
        .service
        .record_action(
            &ctx.staff.caller,
            "review.request.decline",
            Some(("request", rid)),
            json!({ "title": r.title }),
        )
        .await
        .ok();
    Ok(Json(r))
}
