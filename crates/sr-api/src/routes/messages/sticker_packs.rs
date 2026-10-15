//! Sticker packs (G-44, P11-MSG-1): platform-wide sets of standard stickers.
//!
//! * `GET /sticker-packs`: the published packs with their stickers (Discord shape,
//!   `{ "sticker_packs": [...] }`). Empty until the owner supplies artwork and a pack is
//!   installed; the sticker picker shows guild stickers only until then.
//! * `GET /stickers/{sticker_id}/url`: a short-lived signed CDN URL for a pack sticker.
//!
//! Sending is unchanged: a pack sticker is a standard sticker (no guild), which the message
//! create path already accepts in any channel.

use super::deps::MessagesDeps;
use super::error::MsgResult;
use crate::routes::settings::expressions::{SignedUrlObject, UrlQuery, signed_media_url};
use crate::routes::{Ext, MaybeExt};
use crate::{HttpError, Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use serde::Serialize;
use sr_core::ErrorCode;
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/sticker-packs", get(list_packs))
        .route("/stickers/{sticker_id}/url", get(sticker_url))
}

/// A sticker inside a pack.
#[derive(Debug, Serialize, ToSchema)]
pub struct PackStickerObject {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: String,
    /// 1 = standard sticker.
    #[serde(rename = "type")]
    pub kind: u8,
    /// 1 PNG, 2 APNG, 3 Lottie, 4 GIF.
    pub format_type: i16,
    pub available: bool,
    pub pack_id: String,
}

/// A sticker pack.
#[derive(Debug, Serialize, ToSchema)]
pub struct StickerPackObject {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub cover_sticker_id: Option<String>,
    pub stickers: Vec<PackStickerObject>,
}

/// Body of `GET /sticker-packs`.
#[derive(Debug, Serialize, ToSchema)]
pub struct StickerPacksObject {
    pub sticker_packs: Vec<StickerPackObject>,
}

/// The published sticker packs (any signed-in user). Empty while no pack is installed.
#[utoipa::path(
    get,
    path = "/sticker-packs",
    responses((status = 200, body = StickerPacksObject), (status = 401, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_packs(
    _p: Principal,
    MaybeExt(deps): MaybeExt<MessagesDeps>,
) -> MsgResult<Json<StickerPacksObject>> {
    let packs = match deps.as_ref().and_then(|d| d.rich.store.as_ref()) {
        Some(store) => store.sticker_packs().await?,
        None => Vec::new(),
    };
    Ok(Json(StickerPacksObject {
        sticker_packs: packs
            .into_iter()
            .map(|p| {
                let pid = p.id.to_string();
                StickerPackObject {
                    id: pid.clone(),
                    name: p.name,
                    description: p.description,
                    cover_sticker_id: p.cover_sticker_id.map(|c| c.to_string()),
                    stickers: p
                        .stickers
                        .into_iter()
                        .map(|s| PackStickerObject {
                            id: s.id.to_string(),
                            name: s.name,
                            description: s.description,
                            tags: s.tags,
                            kind: 1,
                            format_type: s.format_type,
                            available: s.available,
                            pack_id: pid.clone(),
                        })
                        .collect(),
                }
            })
            .collect(),
    }))
}

/// A short-lived signed CDN URL for a pack sticker (any signed-in user).
#[utoipa::path(
    get,
    path = "/stickers/{sticker_id}/url",
    params(
        ("sticker_id" = String, Path, description = "Pack sticker snowflake"),
        ("size" = Option<u32>, Query, description = "Power of two, 16 to 4096"),
        ("format" = Option<String>, Query, description = "png, jpg, webp or gif"),
    ),
    responses((status = 200, body = SignedUrlObject), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn sticker_url(
    _p: Principal,
    State(state): State<AppState>,
    MaybeExt(deps): MaybeExt<MessagesDeps>,
    Ext(cdn): Ext<crate::routes::cdn::CdnDeps>,
    Path(sticker_id): Path<String>,
    Query(q): Query<UrlQuery>,
) -> ApiResult<Json<SignedUrlObject>> {
    let id: u64 = sticker_id
        .parse()
        .ok()
        .filter(|n| *n != 0)
        .ok_or(ErrorCode::UnknownEmoji)?;
    let store = deps
        .as_ref()
        .and_then(|d| d.rich.store.as_ref())
        .ok_or(ErrorCode::UnknownEmoji)?;
    let key = store
        .pack_sticker_media(id)
        .await
        .map_err(|_| HttpError::from(ErrorCode::SrInternal))?
        .ok_or(ErrorCode::UnknownEmoji)?;
    Ok(Json(signed_media_url(&state, &cdn, &key, &q)?))
}
