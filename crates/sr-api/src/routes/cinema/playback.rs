//! Playback endpoints of the Cinema player (docs/specs/cinema-media.md section 10):
//!
//! * `GET /cinema/assets/{asset_id}/playback`: the signed master playlist URL with its expiry, the subtitle
//!   and audio tracks, scrub-thumbnail info and the skip markers;
//! * `POST /cinema/assets/{asset_id}/refresh`: a fresh grant before the old one expires.
//!
//! Both run the catalog play check first ([`sr_cinema::CinemaService::authorize_asset`]: the asset must belong
//! to a published title, episode, cut or trailer, and a restricted title needs age confirmation, 900011), and
//! only then read renditions through a [`PlaybackSource`]. Times are Unix seconds, as the player expects.
//!
//! [`PlaybackDeps`] also implements the CDN's [`PlaylistRenderer`]: every `.m3u8` of `/cdn/hls/<asset>/...` and
//! the sprite map are rendered on request with `sr_media::playlist`, so the URLs inside carry fresh signatures.

use super::{MemberCtx, id, map_err};
use crate::{
    error::{ApiResult, HttpError},
    openapi::ErrorBody,
    routes::cdn::{PlaylistRenderer, Rendered},
};
use axum::{Json, extract::Path};
use serde::Serialize;
use sr_cinema::model::Marker;
use sr_core::{Clock, ErrorCode};
use sr_media::playback::{PlaybackAsset, PlaybackSource};
use sr_media::playlist::{self, PlaylistConfig, UrlSigner};
use std::sync::Arc;
use utoipa::ToSchema;

/// What the playback routes and the playlist renderer need: renditions, the signer and a clock.
pub struct PlaybackDeps {
    source: Arc<dyn PlaybackSource>,
    signer: Arc<dyn UrlSigner>,
    clock: Arc<dyn Clock>,
}

impl PlaybackDeps {
    /// `signer` is the CDN signer (the same one the CDN route verifies with).
    pub fn new(
        source: Arc<dyn PlaybackSource>,
        signer: Arc<dyn UrlSigner>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            source,
            signer,
            clock,
        }
    }

    fn now(&self) -> u64 {
        u64::try_from(self.clock.now_unix_ms() / 1000).unwrap_or(0)
    }

    fn config(asset: i64) -> PlaylistConfig {
        PlaylistConfig::new(format!("/cdn/hls/{asset}"))
    }

    async fn asset(&self, id: i64) -> Result<PlaybackAsset, HttpError> {
        match self.source.asset(id).await {
            Ok(Some(a)) => Ok(a),
            Ok(None) => Err(super::not_found("asset")),
            Err(e) => {
                tracing::error!(error = %e, "playback source failed");
                Err(ErrorCode::SrInternal.into())
            }
        }
    }
}

/// One subtitle track the player can load.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaSubtitleTrack)]
pub struct SubtitleTrack {
    pub id: String,
    pub language: Option<String>,
    pub label: String,
    pub forced: bool,
    pub is_default: bool,
    /// Signed WebVTT URL.
    pub vtt_url: String,
    /// Signed ASS URL when the source was styled (the player may render it instead).
    pub ass_url: Option<String>,
}

/// One audio rendition (the master playlist offers them to the player's own track menu).
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaAudioTrack)]
pub struct AudioTrack {
    pub id: String,
    pub language: Option<String>,
    pub label: String,
    pub channels: u32,
    pub is_default: bool,
}

/// Scrub-thumbnail geometry (the sprite map itself is `sprites_url`).
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaSpriteInfo)]
pub struct SpriteInfo {
    pub interval_secs: u32,
    pub thumb_width: u32,
    pub thumb_height: u32,
    pub columns: u32,
    pub rows: u32,
    pub thumbs_total: u32,
}

/// Everything the player needs to start an asset.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaAssetPlayback)]
pub struct AssetPlayback {
    /// Signed URL of the master playlist.
    pub master_url: String,
    /// Unix seconds when every URL below stops working.
    pub expires_at: u64,
    /// Unix seconds when the player should ask `refresh` for a new grant.
    pub refresh_at: u64,
    pub duration_secs: Option<f64>,
    pub subtitles: Vec<SubtitleTrack>,
    pub audio_tracks: Vec<AudioTrack>,
    /// Signed WebVTT map of the scrub thumbnails (`#xywh=` cues), when the asset has sprites.
    pub sprites_url: Option<String>,
    pub sprite: Option<SpriteInfo>,
    /// Skip windows (intro, recap, credits) of the title or episode the asset belongs to.
    pub markers: Vec<Marker>,
}

/// A fresh manifest grant.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaPlaybackGrant)]
pub struct PlaybackGrant {
    pub master_url: String,
    pub expires_at: u64,
    pub refresh_at: u64,
}

fn deps(ctx: &MemberCtx) -> Result<Arc<PlaybackDeps>, HttpError> {
    ctx.deps.playback.clone().ok_or_else(|| {
        tracing::error!("cinema playback needs PlaybackDeps registered on CinemaDeps");
        ErrorCode::SrInternal.into()
    })
}

fn label(name: &str, lang: Option<&str>) -> String {
    if !name.is_empty() {
        name.to_owned()
    } else {
        lang.unwrap_or("Track").to_owned()
    }
}

fn build(d: &PlaybackDeps, asset: &PlaybackAsset, markers: Vec<Marker>, now: u64) -> AssetPlayback {
    let cfg = PlaybackDeps::config(asset.id);
    let grant = playlist::grant(&cfg, d.signer.as_ref(), now);
    let expires = grant.expires_at;
    let url = |rel: &str| {
        d.signer
            .signed_url(&format!("{}/{}", cfg.base_path, rel), expires)
    };
    let subtitles = asset
        .media
        .subtitles
        .iter()
        .enumerate()
        .map(|(n, s)| {
            let extra = asset.extra(&s.vtt_path);
            SubtitleTrack {
                id: extra.map_or_else(|| n.to_string(), |e| e.id.clone()),
                language: s.language_tag.clone(),
                label: label(&s.name, s.language_tag.as_deref()),
                forced: s.forced,
                is_default: s.is_default,
                vtt_url: url(&s.vtt_path),
                ass_url: extra.and_then(|e| e.ass_path.as_deref()).map(url),
            }
        })
        .collect();
    let audio_tracks = asset
        .media
        .audios
        .iter()
        .enumerate()
        .map(|(n, a)| AudioTrack {
            id: n.to_string(),
            language: a.language_tag.clone(),
            label: label(&a.name, a.language_tag.as_deref()),
            channels: a.channels,
            is_default: a.is_default,
        })
        .collect();
    AssetPlayback {
        master_url: grant.master_url,
        expires_at: grant.expires_at,
        refresh_at: grant.refresh_at,
        duration_secs: asset.duration_secs(),
        subtitles,
        audio_tracks,
        sprites_url: asset.sprite.as_ref().map(|s| url(&s.vtt_path)),
        sprite: asset.sprite.as_ref().map(|s| SpriteInfo {
            interval_secs: s.interval_secs,
            thumb_width: s.thumb_width,
            thumb_height: s.thumb_height,
            columns: s.columns,
            rows: s.rows,
            thumbs_total: s.thumbs_total,
        }),
        markers,
    }
}

/// Start playing an asset: master playlist URL, tracks, thumbnails and skip markers. The asset must belong to
/// a published title, episode, cut or trailer; a restricted title answers 403 with code 900011 until the
/// account confirmed it is 18 or older (D-184).
#[utoipa::path(
    get, path = "/cinema/assets/{asset_id}/playback", operation_id = "cinema_asset_playback",
    params(("asset_id" = String, Path, description = "Asset id from the play target")),
    responses(
        (status = 200, body = AssetPlayback),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody, description = "Age confirmation required (code 900011)"),
        (status = 404, body = ErrorBody)
    ),
    security(("session" = []))
)]
pub async fn asset_playback(
    ctx: MemberCtx,
    Path(asset_id): Path<String>,
) -> ApiResult<Json<AssetPlayback>> {
    let asset_id = id(&asset_id, "asset")?;
    let d = deps(&ctx)?;
    let access = ctx
        .deps
        .service
        .authorize_asset(ctx.viewer, asset_id)
        .await
        .map_err(map_err)?;
    let asset = d.asset(asset_id).await?;
    Ok(Json(build(&d, &asset, access.markers, d.now())))
}

/// A new manifest grant for an asset being played, before the old one expires. Runs the same checks as
/// `playback`; nothing is stored per viewer.
#[utoipa::path(
    post, path = "/cinema/assets/{asset_id}/refresh", operation_id = "cinema_asset_refresh",
    params(("asset_id" = String, Path, description = "Asset id from the play target")),
    responses(
        (status = 200, body = PlaybackGrant),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody, description = "Age confirmation required (code 900011)"),
        (status = 404, body = ErrorBody)
    ),
    security(("session" = []))
)]
pub async fn asset_refresh(
    ctx: MemberCtx,
    Path(asset_id): Path<String>,
) -> ApiResult<Json<PlaybackGrant>> {
    let asset_id = id(&asset_id, "asset")?;
    let d = deps(&ctx)?;
    ctx.deps
        .service
        .authorize_asset(ctx.viewer, asset_id)
        .await
        .map_err(map_err)?;
    d.asset(asset_id).await?;
    let g = playlist::grant(&PlaybackDeps::config(asset_id), d.signer.as_ref(), d.now());
    Ok(Json(PlaybackGrant {
        master_url: g.master_url,
        expires_at: g.expires_at,
        refresh_at: g.refresh_at,
    }))
}

const PLAYLIST: &str = "application/vnd.apple.mpegurl";

#[async_trait::async_trait]
impl PlaylistRenderer for PlaybackDeps {
    async fn render(&self, asset_id: i64, rel: &str, now: u64) -> Option<Rendered> {
        let is_playlist = rel.ends_with(".m3u8");
        let sprite_map = rel.ends_with("/sprites.vtt") || rel == "sprites.vtt";
        if !is_playlist && !sprite_map {
            return None;
        }
        let asset = self.source.asset(asset_id).await.ok().flatten()?;
        let cfg = Self::config(asset_id);
        let signer = self.signer.as_ref();
        let media = &asset.media;
        let (content_type, body) = if rel == "master.m3u8" {
            (PLAYLIST, playlist::render_master(media, &cfg, signer, now))
        } else if sprite_map {
            let sprite = asset.sprite.as_ref().filter(|s| s.vtt_path == rel)?;
            let secs = asset.duration_secs()?;
            (
                "text/vtt",
                playlist::render_sprite_vtt(sprite, secs, &cfg, signer, now),
            )
        } else if let Some(v) = media.videos.iter().find(|v| v.playlist_path == rel) {
            (
                PLAYLIST,
                playlist::render_video_playlist(v, &cfg, signer, now),
            )
        } else if let Some(a) = media.audios.iter().find(|a| a.playlist_path == rel) {
            (
                PLAYLIST,
                playlist::render_audio_playlist(a, &cfg, signer, now),
            )
        } else {
            let s = media.subtitles.iter().find(|s| s.playlist_path == rel)?;
            (
                PLAYLIST,
                playlist::render_subtitle_playlist(s, &cfg, signer, now),
            )
        };
        Some(Rendered { content_type, body })
    }

    async fn storage_key(&self, asset_id: i64, rel: &str) -> Option<String> {
        let asset = self.source.asset(asset_id).await.ok().flatten()?;
        Some(format!("{}/{}", asset.storage_prefix, rel))
    }
}
