//! Holds and appeals (MOD-HOLD-001 to -004, MOD-APL-001 to -003). Holds are placed and released by platform
//! staff only (D-213); appeals are filed from the ban screen and decided by a different staff member (D-215).

use super::{Actor, ModerationDeps, id, map_err, staff_result};
use crate::{
    error::{ApiResult, HttpError},
    openapi::ErrorBody,
    routes::{
        Ext,
        auth::{AuthRouteError, Svc},
    },
    state::AppState,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sr_admin::model::id as id_str;
use sr_core::{ApiError, ErrorCode, PlatformPermissions as P};
use sr_moderation::{
    HoldScope, ModError,
    model::{AppealDecision, AppealView, HoldView, Who},
};
use utoipa::{IntoParams, ToSchema};

/// Body of a hold.
#[derive(Debug, Deserialize, ToSchema)]
pub struct HoldBody {
    /// `message` (with its attachments), `attachment`, `submission` or `user_messages`.
    pub kind: String,
    /// The message, attachment or submission; for `user_messages` the account.
    pub id: String,
    /// `user_messages` only: the window, Unix milliseconds (`since_ms` inclusive, `until_ms` exclusive).
    pub since_ms: Option<i64>,
    pub until_ms: Option<i64>,
    /// Why. Required.
    pub reason: Option<String>,
}

/// What a placed hold froze.
#[derive(Debug, Serialize, ToSchema)]
pub struct HoldPlaced {
    #[serde(with = "id_str")]
    #[schema(value_type = String)]
    pub hold_id: i64,
    pub messages: i64,
    pub attachments: i64,
    pub submissions: i64,
}

/// Freeze content for a case (platform staff, `MANAGE_CASES`; D-213). A hold overrides the author's deletion
/// rights (D-122) and never expires by itself. 409 when everything in scope is already held.
#[utoipa::path(
    post, path = "/moderation/cases/{case_id}/holds", request_body = HoldBody,
    params(("case_id" = String, Path, description = "Case snowflake")),
    responses((status = 201, body = HoldPlaced), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn place(
    a: Actor,
    Path(case_id): Path<String>,
    Json(b): Json<HoldBody>,
) -> ApiResult<(StatusCode, Json<HoldPlaced>)> {
    let cid = id(&case_id, "case")?;
    let ctx = a.staff_only()?;
    let target = id(&b.id, "target")?;
    let scope = match b.kind.as_str() {
        "message" => HoldScope::Message(target),
        "attachment" => HoldScope::Attachment(target),
        "submission" => HoldScope::Submission(target),
        "user_messages" => HoldScope::UserMessages {
            account_id: target,
            since_ms: b.since_ms.ok_or_else(|| {
                map_err(ModError::Invalid(
                    "since_ms is required for user_messages".into(),
                ))
            })?,
            until_ms: b.until_ms.ok_or_else(|| {
                map_err(ModError::Invalid(
                    "until_ms is required for user_messages".into(),
                ))
            })?,
        },
        _ => {
            return Err(map_err(ModError::Invalid(
                "kind must be message, attachment, submission or user_messages".into(),
            )));
        }
    };
    let r = a
        .svc()
        .place_hold(&Who::Staff(&ctx.caller), cid, scope, b.reason.as_deref())
        .await;
    let out = staff_result(ctx, "moderation.hold.place", Some(("case", cid)), r).await?;
    Ok((
        StatusCode::CREATED,
        Json(HoldPlaced {
            hold_id: out.hold_id.unwrap_or_default(),
            messages: out.messages,
            attachments: out.attachments,
            submissions: out.submissions,
        }),
    ))
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct HoldParams {
    /// Only holds still in force (default true).
    pub active_only: Option<bool>,
    /// 1 to 200, default 100.
    pub limit: Option<i64>,
}

/// Every hold, newest first, with what it freezes and whether it is due for review (`MANAGE_CASES`).
#[utoipa::path(
    get, path = "/moderation/holds", params(HoldParams),
    responses((status = 200, body = Vec<HoldView>), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(a: Actor, Query(p): Query<HoldParams>) -> ApiResult<Json<Vec<HoldView>>> {
    let ctx = a.staff_only()?;
    let r = a
        .svc()
        .list_holds(
            &Who::Staff(&ctx.caller),
            p.active_only.unwrap_or(true),
            p.limit.unwrap_or(100),
        )
        .await;
    staff_result(ctx, "moderation.hold.list", None, r)
        .await
        .map(Json)
}

/// Body of a release.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct HoldReleaseBody {
    /// Why. Required.
    pub reason: Option<String>,
}

/// What a release did.
#[derive(Debug, Serialize, ToSchema)]
pub struct HoldReleased {
    pub messages: i64,
    pub attachments: i64,
    pub submissions: i64,
    /// Deleted messages that the next purge run takes.
    pub purgeable_messages: i64,
}

/// Release a hold (`MANAGE_CASES`, a reason). Deleted rows become purgeable at the next job run.
#[utoipa::path(
    post, path = "/moderation/holds/{hold_id}/release", request_body = HoldReleaseBody,
    params(("hold_id" = String, Path, description = "Hold snowflake")),
    responses((status = 200, body = HoldReleased), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn release(
    a: Actor,
    Path(hold_id): Path<String>,
    Json(b): Json<HoldReleaseBody>,
) -> ApiResult<Json<HoldReleased>> {
    let hid = id(&hold_id, "hold")?;
    let ctx = a.staff_only()?;
    let r = a
        .svc()
        .release_hold(&Who::Staff(&ctx.caller), hid, b.reason.as_deref())
        .await;
    staff_result(ctx, "moderation.hold.release", Some(("hold", hid)), r)
        .await
        .map(|o| {
            Json(HoldReleased {
                messages: o.messages,
                attachments: o.attachments,
                submissions: o.submissions,
                purgeable_messages: o.purgeable_messages,
            })
        })
}

/// Mark a hold as reviewed and kept: it leaves the review list until the next window (D-212).
#[utoipa::path(
    post, path = "/moderation/holds/{hold_id}/reviewed",
    params(("hold_id" = String, Path, description = "Hold snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn reviewed(a: Actor, Path(hold_id): Path<String>) -> ApiResult<StatusCode> {
    let hid = id(&hold_id, "hold")?;
    let ctx = a.staff_only()?;
    let r = a
        .svc()
        .mark_hold_reviewed(&Who::Staff(&ctx.caller), hid)
        .await;
    staff_result(ctx, "moderation.hold.reviewed", Some(("hold", hid)), r).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- appeals -----------------------------------------------------------------------------------------------

/// Body of an appeal from the ban screen.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AppealBody {
    /// Username or email of the banned account.
    pub login: String,
    pub password: String,
    /// Up to 2000 characters.
    pub message: String,
}

fn auth_err(e: AuthRouteError) -> HttpError {
    match e {
        AuthRouteError::Api(h) => h,
        AuthRouteError::Throttled { retry_after } => ApiError::with_message(
            ErrorCode::TooManyRequests,
            format!(
                "Too many attempts. Try again in {} seconds.",
                retry_after.max(1)
            ),
        )
        .into(),
    }
}

/// Appeal a platform ban (public). A banned account has no session, so the account proves itself with its
/// credentials, throttled like sign-in. One appeal per decision; there is no second appeal (D-215).
/// The answer says nothing about the ban beyond what the person already knows.
#[utoipa::path(
    post, path = "/moderation/appeals", request_body = AppealBody,
    responses((status = 204, description = "The appeal was filed"), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 409, body = ErrorBody),
              (status = 429, body = ErrorBody)),
    security(())
)]
pub async fn file_appeal(
    State(state): State<AppState>,
    Svc { svc, ip }: Svc,
    Ext(d): Ext<ModerationDeps>,
    Json(b): Json<AppealBody>,
) -> ApiResult<StatusCode> {
    let account = svc
        .appeal_account(&ip, state.now_secs(), &b.login, &b.password)
        .await
        .map_err(auth_err)?;
    d.svc
        .create_appeal(account, &b.message)
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AppealParams {
    /// Only appeals not decided yet (default true).
    pub open_only: Option<bool>,
    pub limit: Option<i64>,
}

/// Appeals, newest first (`MANAGE_CASES`).
#[utoipa::path(
    get, path = "/moderation/appeals", params(AppealParams),
    responses((status = 200, body = Vec<AppealView>), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list_appeals(
    a: Actor,
    Query(p): Query<AppealParams>,
) -> ApiResult<Json<Vec<AppealView>>> {
    let ctx = a.staff_only()?;
    let r = a
        .svc()
        .list_appeals(
            &Who::Staff(&ctx.caller),
            p.open_only.unwrap_or(true),
            p.limit.unwrap_or(100),
        )
        .await;
    staff_result(ctx, "moderation.appeal.list", None, r)
        .await
        .map(Json)
}

/// Body of an appeal decision.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AppealDecisionBody {
    pub decision: AppealDecision,
    /// Why. Required.
    pub reason: Option<String>,
}

/// Decide an appeal (`MANAGE_CASES`). Not the staff member who made the ban, unless they are the Owner and nobody
/// else can review (D-215). Overturning lifts the ban through the admin service (`PLATFORM_BAN`).
#[utoipa::path(
    post, path = "/moderation/appeals/{appeal_id}/decision", request_body = AppealDecisionBody,
    params(("appeal_id" = String, Path, description = "Appeal snowflake")),
    responses((status = 200, body = AppealView), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn decide_appeal(
    a: Actor,
    Path(appeal_id): Path<String>,
    Json(b): Json<AppealDecisionBody>,
) -> ApiResult<Json<AppealView>> {
    let aid = id(&appeal_id, "appeal")?;
    let ctx = a.staff_only()?;
    let who = Who::Staff(&ctx.caller);
    let reason = sr_moderation::rules::reason(b.reason.as_deref()).map_err(map_err)?;

    // Could somebody other than the caller review it? The reviewer rule (D-215) only bites when the caller made
    // the ban; the staff list is read from the repository and only counted.
    let staff = ctx
        .svc()
        .repo()
        .list_staff()
        .await
        .map_err(crate::routes::admin::map_err)?;
    let others = staff.iter().any(|m| {
        m.account_id != ctx.caller.user_id
            && (m.is_owner
                || m.platform_permissions
                    .parse::<u64>()
                    .ok()
                    .map(P::from_bits_truncate)
                    .is_some_and(|p| p.has(P::MANAGE_CASES)))
    });
    let checked = a.svc().check_appeal_review(&who, aid, others).await;
    let appeal = staff_result(
        ctx,
        "moderation.appeal.decide",
        Some(("appeal", aid)),
        checked,
    )
    .await?;

    if b.decision == AppealDecision::Overturned {
        ctx.svc()
            .unban_person(&ctx.caller, appeal.person_id, Some(&reason))
            .await
            .map_err(crate::routes::admin::map_err)?;
    }
    let r = a
        .svc()
        .decide_appeal(&who, aid, b.decision, Some(&reason), others)
        .await;
    staff_result(ctx, "moderation.appeal.decide", Some(("appeal", aid)), r)
        .await
        .map(Json)
}
