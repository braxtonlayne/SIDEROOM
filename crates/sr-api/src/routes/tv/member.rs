//! Member endpoints: the channel list, the guide, now/next, tuning and the live playlists.

use super::{MemberCtx, id, map_err};
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
use sr_tv::live::Track;
use sr_tv::model::Channel;
use sr_tv::service::{Guide, NowView, Tune};
use utoipa::{IntoParams, ToSchema};

const PLAYLIST: &str = "application/vnd.apple.mpegurl";

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = TvChannelList)]
pub struct ChannelList {
    pub items: Vec<Channel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = TvTimezoneList)]
pub struct TimezoneList {
    pub items: Vec<String>,
}

/// Enabled channels by number, each with what is on now and next.
#[utoipa::path(
    get, path = "/tv/channels", operation_id = "tv_channels",
    responses((status = 200, body = ChannelList), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn channels(ctx: MemberCtx) -> ApiResult<Json<ChannelList>> {
    ctx.deps
        .service
        .channels()
        .await
        .map(|items| Json(ChannelList { items }))
        .map_err(map_err)
}

/// The time zones a channel can use.
#[utoipa::path(
    get, path = "/tv/timezones", operation_id = "tv_timezones",
    responses((status = 200, body = TimezoneList), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn timezones(_ctx: MemberCtx) -> Json<TimezoneList> {
    Json(TimezoneList {
        items: sr_tv::tz::zone_names(),
    })
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GuideParams {
    /// Start of the window, Unix milliseconds. Default: now rounded down to the half hour.
    pub from: Option<i64>,
    /// End of the window, Unix milliseconds. Default: `from` plus 4 hours; at most 48 hours.
    pub to: Option<i64>,
    /// Comma-separated channel ids. Default: every enabled channel.
    pub channels: Option<String>,
}

/// The programme guide for a window.
#[utoipa::path(
    get, path = "/tv/guide", operation_id = "tv_guide", params(GuideParams),
    responses((status = 200, body = Guide), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn guide(ctx: MemberCtx, Query(p): Query<GuideParams>) -> ApiResult<Json<Guide>> {
    let ids: Vec<i64> = p
        .channels
        .iter()
        .flat_map(|s| s.split(','))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| id(s, "channel"))
        .collect::<Result<_, _>>()?;
    ctx.deps
        .service
        .guide(p.from, p.to, &ids)
        .await
        .map(Json)
        .map_err(map_err)
}

/// One channel with what is on now and next.
#[utoipa::path(
    get, path = "/tv/channels/{channel_id}", operation_id = "tv_channel",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = Channel), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn channel(ctx: MemberCtx, Path(channel_id): Path<String>) -> ApiResult<Json<Channel>> {
    ctx.deps
        .service
        .channel(id(&channel_id, "channel")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// What is on a channel now, how far into it the wall clock is, and what comes next.
#[utoipa::path(
    get, path = "/tv/channels/{channel_id}/now", operation_id = "tv_channel_now",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = NowView), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn now(ctx: MemberCtx, Path(channel_id): Path<String>) -> ApiResult<Json<NowView>> {
    ctx.deps
        .service
        .now(id(&channel_id, "channel")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Open a channel: what is on and the API path of its live master playlist. A restricted programme on air
/// answers 403 with code 900011 until the account confirmed it is 18 or older (D-184).
#[utoipa::path(
    get, path = "/tv/channels/{channel_id}/tune", operation_id = "tv_channel_tune",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses(
        (status = 200, body = Tune), (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody, description = "Age confirmation required (code 900011)"),
        (status = 404, body = ErrorBody)
    ),
    security(("session" = []))
)]
pub async fn tune(ctx: MemberCtx, Path(channel_id): Path<String>) -> ApiResult<Json<Tune>> {
    ctx.deps
        .service
        .tune(ctx.viewer, id(&channel_id, "channel")?)
        .await
        .map(Json)
        .map_err(map_err)
}

fn playlist(body: String) -> Response {
    let mut r = (StatusCode::OK, body).into_response();
    let h = r.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(PLAYLIST));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

/// The live master playlist: the rungs every upcoming programme has, one audio group.
#[utoipa::path(
    get, path = "/tv/channels/{channel_id}/live/master.m3u8", operation_id = "tv_live_master",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses(
        (status = 200, content_type = "application/vnd.apple.mpegurl", body = String),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody, description = "Age confirmation required (code 900011)"),
        (status = 404, body = ErrorBody, description = "Unknown channel, or off air")
    ),
    security(("session" = []))
)]
pub async fn live_master(
    ctx: MemberCtx,
    Path(channel_id): Path<String>,
) -> Result<Response, HttpError> {
    ctx.deps
        .service
        .live_master(ctx.viewer, id(&channel_id, "channel")?)
        .await
        .map(playlist)
        .map_err(map_err)
}

/// The live audio playlist (the default track of each programme).
#[utoipa::path(
    get, path = "/tv/channels/{channel_id}/live/audio.m3u8", operation_id = "tv_live_audio",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses(
        (status = 200, content_type = "application/vnd.apple.mpegurl", body = String),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody, description = "Age confirmation required (code 900011)"),
        (status = 404, body = ErrorBody, description = "Unknown channel, or off air")
    ),
    security(("session" = []))
)]
pub async fn live_audio(
    ctx: MemberCtx,
    Path(channel_id): Path<String>,
) -> Result<Response, HttpError> {
    ctx.deps
        .service
        .live_media(ctx.viewer, id(&channel_id, "channel")?, Track::Audio)
        .await
        .map(playlist)
        .map_err(map_err)
}

/// The live video playlist of a rung: a sliding window of the segments on air. `file` is `{height}.m3u8`.
/// Fetching it counts the member as a viewer of the channel.
#[utoipa::path(
    get, path = "/tv/channels/{channel_id}/live/v/{file}", operation_id = "tv_live_video",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("file" = String, Path, description = "`{height}.m3u8`, for example `720.m3u8`")
    ),
    responses(
        (status = 200, content_type = "application/vnd.apple.mpegurl", body = String),
        (status = 400, body = ErrorBody),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody, description = "Age confirmation required (code 900011)"),
        (status = 404, body = ErrorBody, description = "Unknown channel, or off air")
    ),
    security(("session" = []))
)]
pub async fn live_video(
    ctx: MemberCtx,
    Path((channel_id, file)): Path<(String, String)>,
) -> Result<Response, HttpError> {
    let height: u32 = file
        .strip_suffix(".m3u8")
        .and_then(|h| h.parse().ok())
        .filter(|h| (1..=8640).contains(h))
        .ok_or_else(|| super::not_found("playlist"))?;
    ctx.deps
        .service
        .live_media(
            ctx.viewer,
            id(&channel_id, "channel")?,
            Track::Video(height),
        )
        .await
        .map(playlist)
        .map_err(map_err)
}
