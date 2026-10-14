//! More account actions (ADM-PPL-003): rename, change email, badges, forced password reset.

use super::ctx::{ReasonBody, StaffCtx, id, map_err, reason};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{Json, extract::Path, http::HeaderMap};
use serde::{Deserialize, Deserializer, Serialize};
use sr_admin::AccountSummary;
use utoipa::ToSchema;

/// `null` and "missing" are different here: null clears the display name.
fn double_option<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Ok(Some(Option::<String>::deserialize(d)?))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RenameBody {
    /// New username (2-32 of a-z, 0-9, `_`, `.`). Leave out to keep it.
    pub username: Option<String>,
    /// New display name; `null` or empty clears it. Leave out to keep it.
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub display_name: Option<Option<String>>,
    pub reason: Option<String>,
}

/// Change the username and/or display name (`MANAGE_ACCOUNTS`).
#[utoipa::path(
    post, path = "/admin/accounts/{account_id}/rename", request_body = RenameBody,
    params(("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, body = AccountSummary), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn rename(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
    headers: HeaderMap,
    Json(b): Json<RenameBody>,
) -> ApiResult<Json<AccountSummary>> {
    let r = reason(&headers, b.reason.as_deref());
    ctx.svc()
        .rename_account(
            &ctx.caller,
            id(&account_id)?,
            b.username.as_deref(),
            b.display_name.as_ref().map(Option::as_deref),
            r.as_deref(),
        )
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ChangeEmailBody {
    pub email: String,
    pub reason: Option<String>,
}

/// Replace an account's email; it becomes unverified and links sent to the old address
/// stop working (`MANAGE_ACCOUNTS`). Needs a fresh second factor.
#[utoipa::path(
    post, path = "/admin/accounts/{account_id}/change-email", request_body = ChangeEmailBody,
    params(("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, body = AccountSummary), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn change_email(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
    headers: HeaderMap,
    Json(b): Json<ChangeEmailBody>,
) -> ApiResult<Json<AccountSummary>> {
    let r = reason(&headers, b.reason.as_deref());
    let aid = id(&account_id)?;
    ctx.require_reauth().await?;
    ctx.svc()
        .change_email(&ctx.caller, aid, &b.email, r.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BadgesBody {
    /// Badge names to add: `early_member`, `contributor`, `bug_hunter`, `honorary`.
    #[serde(default)]
    pub grant: Vec<String>,
    /// Badge names to remove.
    #[serde(default)]
    pub revoke: Vec<String>,
    pub reason: Option<String>,
}

/// Grant and revoke public badges (`MANAGE_ACCOUNTS`).
#[utoipa::path(
    post, path = "/admin/accounts/{account_id}/badges", request_body = BadgesBody,
    params(("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, body = AccountSummary), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn badges(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
    headers: HeaderMap,
    Json(b): Json<BadgesBody>,
) -> ApiResult<Json<AccountSummary>> {
    let r = reason(&headers, b.reason.as_deref());
    ctx.svc()
        .set_badges(
            &ctx.caller,
            id(&account_id)?,
            &b.grant,
            &b.revoke,
            r.as_deref(),
        )
        .await
        .map(Json)
        .map_err(map_err)
}

/// Result of a forced password reset.
#[derive(Debug, Serialize, ToSchema)]
pub struct ForcePasswordResetResult {
    pub sessions_revoked: u64,
}

/// Make the password unusable and end every session; the account's owner must use
/// "Forgot password" to get back in (`MANAGE_ACCOUNTS`). Needs a fresh second factor.
#[utoipa::path(
    post, path = "/admin/accounts/{account_id}/force-password-reset",
    params(("account_id" = String, Path, description = "Account snowflake")),
    request_body(content = ReasonBody, description = "Reason (or X-Audit-Log-Reason header)"),
    responses((status = 200, body = ForcePasswordResetResult), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn force_password_reset(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
    headers: HeaderMap,
    body: Option<Json<ReasonBody>>,
) -> ApiResult<Json<ForcePasswordResetResult>> {
    let r = reason(&headers, body.and_then(|Json(b)| b.reason).as_deref());
    let aid = id(&account_id)?;
    ctx.require_reauth().await?;
    ctx.svc()
        .force_password_reset(&ctx.caller, aid, r.as_deref())
        .await
        .map(|n| {
            Json(ForcePasswordResetResult {
                sessions_revoked: n,
            })
        })
        .map_err(map_err)
}
