//! Share a timer with a voice channel (UTL-DEV-001). Thin translation over [`VoiceBackend::voice_timer_set`];
//! the rules (who may, 500-character status line) are the status line's, and the state is the gateway's.

use crate::routes::Ext;
use crate::routes::guilds::util::{actor, id};
use crate::routes::voice::VoiceApi;
use crate::{Principal, error::ApiResult, openapi::ErrorBody};
use axum::{Json, extract::Path, http::StatusCode};
use serde::Deserialize;
use sr_core::ChannelId;
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
pub struct VoiceTimerBody {
    /// The timer to share: `{ kind: pomodoro|stopwatch|countdown, phase: running|paused, segment, startedMs,
    /// accumulatedMs, durationMs, completed }`. `null` stops sharing.
    #[schema(value_type = Option<Object>)]
    pub timer: Option<serde_json::Value>,
    /// The status line that goes with the timer ("Focus 24:12"); required when `timer` is set.
    #[serde(default)]
    pub label: String,
}

/// Share (or stop sharing) a timer with a voice channel. Needs `SET_VOICE_CHANNEL_STATUS` and a seat in the
/// channel, or `MANAGE_CHANNELS`, exactly like the status line it also sets. Announced as `VOICE_TIMER_UPDATE`.
#[utoipa::path(
    put,
    path = "/channels/{channel_id}/voice-timer",
    params(("channel_id" = String, Path, description = "Voice or stage channel snowflake")),
    request_body = VoiceTimerBody,
    responses(
        (status = 204, description = "Applied"),
        (status = 400, description = "Not a voice channel, a malformed timer, or a label over 500 characters", body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn set(
    p: Principal,
    Ext(api): Ext<VoiceApi>,
    Path(channel_id): Path<String>,
    Json(body): Json<VoiceTimerBody>,
) -> ApiResult<StatusCode> {
    let me = actor(&p)?;
    let channel: ChannelId = id("channel_id", &channel_id)?;
    api.0
        .voice_timer_set(me, channel, body.timer, body.label)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
