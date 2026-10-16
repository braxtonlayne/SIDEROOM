//! Custom emoji and stickers (LNG-SSET-021..024, open question 135): CRUD with the
//! slot and size limits enforced by `SettingsService`.
//!
//! Uploads arrive as data URIs (`data:image/png;base64,...`), like Discord's emoji
//! endpoint, for stickers too (SIDE ROOM accepts JSON instead of multipart).

use super::deps::{GuildAccess, SettingsDeps, actor, id, invalid, re, se};
use super::events::SettingsEvent;
use crate::routes::Ext;
use crate::routes::guilds::dto::UserRef;
use crate::routes::guilds::safety::MfaGate;
use crate::routes::messages::error::ApiJson;
use crate::{HttpError, Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
};
use bytes::Bytes;
use data_encoding::BASE64;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sr_chat::settings::expressions::{Emoji, Sticker, StickerFormat};
use sr_chat::settings::repo::ExpressionRepository;
use sr_chat::settings::service::{NewEmoji, NewSticker, StickerPatch};
use sr_core::{EmojiId, ErrorCode, GuildId, RoleId, StickerId};
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/guilds/{guild_id}/emojis/{emoji_id}/url", get(emoji_url))
        .route(
            "/guilds/{guild_id}/stickers/{sticker_id}/url",
            get(sticker_url),
        )
        .route(
            "/guilds/{guild_id}/emojis",
            get(list_emojis).post(create_emoji),
        )
        .route(
            "/guilds/{guild_id}/emojis/{emoji_id}",
            get(get_emoji).patch(modify_emoji).delete(delete_emoji),
        )
        .route(
            "/guilds/{guild_id}/stickers",
            get(list_stickers).post(create_sticker),
        )
        .route(
            "/guilds/{guild_id}/stickers/{sticker_id}",
            get(get_sticker)
                .patch(modify_sticker)
                .delete(delete_sticker),
        )
}

// ---- upload decoding -------------------------------------------------------------

/// What the magic bytes of an upload say it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Image {
    Png { animated: bool },
    Jpeg,
    Gif { animated: bool },
    Webp { animated: bool },
    Lottie,
}

impl Image {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png { .. } => "png",
            Self::Jpeg => "jpg",
            Self::Gif { .. } => "gif",
            Self::Webp { .. } => "webp",
            Self::Lottie => "json",
        }
    }

    pub fn animated(self) -> bool {
        matches!(
            self,
            Self::Png { animated: true }
                | Self::Gif { animated: true }
                | Self::Webp { animated: true }
        )
    }
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// Sniff the type of `data`. The declared MIME type is not trusted.
pub fn sniff(data: &[u8]) -> Option<Image> {
    if data.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        // APNG carries an `acTL` chunk before the first image data chunk.
        return Some(Image::Png {
            animated: contains(&data[..data.len().min(4096)], b"acTL"),
        });
    }
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(Image::Jpeg);
    }
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        // Each frame has a graphic-control extension (`21 F9`).
        let frames = data.windows(2).filter(|w| *w == [0x21, 0xF9]).count();
        return Some(Image::Gif {
            animated: frames > 1,
        });
    }
    if data.len() > 16 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return Some(Image::Webp {
            animated: contains(&data[..data.len().min(64)], b"ANIM"),
        });
    }
    let trimmed = data.iter().skip_while(|b| b.is_ascii_whitespace());
    if trimmed.clone().next() == Some(&b'{')
        && serde_json::from_slice::<Value>(data)
            .ok()
            .is_some_and(|v| v.get("v").is_some() && v.get("layers").is_some())
    {
        return Some(Image::Lottie);
    }
    None
}

/// Decode `data:<mime>;base64,<payload>`.
pub fn decode_data_uri(field: &str, uri: &str) -> ApiResult<Bytes> {
    let bad = |m: &str| invalid(field, "IMAGE_INVALID", m);
    let rest = uri
        .strip_prefix("data:")
        .ok_or_else(|| bad("Expected a data URI."))?;
    let (meta, payload) = rest
        .split_once(',')
        .ok_or_else(|| bad("Malformed data URI."))?;
    if !meta.ends_with(";base64") {
        return Err(bad("Only base64 data URIs are supported."));
    }
    let clean: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
    BASE64
        .decode(clean.as_bytes())
        .map(Bytes::from)
        .map_err(|_| bad("Invalid base64 data."))
}

// ---- objects ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EmojiObject {
    pub id: String,
    pub name: String,
    pub roles: Vec<String>,
    pub user: UserRef,
    pub require_colons: bool,
    pub managed: bool,
    pub animated: bool,
    pub available: bool,
}

impl EmojiObject {
    fn of(
        e: &Emoji,
        users: &std::collections::HashMap<sr_core::UserId, sr_chat::repo::UserSummary>,
    ) -> Self {
        Self {
            id: e.id.to_string(),
            name: e.name.clone(),
            roles: e.role_ids.iter().map(ToString::to_string).collect(),
            user: UserRef::of(e.creator_id, users),
            require_colons: true,
            managed: false,
            animated: e.animated,
            available: e.available,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct StickerObject {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: String,
    /// 2 = guild sticker.
    #[serde(rename = "type")]
    pub kind: u8,
    /// 1 PNG, 2 APNG, 3 Lottie, 4 GIF.
    pub format_type: i16,
    pub available: bool,
    pub guild_id: String,
    pub user: UserRef,
}

impl StickerObject {
    fn of(
        s: &Sticker,
        users: &std::collections::HashMap<sr_core::UserId, sr_chat::repo::UserSummary>,
    ) -> Self {
        Self {
            id: s.id.to_string(),
            name: s.name.clone(),
            description: s.description.clone(),
            tags: s.tags.clone(),
            kind: 2,
            format_type: s.format.code(),
            available: s.available,
            guild_id: s.guild_id.to_string(),
            user: UserRef::of(s.creator_id, users),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateEmojiBody {
    /// 2-32 letters, digits and underscores.
    pub name: String,
    /// `data:image/png;base64,...` (PNG, JPEG, GIF or WebP; at most 256 KiB).
    pub image: String,
    /// Role ids that may use the emoji; empty or absent = everyone.
    #[serde(default)]
    pub roles: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ModifyEmojiBody {
    pub name: Option<String>,
    pub roles: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateStickerBody {
    pub name: String,
    pub description: Option<String>,
    /// A related emoji or comma-separated tags.
    pub tags: String,
    /// Data URI: PNG, APNG, GIF or Lottie JSON (at most 512 KiB).
    pub file: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ModifyStickerBody {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub description: Option<Option<String>>,
    pub tags: Option<String>,
}

fn double_option<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

fn role_ids(acc: &GuildAccess, raw: &[String]) -> ApiResult<Vec<RoleId>> {
    let mut out = Vec::with_capacity(raw.len());
    for r in raw {
        let rid: RoleId = id("roles", r)?;
        if !acc.roles.iter().any(|x| x.id == rid) {
            return Err(invalid("roles", "UNKNOWN_ROLE", "Unknown role."));
        }
        if !out.contains(&rid) {
            out.push(rid);
        }
    }
    Ok(out)
}

async fn publish_emojis(deps: &SettingsDeps, guild: GuildId) {
    let Ok(list) = deps.service.repo().list_emoji(guild).await else {
        return;
    };
    let Ok(users) = deps.users(list.iter().map(|e| e.creator_id)).await else {
        return;
    };
    let emojis: Vec<Value> = list
        .iter()
        .filter_map(|e| serde_json::to_value(EmojiObject::of(e, &users)).ok())
        .collect();
    deps.publish(SettingsEvent::EmojisUpdated {
        guild,
        payload: json!({ "guild_id": guild.to_string(), "emojis": emojis }),
    })
    .await;
}

async fn publish_stickers(deps: &SettingsDeps, guild: GuildId) {
    let Ok(list) = deps.service.repo().list_stickers(guild).await else {
        return;
    };
    let Ok(users) = deps.users(list.iter().map(|s| s.creator_id)).await else {
        return;
    };
    let stickers: Vec<Value> = list
        .iter()
        .filter_map(|s| serde_json::to_value(StickerObject::of(s, &users)).ok())
        .collect();
    deps.publish(SettingsEvent::StickersUpdated {
        guild,
        payload: json!({ "guild_id": guild.to_string(), "stickers": stickers }),
    })
    .await;
}

async fn store(deps: &SettingsDeps, key: &str, data: Bytes) -> ApiResult<()> {
    deps.storage.put(key, data).await.map_err(|e| {
        tracing::error!(error = %e, key, "object storage failure");
        HttpError::from(ErrorCode::SrInternal)
    })
}

/// Longest edge of a stored custom emoji (LNG-EXP-002).
pub const EMOJI_PX: u32 = 128;
/// Longest edge of a stored guild sticker (LNG-EXP-003).
pub const STICKER_PX: u32 = 320;

/// Shrink an upload to fit `px` x `px`, keeping the aspect ratio (animated GIFs stay
/// animated, metadata is stripped by the re-encode). Anything the image pipeline cannot
/// decode (animated WebP or APNG, a truncated file) is stored as uploaded: the byte
/// limits already applied, and the CDN re-validates every variant it serves. Returns the
/// bytes to store and their file extension.
pub(crate) fn fit_image(data: Bytes, fallback_ext: &'static str, px: u32) -> (Bytes, &'static str) {
    use sr_storage::img::{Fit, Limits, normalize};
    match normalize(&data, Fit::Bound { w: px, h: px }, &Limits::default()) {
        Ok(n) => (Bytes::from(n.bytes), n.format.ext()),
        Err(_) => (data, fallback_ext),
    }
}

// ---- emoji -----------------------------------------------------------------------

/// List a guild's emoji (any member).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/emojis",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<EmojiObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_emojis(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<Vec<EmojiObject>>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    deps.guild_access(user, gid).await?;
    let list = deps.service.repo().list_emoji(gid).await.map_err(re)?;
    let users = deps.users(list.iter().map(|e| e.creator_id)).await?;
    Ok(Json(
        list.iter().map(|e| EmojiObject::of(e, &users)).collect(),
    ))
}

/// Get one emoji (any member).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/emojis/{emoji_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("emoji_id" = String, Path, description = "Emoji snowflake"),
    ),
    responses((status = 200, body = EmojiObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_emoji(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path((guild_id, emoji_id)): Path<(String, String)>,
) -> ApiResult<Json<EmojiObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let eid: EmojiId = id("emoji_id", &emoji_id)?;
    deps.guild_access(user, gid).await?;
    let e = deps
        .service
        .repo()
        .get_emoji(gid, eid)
        .await
        .map_err(re)?
        .ok_or(ErrorCode::UnknownEmoji)?;
    let users = deps.users([e.creator_id]).await?;
    Ok(Json(EmojiObject::of(&e, &users)))
}

/// Upload an emoji (`CREATE_GUILD_EXPRESSIONS` or `MANAGE_GUILD_EXPRESSIONS`).
/// 250 static and 250 animated slots for every server (open question 135).
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/emojis",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = CreateEmojiBody,
    responses(
        (status = 201, body = EmojiObject),
        (status = 400, body = ErrorBody, description = "Invalid image, or 30008 when the slots are full"),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create_emoji(
    p: Principal,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<CreateEmojiBody>,
) -> ApiResult<(StatusCode, Json<EmojiObject>)> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    acc.require_mfa(
        &gate,
        user,
        sr_core::permissions::Permissions::MANAGE_GUILD_EXPRESSIONS,
    )
    .await?;
    let data = decode_data_uri("image", &body.image)?;
    let img = match sniff(&data) {
        Some(i @ (Image::Png { .. } | Image::Jpeg | Image::Gif { .. } | Image::Webp { .. })) => i,
        _ => {
            return Err(invalid(
                "image",
                "IMAGE_INVALID",
                "Must be a PNG, JPEG, GIF or WebP image.",
            ));
        }
    };
    let roles = role_ids(&acc, &body.roles)?;
    let original_len = data.len() as u64;
    let (data, ext) = fit_image(data, img.extension(), EMOJI_PX);
    let key = format!("emoji/{gid}/{}.{ext}", deps.service.new_id());
    let emoji = deps
        .service
        .create_emoji(
            acc.perms,
            user,
            gid,
            NewEmoji {
                name: body.name,
                animated: img.animated(),
                size_bytes: original_len,
                media_key: key.clone(),
                role_ids: roles,
            },
        )
        .await
        .map_err(se)?;
    if let Err(e) = store(&deps, &key, data).await {
        // Do not leave an emoji whose image is missing.
        let _ = deps.service.repo().delete_emoji(gid, emoji.id).await;
        return Err(e);
    }
    publish_emojis(&deps, gid).await;
    let users = deps.users([user]).await?;
    Ok((StatusCode::CREATED, Json(EmojiObject::of(&emoji, &users))))
}

/// Rename an emoji or change its role restriction (`MANAGE_GUILD_EXPRESSIONS`).
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/emojis/{emoji_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("emoji_id" = String, Path, description = "Emoji snowflake"),
    ),
    request_body = ModifyEmojiBody,
    responses((status = 200, body = EmojiObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn modify_emoji(
    p: Principal,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Path((guild_id, emoji_id)): Path<(String, String)>,
    ApiJson(body): ApiJson<ModifyEmojiBody>,
) -> ApiResult<Json<EmojiObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let eid: EmojiId = id("emoji_id", &emoji_id)?;
    let acc = deps.guild_access(user, gid).await?;
    acc.require_mfa(
        &gate,
        user,
        sr_core::permissions::Permissions::MANAGE_GUILD_EXPRESSIONS,
    )
    .await?;
    let mut emoji = deps
        .service
        .repo()
        .get_emoji(gid, eid)
        .await
        .map_err(re)?
        .ok_or(ErrorCode::UnknownEmoji)?;
    if let Some(name) = &body.name
        && *name != emoji.name
    {
        emoji = deps
            .service
            .rename_emoji(acc.perms, user, gid, eid, name)
            .await
            .map_err(se)?;
    }
    if let Some(raw) = &body.roles {
        let roles = role_ids(&acc, raw)?;
        emoji = deps
            .service
            .set_emoji_roles(acc.perms, user, gid, eid, roles)
            .await
            .map_err(se)?;
    }
    publish_emojis(&deps, gid).await;
    let users = deps.users([emoji.creator_id]).await?;
    Ok(Json(EmojiObject::of(&emoji, &users)))
}

/// Delete an emoji (`MANAGE_GUILD_EXPRESSIONS`).
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/emojis/{emoji_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("emoji_id" = String, Path, description = "Emoji snowflake"),
    ),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_emoji(
    p: Principal,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Path((guild_id, emoji_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let eid: EmojiId = id("emoji_id", &emoji_id)?;
    let acc = deps.guild_access(user, gid).await?;
    acc.require_mfa(
        &gate,
        user,
        sr_core::permissions::Permissions::MANAGE_GUILD_EXPRESSIONS,
    )
    .await?;
    let key = deps
        .service
        .repo()
        .get_emoji(gid, eid)
        .await
        .map_err(re)?
        .map(|e| e.media_key);
    deps.service
        .delete_emoji(acc.perms, user, gid, eid)
        .await
        .map_err(se)?;
    if let Some(key) = key
        && let Err(e) = deps.storage.delete(&key).await
    {
        tracing::warn!(error = %e, key, "could not delete emoji image");
    }
    publish_emojis(&deps, gid).await;
    Ok(StatusCode::NO_CONTENT)
}

// ---- stickers --------------------------------------------------------------------

/// List a guild's stickers (any member).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/stickers",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<StickerObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_stickers(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<Vec<StickerObject>>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    deps.guild_access(user, gid).await?;
    let list = deps.service.repo().list_stickers(gid).await.map_err(re)?;
    let users = deps.users(list.iter().map(|s| s.creator_id)).await?;
    Ok(Json(
        list.iter().map(|s| StickerObject::of(s, &users)).collect(),
    ))
}

/// Get one sticker (any member).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/stickers/{sticker_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("sticker_id" = String, Path, description = "Sticker snowflake"),
    ),
    responses((status = 200, body = StickerObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_sticker(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path((guild_id, sticker_id)): Path<(String, String)>,
) -> ApiResult<Json<StickerObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let sid: StickerId = id("sticker_id", &sticker_id)?;
    deps.guild_access(user, gid).await?;
    let s = deps
        .service
        .repo()
        .get_sticker(gid, sid)
        .await
        .map_err(re)?
        .ok_or(ErrorCode::UnknownEmoji)?;
    let users = deps.users([s.creator_id]).await?;
    Ok(Json(StickerObject::of(&s, &users)))
}

/// Upload a sticker (`MANAGE_GUILD_EXPRESSIONS`); 60 slots for every server.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/stickers",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = CreateStickerBody,
    responses(
        (status = 201, body = StickerObject),
        (status = 400, body = ErrorBody, description = "Invalid file, or 30039 when the slots are full"),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create_sticker(
    p: Principal,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<CreateStickerBody>,
) -> ApiResult<(StatusCode, Json<StickerObject>)> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    acc.require_mfa(
        &gate,
        user,
        sr_core::permissions::Permissions::MANAGE_GUILD_EXPRESSIONS,
    )
    .await?;
    let data = decode_data_uri("file", &body.file)?;
    let (format, img) = match sniff(&data) {
        Some(i @ Image::Png { animated: false }) => (StickerFormat::Png, i),
        Some(i @ Image::Png { animated: true }) => (StickerFormat::Apng, i),
        Some(i @ Image::Gif { .. }) => (StickerFormat::Gif, i),
        Some(i @ Image::Lottie) => (StickerFormat::Lottie, i),
        _ => {
            return Err(invalid(
                "file",
                "IMAGE_INVALID",
                "Must be a PNG, APNG, GIF or Lottie file.",
            ));
        }
    };
    let original_len = data.len() as u64;
    // Static PNGs and GIFs are shrunk to the sticker size; APNG and Lottie are stored as is.
    let (data, ext) = if matches!(format, StickerFormat::Png | StickerFormat::Gif) {
        fit_image(data, img.extension(), STICKER_PX)
    } else {
        (data, img.extension())
    };
    let key = format!("sticker/{gid}/{}.{ext}", deps.service.new_id());
    let sticker = deps
        .service
        .create_sticker(
            acc.perms,
            user,
            gid,
            NewSticker {
                name: body.name,
                description: body.description,
                tags: body.tags,
                format,
                size_bytes: original_len,
                media_key: key.clone(),
            },
        )
        .await
        .map_err(se)?;
    if let Err(e) = store(&deps, &key, data).await {
        let _ = deps.service.repo().delete_sticker(gid, sticker.id).await;
        return Err(e);
    }
    publish_stickers(&deps, gid).await;
    let users = deps.users([user]).await?;
    Ok((
        StatusCode::CREATED,
        Json(StickerObject::of(&sticker, &users)),
    ))
}

/// Edit a sticker's name, description or tags (`MANAGE_GUILD_EXPRESSIONS`).
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/stickers/{sticker_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("sticker_id" = String, Path, description = "Sticker snowflake"),
    ),
    request_body = ModifyStickerBody,
    responses((status = 200, body = StickerObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn modify_sticker(
    p: Principal,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Path((guild_id, sticker_id)): Path<(String, String)>,
    ApiJson(body): ApiJson<ModifyStickerBody>,
) -> ApiResult<Json<StickerObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let sid: StickerId = id("sticker_id", &sticker_id)?;
    let acc = deps.guild_access(user, gid).await?;
    acc.require_mfa(
        &gate,
        user,
        sr_core::permissions::Permissions::MANAGE_GUILD_EXPRESSIONS,
    )
    .await?;
    let s = deps
        .service
        .edit_sticker(
            acc.perms,
            user,
            gid,
            sid,
            StickerPatch {
                name: body.name,
                description: body.description,
                tags: body.tags,
            },
        )
        .await
        .map_err(se)?;
    publish_stickers(&deps, gid).await;
    let users = deps.users([s.creator_id]).await?;
    Ok(Json(StickerObject::of(&s, &users)))
}

/// Delete a sticker (`MANAGE_GUILD_EXPRESSIONS`).
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/stickers/{sticker_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("sticker_id" = String, Path, description = "Sticker snowflake"),
    ),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_sticker(
    p: Principal,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Path((guild_id, sticker_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let sid: StickerId = id("sticker_id", &sticker_id)?;
    let acc = deps.guild_access(user, gid).await?;
    acc.require_mfa(
        &gate,
        user,
        sr_core::permissions::Permissions::MANAGE_GUILD_EXPRESSIONS,
    )
    .await?;
    let key = deps
        .service
        .repo()
        .get_sticker(gid, sid)
        .await
        .map_err(re)?
        .map(|s| s.media_key);
    deps.service
        .delete_sticker(acc.perms, user, gid, sid)
        .await
        .map_err(se)?;
    if let Some(key) = key
        && let Err(e) = deps.storage.delete(&key).await
    {
        tracing::warn!(error = %e, key, "could not delete sticker file");
    }
    publish_stickers(&deps, gid).await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];

    #[test]
    fn sniffs_by_magic_bytes() {
        assert_eq!(sniff(PNG), Some(Image::Png { animated: false }));
        let mut apng = PNG.to_vec();
        apng.extend_from_slice(b"....acTL....");
        assert_eq!(sniff(&apng), Some(Image::Png { animated: true }));
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some(Image::Jpeg));
        let mut gif = b"GIF89a".to_vec();
        gif.extend_from_slice(&[0x21, 0xF9, 0, 0x21, 0xF9]);
        assert_eq!(sniff(&gif), Some(Image::Gif { animated: true }));
        assert_eq!(
            sniff(b"GIF89a\x21\xF9"),
            Some(Image::Gif { animated: false })
        );
        assert_eq!(sniff(br#"{"v":"5.7","layers":[]}"#), Some(Image::Lottie));
        assert_eq!(sniff(b"<svg/>"), None);
        assert_eq!(sniff(b""), None);
    }

    fn png(w: u32, h: u32) -> Bytes {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([200, 30, 30, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        Bytes::from(out.into_inner())
    }

    #[test]
    fn emoji_and_stickers_are_shrunk_but_keep_their_aspect() {
        let (b, ext) = fit_image(png(300, 200), "png", EMOJI_PX);
        assert_eq!(ext, "png");
        let d = image::load_from_memory(&b).unwrap();
        assert_eq!((d.width(), d.height()), (128, 85));
        let (b, _) = fit_image(png(900, 900), "png", STICKER_PX);
        let d = image::load_from_memory(&b).unwrap();
        assert_eq!((d.width(), d.height()), (320, 320));
        // Already small: not enlarged.
        let (b, _) = fit_image(png(64, 64), "png", EMOJI_PX);
        assert_eq!(image::load_from_memory(&b).unwrap().width(), 64);
        // Undecodable bytes are kept as uploaded with the sniffed extension.
        let (b, ext) = fit_image(Bytes::from_static(PNG), "png", EMOJI_PX);
        assert_eq!((&b[..], ext), (PNG, "png"));
    }

    #[test]
    fn decodes_data_uris() {
        let uri = format!("data:image/png;base64,{}", BASE64.encode(PNG));
        assert_eq!(&decode_data_uri("image", &uri).unwrap()[..], PNG);
        assert!(decode_data_uri("image", "http://x/y.png").is_err());
        assert!(decode_data_uri("image", "data:image/png,abc").is_err());
        assert!(decode_data_uri("image", "data:image/png;base64,***").is_err());
    }
}

// ---- signed image URLs -----------------------------------------------------------

/// Query of the emoji and sticker `url` endpoints.
#[derive(Debug, Default, Deserialize)]
pub struct UrlQuery {
    /// Longest edge: a power of two from 16 to 4096.
    pub size: Option<u32>,
    /// `png`, `jpg`, `webp` or `gif`; omitted follows the source and `Accept`.
    pub format: Option<String>,
}

/// A short-lived signed CDN URL.
#[derive(Debug, Serialize, ToSchema)]
pub struct SignedUrlObject {
    /// Relative to the public origin: `/cdn/...?size=..&ex=..&sig=..`.
    pub url: String,
    /// Unix seconds.
    pub expires_at: u64,
}

/// Lifetime of minted emoji and sticker URLs.
const URL_TTL_SECS: u64 = 3600;

pub(crate) fn signed_media_url(
    state: &AppState,
    cdn: &crate::routes::cdn::CdnDeps,
    media_key: &str,
    q: &UrlQuery,
) -> ApiResult<SignedUrlObject> {
    if let Some(s) = q.size
        && !sr_storage::img::valid_size(s)
    {
        return Err(invalid(
            "size",
            "NUMBER_TYPE_BAD_VALUE",
            "Must be a power of two between 16 and 4096.",
        ));
    }
    let format = match q.format.as_deref() {
        None => None,
        Some(f) => Some(
            sr_storage::img::ImgFormat::from_name(f)
                .ok_or_else(|| {
                    invalid(
                        "format",
                        "ENUM_TYPE_BAD_VALUE",
                        "Must be png, jpg, webp or gif.",
                    )
                })?
                .ext(),
        ),
    };
    // Object keys are `emoji/<guild>/<id>.<ext>` and `sticker/<guild>/<id>.<ext>`.
    let (prefix, rest) = media_key.split_once('/').ok_or(ErrorCode::SrInternal)?;
    let kind = match prefix {
        "emoji" => "emoji",
        "sticker" => "stickers",
        _ => return Err(ErrorCode::SrInternal.into()),
    };
    let segs: Vec<&str> = rest.split('/').collect();
    let path = crate::routes::cdn::cdn_path(kind, &segs);
    // Lottie JSON is not an image: no resize parameters.
    let is_image = !media_key.ends_with(".json");
    let (size, format) = if is_image {
        (q.size, format)
    } else {
        (None, None)
    };
    let expires_at = u64::try_from(state.now_secs()).unwrap_or(0) + URL_TTL_SECS;
    Ok(SignedUrlObject {
        url: cdn
            .signer
            .signed_variant_url(&path, size, format, expires_at),
        expires_at,
    })
}

/// Mint a short-lived signed CDN URL for an emoji image (any member). Emoji
/// objects carry no URL, so clients ask here and may request `size`/`format`.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/emojis/{emoji_id}/url",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("emoji_id" = String, Path, description = "Emoji snowflake"),
        ("size" = Option<u32>, Query, description = "Power of two, 16 to 4096"),
        ("format" = Option<String>, Query, description = "png, jpg, webp or gif"),
    ),
    responses((status = 200, body = SignedUrlObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn emoji_url(
    p: Principal,
    State(state): State<AppState>,
    Ext(deps): Ext<SettingsDeps>,
    Ext(cdn): Ext<crate::routes::cdn::CdnDeps>,
    Path((guild_id, emoji_id)): Path<(String, String)>,
    Query(q): Query<UrlQuery>,
) -> ApiResult<Json<SignedUrlObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let eid: EmojiId = id("emoji_id", &emoji_id)?;
    deps.guild_access(user, gid).await?;
    let e = deps
        .service
        .repo()
        .get_emoji(gid, eid)
        .await
        .map_err(re)?
        .ok_or(ErrorCode::UnknownEmoji)?;
    Ok(Json(signed_media_url(&state, &cdn, &e.media_key, &q)?))
}

/// Mint a short-lived signed CDN URL for a sticker image (any member).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/stickers/{sticker_id}/url",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("sticker_id" = String, Path, description = "Sticker snowflake"),
        ("size" = Option<u32>, Query, description = "Power of two, 16 to 4096"),
        ("format" = Option<String>, Query, description = "png, jpg, webp or gif"),
    ),
    responses((status = 200, body = SignedUrlObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn sticker_url(
    p: Principal,
    State(state): State<AppState>,
    Ext(deps): Ext<SettingsDeps>,
    Ext(cdn): Ext<crate::routes::cdn::CdnDeps>,
    Path((guild_id, sticker_id)): Path<(String, String)>,
    Query(q): Query<UrlQuery>,
) -> ApiResult<Json<SignedUrlObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let sid: StickerId = id("sticker_id", &sticker_id)?;
    deps.guild_access(user, gid).await?;
    let s = deps
        .service
        .repo()
        .get_sticker(gid, sid)
        .await
        .map_err(re)?
        .ok_or(ErrorCode::UnknownEmoji)?;
    Ok(Json(signed_media_url(&state, &cdn, &s.media_key, &q)?))
}
