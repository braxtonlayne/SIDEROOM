//! Graduated moderation (MOD-GRD-001, D-395, wave 44-A). Platform staff only, except the three routes a member
//! uses for their own account (`/sr/moderation/restrictions`, its appeal) and the counts page everyone signed in
//! may read (`/moderation/transparency`).
//!
//! * per-member acts under `/admin/moderation/members/{account_id}/...`: warn, timeout, invite freeze, revoke
//!   unused codes, ban (a reason category is required);
//! * subtree cases under `/admin/moderation/subtree-cases/...`: the case view with every node, clear, freeze and
//!   ban per node, close, and requests for a sterilization or a branch action;
//! * the two-person step: `/admin/moderation/requests/{request_id}/approve|reject`, each behind a 2FA step-up;
//! * the Operator's `POST /admin/moderation/branch-action`: Owner only, 2FA step-up, an approved request.
//!
//! The rules and the SQL live in `sr-moderation`; this module checks the caller, asks for the step-up and maps
//! errors.

use super::{ModerationDeps, id, map_err, staff_result};
use crate::{
    auth::UserOnly,
    error::ApiResult,
    openapi::ErrorBody,
    routes::{
        Ext,
        admin::{StaffCtx, map_err as admin_err},
    },
};
use axum::{
    Json,
    extract::{Path, Query},
};
use serde::Deserialize;
use sr_admin::{BanCategory, BanOutcome};
use sr_core::PlatformPermissions as P;
use sr_moderation::{
    ModError,
    graduated_model::{
        BranchOutcome, ModRequest, MyRestriction, RestrictionView, RevokedInvites,
        SubtreeCaseDetail, SubtreeCaseSummary, Transparency,
    },
    graduated_rules as gr,
    model::{AppealDecision, Who},
};
use utoipa::{IntoParams, ToSchema};

/// A body that is only a reason (also read from the `X-Audit-Log-Reason` header by the staff routes).
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct MemberReasonBody {
    pub reason: Option<String>,
}

fn reason_of(headers: &axum::http::HeaderMap, body: &Option<String>) -> Option<String> {
    crate::routes::admin::reason_from(headers, body.as_deref())
}

// ---- per-member acts -----------------------------------------------------------------------------------------

/// Warn a member: a warning on their record for 180 days, with the strike weight of the moment (`MANAGE_CASES`,
/// reason required). The member gets a system DM.
#[utoipa::path(
    post, path = "/admin/moderation/members/{account_id}/warn", request_body = MemberReasonBody,
    params(("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, body = RestrictionView), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn warn(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(account_id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(b): Json<MemberReasonBody>,
) -> ApiResult<Json<RestrictionView>> {
    let acc = id(&account_id, "account")?;
    let r = deps
        .svc
        .warn_member(
            &Who::Staff(&ctx.caller),
            acc,
            reason_of(&headers, &b.reason).as_deref(),
        )
        .await;
    staff_result(&ctx, "moderation.member.warn", Some(("account", acc)), r)
        .await
        .map(Json)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MemberTimeoutBody {
    /// 1 to 40320 (four weeks).
    pub minutes: i64,
    pub reason: Option<String>,
}

/// Time a member out of posting and voice (`MANAGE_CASES`, reason required). They can still sign in and appeal.
#[utoipa::path(
    post, path = "/admin/moderation/members/{account_id}/timeout", request_body = MemberTimeoutBody,
    params(("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, body = RestrictionView), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn timeout(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(account_id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(b): Json<MemberTimeoutBody>,
) -> ApiResult<Json<RestrictionView>> {
    let acc = id(&account_id, "account")?;
    let r = deps
        .svc
        .timeout_member(
            &Who::Staff(&ctx.caller),
            acc,
            b.minutes,
            reason_of(&headers, &b.reason).as_deref(),
        )
        .await;
    staff_result(&ctx, "moderation.member.timeout", Some(("account", acc)), r)
        .await
        .map(Json)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MemberInviteFreezeBody {
    /// 1 to 365.
    pub days: i64,
    pub reason: Option<String>,
}

/// Freeze a member's invite creation for a chosen number of days (`MANAGE_INVITES`, reason required). Lift it
/// early with `POST /admin/moderation/restrictions/{restriction_id}/lift`.
#[utoipa::path(
    post, path = "/admin/moderation/members/{account_id}/invite-freeze", request_body = MemberInviteFreezeBody,
    params(("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, body = RestrictionView), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn invite_freeze(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(account_id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(b): Json<MemberInviteFreezeBody>,
) -> ApiResult<Json<RestrictionView>> {
    let acc = id(&account_id, "account")?;
    let r = deps
        .svc
        .freeze_member_invites(
            &Who::Staff(&ctx.caller),
            acc,
            b.days,
            reason_of(&headers, &b.reason).as_deref(),
        )
        .await;
    staff_result(
        &ctx,
        "moderation.member.invite_freeze",
        Some(("account", acc)),
        r,
    )
    .await
    .map(Json)
}

/// Revoke every unused invite code the member holds (`MANAGE_INVITES`, reason required). Redeemed invites and the
/// invite graph are not touched.
#[utoipa::path(
    post, path = "/admin/moderation/members/{account_id}/revoke-invites", request_body = MemberReasonBody,
    params(("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, body = RevokedInvites), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn revoke_invites(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(account_id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(b): Json<MemberReasonBody>,
) -> ApiResult<Json<RevokedInvites>> {
    let acc = id(&account_id, "account")?;
    let r = deps
        .svc
        .revoke_unused_invites(
            &Who::Staff(&ctx.caller),
            acc,
            reason_of(&headers, &b.reason).as_deref(),
        )
        .await;
    staff_result(
        &ctx,
        "moderation.member.revoke_invites",
        Some(("account", acc)),
        r,
    )
    .await
    .map(Json)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MemberBanBody {
    /// Required: `spam`, `harassment`, `illegal_content`, `child_safety`, `ban_evasion`, `fraud`,
    /// `platform_abuse`, `nsfw_policy` or `other`.
    pub reason_category: String,
    /// `serious` or `standard`; the category's default when absent (spam, fraud, ban evasion, child safety,
    /// illegal content and platform abuse are serious).
    pub severity: Option<String>,
    pub reason: Option<String>,
}

/// Ban a member's Person with a required reason category (`PLATFORM_BAN`). A serious ban opens a subtree case on
/// the invite tree. The ban can be appealed through the existing appeal routes.
#[utoipa::path(
    post, path = "/admin/moderation/members/{account_id}/ban", request_body = MemberBanBody,
    params(("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, body = BanOutcome), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn ban(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(account_id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(b): Json<MemberBanBody>,
) -> ApiResult<Json<BanOutcome>> {
    let acc = id(&account_id, "account")?;
    let category = gr::reason_category(&b.reason_category).map_err(map_err)?;
    let severity = match b.severity.as_deref() {
        Some("serious") => BanCategory::Serious,
        Some("standard") => BanCategory::Standard,
        Some(_) => {
            return Err(map_err(ModError::Invalid(
                "severity must be serious or standard".into(),
            )));
        }
        None if gr::is_serious_reason(category) => BanCategory::Serious,
        None => BanCategory::Standard,
    };
    let detail = ctx
        .svc()
        .repo()
        .account_detail(acc)
        .await
        .map_err(admin_err)?
        .ok_or_else(|| map_err(ModError::NotFound("account")))?;
    let reason = reason_of(&headers, &b.reason);
    let out = ctx
        .svc()
        .ban_person_with_reason(
            &ctx.caller,
            detail.account.person_id,
            Some(acc),
            severity,
            Some(category),
            reason.as_deref(),
        )
        .await
        .map_err(admin_err)?;
    deps.svc
        .stamp_ban_reason(out.ban.id, category)
        .await
        .map_err(map_err)?;
    Ok(Json(out))
}

/// Every warning and restriction on an account, newest first (`MANAGE_CASES`; the read is logged).
#[utoipa::path(
    get, path = "/admin/moderation/members/{account_id}/actions",
    params(("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, body = Vec<RestrictionView>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn member_actions(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(account_id): Path<String>,
) -> ApiResult<Json<Vec<RestrictionView>>> {
    let acc = id(&account_id, "account")?;
    let r = deps
        .svc
        .member_history(&Who::Staff(&ctx.caller), acc)
        .await;
    staff_result(&ctx, "moderation.member.history", Some(("account", acc)), r)
        .await
        .map(Json)
}

/// Lift a restriction early (`MANAGE_CASES`; reason required).
#[utoipa::path(
    post, path = "/admin/moderation/restrictions/{restriction_id}/lift", request_body = MemberReasonBody,
    params(("restriction_id" = String, Path, description = "Restriction snowflake")),
    responses((status = 200, body = RestrictionView), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn lift(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(restriction_id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(b): Json<MemberReasonBody>,
) -> ApiResult<Json<RestrictionView>> {
    let rid = id(&restriction_id, "restriction")?;
    let r = deps
        .svc
        .lift_restriction(
            &Who::Staff(&ctx.caller),
            rid,
            reason_of(&headers, &b.reason).as_deref(),
        )
        .await;
    staff_result(
        &ctx,
        "moderation.restriction.lift",
        Some(("restriction", rid)),
        r,
    )
    .await
    .map(Json)
}

/// Appeals of restrictions waiting for a decision, oldest first (`MANAGE_CASES`).
#[utoipa::path(
    get, path = "/admin/moderation/restriction-appeals",
    responses((status = 200, body = Vec<RestrictionView>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn restriction_appeals(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
) -> ApiResult<Json<Vec<RestrictionView>>> {
    let r = deps
        .svc
        .open_restriction_appeals(&Who::Staff(&ctx.caller))
        .await;
    staff_result(&ctx, "moderation.restriction.appeals", None, r)
        .await
        .map(Json)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RestrictionAppealDecisionBody {
    pub decision: AppealDecision,
    pub reason: Option<String>,
}

/// Decide the appeal of a restriction (`MANAGE_CASES`): a staff member other than the one who imposed it, or the
/// Owner when nobody else can. `overturned` ends the restriction.
#[utoipa::path(
    post, path = "/admin/moderation/restrictions/{restriction_id}/appeal-decision", request_body = RestrictionAppealDecisionBody,
    params(("restriction_id" = String, Path, description = "Restriction snowflake")),
    responses((status = 200, body = RestrictionView), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn decide_restriction_appeal(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(restriction_id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(b): Json<RestrictionAppealDecisionBody>,
) -> ApiResult<Json<RestrictionView>> {
    let rid = id(&restriction_id, "restriction")?;
    let others = another_reviewer(&ctx).await?;
    let r = deps
        .svc
        .decide_restriction_appeal(
            &Who::Staff(&ctx.caller),
            rid,
            b.decision,
            reason_of(&headers, &b.reason).as_deref(),
            others,
        )
        .await;
    staff_result(
        &ctx,
        "moderation.restriction.appeal_decide",
        Some(("restriction", rid)),
        r,
    )
    .await
    .map(Json)
}

/// Whether somebody other than the caller holds `MANAGE_CASES` (the reviewer rule of D-215).
async fn another_reviewer(ctx: &StaffCtx) -> ApiResult<bool> {
    let staff = ctx.svc().repo().list_staff().await.map_err(admin_err)?;
    Ok(staff.iter().any(|m| {
        m.account_id != ctx.caller.user_id
            && (m.is_owner
                || m.platform_permissions
                    .parse::<u64>()
                    .ok()
                    .map(P::from_bits_truncate)
                    .is_some_and(|p| p.has(P::MANAGE_CASES)))
    }))
}

// ---- the member's side ---------------------------------------------------------------------------------------

/// The restrictions in force on the caller's own account, in plain words: no case, no staff member, no tree.
#[utoipa::path(
    get, path = "/sr/moderation/restrictions",
    responses((status = 200, body = Vec<MyRestriction>), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn my_restrictions(
    UserOnly(p): UserOnly,
    Ext(deps): Ext<ModerationDeps>,
) -> ApiResult<Json<Vec<MyRestriction>>> {
    deps.svc
        .my_restrictions(p.user_id)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RestrictionAppealBody {
    pub message: String,
}

/// Appeal a restriction on the caller's own account (once per restriction). A frozen account can do this.
#[utoipa::path(
    post, path = "/sr/moderation/restrictions/{restriction_id}/appeal", request_body = RestrictionAppealBody,
    params(("restriction_id" = String, Path, description = "Restriction snowflake")),
    responses((status = 200, body = MyRestriction), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn appeal_restriction(
    UserOnly(p): UserOnly,
    Ext(deps): Ext<ModerationDeps>,
    Path(restriction_id): Path<String>,
    Json(b): Json<RestrictionAppealBody>,
) -> ApiResult<Json<MyRestriction>> {
    let rid = id(&restriction_id, "restriction")?;
    deps.svc
        .appeal_restriction(p.user_id, rid, &b.message)
        .await
        .map(Json)
        .map_err(map_err)
}

/// The monthly transparency counts: notices, accounts actioned and frozen, sterilizations and appeals, by offense
/// category. Counts only, never handles or tree shape. Any signed-in member may read it.
#[utoipa::path(
    get, path = "/moderation/transparency",
    responses((status = 200, body = Transparency), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn transparency(
    _p: UserOnly,
    Ext(deps): Ext<ModerationDeps>,
) -> ApiResult<Json<Transparency>> {
    deps.svc.transparency().await.map(Json).map_err(map_err)
}

// ---- subtree cases -------------------------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SubtreeCaseListParams {
    /// Only open cases.
    pub open: Option<bool>,
}

/// Subtree cases, open first (`MANAGE_CASES`).
#[utoipa::path(
    get, path = "/admin/moderation/subtree-cases", params(SubtreeCaseListParams),
    responses((status = 200, body = Vec<SubtreeCaseSummary>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list_cases(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Query(q): Query<SubtreeCaseListParams>,
) -> ApiResult<Json<Vec<SubtreeCaseSummary>>> {
    let r = deps
        .svc
        .list_subtree_cases(&Who::Staff(&ctx.caller), q.open.unwrap_or(false))
        .await;
    staff_result(&ctx, "moderation.subtree_case.list", None, r)
        .await
        .map(Json)
}

/// One case with every node (activity stats, lineage, status) and the requests on it (`MANAGE_CASES`; logged).
#[utoipa::path(
    get, path = "/admin/moderation/subtree-cases/{case_id}",
    params(("case_id" = String, Path, description = "Subtree case snowflake")),
    responses((status = 200, body = SubtreeCaseDetail), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_case(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(case_id): Path<String>,
) -> ApiResult<Json<SubtreeCaseDetail>> {
    let cid = id(&case_id, "case")?;
    let r = deps.svc.subtree_case(&Who::Staff(&ctx.caller), cid).await;
    staff_result(&ctx, "moderation.subtree_case.view", Some(("case", cid)), r)
        .await
        .map(Json)
}

/// Close a case (`MANAGE_CASES`, reason required). Restrictions already imposed stand until they end or are lifted.
#[utoipa::path(
    post, path = "/admin/moderation/subtree-cases/{case_id}/close", request_body = MemberReasonBody,
    params(("case_id" = String, Path, description = "Subtree case snowflake")),
    responses((status = 200, description = "Closed"), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn close_case(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(case_id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(b): Json<MemberReasonBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let cid = id(&case_id, "case")?;
    let r = deps
        .svc
        .close_subtree_case(
            &Who::Staff(&ctx.caller),
            cid,
            reason_of(&headers, &b.reason).as_deref(),
        )
        .await;
    staff_result(&ctx, "moderation.subtree_case.close", Some(("case", cid)), r).await?;
    Ok(Json(serde_json::json!({ "closed": true })))
}

/// Clear a node of a case: lift its freeze and mark it cleared (`MANAGE_CASES`, reason required). Clearing an
/// automatically frozen account stops its 30-day removal.
#[utoipa::path(
    post, path = "/admin/moderation/subtree-cases/{case_id}/nodes/{account_id}/clear", request_body = MemberReasonBody,
    params(("case_id" = String, Path, description = "Subtree case snowflake"), ("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, description = "Cleared"), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn clear_node(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path((case_id, account_id)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(b): Json<MemberReasonBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let (cid, acc) = (id(&case_id, "case")?, id(&account_id, "account")?);
    let r = deps
        .svc
        .clear_node(
            &Who::Staff(&ctx.caller),
            cid,
            acc,
            reason_of(&headers, &b.reason).as_deref(),
        )
        .await;
    staff_result(&ctx, "moderation.subtree_case.clear", Some(("account", acc)), r).await?;
    Ok(Json(serde_json::json!({ "cleared": true })))
}

/// Freeze a node on staff's own evidence: 12 months, no automatic removal, appealable (`MANAGE_CASES`).
#[utoipa::path(
    post, path = "/admin/moderation/subtree-cases/{case_id}/nodes/{account_id}/freeze", request_body = MemberReasonBody,
    params(("case_id" = String, Path, description = "Subtree case snowflake"), ("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, description = "Frozen"), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn freeze_node(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path((case_id, account_id)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(b): Json<MemberReasonBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let (cid, acc) = (id(&case_id, "case")?, id(&account_id, "account")?);
    let r = deps
        .svc
        .freeze_node(
            &Who::Staff(&ctx.caller),
            cid,
            acc,
            reason_of(&headers, &b.reason).as_deref(),
        )
        .await;
    staff_result(&ctx, "moderation.subtree_case.freeze", Some(("account", acc)), r).await?;
    Ok(Json(serde_json::json!({ "frozen": true })))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct NodeBanBody {
    /// A `ban_reason_category` value (required).
    pub reason_category: String,
    pub reason: Option<String>,
}

/// Ban a node of a case on its own evidence (`MANAGE_CASES` and `PLATFORM_BAN`). The ban is standard severity, so
/// it never opens another case; the person can appeal it like any ban.
#[utoipa::path(
    post, path = "/admin/moderation/subtree-cases/{case_id}/nodes/{account_id}/ban", request_body = NodeBanBody,
    params(("case_id" = String, Path, description = "Subtree case snowflake"), ("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, body = BanOutcome), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn ban_node(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path((case_id, account_id)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
    Json(b): Json<NodeBanBody>,
) -> ApiResult<Json<BanOutcome>> {
    let (cid, acc) = (id(&case_id, "case")?, id(&account_id, "account")?);
    let category = gr::reason_category(&b.reason_category).map_err(map_err)?;
    let who = Who::Staff(&ctx.caller);
    let person = staff_result(
        &ctx,
        "moderation.subtree_case.node_ban",
        Some(("account", acc)),
        deps.svc.node_person(&who, cid, acc).await,
    )
    .await?;
    let reason = reason_of(&headers, &b.reason);
    let out = ctx
        .svc()
        .ban_person_with_reason(
            &ctx.caller,
            person,
            Some(acc),
            BanCategory::Standard,
            Some(category),
            reason.as_deref(),
        )
        .await
        .map_err(admin_err)?;
    deps.svc
        .stamp_ban_reason(out.ban.id, category)
        .await
        .map_err(map_err)?;
    deps.svc
        .mark_node_banned(&who, cid, acc, reason.as_deref().unwrap_or(""))
        .await
        .map_err(map_err)?;
    Ok(Json(out))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SubtreeRequestBody {
    /// `sterilize` (needs `account_id`) or `branch_action` (needs `action`; Owner only).
    pub kind: String,
    pub account_id: Option<String>,
    /// `freeze` or `ban` (branch action).
    pub action: Option<String>,
    /// Why, in at least 20 characters.
    pub reason: String,
}

/// Propose a sterilization of one node (`can_invite = false` for 12 months, `MANAGE_CASES`) or the Operator's branch
/// action on the whole subtree (Owner only). Needs a fresh 2FA step-up; nothing takes effect until a second staff
/// member approves.
#[utoipa::path(
    post, path = "/admin/moderation/subtree-cases/{case_id}/requests", request_body = SubtreeRequestBody,
    params(("case_id" = String, Path, description = "Subtree case snowflake")),
    responses((status = 200, body = ModRequest), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn new_request(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(case_id): Path<String>,
    Json(b): Json<SubtreeRequestBody>,
) -> ApiResult<Json<ModRequest>> {
    let cid = id(&case_id, "case")?;
    let who = Who::Staff(&ctx.caller);
    let r = match b.kind.as_str() {
        "sterilize" => {
            let acc = id(b.account_id.as_deref().unwrap_or(""), "account")?;
            ctx.require_reauth().await?;
            deps.svc.request_sterilization(&who, cid, acc, &b.reason).await
        }
        "branch_action" => {
            ctx.require_reauth().await?;
            deps.svc
                .request_branch_action(&who, cid, b.action.as_deref().unwrap_or(""), &b.reason)
                .await
        }
        _ => Err(ModError::Invalid(
            "kind must be sterilize or branch_action".into(),
        )),
    };
    staff_result(&ctx, "moderation.request.create", Some(("case", cid)), r)
        .await
        .map(Json)
}

/// Requests waiting for a second staff member or the Operator, then the recent ones (`MANAGE_CASES`).
#[utoipa::path(
    get, path = "/admin/moderation/requests",
    responses((status = 200, body = Vec<ModRequest>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list_requests(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
) -> ApiResult<Json<Vec<ModRequest>>> {
    let r = deps.svc.list_requests(&Who::Staff(&ctx.caller)).await;
    staff_result(&ctx, "moderation.request.list", None, r)
        .await
        .map(Json)
}

/// Approve a request as the second staff member (`MANAGE_CASES`; never the proposer). Needs a fresh 2FA step-up.
/// A sterilization takes effect now; a branch action becomes executable by the Operator.
#[utoipa::path(
    post, path = "/admin/moderation/requests/{request_id}/approve",
    params(("request_id" = String, Path, description = "Request snowflake")),
    responses((status = 200, body = ModRequest), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn approve_request(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(request_id): Path<String>,
) -> ApiResult<Json<ModRequest>> {
    let rid = id(&request_id, "request")?;
    ctx.require_reauth().await?;
    let r = deps.svc.approve_request(&Who::Staff(&ctx.caller), rid).await;
    staff_result(&ctx, "moderation.request.approve", Some(("request", rid)), r)
        .await
        .map(Json)
}

/// Reject a waiting request (`MANAGE_CASES`, reason required); the proposer withdrawing their own cancels it.
#[utoipa::path(
    post, path = "/admin/moderation/requests/{request_id}/reject", request_body = MemberReasonBody,
    params(("request_id" = String, Path, description = "Request snowflake")),
    responses((status = 200, body = ModRequest), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn reject_request(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(request_id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(b): Json<MemberReasonBody>,
) -> ApiResult<Json<ModRequest>> {
    let rid = id(&request_id, "request")?;
    let r = deps
        .svc
        .reject_request(
            &Who::Staff(&ctx.caller),
            rid,
            reason_of(&headers, &b.reason).as_deref(),
        )
        .await;
    staff_result(&ctx, "moderation.request.reject", Some(("request", rid)), r)
        .await
        .map(Json)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BranchActionBody {
    /// The approved branch-action request.
    pub request_id: String,
}

/// The Operator's branch action (D-395): apply the approved request's action (freeze or ban) to every descendant
/// of the case at once. Owner only, behind a 2FA step-up, with a written reason and a second staff approver
/// already recorded. Every affected account keeps its own appeal; freezes last 12 months. One hash-chained audit
/// entry names the case, the approver and the node count, and an anonymised notice is emitted.
#[utoipa::path(
    post, path = "/admin/moderation/branch-action", request_body = BranchActionBody,
    responses((status = 200, body = BranchOutcome), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn branch_action(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Json(b): Json<BranchActionBody>,
) -> ApiResult<Json<BranchOutcome>> {
    // The Operator only: refused before anything else, so a non-Operator learns nothing about the request.
    if !ctx.caller.is_owner {
        let _ = ctx
            .svc()
            .record_denied(&ctx.caller, "moderation.branch_action", None, "not the Operator")
            .await;
        return Err(map_err(ModError::MissingPermission("OWNER")));
    }
    let rid = id(&b.request_id, "request")?;
    ctx.require_reauth().await?;
    let r = deps
        .svc
        .execute_branch_action(&Who::Staff(&ctx.caller), rid, ctx.svc())
        .await;
    staff_result(
        &ctx,
        "moderation.branch_action",
        Some(("request", rid)),
        r,
    )
    .await
    .map(Json)
}
