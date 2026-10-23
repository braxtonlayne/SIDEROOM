//! Staff dashboards (MOD-DASH-002, -005 to -010; spec section 9). One read document whose sections follow the
//! caller's platform bits, the reports inbox and its bulk dismissal. Aggregates only: no reporter, no reported
//! person and no staff member appears in any answer.

use super::{Actor, id, map_err, staff_result};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{Json, extract::Query};
use serde::Deserialize;
use sr_moderation::{
    ModError,
    dashboard::{DashInbox, DashboardView, InboxDismissOutcome, InboxPage},
    model::Who,
};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DashboardParams {
    /// The window of the figures in days, 1 to 90, default 30.
    pub days: Option<i32>,
}

/// The dashboard (Admin > Moderation > Dashboard). A section is present only when the caller holds the bit
/// that reads it: `MANAGE_CASES` (inbox counts, health, retention check, age and child-safety metrics),
/// `PLATFORM_BAN` (bans by reason, severity and age), `MANAGE_TAKEDOWNS` (the register and the data requests).
/// 403 when the caller holds none of them.
#[utoipa::path(
    get, path = "/moderation/dashboard", params(DashboardParams),
    responses((status = 200, body = DashboardView), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get(a: Actor, Query(p): Query<DashboardParams>) -> ApiResult<Json<DashboardView>> {
    let ctx = a.staff_only()?;
    let r = a.svc().dashboard(&ctx.caller, p.days).await;
    staff_result(ctx, "moderation.dashboard", None, r)
        .await
        .map(Json)
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct InboxParams {
    /// `next_cursor` of the previous page.
    pub cursor: Option<String>,
    /// 1 to 100, default 25.
    pub limit: Option<i64>,
}

/// The reports inbox: cases still `open` with nobody assigned, each with the number of reports nobody has
/// looked at (several means duplicates of one target), oldest first (`MANAGE_CASES`). The reporter is never
/// part of the answer.
#[utoipa::path(
    get, path = "/moderation/dashboard/inbox", params(InboxParams),
    responses((status = 200, body = InboxPage), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn inbox(a: Actor, Query(p): Query<InboxParams>) -> ApiResult<Json<InboxPage>> {
    let ctx = a.staff_only()?;
    let cursor = p
        .cursor
        .as_deref()
        .map(|c| {
            c.parse::<i64>()
                .map_err(|_| map_err(ModError::Invalid("bad cursor".into())))
        })
        .transpose()?;
    let r = a
        .svc()
        .inbox(&Who::Staff(&ctx.caller), cursor, p.limit.unwrap_or(25))
        .await;
    staff_result(ctx, "moderation.inbox.list", None, r)
        .await
        .map(Json)
}

/// How many reports wait in the inbox, how many cases they make up and how old the oldest is (`MANAGE_CASES`).
/// The same figures as the dashboard's inbox section, for the Review hub tile.
#[utoipa::path(
    get, path = "/moderation/dashboard/inbox/count",
    responses((status = 200, body = DashInbox), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn inbox_count(a: Actor) -> ApiResult<Json<DashInbox>> {
    let ctx = a.staff_only()?;
    let r = a.svc().inbox_count(&Who::Staff(&ctx.caller)).await;
    staff_result(ctx, "moderation.inbox.count", None, r)
        .await
        .map(Json)
}

/// Body of a bulk dismissal.
#[derive(Debug, Deserialize, ToSchema)]
pub struct InboxDismissBody {
    /// The cases to dismiss, at most 100.
    pub case_ids: Vec<String>,
    /// Why they are dismissed. Required; it becomes the outcome of every case.
    pub reason: Option<String>,
}

/// Dismiss several inbox cases with one reason (`MANAGE_CASES`). Each case goes through the normal state change,
/// with its own timeline event and access-log row; one that is no longer waiting is reported back as skipped.
#[utoipa::path(
    post, path = "/moderation/dashboard/inbox/dismiss", request_body = InboxDismissBody,
    responses((status = 200, body = InboxDismissOutcome), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn dismiss(
    a: Actor,
    Json(b): Json<InboxDismissBody>,
) -> ApiResult<Json<InboxDismissOutcome>> {
    let ctx = a.staff_only()?;
    let ids = b
        .case_ids
        .iter()
        .map(|c| id(c, "case"))
        .collect::<Result<Vec<_>, _>>()?;
    let r = a
        .svc()
        .dismiss_inbox(&Who::Staff(&ctx.caller), &ids, b.reason.as_deref())
        .await;
    staff_result(ctx, "moderation.inbox.dismiss", None, r)
        .await
        .map(Json)
}
