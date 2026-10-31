//! Invite graph, credits, invites and suspicion (`MANAGE_INVITES`).

use super::ctx::{ReasonBody, StaffCtx, id, map_err, reason};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{
    Json,
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use sr_admin::{
    CreditLedger, InactiveRoots, InviteControls, InviteDescendants, InvitePage, InviteQuery,
    InviteRow, InviteTree, SuspectPerson,
};
use utoipa::ToSchema;

/// Inviter chain (root first) and stats of an account. The read is logged.
#[utoipa::path(
    get, path = "/admin/accounts/{account_id}/invite-tree",
    params(("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, body = InviteTree), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn tree(ctx: StaffCtx, Path(account_id): Path<String>) -> ApiResult<Json<InviteTree>> {
    ctx.svc()
        .invite_tree(&ctx.caller, id(&account_id)?)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Default, Deserialize)]
pub struct DepthParams {
    /// 1 to 20, default 3.
    pub depth: Option<i32>,
}

/// Accounts invited, directly or not, up to `depth` hops (at most 500 nodes).
#[utoipa::path(
    get, path = "/admin/accounts/{account_id}/invite-descendants",
    params(("account_id" = String, Path, description = "Account snowflake"),
           ("depth" = Option<i32>, Query, description = "1-20, default 3")),
    responses((status = 200, body = InviteDescendants), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn descendants(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
    Query(p): Query<DepthParams>,
) -> ApiResult<Json<InviteDescendants>> {
    ctx.svc()
        .invite_descendants(&ctx.caller, id(&account_id)?, p.depth)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Default, Deserialize)]
pub struct LimitParams {
    pub limit: Option<i64>,
}

/// Credit balance and ledger (newest first).
#[utoipa::path(
    get, path = "/admin/accounts/{account_id}/credits",
    params(("account_id" = String, Path, description = "Account snowflake"),
           ("limit" = Option<i64>, Query, description = "1-100, default 50")),
    responses((status = 200, body = CreditLedger), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn ledger(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
    Query(p): Query<LimitParams>,
) -> ApiResult<Json<CreditLedger>> {
    ctx.svc()
        .credit_ledger(&ctx.caller, id(&account_id)?, p.limit)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AdjustBody {
    /// Positive grants credits, negative revokes them (-1000..1000, not 0).
    pub delta: i32,
    /// Required.
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdjustResult {
    pub balance: i64,
}

/// Grant or revoke invite credits with a reason.
#[utoipa::path(
    post, path = "/admin/accounts/{account_id}/credits", request_body = AdjustBody,
    params(("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, body = AdjustResult), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn adjust(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
    headers: HeaderMap,
    Json(b): Json<AdjustBody>,
) -> ApiResult<Json<AdjustResult>> {
    let r = reason(&headers, b.reason.as_deref());
    ctx.svc()
        .adjust_credits(&ctx.caller, id(&account_id)?, b.delta, r.as_deref())
        .await
        .map(|balance| Json(AdjustResult { balance }))
        .map_err(map_err)
}

#[derive(Debug, Default, Deserialize)]
pub struct InviteParams {
    pub issuer_id: Option<String>,
    /// `open`, `revoked` or `redeemed`.
    pub state: Option<String>,
    pub after: Option<String>,
    pub limit: Option<i64>,
}

/// List platform invites, newest first.
#[utoipa::path(
    get, path = "/admin/invites",
    params(
        ("issuer_id" = Option<String>, Query, description = "Only this issuer"),
        ("state" = Option<String>, Query, description = "open, revoked, redeemed"),
        ("after" = Option<String>, Query, description = "Cursor"),
        ("limit" = Option<i64>, Query, description = "1-100, default 50"),
    ),
    responses((status = 200, body = InvitePage), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(ctx: StaffCtx, Query(p): Query<InviteParams>) -> ApiResult<Json<InvitePage>> {
    let q = InviteQuery {
        issuer_id: p.issuer_id.as_deref().map(id).transpose()?,
        state: p.state,
        after: p.after.as_deref().map(id).transpose()?,
        limit: 0,
    };
    ctx.svc()
        .list_invites(&ctx.caller, q, p.limit)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Revoke an open invite.
#[utoipa::path(
    delete, path = "/admin/invites/{invite_id}",
    params(("invite_id" = String, Path, description = "Invite snowflake")),
    responses((status = 200, body = InviteRow), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn revoke(
    ctx: StaffCtx,
    Path(invite_id): Path<String>,
    headers: HeaderMap,
    body: Option<Json<ReasonBody>>,
) -> ApiResult<Json<InviteRow>> {
    let r = reason(&headers, body.and_then(|Json(b)| b.reason).as_deref());
    ctx.svc()
        .revoke_invite(&ctx.caller, id(&invite_id)?, r.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

/// The invite controls: Lock the Doors, the quotas and what the last prune pass did (OPS-INV-001).
#[utoipa::path(
    get, path = "/admin/invites/controls",
    responses((status = 200, body = InviteControls), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn controls(ctx: StaffCtx) -> ApiResult<Json<InviteControls>> {
    ctx.svc()
        .invite_controls(&ctx.caller)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LockBody {
    /// `true` closes the doors: no registration succeeds, even with a valid invite. `false` opens them.
    pub locked: bool,
    /// Required: why the doors are being closed or opened. It goes into the audit log.
    pub reason: Option<String>,
}

/// Lock the Doors or open them again. Existing members are unaffected. Each change is one audit log row
/// (`invites.lock` or `invites.unlock`).
#[utoipa::path(
    put, path = "/admin/invites/lock", request_body = LockBody,
    responses((status = 200, body = InviteControls), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn set_lock(
    ctx: StaffCtx,
    headers: HeaderMap,
    Json(b): Json<LockBody>,
) -> ApiResult<Json<InviteControls>> {
    let r = reason(&headers, b.reason.as_deref());
    ctx.svc()
        .set_invites_locked(&ctx.caller, b.locked, r.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

/// Invite subtrees whose first member never activated, marked by the `invites.prune` job. Display only: the
/// invite graph itself is never changed.
#[utoipa::path(
    get, path = "/admin/invites/inactive",
    params(("limit" = Option<i64>, Query, description = "1-200, default 50")),
    responses((status = 200, body = InactiveRoots), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn inactive(
    ctx: StaffCtx,
    Query(p): Query<LimitParams>,
) -> ApiResult<Json<InactiveRoots>> {
    ctx.svc()
        .inactive_invite_roots(&ctx.caller, p.limit)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Persons with suspicion above zero, highest first.
#[utoipa::path(
    get, path = "/admin/suspicion",
    responses((status = 200, body = Vec<SuspectPerson>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn suspects(ctx: StaffCtx) -> ApiResult<Json<Vec<SuspectPerson>>> {
    ctx.svc()
        .list_suspects(&ctx.caller)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Clear a Person's suspicion level (reason required).
#[utoipa::path(
    post, path = "/admin/persons/{person_id}/clear-suspicion",
    params(("person_id" = String, Path, description = "Person snowflake")),
    request_body(content = ReasonBody, description = "Reason"),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn clear_suspicion(
    ctx: StaffCtx,
    Path(person_id): Path<String>,
    headers: HeaderMap,
    body: Option<Json<ReasonBody>>,
) -> ApiResult<StatusCode> {
    let r = reason(&headers, body.and_then(|Json(b)| b.reason).as_deref());
    ctx.svc()
        .clear_suspicion(&ctx.caller, id(&person_id)?, r.as_deref())
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}
