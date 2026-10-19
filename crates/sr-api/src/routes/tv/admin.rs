//! Staff endpoints (`/admin/tv/*`, `MANAGE_TV`): channels, schedule preview and publish, manual placement,
//! "on now" control and the report of what aired. Every successful change is written to the access log.

use super::{StaffTv, id, map_err};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sr_tv::model::{Block, Channel, ChannelInput};
use sr_tv::service::{NowView, PlaceInput, Preview, Published, Report, SetNowInput, SlateStatus};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = TvAdminChannelList)]
pub struct ChannelListBody {
    pub items: Vec<Channel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = TvBlockList)]
pub struct BlockList {
    pub items: Vec<Block>,
}

/// All channels with their rules, disabled ones too.
#[utoipa::path(
    get, path = "/admin/tv/channels", operation_id = "admin_tv_channels",
    responses((status = 200, body = ChannelListBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn channels(ctx: StaffTv) -> ApiResult<Json<ChannelListBody>> {
    ctx.svc()
        .admin_channels(ctx.actor())
        .await
        .map(|items| Json(ChannelListBody { items }))
        .map_err(map_err)
}

/// Create a channel (`MANAGE_TV`). A taken number is 409.
#[utoipa::path(
    post, path = "/admin/tv/channels", operation_id = "admin_tv_create_channel", request_body = ChannelInput,
    responses((status = 201, body = Channel), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_channel(
    ctx: StaffTv,
    Json(b): Json<ChannelInput>,
) -> ApiResult<(StatusCode, Json<Channel>)> {
    let c = ctx
        .svc()
        .create_channel(ctx.actor(), b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "tv.channel.create",
        ("channel", c.id),
        json!({ "number": c.number, "name": c.name }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(c)))
}

/// A channel with its rules.
#[utoipa::path(
    get, path = "/admin/tv/channels/{channel_id}", operation_id = "admin_tv_channel",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = Channel), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn channel(ctx: StaffTv, Path(channel_id): Path<String>) -> ApiResult<Json<Channel>> {
    ctx.svc()
        .admin_channel(ctx.actor(), id(&channel_id, "channel")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Replace the editable fields of a channel.
#[utoipa::path(
    put, path = "/admin/tv/channels/{channel_id}", operation_id = "admin_tv_update_channel", request_body = ChannelInput,
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = Channel), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn update_channel(
    ctx: StaffTv,
    Path(channel_id): Path<String>,
    Json(b): Json<ChannelInput>,
) -> ApiResult<Json<Channel>> {
    let cid = id(&channel_id, "channel")?;
    let c = ctx
        .svc()
        .update_channel(ctx.actor(), cid, b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "tv.channel.update",
        ("channel", cid),
        json!({ "number": c.number, "name": c.name, "enabled": c.enabled }),
    )
    .await;
    Ok(Json(c))
}

/// Delete a channel with its schedule.
#[utoipa::path(
    delete, path = "/admin/tv/channels/{channel_id}", operation_id = "admin_tv_delete_channel",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_channel(ctx: StaffTv, Path(channel_id): Path<String>) -> ApiResult<StatusCode> {
    let cid = id(&channel_id, "channel")?;
    let (number, name) = ctx
        .svc()
        .delete_channel(ctx.actor(), cid)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "tv.channel.delete",
        ("channel", cid),
        json!({ "number": number, "name": name }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct WindowParams {
    /// Unix milliseconds; default now.
    pub from: Option<i64>,
    /// Unix milliseconds; default `from` plus 24 hours; at most 7 days.
    pub to: Option<i64>,
}

/// Stored blocks of a channel overlapping a window (no lazy extension).
#[utoipa::path(
    get, path = "/admin/tv/channels/{channel_id}/schedule", operation_id = "admin_tv_schedule",
    params(("channel_id" = String, Path, description = "Channel snowflake"), WindowParams),
    responses((status = 200, body = BlockList), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn schedule(
    ctx: StaffTv,
    Path(channel_id): Path<String>,
    Query(p): Query<WindowParams>,
) -> ApiResult<Json<BlockList>> {
    ctx.svc()
        .admin_schedule(ctx.actor(), id(&channel_id, "channel")?, p.from, p.to)
        .await
        .map(|items| Json(BlockList { items }))
        .map_err(map_err)
}

#[derive(Debug, Default, Deserialize, ToSchema)]
#[schema(as = TvPreviewBody)]
pub struct PreviewBody {
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
}

/// Generate a schedule from the current rules without storing anything.
#[utoipa::path(
    post, path = "/admin/tv/channels/{channel_id}/preview", operation_id = "admin_tv_preview", request_body = PreviewBody,
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = Preview), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn preview(
    ctx: StaffTv,
    Path(channel_id): Path<String>,
    body: Option<Json<PreviewBody>>,
) -> ApiResult<Json<Preview>> {
    let b = body.map(|j| j.0).unwrap_or_default();
    ctx.svc()
        .preview(ctx.actor(), id(&channel_id, "channel")?, b.from_ms, b.to_ms)
        .await
        .map(Json)
        .map_err(map_err)
}

#[derive(Debug, Default, Deserialize, ToSchema)]
#[schema(as = TvPublishBody)]
pub struct PublishBody {
    /// 1 to 336, default 168 (one week).
    pub horizon_hours: Option<u32>,
}

/// Regenerate the part of the schedule that has not started from the current rules and store it.
#[utoipa::path(
    post, path = "/admin/tv/channels/{channel_id}/publish", operation_id = "admin_tv_publish", request_body = PublishBody,
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = Published), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn publish(
    ctx: StaffTv,
    Path(channel_id): Path<String>,
    body: Option<Json<PublishBody>>,
) -> ApiResult<Json<Published>> {
    let cid = id(&channel_id, "channel")?;
    let b = body.map(|j| j.0).unwrap_or_default();
    let r = ctx
        .svc()
        .publish(ctx.actor(), cid, b.horizon_hours)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "tv.schedule.publish",
        ("channel", cid),
        json!({ "from_ms": r.from_ms, "to_ms": r.to_ms, "written": r.written, "removed": r.removed }),
    )
    .await;
    Ok(Json(r))
}

/// Place a programme at a time; what it overlaps is trimmed, split or removed.
#[utoipa::path(
    post, path = "/admin/tv/channels/{channel_id}/blocks", operation_id = "admin_tv_place_block", request_body = PlaceInput,
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 201, body = Block), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn place_block(
    ctx: StaffTv,
    Path(channel_id): Path<String>,
    Json(b): Json<PlaceInput>,
) -> ApiResult<(StatusCode, Json<Block>)> {
    let cid = id(&channel_id, "channel")?;
    let block = ctx
        .svc()
        .place_block(ctx.actor(), cid, b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "tv.block.place",
        ("channel", cid),
        json!({ "block": block.id.to_string(), "title": block.title,
                "start_ms": block.start_ms, "end_ms": block.end_ms }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(block)))
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AtParams {
    /// Unix milliseconds; default now ("simulate now").
    pub at_ms: Option<i64>,
}

/// What is on at any moment, from the stored blocks.
#[utoipa::path(
    get, path = "/admin/tv/channels/{channel_id}/now", operation_id = "admin_tv_now",
    params(("channel_id" = String, Path, description = "Channel snowflake"), AtParams),
    responses((status = 200, body = NowView), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn now(
    ctx: StaffTv,
    Path(channel_id): Path<String>,
    Query(p): Query<AtParams>,
) -> ApiResult<Json<NowView>> {
    ctx.svc()
        .admin_now(ctx.actor(), id(&channel_id, "channel")?, p.at_ms)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Put a programme on the air now, cutting the block on air.
#[utoipa::path(
    put, path = "/admin/tv/channels/{channel_id}/now", operation_id = "admin_tv_set_now", request_body = SetNowInput,
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = Block), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn set_now(
    ctx: StaffTv,
    Path(channel_id): Path<String>,
    Json(b): Json<SetNowInput>,
) -> ApiResult<Json<Block>> {
    let cid = id(&channel_id, "channel")?;
    let block = ctx
        .svc()
        .set_now(ctx.actor(), cid, b)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "tv.now.set",
        ("channel", cid),
        json!({ "block": block.id.to_string(), "title": block.title }),
    )
    .await;
    Ok(Json(block))
}

/// Remove one block from a schedule.
#[utoipa::path(
    delete, path = "/admin/tv/blocks/{block_id}", operation_id = "admin_tv_remove_block",
    params(("block_id" = String, Path, description = "Block snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn remove_block(ctx: StaffTv, Path(block_id): Path<String>) -> ApiResult<StatusCode> {
    let bid = id(&block_id, "block")?;
    let b = ctx
        .svc()
        .remove_block(ctx.actor(), bid)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "tv.block.remove",
        ("channel", b.channel_id),
        json!({ "block": bid.to_string(), "title": b.title, "start_ms": b.start_ms }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReportParams {
    /// Unix milliseconds; default 24 hours before `to`.
    pub from: Option<i64>,
    /// Unix milliseconds; default now (never later).
    pub to: Option<i64>,
    /// One channel id; default all.
    pub channel: Option<String>,
}

/// What aired in a window (at most 31 days).
#[utoipa::path(
    get, path = "/admin/tv/report", operation_id = "admin_tv_report", params(ReportParams),
    responses((status = 200, body = Report), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn report(ctx: StaffTv, Query(p): Query<ReportParams>) -> ApiResult<Json<Report>> {
    let channel = p.channel.as_deref().map(|c| id(c, "channel")).transpose()?;
    ctx.svc()
        .report(ctx.actor(), p.from, p.to, channel)
        .await
        .map(Json)
        .map_err(map_err)
}

/// The off-air slate: whether one is configured (platform setting `tv.slate_asset`, made by `sr-cli tv slate`,
/// D-269), its asset id and whether it plays.
#[utoipa::path(
    get, path = "/admin/tv/slate", operation_id = "admin_tv_slate",
    responses((status = 200, body = SlateStatus), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn slate(ctx: StaffTv) -> ApiResult<Json<SlateStatus>> {
    ctx.svc()
        .slate_status(ctx.actor())
        .await
        .map(Json)
        .map_err(map_err)
}
