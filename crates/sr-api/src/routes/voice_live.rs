//! Live voice REST: stage instances (VOX-STG-001), stream previews (VOX-SIG-010) and DM
//! call ringing (VOX-SIG-008). Thin translation over [`VoiceBackend`]; the state lives in
//! the gateway.
//!
//! * `POST /stage-instances`, `GET|PATCH|DELETE /stage-instances/{channel_id}`,
//!   `GET /guilds/{guild_id}/stage-instances` (SIDE ROOM list, Discord has none).
//! * `POST|GET /streams/{stream_key}/preview`: the streamer's thumbnail.
//! * `GET /channels/{id}/call`, `POST /channels/{id}/call/ring`,
//!   `POST /channels/{id}/call/stop-ringing`.

use crate::routes::Ext;
use crate::routes::guilds::util::{actor, id};
use crate::routes::settings::expressions::{decode_data_uri, sniff};
use crate::routes::voice::{StageInstanceObject, VoiceApi, VoiceError};
use crate::{HttpError, Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{
    Json, Router,
    extract::Path,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use sr_core::{ApiError, ChannelId, GuildId, UserId};
use utoipa::{OpenApi, ToSchema};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/stage-instances", post(create_stage_instance))
        .route(
            "/stage-instances/{channel_id}",
            get(get_stage_instance)
                .patch(modify_stage_instance)
                .delete(delete_stage_instance),
        )
        .route(
            "/guilds/{guild_id}/stage-instances",
            get(list_stage_instances),
        )
        .route(
            "/streams/{stream_key}/preview",
            post(upload_stream_preview).get(get_stream_preview),
        )
        .route("/channels/{channel_id}/call", get(get_call))
        .route("/channels/{channel_id}/call/ring", post(ring_call))
        .route(
            "/channels/{channel_id}/call/stop-ringing",
            post(stop_ringing_call),
        )
}

// ---- stage instances -----------------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateStageInstanceBody {
    /// The stage channel.
    pub channel_id: String,
    /// 1 to 120 characters.
    pub topic: String,
    /// 2 (`GUILD_ONLY`), the default and the only level in use.
    pub privacy_level: Option<u8>,
    /// Accepted for Discord compatibility; stages do not notify `@everyone` here.
    pub send_start_notification: Option<bool>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ModifyStageInstanceBody {
    pub topic: Option<String>,
    pub privacy_level: Option<u8>,
}

/// Start a stage in a stage channel. Needs Manage Channels, Mute Members and Move Members.
/// Announced as `STAGE_INSTANCE_CREATE`.
#[utoipa::path(
    post,
    path = "/stage-instances",
    request_body = CreateStageInstanceBody,
    responses(
        (status = 200, body = StageInstanceObject),
        (status = 400, description = "Bad topic, not a stage channel, or a stage is already open (150006)", body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create_stage_instance(
    p: Principal,
    Ext(api): Ext<VoiceApi>,
    Json(body): Json<CreateStageInstanceBody>,
) -> ApiResult<Json<StageInstanceObject>> {
    let me = actor(&p)?;
    let channel: ChannelId = id("channel_id", &body.channel_id)?;
    Ok(Json(
        api.0
            .stage_create(me, channel, body.topic, body.privacy_level)
            .await?,
    ))
}

/// The stage running in a channel (members who can see it).
#[utoipa::path(
    get,
    path = "/stage-instances/{channel_id}",
    params(("channel_id" = String, Path, description = "Stage channel snowflake")),
    responses((status = 200, body = StageInstanceObject), (status = 404, description = "No stage is running (10067)", body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_stage_instance(
    p: Principal,
    Ext(api): Ext<VoiceApi>,
    Path(channel_id): Path<String>,
) -> ApiResult<Json<StageInstanceObject>> {
    let me = actor(&p)?;
    let channel: ChannelId = id("channel_id", &channel_id)?;
    Ok(Json(api.0.stage_get(me, channel).await?))
}

/// The stages running in a server, for the channels the caller can see.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/stage-instances",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<StageInstanceObject>), (status = 403, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_stage_instances(
    p: Principal,
    Ext(api): Ext<VoiceApi>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<Vec<StageInstanceObject>>> {
    let me = actor(&p)?;
    let guild: GuildId = id("guild_id", &guild_id)?;
    Ok(Json(api.0.stage_list(me, guild).await?))
}

/// Change a stage's topic (moderators). Announced as `STAGE_INSTANCE_UPDATE`.
#[utoipa::path(
    patch,
    path = "/stage-instances/{channel_id}",
    params(("channel_id" = String, Path, description = "Stage channel snowflake")),
    request_body = ModifyStageInstanceBody,
    responses((status = 200, body = StageInstanceObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn modify_stage_instance(
    p: Principal,
    Ext(api): Ext<VoiceApi>,
    Path(channel_id): Path<String>,
    Json(body): Json<ModifyStageInstanceBody>,
) -> ApiResult<Json<StageInstanceObject>> {
    let me = actor(&p)?;
    let channel: ChannelId = id("channel_id", &channel_id)?;
    if body.topic.is_none() && body.privacy_level.is_none() {
        return Err(HttpError(ApiError::invalid_form().field(
            "topic",
            "BASE_TYPE_REQUIRED",
            "Provide a topic or a privacy level.",
        )));
    }
    Ok(Json(
        api.0
            .stage_update(me, channel, body.topic, body.privacy_level)
            .await?,
    ))
}

/// End a stage (moderators). Announced as `STAGE_INSTANCE_DELETE`; speakers go back to
/// the audience.
#[utoipa::path(
    delete,
    path = "/stage-instances/{channel_id}",
    params(("channel_id" = String, Path, description = "Stage channel snowflake")),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_stage_instance(
    p: Principal,
    Ext(api): Ext<VoiceApi>,
    Path(channel_id): Path<String>,
) -> ApiResult<StatusCode> {
    let me = actor(&p)?;
    let channel: ChannelId = id("channel_id", &channel_id)?;
    api.0.stage_end(me, channel).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- stream previews -------------------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
pub struct StreamPreviewBody {
    /// `data:image/jpeg;base64,...`, at most 256 KiB once decoded.
    pub thumbnail: String,
}

/// The streamer sends a fresh thumbnail (every few minutes). At most one upload per
/// five seconds.
#[utoipa::path(
    post,
    path = "/streams/{stream_key}/preview",
    params(("stream_key" = String, Path, description = "`guild:{guild}:{channel}:{user}`")),
    request_body = StreamPreviewBody,
    responses(
        (status = 204),
        (status = 400, description = "Not a JPEG", body = ErrorBody),
        (status = 404, description = "Not your stream", body = ErrorBody),
        (status = 413, body = ErrorBody),
        (status = 429, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn upload_stream_preview(
    p: Principal,
    Ext(api): Ext<VoiceApi>,
    Path(stream_key): Path<String>,
    Json(body): Json<StreamPreviewBody>,
) -> ApiResult<StatusCode> {
    let me = actor(&p)?;
    let data = decode_data_uri("thumbnail", &body.thumbnail)?;
    if !matches!(
        sniff(&data),
        Some(crate::routes::settings::expressions::Image::Jpeg)
    ) {
        return Err(VoiceError::NotJpeg.into());
    }
    api.0
        .stream_preview_put(me, &stream_key, data.to_vec())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The newest thumbnail of a stream (`image/jpeg`), for members who can see the channel.
#[utoipa::path(
    get,
    path = "/streams/{stream_key}/preview",
    params(("stream_key" = String, Path, description = "`guild:{guild}:{channel}:{user}`")),
    responses(
        (status = 200, content_type = "image/jpeg", body = Vec<u8>),
        (status = 304),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_stream_preview(
    p: Principal,
    Ext(api): Ext<VoiceApi>,
    headers: HeaderMap,
    Path(stream_key): Path<String>,
) -> ApiResult<Response> {
    let me = actor(&p)?;
    let pv = api.0.stream_preview_get(me, &stream_key).await?;
    let etag = format!("\"{}\"", pv.uploaded_ms);
    let mut resp = if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        == Some(etag.as_str())
    {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        (
            [(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"))],
            pv.jpeg,
        )
            .into_response()
    };
    if let Ok(v) = HeaderValue::from_str(&etag) {
        resp.headers_mut().insert(header::ETAG, v);
    }
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-cache"),
    );
    Ok(resp)
}

// ---- DM calls ---------------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct CallRecipientsBody {
    /// Users to ring or stop ringing; absent means everyone who is not connected (ring) or
    /// everyone still ringing / yourself (stop).
    pub recipients: Option<Vec<String>>,
}

fn recipients(body: Option<Json<CallRecipientsBody>>) -> ApiResult<Option<Vec<UserId>>> {
    let Some(Json(b)) = body else {
        return Ok(None);
    };
    b.recipients
        .map(|r| {
            r.iter()
                .map(|u| id::<UserId>("recipients", u))
                .collect::<ApiResult<Vec<_>>>()
        })
        .transpose()
}

/// The call running in a DM or group DM (`CALL_CREATE` shape: who is ringing, who is in it).
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/call",
    params(("channel_id" = String, Path, description = "DM channel snowflake")),
    responses((status = 200, description = "`{channel_id, message_id, region, ringing, voice_states}`"), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_call(
    p: Principal,
    Ext(api): Ext<VoiceApi>,
    Path(channel_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let me = actor(&p)?;
    let channel: ChannelId = id("channel_id", &channel_id)?;
    Ok(Json(api.0.call_get(me, channel).await?))
}

/// Ring members of a DM who are not in the call. The caller must be in the call. Announced
/// as `CALL_UPDATE`.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/call/ring",
    params(("channel_id" = String, Path, description = "DM channel snowflake")),
    request_body(content = CallRecipientsBody, description = "Optional"),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn ring_call(
    p: Principal,
    Ext(api): Ext<VoiceApi>,
    Path(channel_id): Path<String>,
    body: Option<Json<CallRecipientsBody>>,
) -> ApiResult<StatusCode> {
    let me = actor(&p)?;
    let channel: ChannelId = id("channel_id", &channel_id)?;
    api.0.call_ring(me, channel, recipients(body)?).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Decline an incoming call (no body), or stop ringing others from inside the call.
/// Announced as `CALL_UPDATE`.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/call/stop-ringing",
    params(("channel_id" = String, Path, description = "DM channel snowflake")),
    request_body(content = CallRecipientsBody, description = "Optional"),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn stop_ringing_call(
    p: Principal,
    Ext(api): Ext<VoiceApi>,
    Path(channel_id): Path<String>,
    body: Option<Json<CallRecipientsBody>>,
) -> ApiResult<StatusCode> {
    let me = actor(&p)?;
    let channel: ChannelId = id("channel_id", &channel_id)?;
    api.0
        .call_stop_ringing(me, channel, recipients(body)?)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// OpenAPI fragment; merged by [`super::voice::openapi`].
#[derive(OpenApi)]
#[openapi(
    paths(
        create_stage_instance,
        get_stage_instance,
        list_stage_instances,
        modify_stage_instance,
        delete_stage_instance,
        upload_stream_preview,
        get_stream_preview,
        get_call,
        ring_call,
        stop_ringing_call
    ),
    components(schemas(
        CreateStageInstanceBody,
        ModifyStageInstanceBody,
        StageInstanceObject,
        StreamPreviewBody,
        CallRecipientsBody
    ))
)]
pub struct VoiceLiveDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    VoiceLiveDoc::openapi()
}

#[cfg(test)]
#[path = "voice_live_tests.rs"]
mod tests;
