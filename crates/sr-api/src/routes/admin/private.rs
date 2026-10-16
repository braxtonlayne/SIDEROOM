//! Second-factor window and the private content viewer (ADM-STF-009, ADM-COM-004).
//!
//! Every read here needs `READ_PRIVATE_CONTENT`, a reason and a second factor checked in the
//! last 15 minutes, and writes a hash-chained `private_read` row before any content leaves.

use super::ctx::{StaffCtx, id, map_err, reason};
use crate::error::HttpError;
use crate::{
    error::ApiResult,
    openapi::ErrorBody,
    routes::auth::{STAFF_MFA_WINDOW_SECS, StaffMfa},
};
use axum::{
    Json,
    extract::{Path, Query},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use sr_admin::{PrivateChannel, PrivateMessagePage};
use utoipa::ToSchema;

/// Where the session stands on the second-factor window.
#[derive(Debug, Serialize, ToSchema)]
pub struct ReauthStatus {
    /// A second factor was checked on this session in the last 15 minutes.
    pub fresh: bool,
    /// The account has a second factor (TOTP or passkey). Without one the sensitive
    /// admin actions stay closed.
    pub enrolled: bool,
    /// Length of the window in seconds.
    pub window_secs: i64,
    /// Whether the installation demands the window at all.
    pub required: bool,
}

/// `GET /admin/reauth`: is the second-factor window open? (any staff member)
#[utoipa::path(
    get, path = "/admin/reauth",
    responses((status = 200, body = ReauthStatus), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn reauth_status(ctx: StaffCtx) -> ApiResult<Json<ReauthStatus>> {
    let state = ctx.mfa_state().await?;
    Ok(Json(ReauthStatus {
        fresh: state == StaffMfa::Fresh,
        enrolled: state != StaffMfa::NotEnrolled,
        window_secs: STAFF_MFA_WINDOW_SECS,
        required: ctx.deps.require_reauth,
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = AdminReauthBody)]
pub struct ReauthBody {
    /// `totp`, `recovery` or `passkey` (then `code` is the JSON of the passkey assertion).
    pub method: String,
    pub code: String,
}

/// `POST /admin/reauth`: prove a second factor to open the 15-minute window for this session.
#[utoipa::path(
    post, path = "/admin/reauth", request_body = ReauthBody,
    responses((status = 200, body = ReauthStatus), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 429, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn reauth(ctx: StaffCtx, Json(b): Json<ReauthBody>) -> ApiResult<Json<ReauthStatus>> {
    ctx.prove_second_factor(&b.method, &b.code).await?;
    reauth_status(ctx).await
}

/// The second-factor gate of the private routes: a refusal is logged as `denied`.
async fn gate(ctx: &StaffCtx, action: &str, target: (&str, i64)) -> Result<(), HttpError> {
    if let Err(e) = ctx.require_reauth().await {
        let _ = ctx
            .svc()
            .record_denied(&ctx.caller, action, Some(target), "second factor required")
            .await;
        return Err(e);
    }
    Ok(())
}

#[derive(Debug, Default, Deserialize)]
pub struct ReadParams {
    /// Page: only messages older than this id.
    pub before: Option<String>,
    /// 1 to 100, default 50.
    pub limit: Option<i64>,
    /// Why (or the `X-Audit-Log-Reason` header). Required.
    pub reason: Option<String>,
}

/// Read a conversation, including deleted messages younger than the retention window and
/// edit history (`READ_PRIVATE_CONTENT`). Reason and a fresh second factor required; the
/// request is logged before the content is returned.
#[utoipa::path(
    get, path = "/admin/private/channels/{channel_id}/messages",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake (DM, group DM or server channel)"),
        ("before" = Option<String>, Query, description = "Only messages older than this id"),
        ("limit" = Option<i64>, Query, description = "1-100, default 50"),
        ("reason" = Option<String>, Query, description = "Why; or the X-Audit-Log-Reason header"),
    ),
    responses((status = 200, body = PrivateMessagePage), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn messages(
    ctx: StaffCtx,
    Path(channel_id): Path<String>,
    headers: HeaderMap,
    Query(p): Query<ReadParams>,
) -> ApiResult<Json<PrivateMessagePage>> {
    let r = reason(&headers, p.reason.as_deref());
    let cid = id(&channel_id)?;
    let before = p.before.as_deref().map(id).transpose()?;
    gate(&ctx, "private.read", ("channel", cid)).await?;
    ctx.svc()
        .read_private_messages(&ctx.caller, cid, before, p.limit, r.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Default, Deserialize)]
pub struct ReasonParams {
    pub reason: Option<String>,
}

/// DM and group DM channels an account takes part in (`READ_PRIVATE_CONTENT`; logged).
#[utoipa::path(
    get, path = "/admin/private/accounts/{account_id}/dms",
    params(
        ("account_id" = String, Path, description = "Account snowflake"),
        ("reason" = Option<String>, Query, description = "Why; or the X-Audit-Log-Reason header"),
    ),
    responses((status = 200, body = Vec<PrivateChannel>), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn account_dms(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
    headers: HeaderMap,
    Query(p): Query<ReasonParams>,
) -> ApiResult<Json<Vec<PrivateChannel>>> {
    let r = reason(&headers, p.reason.as_deref());
    let aid = id(&account_id)?;
    gate(&ctx, "private.list_dms", ("account", aid)).await?;
    ctx.svc()
        .private_dm_channels(&ctx.caller, aid, r.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

/// Channels of a server that `@everyone` cannot see (`READ_PRIVATE_CONTENT`; logged).
#[utoipa::path(
    get, path = "/admin/private/guilds/{guild_id}/channels",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("reason" = Option<String>, Query, description = "Why; or the X-Audit-Log-Reason header"),
    ),
    responses((status = 200, body = Vec<PrivateChannel>), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn guild_channels(
    ctx: StaffCtx,
    Path(guild_id): Path<String>,
    headers: HeaderMap,
    Query(p): Query<ReasonParams>,
) -> ApiResult<Json<Vec<PrivateChannel>>> {
    let r = reason(&headers, p.reason.as_deref());
    let gid = id(&guild_id)?;
    gate(&ctx, "private.list_channels", ("guild", gid)).await?;
    ctx.svc()
        .private_guild_channels(&ctx.caller, gid, r.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}
