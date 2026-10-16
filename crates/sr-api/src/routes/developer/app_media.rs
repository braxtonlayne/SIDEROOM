//! Application emoji and Rich Presence art assets (DEV-PORT-003, Emojis and Rich Presence
//! tabs). Owner session only. Images arrive as `data:` URIs like guild emoji, are sniffed,
//! shrunk and stored under `emoji/app/...` and `emoji/app-assets/...`; the list objects carry
//! a short-lived signed URL for the portal thumbnails.
//!
//! Application emoji are managed here; sending them in messages and rendering them is not
//! built (a bot cannot yet use `<:name:id>` of an application emoji).

use super::{DevDeps, app_err, invalid, parse_id};
use crate::{
    AppState,
    auth::SessionOnly,
    openapi::ErrorBody,
    routes::{
        Ext,
        messages::{MessagesDeps, error::ApiJson, error::MsgError, error::MsgResult},
        settings::expressions::{
            Image, UrlQuery, decode_data_uri, fit_image, signed_media_url, sniff,
        },
    },
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sr_apps::model::{AppAsset, AppEmoji};
use sr_core::ErrorCode;
use utoipa::ToSchema;

const APP: ErrorCode = ErrorCode::UnknownApplication;
const ITEM: ErrorCode = ErrorCode::SrNotFound;

/// Largest upload of an application emoji (same as guild emoji).
const MAX_EMOJI_BYTES: usize = 256 * 1024;
/// Largest upload of a Rich Presence asset.
const MAX_ASSET_BYTES: usize = 2 * 1024 * 1024;
/// Longest edge of a stored emoji.
const EMOJI_PX: u32 = 128;
/// Longest edge of a stored asset (Discord: 1024 for large images).
const ASSET_PX: u32 = 1024;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AppEmojiObject {
    pub id: String,
    pub name: String,
    pub animated: bool,
    /// Relative signed CDN URL for the portal; expires.
    pub url: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AppEmojiList {
    pub items: Vec<AppEmojiObject>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateAppEmojiBody {
    /// 2 to 32 letters, digits and underscores.
    pub name: String,
    /// `data:image/png;base64,...` (PNG, JPEG, GIF or WebP; at most 256 KiB).
    pub image: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RenameAppEmojiBody {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AppAssetObject {
    pub id: String,
    pub name: String,
    /// 1 large image, 2 small image.
    #[serde(rename = "type")]
    pub kind: i16,
    /// Relative signed CDN URL for the portal; expires.
    pub url: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateAppAssetBody {
    /// 1 to 32 of lowercase letters, digits, `_` and `-`.
    pub name: String,
    /// 1 large image (default), 2 small image.
    #[serde(rename = "type", default = "large")]
    pub kind: i16,
    /// `data:image/png;base64,...` (PNG, JPEG or WebP; at most 2 MiB).
    pub image: String,
}

fn large() -> i16 {
    1
}

/// A stored image: sniffed (PNG, JPEG, GIF or WebP), size-checked, shrunk. Returns the bytes
/// to store, their extension, the original size and whether it is animated.
fn prepare(
    field: &str,
    uri: &str,
    max_bytes: usize,
    px: u32,
    allow_gif: bool,
) -> MsgResult<(bytes::Bytes, &'static str, usize, bool)> {
    let data = decode_data_uri(field, uri).map_err(MsgError::from)?;
    if data.len() > max_bytes {
        return Err(invalid(
            field,
            "IMAGE_TOO_LARGE",
            &format!("The image may be at most {} KiB.", max_bytes / 1024),
        ));
    }
    let img = match sniff(&data) {
        Some(i @ (Image::Png { .. } | Image::Jpeg | Image::Webp { .. })) => i,
        Some(i @ Image::Gif { .. }) if allow_gif => i,
        _ => {
            return Err(invalid(
                field,
                "IMAGE_INVALID",
                if allow_gif {
                    "Must be a PNG, JPEG, GIF or WebP image."
                } else {
                    "Must be a PNG, JPEG or WebP image."
                },
            ));
        }
    };
    let original = data.len();
    let (bytes, ext) = fit_image(data, img.extension(), px);
    Ok((bytes, ext, original, img.animated()))
}

async fn put(messages: &MessagesDeps, key: &str, data: bytes::Bytes) -> MsgResult<()> {
    messages.storage.put(key, data).await.map_err(|e| {
        tracing::error!(error = %e, key, "object storage failure");
        MsgError::from(ErrorCode::SrInternal)
    })
}

fn sign(state: &AppState, cdn: &crate::routes::cdn::CdnDeps, key: &str) -> MsgResult<String> {
    Ok(signed_media_url(state, cdn, key, &UrlQuery::default())
        .map_err(MsgError::from)?
        .url)
}

fn emoji_object(
    state: &AppState,
    cdn: &crate::routes::cdn::CdnDeps,
    e: &AppEmoji,
) -> MsgResult<AppEmojiObject> {
    Ok(AppEmojiObject {
        id: e.id.to_string(),
        name: e.name.clone(),
        animated: e.animated,
        url: sign(state, cdn, &e.media_key)?,
        created_at: super::iso(e.created_at_ms),
    })
}

fn asset_object(
    state: &AppState,
    cdn: &crate::routes::cdn::CdnDeps,
    a: &AppAsset,
) -> MsgResult<AppAssetObject> {
    Ok(AppAssetObject {
        id: a.id.to_string(),
        name: a.name.clone(),
        kind: a.kind,
        url: sign(state, cdn, &a.media_key)?,
    })
}

/// Emoji of my application.
#[utoipa::path(
    get, path = "/applications/{application_id}/emojis", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake")),
    responses((status = 200, body = AppEmojiList), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list_emojis(
    SessionOnly(p): SessionOnly,
    State(state): State<AppState>,
    Ext(deps): Ext<DevDeps>,
    Ext(cdn): Ext<crate::routes::cdn::CdnDeps>,
    Path(app): Path<String>,
) -> MsgResult<Json<AppEmojiList>> {
    let app = parse_id(&app, APP)?;
    let rows = deps
        .apps
        .list_app_emojis(p.user_id, app)
        .await
        .map_err(app_err)?;
    let items = rows
        .iter()
        .map(|e| emoji_object(&state, &cdn, e))
        .collect::<MsgResult<Vec<_>>>()?;
    Ok(Json(AppEmojiList { items }))
}

/// Upload an emoji for my application.
#[utoipa::path(
    post, path = "/applications/{application_id}/emojis", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake")),
    request_body = CreateAppEmojiBody,
    responses((status = 201, body = AppEmojiObject), (status = 400, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_emoji(
    SessionOnly(p): SessionOnly,
    State(state): State<AppState>,
    Ext(deps): Ext<DevDeps>,
    Ext(messages): Ext<MessagesDeps>,
    Ext(cdn): Ext<crate::routes::cdn::CdnDeps>,
    Path(app): Path<String>,
    ApiJson(body): ApiJson<CreateAppEmojiBody>,
) -> MsgResult<(StatusCode, Json<AppEmojiObject>)> {
    let app = parse_id(&app, APP)?;
    // Fail on ownership and the name before touching storage.
    deps.apps.owned(p.user_id, app).await.map_err(app_err)?;
    if !sr_apps::service::valid_emoji_name(&body.name) {
        return Err(invalid(
            "name",
            "STRING_TYPE_REGEX",
            "Use 2 to 32 letters, digits or underscores.",
        ));
    }
    let (data, ext, original, animated) =
        prepare("image", &body.image, MAX_EMOJI_BYTES, EMOJI_PX, true)?;
    let id = deps.apps.new_media_id();
    let key = format!("emoji/app/{app}/{id}.{ext}");
    put(&messages, &key, data).await?;
    let e = match deps
        .apps
        .add_app_emoji(
            p.user_id,
            app,
            sr_apps::service::NewAppEmoji {
                id,
                name: body.name.clone(),
                animated,
                size_bytes: i64::try_from(original).unwrap_or(0),
                media_key: key.clone(),
            },
        )
        .await
    {
        Ok(e) => e,
        Err(err) => {
            // Do not leave an image without a row.
            let _ = messages.storage.delete(&key).await;
            return Err(app_err(err));
        }
    };
    Ok((StatusCode::CREATED, Json(emoji_object(&state, &cdn, &e)?)))
}

/// Rename an emoji of my application.
#[utoipa::path(
    patch, path = "/applications/{application_id}/emojis/{emoji_id}", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake"),
           ("emoji_id" = String, Path, description = "Emoji snowflake")),
    request_body = RenameAppEmojiBody,
    responses((status = 200, body = AppEmojiObject), (status = 400, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn rename_emoji(
    SessionOnly(p): SessionOnly,
    State(state): State<AppState>,
    Ext(deps): Ext<DevDeps>,
    Ext(cdn): Ext<crate::routes::cdn::CdnDeps>,
    Path((app, id)): Path<(String, String)>,
    ApiJson(body): ApiJson<RenameAppEmojiBody>,
) -> MsgResult<Json<AppEmojiObject>> {
    let (app, id) = (parse_id(&app, APP)?, parse_id(&id, ITEM)?);
    let e = deps
        .apps
        .rename_app_emoji(p.user_id, app, id, &body.name)
        .await
        .map_err(app_err)?;
    Ok(Json(emoji_object(&state, &cdn, &e)?))
}

/// Delete an emoji of my application.
#[utoipa::path(
    delete, path = "/applications/{application_id}/emojis/{emoji_id}", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake"),
           ("emoji_id" = String, Path, description = "Emoji snowflake")),
    responses((status = 204), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_emoji(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    Ext(messages): Ext<MessagesDeps>,
    Path((app, id)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let (app, id) = (parse_id(&app, APP)?, parse_id(&id, ITEM)?);
    let e = deps
        .apps
        .delete_app_emoji(p.user_id, app, id)
        .await
        .map_err(app_err)?;
    // The row is gone; a failed image delete only leaves an orphan object.
    let _ = messages.storage.delete(&e.media_key).await;
    Ok(StatusCode::NO_CONTENT)
}

/// Rich Presence assets of my application.
#[utoipa::path(
    get, path = "/applications/{application_id}/assets", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake")),
    responses((status = 200, body = Vec<AppAssetObject>), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list_assets(
    SessionOnly(p): SessionOnly,
    State(state): State<AppState>,
    Ext(deps): Ext<DevDeps>,
    Ext(cdn): Ext<crate::routes::cdn::CdnDeps>,
    Path(app): Path<String>,
) -> MsgResult<Json<Vec<AppAssetObject>>> {
    let app = parse_id(&app, APP)?;
    let rows = deps
        .apps
        .list_app_assets(p.user_id, app)
        .await
        .map_err(app_err)?;
    Ok(Json(
        rows.iter()
            .map(|a| asset_object(&state, &cdn, a))
            .collect::<MsgResult<Vec<_>>>()?,
    ))
}

/// Upload a Rich Presence asset.
#[utoipa::path(
    post, path = "/applications/{application_id}/assets", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake")),
    request_body = CreateAppAssetBody,
    responses((status = 201, body = AppAssetObject), (status = 400, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_asset(
    SessionOnly(p): SessionOnly,
    State(state): State<AppState>,
    Ext(deps): Ext<DevDeps>,
    Ext(messages): Ext<MessagesDeps>,
    Ext(cdn): Ext<crate::routes::cdn::CdnDeps>,
    Path(app): Path<String>,
    ApiJson(body): ApiJson<CreateAppAssetBody>,
) -> MsgResult<(StatusCode, Json<AppAssetObject>)> {
    let app = parse_id(&app, APP)?;
    deps.apps.owned(p.user_id, app).await.map_err(app_err)?;
    if !sr_apps::service::valid_asset_name(&body.name) {
        return Err(invalid(
            "name",
            "STRING_TYPE_REGEX",
            "Use 1 to 32 lowercase letters, digits, underscores or dashes.",
        ));
    }
    if !matches!(body.kind, 1 | 2) {
        return Err(invalid(
            "type",
            "BASE_TYPE_CHOICES",
            "Must be 1 (large image) or 2 (small image).",
        ));
    }
    let (data, ext, _, _) = prepare("image", &body.image, MAX_ASSET_BYTES, ASSET_PX, false)?;
    let id = deps.apps.new_media_id();
    let key = format!("emoji/app-assets/{app}/{id}.{ext}");
    put(&messages, &key, data).await?;
    let a = match deps
        .apps
        .add_app_asset(p.user_id, app, id, &body.name, body.kind, key.clone())
        .await
    {
        Ok(a) => a,
        Err(err) => {
            let _ = messages.storage.delete(&key).await;
            return Err(app_err(err));
        }
    };
    Ok((StatusCode::CREATED, Json(asset_object(&state, &cdn, &a)?)))
}

/// Delete a Rich Presence asset.
#[utoipa::path(
    delete, path = "/applications/{application_id}/assets/{asset_id}", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake"),
           ("asset_id" = String, Path, description = "Asset snowflake")),
    responses((status = 204), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_asset(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    Ext(messages): Ext<MessagesDeps>,
    Path((app, id)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let (app, id) = (parse_id(&app, APP)?, parse_id(&id, ITEM)?);
    let a = deps
        .apps
        .delete_app_asset(p.user_id, app, id)
        .await
        .map_err(app_err)?;
    let _ = messages.storage.delete(&a.media_key).await;
    Ok(StatusCode::NO_CONTENT)
}
