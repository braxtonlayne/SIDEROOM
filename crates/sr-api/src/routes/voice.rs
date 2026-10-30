//! Voice REST API: coturn credentials and moderator voice-state changes
//! (VOX-SIG-006, VOX-SIG-007). The state itself lives in the gateway; this module only
//! translates HTTP, so the composition root registers a [`VoiceApi`] that wraps it.
//!
//! * `GET /sr/voice/turn`: ephemeral TURN credentials (ADR-0004).
//! * `PATCH /guilds/{guild_id}/voice-states/{user_id}`: server mute/deafen, move, disconnect.
//!
//! Wire details: docs/05-api-and-gateway.md section 6.5.

pub mod jukebox;
pub mod rooms;

use crate::routes::Ext;
use crate::routes::guilds::dto::nullable;
use crate::routes::guilds::util::{actor, id};
use crate::{HttpError, Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::Path,
    http::{HeaderMap, StatusCode},
    routing::{get, patch},
};
use serde::{Deserialize, Serialize};
use sr_core::{ApiError, ChannelId, ErrorCode, GuildId, UserId};
use std::sync::Arc;
use utoipa::{OpenApi, ToSchema};

/// A moderator's requested change.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VoicePatch {
    pub mute: Option<bool>,
    pub deaf: Option<bool>,
    /// `Some(None)` disconnects, `Some(Some(c))` moves.
    pub channel: Option<Option<ChannelId>>,
    /// Stage: `false` invites the member to speak, `true` sends them back to the audience.
    pub suppress: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceError {
    NotInVoice,
    MissingPermissions,
    UnknownChannel,
    NotVoiceChannel,
    /// The channel is not a stage channel.
    NotStageChannel,
    /// No stage is running in the channel.
    NoStageInstance,
    /// A stage is already running in the channel.
    StageExists,
    InvalidTopic,
    InvalidPrivacy,
    /// A voice channel status longer than 500 characters.
    InvalidStatus,
    /// The stream, call or preview does not exist, or is not yours to see.
    NotFound,
    NotJpeg,
    TooLarge,
    RateLimited,
    /// The caller is not connected to the call.
    NotInCall,
    /// A named recipient is not in the DM.
    NotAMember,
    /// Muted, deafened, in the stage audience or in the AFK channel: nobody would hear it.
    Silenced,
    /// Another sound was played a moment ago.
    Cooldown,
    /// Voice is not enabled on this installation.
    Unavailable,
}

/// TURN credentials, coturn REST style.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct TurnCredentialsObject {
    pub username: String,
    pub password: String,
    /// Seconds until the credentials expire.
    pub ttl: u64,
    pub uris: Vec<String>,
}

/// A stage instance, in Discord's shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct StageInstanceObject {
    pub id: String,
    pub guild_id: String,
    pub channel_id: String,
    pub topic: String,
    /// 2 = `GUILD_ONLY`, the only level in use.
    pub privacy_level: u8,
    pub discoverable_disabled: bool,
    pub guild_scheduled_event_id: Option<String>,
}

/// What a member changes about their own stage state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelfStageRequest {
    /// `false` to become a speaker (needs Mute Members), `true` to step down.
    pub suppress: Option<bool>,
    /// `Some(true)` raises a hand, `Some(false)` lowers it.
    pub request_to_speak: Option<bool>,
}

/// A soundboard sound being played in a voice channel.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundEffect {
    pub sound_id: String,
    /// 0 to 1.
    pub volume: f64,
    pub emoji_id: Option<String>,
    pub emoji_name: Option<String>,
}

/// The newest preview thumbnail of a stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamPreview {
    pub jpeg: Vec<u8>,
    /// Unix milliseconds of the upload; doubles as the ETag.
    pub uploaded_ms: i64,
}

/// What the routes need from the voice system.
///
/// The stage, stream and call methods default to "voice is off" so a backend (or a test
/// double) only implements what it serves.
#[async_trait]
pub trait VoiceBackend: Send + Sync {
    /// Credentials for `user`; `None` when TURN is not configured.
    fn turn_credentials(&self, user: UserId) -> Option<TurnCredentialsObject>;

    /// Apply `patch` to `target`'s voice state as `actor`: permission checks, the change,
    /// the dispatch and the audit log entry.
    async fn moderate(
        &self,
        actor: UserId,
        guild: GuildId,
        target: UserId,
        patch: VoicePatch,
        reason: Option<String>,
    ) -> Result<(), VoiceError>;

    /// Start a stage in `channel` (moderators).
    async fn stage_create(
        &self,
        _actor: UserId,
        _channel: ChannelId,
        _topic: String,
        _privacy_level: Option<u8>,
    ) -> Result<StageInstanceObject, VoiceError> {
        Err(VoiceError::Unavailable)
    }

    /// The stage running in `channel`, for members who can see it.
    async fn stage_get(
        &self,
        _actor: UserId,
        _channel: ChannelId,
    ) -> Result<StageInstanceObject, VoiceError> {
        Err(VoiceError::Unavailable)
    }

    /// Stages running in `guild` that `actor` can see.
    async fn stage_list(
        &self,
        _actor: UserId,
        _guild: GuildId,
    ) -> Result<Vec<StageInstanceObject>, VoiceError> {
        Err(VoiceError::Unavailable)
    }

    async fn stage_update(
        &self,
        _actor: UserId,
        _channel: ChannelId,
        _topic: Option<String>,
        _privacy_level: Option<u8>,
    ) -> Result<StageInstanceObject, VoiceError> {
        Err(VoiceError::Unavailable)
    }

    async fn stage_end(&self, _actor: UserId, _channel: ChannelId) -> Result<(), VoiceError> {
        Err(VoiceError::Unavailable)
    }

    /// Set (`Some`) or clear (`None` or empty) the status line of a voice or stage channel.
    async fn voice_status_set(
        &self,
        _actor: UserId,
        _channel: ChannelId,
        _status: Option<String>,
    ) -> Result<(), VoiceError> {
        Err(VoiceError::Unavailable)
    }

    /// Raise or lower a hand, or step down from the stage (`PATCH .../voice-states/@me`).
    async fn stage_self(
        &self,
        _actor: UserId,
        _guild: GuildId,
        _channel: ChannelId,
        _req: SelfStageRequest,
    ) -> Result<(), VoiceError> {
        Err(VoiceError::Unavailable)
    }

    /// The streamer replaces the stream's preview thumbnail.
    async fn stream_preview_put(
        &self,
        _actor: UserId,
        _stream_key: &str,
        _jpeg: Vec<u8>,
    ) -> Result<(), VoiceError> {
        Err(VoiceError::Unavailable)
    }

    async fn stream_preview_get(
        &self,
        _actor: UserId,
        _stream_key: &str,
    ) -> Result<StreamPreview, VoiceError> {
        Err(VoiceError::Unavailable)
    }

    /// The running call of a DM, as `CALL_CREATE` shows it.
    async fn call_get(
        &self,
        _actor: UserId,
        _channel: ChannelId,
    ) -> Result<serde_json::Value, VoiceError> {
        Err(VoiceError::Unavailable)
    }

    async fn call_ring(
        &self,
        _actor: UserId,
        _channel: ChannelId,
        _recipients: Option<Vec<UserId>>,
    ) -> Result<(), VoiceError> {
        Err(VoiceError::Unavailable)
    }

    /// Announce that `actor` played a sound in `channel` (they must be in it and audible;
    /// at most one play per few seconds).
    async fn soundboard_play(
        &self,
        _actor: UserId,
        _guild: GuildId,
        _channel: ChannelId,
        _effect: SoundEffect,
    ) -> Result<(), VoiceError> {
        Err(VoiceError::Unavailable)
    }

    async fn call_stop_ringing(
        &self,
        _actor: UserId,
        _channel: ChannelId,
        _recipients: Option<Vec<UserId>>,
    ) -> Result<(), VoiceError> {
        Err(VoiceError::Unavailable)
    }
}

/// Registered on the composition root with `AppStateBuilder::extension`.
#[derive(Clone)]
pub struct VoiceApi(pub Arc<dyn VoiceBackend>);

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sr/voice/turn", get(turn_credentials))
        .route(
            "/guilds/{guild_id}/voice-states/{user_id}",
            patch(modify_voice_state),
        )
        .merge(super::voice_live::router())
        .merge(jukebox::router())
        .merge(rooms::router())
}

impl From<VoiceError> for HttpError {
    fn from(e: VoiceError) -> Self {
        HttpError(match e {
            VoiceError::NotInVoice => ApiError::new(ErrorCode::TargetNotInVoice),
            VoiceError::MissingPermissions => ApiError::new(ErrorCode::MissingPermissions),
            VoiceError::UnknownChannel => ApiError::new(ErrorCode::UnknownChannel),
            VoiceError::NotVoiceChannel => ApiError::invalid_form().field(
                "channel_id",
                "CHANNEL_TYPE_INVALID",
                "Must be a voice channel.",
            ),
            VoiceError::NotStageChannel => ApiError::invalid_form().field(
                "channel_id",
                "CHANNEL_TYPE_INVALID",
                "Must be a stage channel.",
            ),
            VoiceError::NoStageInstance => ApiError::new(ErrorCode::UnknownStageInstance),
            VoiceError::StageExists => ApiError::new(ErrorCode::StageAlreadyOpen),
            VoiceError::InvalidTopic => ApiError::invalid_form().field(
                "topic",
                "BASE_TYPE_BAD_LENGTH",
                "Must be between 1 and 120 characters.",
            ),
            VoiceError::InvalidStatus => ApiError::invalid_form().field(
                "status",
                "BASE_TYPE_BAD_LENGTH",
                "Must be at most 500 characters.",
            ),
            VoiceError::InvalidPrivacy => ApiError::invalid_form().field(
                "privacy_level",
                "ENUM_TYPE_COERCE",
                "Must be 2 (GUILD_ONLY).",
            ),
            VoiceError::NotFound => ApiError::new(ErrorCode::SrNotFound),
            VoiceError::NotJpeg => ApiError::invalid_form().field(
                "thumbnail",
                "IMAGE_INVALID",
                "Must be a JPEG image.",
            ),
            VoiceError::TooLarge => ApiError::new(ErrorCode::RequestEntityTooLarge),
            VoiceError::RateLimited => ApiError::new(ErrorCode::TooManyRequests),
            VoiceError::NotInCall => ApiError::new(ErrorCode::SrConflict),
            VoiceError::Silenced => ApiError::with_message(
                ErrorCode::SrConflict,
                "You cannot play sounds while muted, deafened or in the audience.",
            ),
            VoiceError::Cooldown => ApiError::new(ErrorCode::SrSoundboardCooldown),
            VoiceError::NotAMember => ApiError::invalid_form().field(
                "recipients",
                "BASE_TYPE_INVALID",
                "Every recipient must be a member of this conversation.",
            ),
            VoiceError::Unavailable => ApiError::new(ErrorCode::SrFeatureDisabled),
        })
    }
}

/// Short-lived TURN credentials for the caller (HMAC-SHA1 over `<expiry>:<user id>`,
/// valid for `ttl` seconds). The same credentials are placed in `VOICE_SERVER_UPDATE`.
#[utoipa::path(
    get,
    path = "/sr/voice/turn",
    responses(
        (status = 200, body = TurnCredentialsObject),
        (status = 403, description = "TURN is not configured on this installation", body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn turn_credentials(
    p: Principal,
    Ext(api): Ext<VoiceApi>,
) -> ApiResult<Json<TurnCredentialsObject>> {
    let user = actor(&p)?;
    api.0
        .turn_credentials(user)
        .map(Json)
        .ok_or_else(|| HttpError(ApiError::new(ErrorCode::SrFeatureDisabled)))
}

/// Body of `PATCH /guilds/{id}/voice-states/{user_id}`.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ModifyVoiceStateBody {
    /// A voice channel to move the user to, or `null` to disconnect them.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub channel_id: Option<Option<String>>,
    /// Server mute (`MUTE_MEMBERS`).
    pub mute: Option<bool>,
    /// Server deafen (`DEAFEN_MEMBERS`).
    pub deaf: Option<bool>,
    /// Stage channels: `false` invites the member to speak (`MUTE_MEMBERS`; for `@me`,
    /// the member becomes a speaker, which needs `MUTE_MEMBERS` too), `true` sends them
    /// back to the audience (anyone may do that to themselves).
    pub suppress: Option<bool>,
    /// Stage channels, `@me` only: an ISO 8601 time raises the member's hand (`REQUEST_TO_SPEAK`),
    /// `null` lowers it. The server stamps the time itself; the value only has to be present.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub request_to_speak_timestamp: Option<Option<String>>,
    /// Audit log reason (the `X-Audit-Log-Reason` header wins).
    pub reason: Option<String>,
}

/// Server mute or deafen a member in voice, move them to another voice channel, or
/// disconnect them (`channel_id: null`). `user_id` may be `@me`. Needs `MUTE_MEMBERS`,
/// `DEAFEN_MEMBERS` or `MOVE_MEMBERS` on the user's current voice channel (and
/// `MOVE_MEMBERS` plus `CONNECT` on the destination when moving). The change is announced
/// as `VOICE_STATE_UPDATE` and written to the audit log.
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/voice-states/{user_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("user_id" = String, Path, description = "User snowflake or @me"),
        ("X-Audit-Log-Reason" = Option<String>, Header, description = "Stored as the audit log reason"),
    ),
    request_body = ModifyVoiceStateBody,
    responses(
        (status = 204, description = "Applied (or nothing to change)"),
        (status = 400, description = "Bad body, not a voice channel, or the target is not in voice (40032)", body = ErrorBody),
        (status = 403, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn modify_voice_state(
    p: Principal,
    Ext(api): Ext<VoiceApi>,
    headers: HeaderMap,
    Path((guild_id, user_id)): Path<(String, String)>,
    Json(body): Json<ModifyVoiceStateBody>,
) -> ApiResult<StatusCode> {
    let me = actor(&p)?;
    let guild: GuildId = id("guild_id", &guild_id)?;
    let target: UserId = if user_id == "@me" {
        me
    } else {
        id("user_id", &user_id)?
    };
    if body.channel_id.is_none()
        && body.mute.is_none()
        && body.deaf.is_none()
        && body.suppress.is_none()
        && body.request_to_speak_timestamp.is_none()
    {
        return Err(HttpError(ApiError::invalid_form().field(
            "mute",
            "BASE_TYPE_REQUIRED",
            "Provide at least one of channel_id, mute, deaf, suppress or request_to_speak_timestamp.",
        )));
    }
    let channel = match body.channel_id {
        None => None,
        Some(None) => Some(None),
        Some(Some(c)) => Some(Some(id::<ChannelId>("channel_id", &c)?)),
    };
    // A member changing their own stage state: raise or lower a hand, step down.
    if target == me
        && body.mute.is_none()
        && body.deaf.is_none()
        && (body.request_to_speak_timestamp.is_some() || body.suppress.is_some())
    {
        let Some(Some(channel)) = channel else {
            return Err(HttpError(ApiError::invalid_form().field(
                "channel_id",
                "BASE_TYPE_REQUIRED",
                "The stage channel you are in is required.",
            )));
        };
        api.0
            .stage_self(
                me,
                guild,
                channel,
                SelfStageRequest {
                    suppress: body.suppress,
                    request_to_speak: body.request_to_speak_timestamp.map(|t| t.is_some()),
                },
            )
            .await?;
        return Ok(StatusCode::NO_CONTENT);
    }
    if body.request_to_speak_timestamp.is_some() {
        return Err(HttpError(ApiError::invalid_form().field(
            "request_to_speak_timestamp",
            "BASE_TYPE_INVALID",
            "Only @me can raise a hand.",
        )));
    }
    let reason = headers
        .get("x-audit-log-reason")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .or(body.reason);
    api.0
        .moderate(
            me,
            guild,
            target,
            VoicePatch {
                mute: body.mute,
                deaf: body.deaf,
                channel,
                suppress: body.suppress,
            },
            reason,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// OpenAPI fragment for these routes; merge into [`crate::openapi::base`].
#[derive(OpenApi)]
#[openapi(
    paths(turn_credentials, modify_voice_state),
    components(schemas(ErrorBody, TurnCredentialsObject, ModifyVoiceStateBody))
)]
pub struct VoiceApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    let mut doc = VoiceApiDoc::openapi();
    doc.merge(super::voice_live::openapi());
    doc.merge(jukebox::openapi());
    doc.merge(rooms::openapi());
    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ApiSettings,
        auth::{BotIdentity, memory::MemoryBots},
        build_router,
    };
    use axum::{
        body::Body,
        http::{Method, Request},
    };
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use std::sync::Mutex;
    use tower::ServiceExt;

    type Call = (UserId, GuildId, UserId, VoicePatch, Option<String>);

    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<Call>>,
        result: Mutex<Option<VoiceError>>,
        turn: bool,
    }

    #[async_trait]
    impl VoiceBackend for Fake {
        fn turn_credentials(&self, user: UserId) -> Option<TurnCredentialsObject> {
            self.turn.then(|| TurnCredentialsObject {
                username: format!("1:{user}"),
                password: "pw".into(),
                ttl: 3600,
                uris: vec!["turn:t:3478".into()],
            })
        }

        async fn moderate(
            &self,
            actor: UserId,
            guild: GuildId,
            target: UserId,
            patch: VoicePatch,
            reason: Option<String>,
        ) -> Result<(), VoiceError> {
            self.calls
                .lock()
                .unwrap()
                .push((actor, guild, target, patch, reason));
            self.result.lock().unwrap().map_or(Ok(()), Err)
        }
    }

    fn app(fake: Arc<Fake>) -> Router {
        let bots = Arc::new(MemoryBots::new());
        bots.insert(
            "mod",
            BotIdentity {
                user_id: 4,
                application_id: 1,
            },
        );
        let mut settings = ApiSettings::default();
        settings.rate_limit.enabled = false;
        let state = AppState::builder()
            .bots(bots)
            .settings(settings)
            .extension(Arc::new(VoiceApi(fake)))
            .build();
        build_router(state, router())
    }

    async fn call(
        app: &Router,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/v1{path}"))
            .header("authorization", "Bot mod");
        let body = match body {
            Some(v) => {
                req = req.header("content-type", "application/json");
                Body::from(v.to_string())
            }
            None => Body::empty(),
        };
        let resp = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
        let (parts, body) = resp.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        (
            parts.status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    #[tokio::test]
    async fn turn_credentials_are_returned_or_refused() {
        let on = app(Arc::new(Fake {
            turn: true,
            ..Fake::default()
        }));
        let (s, v) = call(&on, Method::GET, "/sr/voice/turn", None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["username"], "1:4");
        assert_eq!(v["ttl"], 3600);
        assert_eq!(v["uris"], json!(["turn:t:3478"]));
        let off = app(Arc::new(Fake::default()));
        let (s, v) = call(&off, Method::GET, "/sr/voice/turn", None).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        assert_eq!(v["code"], 900_001);
    }

    #[tokio::test]
    async fn patch_translates_the_body() {
        let fake = Arc::new(Fake::default());
        let app = app(fake.clone());
        let path = "/guilds/10/voice-states/20";

        let (s, _) = call(
            &app,
            Method::PATCH,
            path,
            Some(json!({"mute": true, "reason": "loud"})),
        )
        .await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        let (s, _) = call(&app, Method::PATCH, path, Some(json!({"channel_id": "30"}))).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        let (s, _) = call(&app, Method::PATCH, path, Some(json!({"channel_id": null}))).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        let (s, _) = call(
            &app,
            Method::PATCH,
            "/guilds/10/voice-states/@me",
            Some(json!({"deaf": false})),
        )
        .await;
        assert_eq!(s, StatusCode::NO_CONTENT);

        let calls = fake.calls.lock().unwrap();
        assert_eq!(calls.len(), 4);
        assert_eq!(calls[0].0, UserId::new(4));
        assert_eq!(calls[0].1, GuildId::new(10));
        assert_eq!(calls[0].2, UserId::new(20));
        assert_eq!(calls[0].3.mute, Some(true));
        assert_eq!(calls[0].4.as_deref(), Some("loud"));
        assert_eq!(calls[1].3.channel, Some(Some(ChannelId::new(30))));
        assert_eq!(calls[2].3.channel, Some(None));
        // `@me` means the caller.
        assert_eq!(calls[3].2, UserId::new(4));
        assert_eq!(calls[3].3.deaf, Some(false));
    }

    #[tokio::test]
    async fn bad_bodies_never_reach_the_backend() {
        let fake = Arc::new(Fake::default());
        let app = app(fake.clone());
        for body in [json!({}), json!({"channel_id": "not a snowflake"})] {
            let (s, v) = call(
                &app,
                Method::PATCH,
                "/guilds/10/voice-states/20",
                Some(body),
            )
            .await;
            assert_eq!(s, StatusCode::BAD_REQUEST, "{v}");
            assert_eq!(v["code"], 50035);
        }
        // A wrong JSON type is rejected by the extractor before any handler runs.
        let (s, _) = call(
            &app,
            Method::PATCH,
            "/guilds/10/voice-states/20",
            Some(json!({"mute": "yes"})),
        )
        .await;
        assert!(s.is_client_error());
        let (s, _) = call(
            &app,
            Method::PATCH,
            "/guilds/x/voice-states/20",
            Some(json!({"mute": true})),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        assert!(fake.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn backend_errors_map_to_discord_errors() {
        let fake = Arc::new(Fake::default());
        let app = app(fake.clone());
        for (err, status, code) in [
            (VoiceError::NotInVoice, StatusCode::BAD_REQUEST, 40032),
            (VoiceError::MissingPermissions, StatusCode::FORBIDDEN, 50013),
            (VoiceError::UnknownChannel, StatusCode::NOT_FOUND, 10003),
            (VoiceError::NotVoiceChannel, StatusCode::BAD_REQUEST, 50035),
            (VoiceError::NotStageChannel, StatusCode::BAD_REQUEST, 50035),
            (VoiceError::NoStageInstance, StatusCode::NOT_FOUND, 10067),
            (VoiceError::StageExists, StatusCode::BAD_REQUEST, 150006),
            (VoiceError::Unavailable, StatusCode::FORBIDDEN, 900_001),
        ] {
            *fake.result.lock().unwrap() = Some(err);
            let (s, v) = call(
                &app,
                Method::PATCH,
                "/guilds/10/voice-states/20",
                Some(json!({"mute": true})),
            )
            .await;
            assert_eq!((s, v["code"].as_u64()), (status, Some(code)), "{err:?}");
        }
    }

    #[test]
    fn the_routes_are_documented() {
        let doc = serde_json::to_value(openapi()).unwrap();
        let paths = doc["paths"].as_object().unwrap();
        assert!(paths.contains_key("/sr/voice/turn"));
        assert!(paths.contains_key("/guilds/{guild_id}/voice-states/{user_id}"));
    }
}
