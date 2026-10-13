//! Staff management: who is staff, appointments, roles.

use super::ctx::{ReasonBody, StaffCtx, id, map_err, reason};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{Json, extract::Path, http::HeaderMap};
use serde::{Deserialize, Serialize};
use sr_admin::{StaffMember, StaffRole};
use utoipa::ToSchema;

/// The caller's own platform standing.
#[derive(Debug, Serialize, ToSchema)]
pub struct Me {
    pub user_id: String,
    /// `PlatformPermissions` bits as a decimal string (`OWNER` is bit 63).
    pub platform_permissions: String,
    pub is_owner: bool,
}

/// `GET /admin/me`: the caller's platform permissions (any staff member).
#[utoipa::path(
    get, path = "/admin/me",
    responses((status = 200, body = Me), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn me(ctx: StaffCtx) -> Json<Me> {
    Json(Me {
        user_id: ctx.caller.user_id.to_string(),
        platform_permissions: ctx.caller.permissions.bits().to_string(),
        is_owner: ctx.caller.is_owner,
    })
}

/// List staff (`APPOINT_STAFF`).
#[utoipa::path(
    get, path = "/admin/staff",
    responses((status = 200, body = Vec<StaffMember>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(ctx: StaffCtx) -> ApiResult<Json<Vec<StaffMember>>> {
    ctx.svc()
        .list_staff(&ctx.caller)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Staff Server roles that can be assigned (`APPOINT_STAFF`).
#[utoipa::path(
    get, path = "/admin/staff/roles",
    responses((status = 200, body = Vec<StaffRole>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn roles(ctx: StaffCtx) -> ApiResult<Json<Vec<StaffRole>>> {
    ctx.svc()
        .staff_roles(&ctx.caller)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AppointBody {
    /// Account to appoint.
    pub account_id: String,
    /// Staff Server role ids to start with.
    pub role_ids: Vec<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RolesBody {
    pub role_ids: Vec<String>,
    pub reason: Option<String>,
}

fn ids(raw: &[String]) -> Result<Vec<i64>, crate::HttpError> {
    raw.iter().map(|r| id(r)).collect()
}

/// Appoint an account: adds it to the Staff Server with the roles (`APPOINT_STAFF`).
#[utoipa::path(
    post, path = "/admin/staff", request_body = AppointBody,
    responses((status = 200, body = StaffMember), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn appoint(
    ctx: StaffCtx,
    headers: HeaderMap,
    Json(b): Json<AppointBody>,
) -> ApiResult<Json<StaffMember>> {
    let r = reason(&headers, b.reason.as_deref());
    ctx.svc()
        .appoint(
            &ctx.caller,
            id(&b.account_id)?,
            &ids(&b.role_ids)?,
            r.as_deref(),
        )
        .await
        .map(Json)
        .map_err(map_err)
}

/// Replace a staff member's roles (`APPOINT_STAFF`).
#[utoipa::path(
    put, path = "/admin/staff/{account_id}/roles", request_body = RolesBody,
    params(("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, body = StaffMember), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn set_roles(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
    headers: HeaderMap,
    Json(b): Json<RolesBody>,
) -> ApiResult<Json<StaffMember>> {
    let r = reason(&headers, b.reason.as_deref());
    ctx.svc()
        .set_staff_roles(
            &ctx.caller,
            id(&account_id)?,
            &ids(&b.role_ids)?,
            r.as_deref(),
        )
        .await
        .map(Json)
        .map_err(map_err)
}

/// Revoke an appointment (`APPOINT_STAFF`). The Owner cannot be revoked.
#[utoipa::path(
    delete, path = "/admin/staff/{account_id}",
    params(("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 204), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn revoke(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
    headers: HeaderMap,
    body: Option<Json<ReasonBody>>,
) -> ApiResult<axum::http::StatusCode> {
    let r = reason(&headers, body.and_then(|Json(b)| b.reason).as_deref());
    ctx.svc()
        .revoke_staff(&ctx.caller, id(&account_id)?, r.as_deref())
        .await
        .map_err(map_err)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
