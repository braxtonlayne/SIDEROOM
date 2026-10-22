//! NSFW and age policy tools for staff (MOD-AGE-001 and -003; spec section 7). Mark or unmark a channel, series or
//! title as age-restricted, from a case or from the item page; reset and lock the age confirmation of an account
//! found to be under 18, and suspend one found to be under the minimum age; lift the lock again. Platform staff
//! only. The rules and the SQL live in `sr_moderation::age`.

use super::{Actor, id, staff_result};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{Json, extract::Path, http::StatusCode};
use serde::Deserialize;
use sr_moderation::{AgeBand, AgeChange, AgeTargetKind, UnderageOutcome, model::Who};
use utoipa::ToSchema;

/// Body of `POST /moderation/age/restrictions`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RestrictionBody {
    pub kind: AgeTargetKind,
    /// The channel, series or title (decimal snowflake).
    pub id: String,
    /// `true` marks it age-restricted, `false` removes the mark.
    pub restricted: bool,
    /// Why (required, up to 500 characters; goes to the access log and the case timeline).
    pub reason: Option<String>,
    /// The case this belongs to; its timeline gets the entry and a case being worked moves to `actioned`.
    pub case_id: Option<String>,
}

/// Mark or unmark a channel, library series or cinema title as age-restricted (`MANAGE_CASES`, or the review bit of
/// the kind). The change applies on the next request; open clients are told when a channel changes. Marking a
/// channel needs a server channel that is not a category or a thread.
#[utoipa::path(
    post, path = "/moderation/age/restrictions", request_body = RestrictionBody,
    responses((status = 200, body = AgeChange), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn restrict(a: Actor, Json(b): Json<RestrictionBody>) -> ApiResult<Json<AgeChange>> {
    let ctx = a.staff_only()?;
    let target = id(&b.id, "target")?;
    let case = b.case_id.as_deref().map(|c| id(c, "case")).transpose()?;
    let r = a
        .svc()
        .set_age_restricted(
            &Who::Staff(&ctx.caller),
            b.kind,
            target,
            b.restricted,
            b.reason.as_deref(),
            case,
        )
        .await;
    staff_result(
        ctx,
        "moderation.age.restrict",
        Some((b.kind.as_str(), target)),
        r,
    )
    .await
    .map(Json)
}

/// Body of `POST /moderation/age/underage`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UnderageBody {
    /// The account found to be younger than allowed (decimal snowflake).
    pub account_id: String,
    /// `under_18`: reset and lock the age confirmation. `under_13` (below the minimum registration age): the same,
    /// and the account is suspended.
    pub band: AgeBand,
    /// Why (required, up to 500 characters; kept in the access log and, for a suspension, the audit reason).
    pub note: Option<String>,
    pub case_id: Option<String>,
}

/// Underage handling (D-294, spec 7): reset the age confirmation so the gate asks again, and lock it so the account
/// cannot answer "yes" itself until staff lift the lock. An account under 13 is also suspended (needs
/// `MANAGE_ACCOUNTS`), and its inviter is told only that the invitee did not meet the entry rules. `MANAGE_CASES`.
#[utoipa::path(
    post, path = "/moderation/age/underage", request_body = UnderageBody,
    responses((status = 200, body = UnderageOutcome), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn underage(a: Actor, Json(b): Json<UnderageBody>) -> ApiResult<Json<UnderageOutcome>> {
    let ctx = a.staff_only()?;
    let account = id(&b.account_id, "account")?;
    let case = b.case_id.as_deref().map(|c| id(c, "case")).transpose()?;
    let r = a
        .svc()
        .handle_underage(
            &Who::Staff(&ctx.caller),
            account,
            b.band,
            b.note.as_deref(),
            case,
        )
        .await;
    staff_result(ctx, "moderation.age.underage", Some(("account", account)), r)
        .await
        .map(Json)
}

/// Body of `POST /moderation/age/underage/{account_id}/lift`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct LiftBody {
    /// Why the lock goes (required).
    pub reason: Option<String>,
}

/// Lift the lock on an account's age confirmation, so it may answer the age gate again (`MANAGE_CASES`). A suspension
/// is lifted separately, on the account.
#[utoipa::path(
    post, path = "/moderation/age/underage/{account_id}/lift", request_body = LiftBody,
    params(("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn lift(
    a: Actor,
    Path(account_id): Path<String>,
    Json(b): Json<LiftBody>,
) -> ApiResult<StatusCode> {
    let ctx = a.staff_only()?;
    let account = id(&account_id, "account")?;
    let r = a
        .svc()
        .lift_age_lock(&Who::Staff(&ctx.caller), account, b.reason.as_deref())
        .await;
    staff_result(ctx, "moderation.age.unlock", Some(("account", account)), r).await?;
    Ok(StatusCode::NO_CONTENT)
}
