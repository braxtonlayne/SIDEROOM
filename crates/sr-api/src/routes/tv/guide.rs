//! Imported guides (TV-ADM-003, TV-ADM-004) and favorites (TV-ENG-001): the weekly grid of an
//! `imported_guide` channel, the guide upload with its mapping report, the template download and the
//! member's starred channels.

use super::{MemberCtx, StaffTv, id, map_err};
use crate::{
    error::{ApiResult, HttpError},
    openapi::ErrorBody,
};
use axum::{
    Json,
    extract::{Path, Query},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sr_core::{ApiError, ErrorCode};
use sr_tv::guide::{CSV_TEMPLATE, JSON_TEMPLATE};
use sr_tv::model::Slot;
use sr_tv::service::{ConfirmInput, ConfirmedGuide, ImportView};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = TvSlotList)]
pub struct SlotList {
    pub items: Vec<Slot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = TvGuideImportList)]
pub struct ImportList {
    pub items: Vec<ImportView>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, ToSchema)]
#[schema(as = TvGuideUpload)]
pub struct UploadBody {
    /// `csv` or `json`.
    pub format: String,
    /// The file's text.
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = TvFavoriteList)]
pub struct FavoriteList {
    /// Starred channel ids.
    pub items: Vec<String>,
}

/// The weekly grid of an imported-guide channel.
#[utoipa::path(
    get, path = "/admin/tv/channels/{channel_id}/slots", operation_id = "admin_tv_slots",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = SlotList), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn slots(ctx: StaffTv, Path(channel_id): Path<String>) -> ApiResult<Json<SlotList>> {
    ctx.svc()
        .slots(ctx.actor(), id(&channel_id, "channel")?)
        .await
        .map(|items| Json(SlotList { items }))
        .map_err(map_err)
}

/// Replace the weekly grid (the editor saves it whole) and regenerate the schedule that has not aired.
/// Slots may not overlap; 400 names the clash.
#[utoipa::path(
    put, path = "/admin/tv/channels/{channel_id}/slots", operation_id = "admin_tv_put_slots", request_body = SlotList,
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = SlotList), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn put_slots(
    ctx: StaffTv,
    Path(channel_id): Path<String>,
    Json(b): Json<SlotList>,
) -> ApiResult<Json<SlotList>> {
    let cid = id(&channel_id, "channel")?;
    let items = ctx
        .svc()
        .put_slots(ctx.actor(), cid, b.items)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "tv.slots.replace",
        ("channel", cid),
        json!({ "slots": items.len() }),
    )
    .await;
    Ok(Json(SlotList { items }))
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TemplateParams {
    /// `csv` (default) or `json`.
    pub format: Option<String>,
}

/// The guide template to fill in: columns `weekday, start_time, duration, show, season, episode, note`.
#[utoipa::path(
    get, path = "/admin/tv/guide-template", operation_id = "admin_tv_guide_template", params(TemplateParams),
    responses((status = 200, description = "The template file", content_type = "text/csv"),
              (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn template(
    ctx: StaffTv,
    Query(p): Query<TemplateParams>,
) -> Result<Response, HttpError> {
    // The permission check without touching a channel.
    ctx.svc()
        .admin_channels(ctx.actor())
        .await
        .map_err(map_err)?;
    let (body, ctype, name) = match p.format.as_deref().unwrap_or("csv") {
        "csv" => (CSV_TEMPLATE, "text/csv; charset=utf-8", "tv-guide-template.csv"),
        "json" => (
            JSON_TEMPLATE,
            "application/json",
            "tv-guide-template.json",
        ),
        _ => {
            return Err(
                ApiError::with_message(ErrorCode::InvalidFormBody, "format is csv or json").into(),
            );
        }
    };
    let mut r = body.into_response();
    r.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(ctype));
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")) {
        r.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(r)
}

/// Upload a guide (CSV or JSON text). The answer is the mapping report: each row matched, ambiguous (pick a
/// title), missing (will be skipped) or invalid. Nothing changes until it is confirmed.
#[utoipa::path(
    post, path = "/admin/tv/channels/{channel_id}/imports", operation_id = "admin_tv_import_guide", request_body = UploadBody,
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 201, body = ImportView), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn import(
    ctx: StaffTv,
    Path(channel_id): Path<String>,
    Json(b): Json<UploadBody>,
) -> ApiResult<(StatusCode, Json<ImportView>)> {
    let cid = id(&channel_id, "channel")?;
    let v = ctx
        .svc()
        .import_guide(ctx.actor(), cid, &b.format, &b.content)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "tv.guide.import",
        ("channel", cid),
        json!({
            "import": v.id.to_string(), "matched": v.report.matched, "ambiguous": v.report.ambiguous,
            "missing": v.report.missing, "invalid": v.report.invalid
        }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(v)))
}

/// The channel's recent imports, newest first.
#[utoipa::path(
    get, path = "/admin/tv/channels/{channel_id}/imports", operation_id = "admin_tv_imports",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = ImportList), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn imports(ctx: StaffTv, Path(channel_id): Path<String>) -> ApiResult<Json<ImportList>> {
    ctx.svc()
        .guide_imports(ctx.actor(), id(&channel_id, "channel")?)
        .await
        .map(|items| Json(ImportList { items }))
        .map_err(map_err)
}

/// One import with its report.
#[utoipa::path(
    get, path = "/admin/tv/channels/{channel_id}/imports/{import_id}", operation_id = "admin_tv_import",
    params(("channel_id" = String, Path, description = "Channel snowflake"),
           ("import_id" = String, Path, description = "Import snowflake")),
    responses((status = 200, body = ImportView), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_import(
    ctx: StaffTv,
    Path((channel_id, import_id)): Path<(String, String)>,
) -> ApiResult<Json<ImportView>> {
    ctx.svc()
        .guide_import(
            ctx.actor(),
            id(&channel_id, "channel")?,
            id(&import_id, "import")?,
        )
        .await
        .map(Json)
        .map_err(map_err)
}

/// Confirm an import: the report becomes the channel's weekly grid (replacing it). `choices` picks a title
/// for ambiguous rows; unmatched slots stay in the grid and are skipped on air.
#[utoipa::path(
    post, path = "/admin/tv/channels/{channel_id}/imports/{import_id}/confirm", operation_id = "admin_tv_confirm_import",
    request_body = ConfirmInput,
    params(("channel_id" = String, Path, description = "Channel snowflake"),
           ("import_id" = String, Path, description = "Import snowflake")),
    responses((status = 200, body = ConfirmedGuide), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn confirm_import(
    ctx: StaffTv,
    Path((channel_id, import_id)): Path<(String, String)>,
    Json(b): Json<ConfirmInput>,
) -> ApiResult<Json<ConfirmedGuide>> {
    let (cid, iid) = (id(&channel_id, "channel")?, id(&import_id, "import")?);
    let done = ctx
        .svc()
        .confirm_guide(ctx.actor(), cid, iid, b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "tv.guide.confirm",
        ("channel", cid),
        json!({ "import": iid.to_string(), "slots": done.slots, "skipped": done.skipped }),
    )
    .await;
    Ok(Json(done))
}

// ---- favorites -----------------------------------------------------------------------------------

/// The caller's starred channels.
#[utoipa::path(
    get, path = "/tv/favorites", operation_id = "tv_favorites",
    responses((status = 200, body = FavoriteList), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn favorites(ctx: MemberCtx) -> ApiResult<Json<FavoriteList>> {
    ctx.deps
        .service
        .favorites(ctx.viewer)
        .await
        .map(|v| {
            Json(FavoriteList {
                items: v.iter().map(ToString::to_string).collect(),
            })
        })
        .map_err(map_err)
}

/// Star a channel (starring one that is starred is fine).
#[utoipa::path(
    put, path = "/tv/favorites/{channel_id}", operation_id = "tv_favorite_set",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 204, description = "Starred"), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn set_favorite(ctx: MemberCtx, Path(channel_id): Path<String>) -> ApiResult<StatusCode> {
    ctx.deps
        .service
        .set_favorite(ctx.viewer, id(&channel_id, "channel")?, true)
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Unstar a channel (unstarring one that is not starred is fine).
#[utoipa::path(
    delete, path = "/tv/favorites/{channel_id}", operation_id = "tv_favorite_clear",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 204, description = "Unstarred"), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn clear_favorite(
    ctx: MemberCtx,
    Path(channel_id): Path<String>,
) -> ApiResult<StatusCode> {
    ctx.deps
        .service
        .set_favorite(ctx.viewer, id(&channel_id, "channel")?, false)
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}
