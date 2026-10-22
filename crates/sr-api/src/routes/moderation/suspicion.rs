//! Inviter suspicion and evasion signals (MOD-SUS-001 to -004; spec section 10; D-031). Platform staff only.
//! The panel needs `MANAGE_INVITES`; everything IP-derived (the signals and the addresses behind them) needs
//! `MANAGE_ACCOUNTS`, and every read is written to the access log. Clearing suspicion stays
//! `POST /admin/persons/{person_id}/clear-suspicion`.

use super::{ModerationDeps, id, map_err, staff_result};
use crate::{
    error::ApiResult,
    openapi::ErrorBody,
    routes::{Ext, admin::StaffCtx},
};
use axum::{
    Json,
    extract::{Path, Query},
};
use serde::Deserialize;
use sr_moderation::{
    ModError,
    model::{
        ScanOutcome, SignalDetail, SignalFilter, SignalState, SignalView, SuspicionPanel, Who,
    },
};
use utoipa::{IntoParams, ToSchema};

/// The suspicion panel of a Person page: history, banned accounts below the Person in the invite tree with their
/// ban reasons, the invite path to the root, the automatic cases, and (with `MANAGE_ACCOUNTS`) the evasion
/// signals. Staff only; the read is logged.
#[utoipa::path(
    get, path = "/moderation/persons/{person_id}/suspicion",
    params(("person_id" = String, Path, description = "Person snowflake")),
    responses((status = 200, body = SuspicionPanel), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn panel(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(person_id): Path<String>,
) -> ApiResult<Json<SuspicionPanel>> {
    let pid = id(&person_id, "person")?;
    let r = deps
        .svc
        .suspicion_panel(&Who::Staff(&ctx.caller), pid)
        .await;
    staff_result(&ctx, "moderation.suspicion.view", Some(("person", pid)), r)
        .await
        .map(Json)
}

/// Compare a Person with the banned Persons now and raise the evasion signals that are justified (`MANAGE_ACCOUNTS`).
/// Nothing is banned, frozen or hidden by it.
#[utoipa::path(
    post, path = "/moderation/persons/{person_id}/evasion-scan",
    params(("person_id" = String, Path, description = "Person snowflake")),
    responses((status = 200, body = ScanOutcome), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn scan(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(person_id): Path<String>,
) -> ApiResult<Json<ScanOutcome>> {
    let pid = id(&person_id, "person")?;
    let r = deps.svc.scan_person_as(&Who::Staff(&ctx.caller), pid).await;
    staff_result(&ctx, "moderation.evasion.scan", Some(("person", pid)), r)
        .await
        .map(Json)
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SignalParams {
    /// `open` (default), `dismissed`, `confirmed`, or `all`.
    pub state: Option<String>,
    /// Only signals that name this Person on either side.
    pub person_id: Option<String>,
    /// 1 to 200, default 100.
    pub limit: Option<i64>,
}

/// Evasion signals, newest first (`MANAGE_ACCOUNTS`; the read is logged). They carry counts and words, never
/// an address.
#[utoipa::path(
    get, path = "/moderation/evasion-signals", params(SignalParams),
    responses((status = 200, body = Vec<SignalView>), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Query(p): Query<SignalParams>,
) -> ApiResult<Json<Vec<SignalView>>> {
    let state = match p.state.as_deref() {
        None | Some("") => Some(SignalState::Open),
        Some("all") => None,
        Some(s) => Some(
            SignalState::parse(s)
                .ok_or_else(|| map_err(ModError::Invalid("state is not a signal state".into())))?,
        ),
    };
    let person_id = match p.person_id.as_deref() {
        None | Some("") => None,
        Some(v) => Some(id(v, "person")?),
    };
    let f = SignalFilter {
        state,
        person_id,
        limit: p.limit.unwrap_or(100),
    };
    let r = deps.svc.list_signals(&Who::Staff(&ctx.caller), &f).await;
    staff_result(&ctx, "moderation.evasion.list", None, r)
        .await
        .map(Json)
}

/// One signal with the addresses behind it, read from the sign-in history of the last 180 days (`MANAGE_ACCOUNTS`).
/// Addresses removed by retention or by an account deletion are not shown. The read is logged.
#[utoipa::path(
    get, path = "/moderation/evasion-signals/{signal_id}",
    params(("signal_id" = String, Path, description = "Signal snowflake")),
    responses((status = 200, body = SignalDetail), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn detail(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(signal_id): Path<String>,
) -> ApiResult<Json<SignalDetail>> {
    let sid = id(&signal_id, "signal")?;
    let r = deps.svc.signal_detail(&Who::Staff(&ctx.caller), sid).await;
    staff_result(&ctx, "moderation.evasion.detail", Some(("signal", sid)), r)
        .await
        .map(Json)
}

/// Body of a signal review.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SignalReviewBody {
    /// `dismissed` (not the same person) or `confirmed` (looks like the same person).
    pub decision: String,
    /// Why. Required.
    pub reason: Option<String>,
}

/// Review a signal (`MANAGE_ACCOUNTS`; reason required; logged). Confirming does not ban anyone: a ban is its own
/// decision under `PLATFORM_BAN`.
#[utoipa::path(
    post, path = "/moderation/evasion-signals/{signal_id}/review", request_body = SignalReviewBody,
    params(("signal_id" = String, Path, description = "Signal snowflake")),
    responses((status = 200, body = SignalView), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn review(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(signal_id): Path<String>,
    Json(b): Json<SignalReviewBody>,
) -> ApiResult<Json<SignalView>> {
    let sid = id(&signal_id, "signal")?;
    let decision = match SignalState::parse(&b.decision) {
        Some(d @ (SignalState::Dismissed | SignalState::Confirmed)) => d,
        _ => {
            return Err(map_err(ModError::Invalid(
                "decision must be dismissed or confirmed".into(),
            )));
        }
    };
    let r = deps
        .svc
        .resolve_signal(
            &Who::Staff(&ctx.caller),
            sid,
            decision,
            b.reason.as_deref().unwrap_or(""),
        )
        .await;
    staff_result(&ctx, "moderation.evasion.resolve", Some(("signal", sid)), r)
        .await
        .map(Json)
}

/// Body of a case link.
#[derive(Debug, Deserialize, ToSchema)]
pub struct LinkCaseBody {
    /// The case staff opened from the signal.
    pub case_id: String,
}

/// Link a signal to the case opened from it (`MANAGE_CASES`; access-logged). 409 when it is already linked.
#[utoipa::path(
    post, path = "/moderation/evasion-signals/{signal_id}/link-case", request_body = LinkCaseBody,
    params(("signal_id" = String, Path, description = "Signal snowflake")),
    responses((status = 200, body = SignalView), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn link_case(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(signal_id): Path<String>,
    Json(b): Json<LinkCaseBody>,
) -> ApiResult<Json<SignalView>> {
    let sid = id(&signal_id, "signal")?;
    let cid = id(&b.case_id, "case")?;
    let r = deps
        .svc
        .link_signal_case(&Who::Staff(&ctx.caller), sid, cid)
        .await;
    staff_result(
        &ctx,
        "moderation.evasion.link_case",
        Some(("signal", sid)),
        r,
    )
    .await
    .map(Json)
}

/// How many signals are open (`MANAGE_ACCOUNTS`; access-logged), for the Review hub.
#[utoipa::path(
    get, path = "/moderation/evasion-signals/count",
    responses((status = 200, body = SignalCount), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn count(ctx: StaffCtx, Ext(deps): Ext<ModerationDeps>) -> ApiResult<Json<SignalCount>> {
    let r = deps.svc.count_open_signals(&Who::Staff(&ctx.caller)).await;
    staff_result(&ctx, "moderation.evasion.count", None, r)
        .await
        .map(|open| Json(SignalCount { open }))
}

/// Open evasion signals.
#[derive(Debug, serde::Serialize, ToSchema)]
pub struct SignalCount {
    pub open: i64,
}
