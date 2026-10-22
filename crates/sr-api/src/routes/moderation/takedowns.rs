//! The takedown register (MOD-TKD-001 to -011; spec 5; D-293): notices are entered by staff with
//! `MANAGE_TAKEDOWNS`, triaged, upheld or rejected, counter-noticed and restored, and the register exports. The
//! item pages of the Library and Cinema ask one small question of the public side: is this held or taken down.

use super::{ModerationDeps, id, map_err, staff_result};
use crate::{
    auth::UserOnly,
    error::ApiResult,
    openapi::ErrorBody,
    routes::{Ext, admin::StaffCtx},
};
use axum::{
    Json,
    extract::{Path, Query},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use sr_moderation::{
    ModError,
    model::{
        ItemKind, ItemStatus, NewItem, NewNotice, NoticeDetail, NoticeState, NoticeSummary,
        RejectOutcome, UpholdOutcome, Who,
    },
};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RegisterParams {
    /// Only one state: `received`, `triage`, `rejected`, `removed`, `counter_noticed`, `restored`, `withdrawn`.
    pub state: Option<String>,
    /// 1 to 500, default 200.
    pub limit: Option<i64>,
}

/// The register, newest first (`MANAGE_TAKEDOWNS`).
#[utoipa::path(
    get, path = "/moderation/takedowns", params(RegisterParams),
    responses((status = 200, body = Vec<NoticeSummary>), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Query(p): Query<RegisterParams>,
) -> ApiResult<Json<Vec<NoticeSummary>>> {
    let state = match p.state.as_deref() {
        None | Some("") => None,
        Some(s) => Some(
            NoticeState::parse(s)
                .ok_or_else(|| map_err(ModError::Invalid("state is not a notice state".into())))?,
        ),
    };
    let r = deps
        .svc
        .list_notices(&Who::Staff(&ctx.caller), state, p.limit.unwrap_or(200))
        .await;
    staff_result(&ctx, "moderation.takedown.list", None, r)
        .await
        .map(Json)
}

/// Enter a notice (staff only; there is no public form, D-293). Opens its case.
#[utoipa::path(
    post, path = "/moderation/takedowns", request_body = NewNotice,
    responses((status = 201, body = NoticeDetail), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Json(b): Json<NewNotice>,
) -> ApiResult<(StatusCode, Json<NoticeDetail>)> {
    let r = deps.svc.create_notice(&Who::Staff(&ctx.caller), &b).await;
    let d = staff_result(&ctx, "moderation.takedown.create", None, r).await?;
    Ok((StatusCode::CREATED, Json(d)))
}

/// One notice with its items, timeline and strike counts (`MANAGE_TAKEDOWNS`).
#[utoipa::path(
    get, path = "/moderation/takedowns/{notice_id}",
    params(("notice_id" = String, Path, description = "Notice snowflake")),
    responses((status = 200, body = NoticeDetail), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(notice_id): Path<String>,
) -> ApiResult<Json<NoticeDetail>> {
    let nid = id(&notice_id, "takedown notice")?;
    let r = deps.svc.get_notice(&Who::Staff(&ctx.caller), nid).await;
    staff_result(&ctx, "moderation.takedown.get", Some(("takedown", nid)), r)
        .await
        .map(Json)
}

/// Name a piece of content the notice covers (triage). 409 when it is already on the notice.
#[utoipa::path(
    post, path = "/moderation/takedowns/{notice_id}/items", request_body = NewItem,
    params(("notice_id" = String, Path, description = "Notice snowflake")),
    responses((status = 200, body = NoticeDetail), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn add_item(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(notice_id): Path<String>,
    Json(b): Json<NewItem>,
) -> ApiResult<Json<NoticeDetail>> {
    let nid = id(&notice_id, "takedown notice")?;
    let r = deps.svc.add_item(&Who::Staff(&ctx.caller), nid, &b).await;
    staff_result(
        &ctx,
        "moderation.takedown.item_add",
        Some(("takedown", nid)),
        r,
    )
    .await
    .map(Json)
}

/// Take back an item named by mistake, while it is only identified (not yet held).
#[utoipa::path(
    delete, path = "/moderation/takedowns/{notice_id}/items/{item_id}",
    params(("notice_id" = String, Path, description = "Notice snowflake"),
           ("item_id" = String, Path, description = "Item snowflake")),
    responses((status = 200, body = NoticeDetail), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn remove_item(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path((notice_id, item_id)): Path<(String, String)>,
) -> ApiResult<Json<NoticeDetail>> {
    let nid = id(&notice_id, "takedown notice")?;
    let iid = id(&item_id, "takedown item")?;
    let r = deps
        .svc
        .remove_item(&Who::Staff(&ctx.caller), nid, iid)
        .await;
    staff_result(
        &ctx,
        "moderation.takedown.item_remove",
        Some(("takedown", nid)),
        r,
    )
    .await
    .map(Json)
}

/// A body that carries a reason.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct TakedownReasonBody {
    /// Why. Required.
    pub reason: Option<String>,
}

/// Freeze the identified items for evidence without removing anything yet (hold, then disable).
#[utoipa::path(
    post, path = "/moderation/takedowns/{notice_id}/hold", request_body = TakedownReasonBody,
    params(("notice_id" = String, Path, description = "Notice snowflake")),
    responses((status = 200, body = NoticeDetail), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn hold(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(notice_id): Path<String>,
    Json(b): Json<TakedownReasonBody>,
) -> ApiResult<Json<NoticeDetail>> {
    let nid = id(&notice_id, "takedown notice")?;
    let r = deps
        .svc
        .hold_notice(&Who::Staff(&ctx.caller), nid, b.reason.as_deref())
        .await;
    staff_result(&ctx, "moderation.takedown.hold", Some(("takedown", nid)), r)
        .await
        .map(Json)
}

/// Reject an incomplete notice. Returns the templated reply staff send to the notifier.
#[utoipa::path(
    post, path = "/moderation/takedowns/{notice_id}/reject", request_body = TakedownReasonBody,
    params(("notice_id" = String, Path, description = "Notice snowflake")),
    responses((status = 200, body = RejectOutcome), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn reject(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(notice_id): Path<String>,
    Json(b): Json<TakedownReasonBody>,
) -> ApiResult<Json<RejectOutcome>> {
    let nid = id(&notice_id, "takedown notice")?;
    let r = deps
        .svc
        .reject_notice(&Who::Staff(&ctx.caller), nid, b.reason.as_deref())
        .await;
    staff_result(
        &ctx,
        "moderation.takedown.reject",
        Some(("takedown", nid)),
        r,
    )
    .await
    .map(Json)
}

/// Uphold a notice: hold and disable every item, block the stored objects, tell the uploaders, count the
/// strike (a review case opens at the threshold, never a suspension).
#[utoipa::path(
    post, path = "/moderation/takedowns/{notice_id}/uphold", request_body = TakedownReasonBody,
    params(("notice_id" = String, Path, description = "Notice snowflake")),
    responses((status = 200, body = UpholdOutcome), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn uphold(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(notice_id): Path<String>,
    Json(b): Json<TakedownReasonBody>,
) -> ApiResult<Json<UpholdOutcome>> {
    let nid = id(&notice_id, "takedown notice")?;
    let r = deps
        .svc
        .uphold_notice(&Who::Staff(&ctx.caller), nid, b.reason.as_deref())
        .await;
    staff_result(
        &ctx,
        "moderation.takedown.uphold",
        Some(("takedown", nid)),
        r,
    )
    .await
    .map(Json)
}

/// Body of a counter-notice.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CounterNoticeBody {
    /// The counter-notice as received, kept verbatim.
    pub text: String,
}

/// Record a counter-notice (staff record it). The content returns after ten business days unless a court
/// filing is noted.
#[utoipa::path(
    post, path = "/moderation/takedowns/{notice_id}/counter-notice", request_body = CounterNoticeBody,
    params(("notice_id" = String, Path, description = "Notice snowflake")),
    responses((status = 200, body = NoticeDetail), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn counter_notice(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(notice_id): Path<String>,
    Json(b): Json<CounterNoticeBody>,
) -> ApiResult<Json<NoticeDetail>> {
    let nid = id(&notice_id, "takedown notice")?;
    let r = deps
        .svc
        .record_counter_notice(&Who::Staff(&ctx.caller), nid, &b.text)
        .await;
    staff_result(
        &ctx,
        "moderation.takedown.counter_notice",
        Some(("takedown", nid)),
        r,
    )
    .await
    .map(Json)
}

/// Body of a court-filing note.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CourtFilingBody {
    /// `true` to note that the notifier went to court (the job will not restore); `false` to clear it.
    pub noted: bool,
    /// Why. Required.
    pub reason: Option<String>,
}

/// Note or clear a court filing on a notice that is in force.
#[utoipa::path(
    post, path = "/moderation/takedowns/{notice_id}/court-filing", request_body = CourtFilingBody,
    params(("notice_id" = String, Path, description = "Notice snowflake")),
    responses((status = 200, body = NoticeDetail), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn court_filing(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(notice_id): Path<String>,
    Json(b): Json<CourtFilingBody>,
) -> ApiResult<Json<NoticeDetail>> {
    let nid = id(&notice_id, "takedown notice")?;
    let r = deps
        .svc
        .set_court_filing(&Who::Staff(&ctx.caller), nid, b.noted, b.reason.as_deref())
        .await;
    staff_result(
        &ctx,
        "moderation.takedown.court_filing",
        Some(("takedown", nid)),
        r,
    )
    .await
    .map(Json)
}

/// Restore the content now (a logged action).
#[utoipa::path(
    post, path = "/moderation/takedowns/{notice_id}/restore", request_body = TakedownReasonBody,
    params(("notice_id" = String, Path, description = "Notice snowflake")),
    responses((status = 200, body = NoticeDetail), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn restore(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(notice_id): Path<String>,
    Json(b): Json<TakedownReasonBody>,
) -> ApiResult<Json<NoticeDetail>> {
    let nid = id(&notice_id, "takedown notice")?;
    let r = deps
        .svc
        .restore_notice(&Who::Staff(&ctx.caller), nid, b.reason.as_deref())
        .await;
    staff_result(
        &ctx,
        "moderation.takedown.restore",
        Some(("takedown", nid)),
        r,
    )
    .await
    .map(Json)
}

/// The notifier withdrew the notice: the content returns and no strike stands.
#[utoipa::path(
    post, path = "/moderation/takedowns/{notice_id}/withdraw", request_body = TakedownReasonBody,
    params(("notice_id" = String, Path, description = "Notice snowflake")),
    responses((status = 200, body = NoticeDetail), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn withdraw(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Path(notice_id): Path<String>,
    Json(b): Json<TakedownReasonBody>,
) -> ApiResult<Json<NoticeDetail>> {
    let nid = id(&notice_id, "takedown notice")?;
    let r = deps
        .svc
        .withdraw_notice(&Who::Staff(&ctx.caller), nid, b.reason.as_deref())
        .await;
    staff_result(
        &ctx,
        "moderation.takedown.withdraw",
        Some(("takedown", nid)),
        r,
    )
    .await
    .map(Json)
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ExportParams {
    /// `csv` (default) or `json`.
    pub format: Option<String>,
}

/// Export the whole register as CSV or JSON (`MANAGE_TAKEDOWNS`; the read is access-logged).
#[utoipa::path(
    get, path = "/moderation/takedowns/export", params(ExportParams),
    responses((status = 200, description = "text/csv, or a JSON array of notices"),
              (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn export(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Query(p): Query<ExportParams>,
) -> ApiResult<Response> {
    let format = p.format.as_deref().unwrap_or("csv");
    if !matches!(format, "csv" | "json") {
        return Err(map_err(ModError::Invalid(
            "format must be csv or json".into(),
        )));
    }
    let r = deps.svc.export_register(&Who::Staff(&ctx.caller)).await;
    let rows = staff_result(&ctx, "moderation.takedown.export", None, r).await?;
    let (body, ctype, name) = if format == "json" {
        (
            serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_owned()),
            "application/json",
            "takedown-register.json",
        )
    } else {
        (
            sr_moderation::register_csv(&rows),
            "text/csv; charset=utf-8",
            "takedown-register.csv",
        )
    };
    let mut res = (StatusCode::OK, body).into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(ctype));
    h.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{name}\""))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(res)
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ItemStatusParams {
    /// `library_series`, `library_chapter`, `cinema_title` or `cinema_episode`.
    pub kind: String,
    /// The series, chapter, title or episode.
    pub id: String,
}

/// Whether a library or cinema item is held or taken down: `none`, `held` or `taken_down`. For the reader and
/// player pages; the answer is only the state.
#[derive(Debug, Serialize, ToSchema)]
pub struct TakedownItemStatus {
    /// `none`, `held` or `taken_down`.
    pub state: String,
}

impl From<ItemStatus> for TakedownItemStatus {
    fn from(s: ItemStatus) -> Self {
        Self { state: s.state }
    }
}

/// Is this library or cinema item held or taken down? (Any signed-in person; reveals nothing but the state.)
#[utoipa::path(
    get, path = "/moderation/item-status", params(ItemStatusParams),
    responses((status = 200, body = TakedownItemStatus), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn item_status(
    _user: UserOnly,
    Ext(deps): Ext<ModerationDeps>,
    Query(p): Query<ItemStatusParams>,
) -> ApiResult<Json<TakedownItemStatus>> {
    let kind = ItemKind::parse(&p.kind)
        .filter(|k| {
            matches!(
                k,
                ItemKind::LibrarySeries
                    | ItemKind::LibraryChapter
                    | ItemKind::CinemaTitle
                    | ItemKind::CinemaEpisode
            )
        })
        .ok_or_else(|| {
            map_err(ModError::Invalid(
                "kind is not a library or cinema item".into(),
            ))
        })?;
    let target = id(&p.id, "item")?;
    deps.svc
        .item_status(kind, target)
        .await
        .map(|s| Json(s.into()))
        .map_err(map_err)
}
