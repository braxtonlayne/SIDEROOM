//! Appointment offers and delegations (ADM-STF-004): the Staff page's appoint modal
//! creates an offer, the appointee answers the system DM, the Owner delegates.

use super::ctx::{ReasonBody, StaffCtx, id, map_err, reason};
use crate::{auth::UserOnly, error::ApiResult, openapi::ErrorBody, routes::Ext};
use axum::{
    Json,
    extract::{Path, Query},
    http::HeaderMap,
};
use serde::Deserialize;
use sr_admin::{Delegation, StaffOffer};
use utoipa::ToSchema;

use super::ctx::AdminDeps;

#[derive(Debug, Deserialize, ToSchema)]
pub struct OfferBody {
    /// Account to invite to the staff.
    pub account_id: String,
    /// Staff Server role ids to start with.
    pub role_ids: Vec<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct OfferListParams {
    /// Only offers still waiting for an answer.
    pub pending: Option<bool>,
    pub limit: Option<i64>,
}

/// Offers, newest first (`APPOINT_STAFF`).
#[utoipa::path(
    get, path = "/admin/staff/offers",
    params(
        ("pending" = Option<bool>, Query, description = "Only pending offers"),
        ("limit" = Option<i64>, Query, description = "1-100, default 50"),
    ),
    responses((status = 200, body = Vec<StaffOffer>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list_offers(
    ctx: StaffCtx,
    Query(p): Query<OfferListParams>,
) -> ApiResult<Json<Vec<StaffOffer>>> {
    ctx.svc()
        .list_offers(&ctx.caller, p.pending.unwrap_or(false), p.limit)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Offer an appointment (`APPOINT_STAFF`). The appointee gets a system DM with Accept and
/// Decline and joins the staff only by accepting. Needs a fresh second factor.
#[utoipa::path(
    post, path = "/admin/staff/offers", request_body = OfferBody,
    responses((status = 200, body = StaffOffer), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_offer(
    ctx: StaffCtx,
    headers: HeaderMap,
    Json(b): Json<OfferBody>,
) -> ApiResult<Json<StaffOffer>> {
    let r = reason(&headers, b.reason.as_deref());
    let account = id(&b.account_id)?;
    let roles = b
        .role_ids
        .iter()
        .map(|r| id(r))
        .collect::<Result<Vec<_>, _>>()?;
    ctx.require_reauth().await?;
    ctx.svc()
        .offer_appointment(&ctx.caller, account, &roles, r.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

/// Withdraw a pending offer (`APPOINT_STAFF`).
#[utoipa::path(
    delete, path = "/admin/staff/offers/{offer_id}",
    params(("offer_id" = String, Path, description = "Offer snowflake")),
    responses((status = 200, body = StaffOffer), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn cancel_offer(
    ctx: StaffCtx,
    Path(offer_id): Path<String>,
    headers: HeaderMap,
    body: Option<Json<ReasonBody>>,
) -> ApiResult<Json<StaffOffer>> {
    let r = reason(&headers, body.and_then(|Json(b)| b.reason).as_deref());
    ctx.svc()
        .cancel_offer(&ctx.caller, id(&offer_id)?, r.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RespondBody {
    /// `true` to accept (and join the staff), `false` to decline.
    pub accept: bool,
}

/// Answer an appointment offer addressed to you. This is what the Accept and Decline
/// buttons of the system DM call. Any signed-in user may call it, for their own offer only.
#[utoipa::path(
    post, path = "/staff/offers/{offer_id}/respond", request_body = RespondBody,
    params(("offer_id" = String, Path, description = "Offer snowflake")),
    responses((status = 200, body = StaffOffer), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn respond(
    UserOnly(p): UserOnly,
    Ext(deps): Ext<AdminDeps>,
    Path(offer_id): Path<String>,
    Json(b): Json<RespondBody>,
) -> ApiResult<Json<StaffOffer>> {
    deps.service
        .respond_to_offer(p.user_id, id(&offer_id)?, b.accept, None)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Open delegations of appointment rights (`APPOINT_STAFF`).
#[utoipa::path(
    get, path = "/admin/staff/delegations",
    responses((status = 200, body = Vec<Delegation>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list_delegations(ctx: StaffCtx) -> ApiResult<Json<Vec<Delegation>>> {
    ctx.svc()
        .list_delegations(&ctx.caller)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DelegateBody {
    /// The most the delegate may hand out: `PlatformPermissions` bits as a decimal string.
    /// Owner-only permissions are refused.
    pub max_permissions: String,
    pub reason: Option<String>,
}

/// Let a staff member appoint others within a ceiling (Owner only, D-132).
#[utoipa::path(
    put, path = "/admin/staff/{account_id}/delegation", request_body = DelegateBody,
    params(("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, body = Delegation), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delegate(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
    headers: HeaderMap,
    Json(b): Json<DelegateBody>,
) -> ApiResult<Json<Delegation>> {
    let r = reason(&headers, b.reason.as_deref());
    let bits: u64 = b.max_permissions.parse().map_err(|_| {
        sr_core::ApiError::invalid_form().field(
            "max_permissions",
            "NUMBER_TYPE_COERCE",
            "Not a permission bitfield.",
        )
    })?;
    let aid = id(&account_id)?;
    ctx.require_reauth().await?;
    ctx.svc()
        .delegate(&ctx.caller, aid, bits, r.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

/// End a delegation (Owner only).
#[utoipa::path(
    delete, path = "/admin/staff/{account_id}/delegation",
    params(("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 204), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn undelegate(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
    headers: HeaderMap,
    body: Option<Json<ReasonBody>>,
) -> ApiResult<axum::http::StatusCode> {
    let r = reason(&headers, body.and_then(|Json(b)| b.reason).as_deref());
    ctx.svc()
        .undelegate(&ctx.caller, id(&account_id)?, r.as_deref())
        .await
        .map_err(map_err)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
