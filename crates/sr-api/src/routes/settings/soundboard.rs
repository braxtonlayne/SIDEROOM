//! Soundboard REST (VOX-SB-001): default sounds, a server's own sounds and playing one in a
//! voice channel. Shapes follow Discord's soundboard endpoints.
//!
//! * `GET /soundboard-default-sounds`, `GET /soundboard-default-sounds/{sound_id}/audio`
//! * `GET|POST /guilds/{guild_id}/soundboard-sounds`
//! * `GET|PATCH|DELETE /guilds/{guild_id}/soundboard-sounds/{sound_id}` and `.../audio`
//! * `POST /channels/{channel_id}/send-soundboard-sound`
//!
//! Uploads arrive as data URIs (`data:audio/ogg;base64,...`) like emoji and stickers. The
//! bytes are kept as uploaded (MP3, Ogg or WAV, at most 5 s and 512 KiB) and served from
//! `.../audio`; the client decodes them and mixes them into its microphone stream, so the
//! server never decodes, transcodes or hears a sound. Playing one only announces it
//! (`VOICE_CHANNEL_EFFECT_SEND`) after the permission, voice-state and cooldown checks.
//! A change to a server's sounds is announced as `GUILD_SOUNDBOARD_SOUNDS_UPDATE`.

use super::deps::{SettingsDeps, actor, id, invalid, re, se};
use super::events::SettingsEvent;
use crate::routes::Ext;
use crate::routes::guilds::dto::UserRef;
use crate::routes::guilds::safety::MfaGate;
use crate::routes::messages::error::ApiJson;
use crate::routes::voice::{SoundEffect, VoiceApi};
use crate::{HttpError, Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{
    Json, Router,
    extract::Path,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use bytes::Bytes;
use data_encoding::BASE64;
use serde::Deserialize;
use serde_json::{Value, json};
use sr_chat::ChannelType;
use sr_chat::settings::repo::ExpressionRepository;
use sr_chat::settings::service::{NewSound, SoundPatch};
use sr_chat::settings::soundboard::{
    DEFAULT_SOUNDS, MAX_SOUND_BYTES, Sound, default_sound, inspect_sound, render_default,
};
use sr_core::permissions::Permissions;
use sr_core::{EmojiId, ErrorCode, GuildId, SoundId};
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/soundboard-default-sounds", get(list_default_sounds))
        .route(
            "/soundboard-default-sounds/{sound_id}/audio",
            get(default_sound_audio),
        )
        .route(
            "/guilds/{guild_id}/soundboard-sounds",
            get(list_sounds).post(create_sound),
        )
        .route(
            "/guilds/{guild_id}/soundboard-sounds/{sound_id}",
            get(get_sound).patch(modify_sound).delete(delete_sound),
        )
        .route(
            "/guilds/{guild_id}/soundboard-sounds/{sound_id}/audio",
            get(sound_audio),
        )
        .route(
            "/channels/{channel_id}/send-soundboard-sound",
            post(send_sound),
        )
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateSoundBody {
    /// 2 to 32 characters.
    pub name: String,
    /// `data:audio/ogg;base64,...` (MP3, Ogg or WAV; at most 5 seconds and 512 KiB).
    pub sound: String,
    /// 0 to 1, default 1.
    pub volume: Option<f64>,
    /// A custom emoji of this server.
    pub emoji_id: Option<String>,
    /// A unicode emoji.
    pub emoji_name: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ModifySoundBody {
    pub name: Option<String>,
    pub volume: Option<f64>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub emoji_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub emoji_name: Option<Option<String>>,
}

fn double_option<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SendSoundBody {
    /// A default sound (`1` to `8`) or a server sound.
    pub sound_id: String,
    /// The server the sound belongs to when it is not the channel's own.
    pub source_guild_id: Option<String>,
}

fn decode_sound_uri(uri: &str) -> ApiResult<Bytes> {
    let bad = |m: &str| invalid("sound", "SOUND_INVALID", m);
    let rest = uri
        .strip_prefix("data:")
        .ok_or_else(|| bad("Expected a data URI."))?;
    let (meta, payload) = rest
        .split_once(',')
        .ok_or_else(|| bad("Malformed data URI."))?;
    if !meta.ends_with(";base64") {
        return Err(bad("Only base64 data URIs are supported."));
    }
    // Refuse before decoding: base64 is 4/3 of the size.
    if payload.len() as u64 > MAX_SOUND_BYTES * 2 {
        return Err(invalid(
            "sound",
            "BINARY_TYPE_MAX_SIZE",
            "file cannot be larger than 512 KiB",
        ));
    }
    let clean: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
    BASE64
        .decode(clean.as_bytes())
        .map(Bytes::from)
        .map_err(|_| bad("Invalid base64 data."))
}

fn sound_value(
    s: &Sound,
    users: &std::collections::HashMap<sr_core::UserId, sr_chat::repo::UserSummary>,
) -> Value {
    s.to_json(serde_json::to_value(UserRef::of(s.creator_id, users)).ok())
}

async fn list_json(deps: &SettingsDeps, guild: GuildId) -> ApiResult<Vec<Value>> {
    let list = deps.service.repo().list_sounds(guild).await.map_err(re)?;
    let users = deps.users(list.iter().map(|s| s.creator_id)).await?;
    Ok(list.iter().map(|s| sound_value(s, &users)).collect())
}

async fn publish_sounds(deps: &SettingsDeps, guild: GuildId) {
    let Ok(list) = deps.service.repo().list_sounds(guild).await else {
        return;
    };
    let Ok(users) = deps.users(list.iter().map(|s| s.creator_id)).await else {
        return;
    };
    let sounds: Vec<Value> = list.iter().map(|s| sound_value(s, &users)).collect();
    deps.publish(SettingsEvent::SoundboardUpdated {
        guild,
        payload: json!({ "guild_id": guild.to_string(), "soundboard_sounds": sounds }),
    })
    .await;
}

/// A sound's emoji: a custom emoji must belong to the server.
async fn emoji_fields(
    deps: &SettingsDeps,
    guild: GuildId,
    emoji_id: Option<&str>,
    emoji_name: Option<String>,
) -> ApiResult<(Option<EmojiId>, Option<String>)> {
    let name = emoji_name.filter(|n| !n.is_empty());
    if let Some(n) = &name
        && n.chars().count() > 32
    {
        return Err(invalid(
            "emoji_name",
            "BASE_TYPE_BAD_LENGTH",
            "must be at most 32 in length",
        ));
    }
    let eid = match emoji_id.filter(|e| !e.is_empty()) {
        None => None,
        Some(raw) => {
            let eid: EmojiId = id("emoji_id", raw)?;
            let known = deps
                .service
                .repo()
                .get_emoji(guild, eid)
                .await
                .map_err(re)?
                .is_some();
            if !known {
                return Err(invalid(
                    "emoji_id",
                    "UNKNOWN_EMOJI",
                    "Not an emoji of this server.",
                ));
            }
            Some(eid)
        }
    };
    if eid.is_some() && name.is_some() {
        return Err(invalid(
            "emoji_name",
            "BASE_TYPE_INVALID",
            "Provide emoji_id or emoji_name, not both.",
        ));
    }
    Ok((eid, name))
}

/// The eight sounds every server and DM can play.
#[utoipa::path(
    get,
    path = "/soundboard-default-sounds",
    responses((status = 200, description = "Array of sound objects (`sound_id` 1 to 8, no `guild_id`)", body = Vec<Value>)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_default_sounds(_p: Principal) -> Json<Vec<Value>> {
    Json(DEFAULT_SOUNDS.iter().map(|s| s.to_json()).collect())
}

/// The audio of a default sound (a short WAV synthesized by the server).
#[utoipa::path(
    get,
    path = "/soundboard-default-sounds/{sound_id}/audio",
    params(("sound_id" = String, Path, description = "1 to 8")),
    responses((status = 200, content_type = "audio/wav", body = Vec<u8>), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn default_sound_audio(
    _p: Principal,
    Path(sound_id): Path<String>,
) -> ApiResult<Response> {
    let def = sound_id
        .parse::<u64>()
        .ok()
        .and_then(default_sound)
        .ok_or(ErrorCode::UnknownSound)?;
    Ok(audio_response(
        render_default(def),
        "audio/wav",
        &format!("\"d{}\"", def.id),
    ))
}

fn audio_response(data: impl Into<Bytes>, mime: &'static str, etag: &str) -> Response {
    let mut resp = (
        [(header::CONTENT_TYPE, HeaderValue::from_static(mime))],
        data.into(),
    )
        .into_response();
    if let Ok(v) = HeaderValue::from_str(etag) {
        resp.headers_mut().insert(header::ETAG, v);
    }
    // Sounds never change once uploaded (a new upload is a new id).
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=86400, immutable"),
    );
    resp
}

/// A server's sounds (any member), Discord's `{items: [...]}`.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/soundboard-sounds",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, description = "`{items: [sound objects]}`"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_sounds(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    deps.guild_access(user, gid).await?;
    Ok(Json(json!({ "items": list_json(&deps, gid).await? })))
}

/// One sound of a server (any member).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/soundboard-sounds/{sound_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("sound_id" = String, Path, description = "Sound snowflake"),
    ),
    responses((status = 200, description = "A sound object"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_sound(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path((guild_id, sound_id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let sid: SoundId = id("sound_id", &sound_id)?;
    deps.guild_access(user, gid).await?;
    let s = deps
        .service
        .repo()
        .get_sound(gid, sid)
        .await
        .map_err(re)?
        .ok_or(ErrorCode::UnknownSound)?;
    let users = deps.users([s.creator_id]).await?;
    Ok(Json(sound_value(&s, &users)))
}

/// The audio file of a server sound, as uploaded (members of the server).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/soundboard-sounds/{sound_id}/audio",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("sound_id" = String, Path, description = "Sound snowflake"),
    ),
    responses((status = 200, content_type = "audio/mpeg", body = Vec<u8>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn sound_audio(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path((guild_id, sound_id)): Path<(String, String)>,
) -> ApiResult<Response> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let sid: SoundId = id("sound_id", &sound_id)?;
    deps.guild_access(user, gid).await?;
    let s = deps
        .service
        .repo()
        .get_sound(gid, sid)
        .await
        .map_err(re)?
        .filter(|s| s.available)
        .ok_or(ErrorCode::UnknownSound)?;
    let data = deps.storage.get(&s.media_key).await.map_err(|e| {
        tracing::error!(error = %e, key = s.media_key, "sound missing from storage");
        HttpError::from(ErrorCode::UnknownSound)
    })?;
    Ok(audio_response(
        data,
        s.format.mime(),
        &format!("\"{}\"", s.id),
    ))
}

/// Upload a sound (`CREATE_GUILD_EXPRESSIONS` or `MANAGE_GUILD_EXPRESSIONS`). 48 slots for
/// every server (D-044).
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/soundboard-sounds",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = CreateSoundBody,
    responses(
        (status = 201, description = "The sound object"),
        (status = 400, description = "Invalid file (type, size, longer than 5 s), or 30045 when the slots are full", body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create_sound(
    p: Principal,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<CreateSoundBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    acc.require_mfa(&gate, user, Permissions::MANAGE_GUILD_EXPRESSIONS)
        .await?;
    let data = decode_sound_uri(&body.sound)?;
    let info = inspect_sound(&data).ok_or_else(|| {
        invalid(
            "sound",
            "SOUND_INVALID",
            "Must be an MP3, Ogg (Opus or Vorbis) or WAV file.",
        )
    })?;
    let (emoji_id, emoji_name) =
        emoji_fields(&deps, gid, body.emoji_id.as_deref(), body.emoji_name).await?;
    let key = format!(
        "sound/{gid}/{}.{}",
        deps.service.new_id(),
        info.format.extension()
    );
    let sound = deps
        .service
        .create_sound(
            acc.perms,
            user,
            gid,
            NewSound {
                name: body.name,
                volume: body.volume.unwrap_or(1.0),
                emoji_id,
                emoji_name,
                format: info.format,
                size_bytes: data.len() as u64,
                duration_ms: info.duration_ms,
                media_key: key.clone(),
            },
        )
        .await
        .map_err(se)?;
    if let Err(e) = deps.storage.put(&key, data).await {
        tracing::error!(error = %e, key, "object storage failure");
        // Do not leave a sound whose file is missing.
        let _ = deps.service.repo().delete_sound(gid, sound.id).await;
        return Err(ErrorCode::SrInternal.into());
    }
    publish_sounds(&deps, gid).await;
    let users = deps.users([user]).await?;
    Ok((StatusCode::CREATED, Json(sound_value(&sound, &users))))
}

/// Rename a sound or change its volume or emoji (`MANAGE_GUILD_EXPRESSIONS`, or the
/// uploader with `CREATE_GUILD_EXPRESSIONS`).
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/soundboard-sounds/{sound_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("sound_id" = String, Path, description = "Sound snowflake"),
    ),
    request_body = ModifySoundBody,
    responses((status = 200, description = "The sound object"), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn modify_sound(
    p: Principal,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Path((guild_id, sound_id)): Path<(String, String)>,
    ApiJson(body): ApiJson<ModifySoundBody>,
) -> ApiResult<Json<Value>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let sid: SoundId = id("sound_id", &sound_id)?;
    let acc = deps.guild_access(user, gid).await?;
    acc.require_mfa(&gate, user, Permissions::MANAGE_GUILD_EXPRESSIONS)
        .await?;
    let emoji = if body.emoji_id.is_some() || body.emoji_name.is_some() {
        let raw_id = body.emoji_id.clone().flatten();
        let name = body.emoji_name.clone().flatten();
        let (eid, ename) = emoji_fields(&deps, gid, raw_id.as_deref(), name).await?;
        Some(if eid.is_none() && ename.is_none() {
            None
        } else {
            Some((eid, ename))
        })
    } else {
        None
    };
    let s = deps
        .service
        .edit_sound(
            acc.perms,
            user,
            gid,
            sid,
            SoundPatch {
                name: body.name,
                volume: body.volume,
                emoji,
            },
        )
        .await
        .map_err(se)?;
    publish_sounds(&deps, gid).await;
    let users = deps.users([s.creator_id]).await?;
    Ok(Json(sound_value(&s, &users)))
}

/// Delete a sound (same permissions as editing).
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/soundboard-sounds/{sound_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("sound_id" = String, Path, description = "Sound snowflake"),
    ),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_sound(
    p: Principal,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Path((guild_id, sound_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let sid: SoundId = id("sound_id", &sound_id)?;
    let acc = deps.guild_access(user, gid).await?;
    acc.require_mfa(&gate, user, Permissions::MANAGE_GUILD_EXPRESSIONS)
        .await?;
    let gone = deps
        .service
        .delete_sound(acc.perms, user, gid, sid)
        .await
        .map_err(se)?;
    if let Err(e) = deps.storage.delete(&gone.media_key).await {
        tracing::warn!(error = %e, key = gone.media_key, "could not delete sound file");
    }
    publish_sounds(&deps, gid).await;
    Ok(StatusCode::NO_CONTENT)
}

/// Play a sound in the voice channel you are in. Needs `USE_SOUNDBOARD` and `SPEAK` in the
/// channel, `USE_EXTERNAL_SOUNDS` for another server's sound, and not being muted or
/// deafened. One play every few seconds. Announced as `VOICE_CHANNEL_EFFECT_SEND`; the
/// client mixes the sound into its own audio.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/send-soundboard-sound",
    params(("channel_id" = String, Path, description = "Voice or stage channel snowflake")),
    request_body = SendSoundBody,
    responses(
        (status = 204),
        (status = 400, description = "Not connected to the channel (40032), or not a voice channel", body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, description = "Unknown sound or channel", body = ErrorBody),
        (status = 409, description = "Muted, deafened or in the audience", body = ErrorBody),
        (status = 429, description = "Another sound was just played (900012)", body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn send_sound(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Ext(voice): Ext<VoiceApi>,
    Path(channel_id): Path<String>,
    ApiJson(body): ApiJson<SendSoundBody>,
) -> ApiResult<StatusCode> {
    let user = actor(&p)?;
    let cid: sr_core::ChannelId = id("channel_id", &channel_id)?;
    let ca = deps.channel_access(user, cid).await?;
    if !matches!(ca.channel.kind, ChannelType::Voice | ChannelType::Stage) {
        return Err(invalid(
            "channel_id",
            "CHANNEL_TYPE_INVALID",
            "Must be a voice channel.",
        ));
    }
    let guild = ca.guild.guild.id;
    if !ca
        .perms
        .has(Permissions::USE_SOUNDBOARD | Permissions::SPEAK | Permissions::CONNECT)
    {
        return Err(ErrorCode::MissingPermissions.into());
    }
    let raw: u64 = body.sound_id.parse().map_err(|_| {
        invalid(
            "sound_id",
            "SNOWFLAKE_TYPE_INVALID",
            "Value is not snowflake.",
        )
    })?;
    let effect = if let Some(def) = default_sound(raw) {
        SoundEffect {
            sound_id: def.id.to_string(),
            volume: 1.0,
            emoji_id: None,
            emoji_name: Some(def.emoji.to_owned()),
        }
    } else {
        let source: GuildId = match &body.source_guild_id {
            Some(g) => id("source_guild_id", g)?,
            None => guild,
        };
        if source != guild {
            // Another server's sounds are free to use (D-044) for its members, wherever the
            // channel allows external sounds.
            if !ca.perms.has(Permissions::USE_EXTERNAL_SOUNDS) {
                return Err(ErrorCode::MissingPermissions.into());
            }
            deps.guild_access(user, source).await?;
        }
        let s = deps
            .service
            .repo()
            .get_sound(source, SoundId::new(raw))
            .await
            .map_err(re)?
            .filter(|s| s.available)
            .ok_or(ErrorCode::UnknownSound)?;
        SoundEffect {
            sound_id: s.id.to_string(),
            volume: s.volume,
            emoji_id: s.emoji_id.map(|e| e.to_string()),
            emoji_name: s.emoji_name.clone(),
        }
    };
    voice.0.soundboard_play(user, guild, cid, effect).await?;
    Ok(StatusCode::NO_CONTENT)
}
