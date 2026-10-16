//! Accounts, Persons and platform bans.

use super::ctx::{ReasonBody, StaffCtx, id, map_err, reason};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{
    Json,
    extract::{Path, Query},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use sr_admin::{
    AccountDetail, AccountPage, AccountQuery, AccountState, AccountSummary, BanCategory,
    BanOutcome, BanPage, BanReason, DeleteRequest, PersonDetail, Reset2faReport, UnbanOutcome,
};
use utoipa::ToSchema;

#[derive(Debug, Default, Deserialize)]
pub struct SearchParams {
    /// Username, display name or email fragment, or an exact account id.
    pub q: Option<String>,
    /// `active`, `deactivated`, `deleted` or `platform_banned`.
    pub state: Option<String>,
    pub person_id: Option<String>,
    /// Accounts linked to this Discord user id.
    pub discord_id: Option<String>,
    /// Accounts that signed in from this IP address (the lookup is logged).
    pub ip: Option<String>,
    /// Cursor: the `next_after` of the previous page.
    pub after: Option<String>,
    /// 1 to 100, default 50.
    pub limit: Option<i64>,
}

/// Search accounts (`MANAGE_ACCOUNTS`), paginated by id.
#[utoipa::path(
    get, path = "/admin/accounts",
    params(
        ("q" = Option<String>, Query, description = "Username, display name, email fragment or exact id"),
        ("state" = Option<String>, Query, description = "active, deactivated, deleted, platform_banned"),
        ("person_id" = Option<String>, Query, description = "Only accounts of this Person"),
        ("discord_id" = Option<String>, Query, description = "Accounts linked to this Discord user id"),
        ("ip" = Option<String>, Query, description = "Accounts that signed in from this IP address"),
        ("after" = Option<String>, Query, description = "Cursor"),
        ("limit" = Option<i64>, Query, description = "1-100, default 50"),
    ),
    responses((status = 200, body = AccountPage), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn search(ctx: StaffCtx, Query(p): Query<SearchParams>) -> ApiResult<Json<AccountPage>> {
    let state = match p.state.as_deref() {
        None => None,
        Some(s) => Some(AccountState::parse(s).ok_or_else(|| {
            sr_core::ApiError::with_message(
                sr_core::ErrorCode::InvalidFormBody,
                "Unknown account state",
            )
        })?),
    };
    let q = AccountQuery {
        q: p.q,
        state,
        person_id: p.person_id.as_deref().map(id).transpose()?,
        discord_id: p.discord_id,
        ip: p.ip,
        after: p.after.as_deref().map(id).transpose()?,
        limit: 0,
    };
    ctx.svc()
        .search_accounts(&ctx.caller, q, p.limit)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Account drawer (`MANAGE_ACCOUNTS`). The read is logged.
#[utoipa::path(
    get, path = "/admin/accounts/{account_id}",
    params(("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, body = AccountDetail), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn detail(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
) -> ApiResult<Json<AccountDetail>> {
    ctx.svc()
        .account_detail(&ctx.caller, id(&account_id)?)
        .await
        .map(Json)
        .map_err(map_err)
}

macro_rules! reason_action {
    ($name:ident, $path:literal, $doc:literal, $body:ty, $call:expr) => {
        #[doc = $doc]
        #[utoipa::path(
            post, path = $path,
            params(("account_id" = String, Path, description = "Account snowflake")),
            request_body(content = ReasonBody, description = "Reason (or X-Audit-Log-Reason header)"),
            responses((status = 200, body = $body), (status = 400, body = ErrorBody),
                      (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
                      (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
            security(("session" = []))
        )]
        pub async fn $name(
            ctx: StaffCtx,
            Path(account_id): Path<String>,
            headers: HeaderMap,
            body: Option<Json<ReasonBody>>,
        ) -> ApiResult<Json<$body>> {
            let r = reason(&headers, body.and_then(|Json(b)| b.reason).as_deref());
            let aid = id(&account_id)?;
            let f = $call;
            f(&ctx, aid, r).await.map(Json).map_err(map_err)
        }
    };
}

reason_action!(
    suspend,
    "/admin/accounts/{account_id}/suspend",
    "Suspend (deactivate) an account; its sessions end (`MANAGE_ACCOUNTS`).",
    AccountSummary,
    |c: &StaffCtx, a, r: Option<String>| {
        let (svc, caller) = (c.deps.clone(), c.caller.clone());
        async move {
            svc.service
                .set_suspended(&caller, a, true, r.as_deref())
                .await
        }
    }
);

reason_action!(
    unsuspend,
    "/admin/accounts/{account_id}/unsuspend",
    "Reactivate a suspended account if the account cap allows (`MANAGE_ACCOUNTS`).",
    AccountSummary,
    |c: &StaffCtx, a, r: Option<String>| {
        let (svc, caller) = (c.deps.clone(), c.caller.clone());
        async move {
            svc.service
                .set_suspended(&caller, a, false, r.as_deref())
                .await
        }
    }
);

/// Result of a forced logout.
#[derive(Debug, Serialize, ToSchema)]
pub struct ForceLogoutResult {
    pub sessions_revoked: u64,
}

/// Revoke every session of an account (`MANAGE_ACCOUNTS`).
#[utoipa::path(
    post, path = "/admin/accounts/{account_id}/force-logout",
    params(("account_id" = String, Path, description = "Account snowflake")),
    request_body(content = ReasonBody, description = "Reason"),
    responses((status = 200, body = ForceLogoutResult), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn force_logout(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
    headers: HeaderMap,
    body: Option<Json<ReasonBody>>,
) -> ApiResult<Json<ForceLogoutResult>> {
    let r = reason(&headers, body.and_then(|Json(b)| b.reason).as_deref());
    ctx.svc()
        .force_logout(&ctx.caller, id(&account_id)?, r.as_deref())
        .await
        .map(|n| {
            Json(ForceLogoutResult {
                sessions_revoked: n,
            })
        })
        .map_err(map_err)
}

/// Remove TOTP, recovery codes and passkeys and end all sessions (`MANAGE_ACCOUNTS`).
/// Needs a second factor checked within the last 15 minutes.
#[utoipa::path(
    post, path = "/admin/accounts/{account_id}/reset-2fa",
    params(("account_id" = String, Path, description = "Account snowflake")),
    request_body(content = ReasonBody, description = "Reason (or X-Audit-Log-Reason header)"),
    responses((status = 200, body = Reset2faReport), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn reset_2fa(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
    headers: HeaderMap,
    body: Option<Json<ReasonBody>>,
) -> ApiResult<Json<Reset2faReport>> {
    let r = reason(&headers, body.and_then(|Json(b)| b.reason).as_deref());
    let aid = id(&account_id)?;
    ctx.require_reauth().await?;
    ctx.svc()
        .reset_2fa(&ctx.caller, aid, r.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

reason_action!(
    delete_request,
    "/admin/accounts/{account_id}/delete-request",
    "Record an account deletion request (stub: nothing is deleted) (`MANAGE_ACCOUNTS`).",
    DeleteRequest,
    |c: &StaffCtx, a, r: Option<String>| {
        let (svc, caller) = (c.deps.clone(), c.caller.clone());
        async move { svc.service.request_delete(&caller, a, r.as_deref()).await }
    }
);

/// Ban body: the reason is the ban note.
#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = AdminBanBody)]
pub struct BanBody {
    /// `serious` raises suspicion on the inviter (D-031); `standard` does not. Omit it to
    /// take the default of `reason_category` (D-211).
    pub category: Option<BanCategory>,
    /// Why the account is banned. `illegal_content`, `child_safety`, `fraud` and
    /// `platform_abuse` default to `serious`; `harassment` and `ban_evasion` to `standard`.
    pub reason_category: Option<BanReason>,
    pub reason: Option<String>,
}

/// The category to store: the explicit one, else the default of the reason category (D-211).
fn ban_category(
    category: Option<BanCategory>,
    reason_category: Option<BanReason>,
) -> Result<BanCategory, crate::HttpError> {
    category
        .or(reason_category.map(BanReason::default_category))
        .ok_or_else(|| {
            map_err(sr_admin::AdminError::Invalid(
                "send a category or a reason_category".into(),
            ))
        })
}

/// Platform-ban the Person behind an account (`PLATFORM_BAN`).
#[utoipa::path(
    post, path = "/admin/accounts/{account_id}/ban", request_body = BanBody,
    params(("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, body = BanOutcome), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn ban_account(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
    headers: HeaderMap,
    Json(b): Json<BanBody>,
) -> ApiResult<Json<BanOutcome>> {
    let r = reason(&headers, b.reason.as_deref());
    ctx.svc()
        .ban_account(
            &ctx.caller,
            id(&account_id)?,
            ban_category(b.category, b.reason_category)?,
            r.as_deref(),
        )
        .await
        .map(Json)
        .map_err(map_err)
}

/// A Person and its accounts (`MANAGE_ACCOUNTS`). The read is logged.
#[utoipa::path(
    get, path = "/admin/persons/{person_id}",
    params(("person_id" = String, Path, description = "Person snowflake")),
    responses((status = 200, body = PersonDetail), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn person(ctx: StaffCtx, Path(person_id): Path<String>) -> ApiResult<Json<PersonDetail>> {
    ctx.svc()
        .person_detail(&ctx.caller, id(&person_id)?)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PersonBanBody {
    /// Omit it to take the default of `reason_category` (D-211).
    pub category: Option<BanCategory>,
    pub reason_category: Option<BanReason>,
    pub reason: Option<String>,
    /// The account the ban is about; a serious ban raises suspicion on its inviter.
    pub source_account_id: Option<String>,
}

/// Platform-ban a Person (`PLATFORM_BAN`).
#[utoipa::path(
    post, path = "/admin/persons/{person_id}/ban", request_body = PersonBanBody,
    params(("person_id" = String, Path, description = "Person snowflake")),
    responses((status = 200, body = BanOutcome), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn ban_person(
    ctx: StaffCtx,
    Path(person_id): Path<String>,
    headers: HeaderMap,
    Json(b): Json<PersonBanBody>,
) -> ApiResult<Json<BanOutcome>> {
    let r = reason(&headers, b.reason.as_deref());
    ctx.svc()
        .ban_person(
            &ctx.caller,
            id(&person_id)?,
            b.source_account_id.as_deref().map(id).transpose()?,
            ban_category(b.category, b.reason_category)?,
            r.as_deref(),
        )
        .await
        .map(Json)
        .map_err(map_err)
}

/// Lift a Person's ban; accounts return while the cap allows (`PLATFORM_BAN`).
#[utoipa::path(
    post, path = "/admin/persons/{person_id}/unban",
    params(("person_id" = String, Path, description = "Person snowflake")),
    request_body(content = ReasonBody, description = "Reason"),
    responses((status = 200, body = UnbanOutcome), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn unban_person(
    ctx: StaffCtx,
    Path(person_id): Path<String>,
    headers: HeaderMap,
    body: Option<Json<ReasonBody>>,
) -> ApiResult<Json<UnbanOutcome>> {
    let r = reason(&headers, body.and_then(|Json(b)| b.reason).as_deref());
    ctx.svc()
        .unban_person(&ctx.caller, id(&person_id)?, r.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Default, Deserialize)]
pub struct BansParams {
    pub active_only: Option<bool>,
    pub after: Option<String>,
    pub limit: Option<i64>,
}

/// List platform bans, newest first (`PLATFORM_BAN`).
#[utoipa::path(
    get, path = "/admin/bans",
    params(
        ("active_only" = Option<bool>, Query, description = "Only bans not lifted"),
        ("after" = Option<String>, Query, description = "Cursor"),
        ("limit" = Option<i64>, Query, description = "1-100, default 50"),
    ),
    responses((status = 200, body = BanPage), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn bans(ctx: StaffCtx, Query(p): Query<BansParams>) -> ApiResult<Json<BanPage>> {
    ctx.svc()
        .list_bans(
            &ctx.caller,
            p.active_only.unwrap_or(false),
            p.after.as_deref().map(id).transpose()?,
            p.limit,
        )
        .await
        .map(Json)
        .map_err(map_err)
}
