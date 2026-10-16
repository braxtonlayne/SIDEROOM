//! The reviewer's video proxy player (REV-RV-002): for a Cinema submission, each video under review
//! with a signed URL of its 480p review proxy, so staff can watch and scrub before the full ladder
//! exists (spec section 2).

use super::{ReviewerCtx, id, map_err};
use crate::routes::cdn::path::{KINDS, cdn_path};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{Json, extract::Path};
use serde::Serialize;
use sr_media::assets::{AssetState, MediaRepository};
use sr_storage::cdn::CdnSigner;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use utoipa::ToSchema;

/// How long a proxy URL works.
const URL_TTL_SECS: u64 = 2 * 60 * 60;

/// A signed `/cdn/...` URL for an object key, when its prefix is one the CDN serves.
pub fn proxy_url(signer: &CdnSigner, key: &str, now_s: u64) -> Option<String> {
    let (kind, prefix) = KINDS
        .iter()
        .find(|(_, p)| key.strip_prefix(*p).is_some_and(|r| r.starts_with('/')))?;
    let segs: Vec<&str> = key[prefix.len() + 1..].split('/').collect();
    Some(signer.signed_url(&cdn_path(kind, &segs), now_s + URL_TTL_SECS))
}

/// Reads the review proxies of media assets and signs their URLs.
pub struct ReviewMedia {
    repo: Arc<dyn MediaRepository>,
    signer: CdnSigner,
}

impl ReviewMedia {
    pub fn new(repo: Arc<dyn MediaRepository>, signer: CdnSigner) -> Self {
        Self { repo, signer }
    }

    async fn view(&self, asset: Option<i64>, now_s: u64) -> (String, Option<String>, Option<i64>) {
        let Some(asset) = asset else {
            return ("none".into(), None, None);
        };
        let Ok(Some(a)) = self.repo.get_asset(asset).await else {
            return ("none".into(), None, None);
        };
        if matches!(
            a.state,
            AssetState::Failed | AssetState::Rejected | AssetState::Removed | AssetState::Expired
        ) {
            return ("failed".into(), None, a.duration_ms);
        }
        match a
            .proxy_key
            .as_deref()
            .and_then(|k| proxy_url(&self.signer, k, now_s))
        {
            Some(url) => ("ready".into(), Some(url), a.duration_ms),
            None => ("processing".into(), None, a.duration_ms),
        }
    }
}

/// One video under review.
#[derive(Debug, Serialize, ToSchema)]
pub struct MediaItem {
    #[serde(with = "sr_review::model::id")]
    #[schema(value_type = String)]
    pub ref_id: i64,
    pub label: String,
    /// `none` (no video yet), `processing` (the proxy is not ready), `ready` or `failed`.
    pub state: String,
    /// Signed URL of the 480p proxy (MP4), valid for two hours; only when `ready`.
    pub url: Option<String>,
    pub duration_ms: Option<i64>,
}

/// The videos of a submission under review.
#[derive(Debug, Serialize, ToSchema)]
pub struct MediaList {
    pub items: Vec<MediaItem>,
}

/// The review proxies of the videos in a submission (reviewers of the kind). Kinds without videos
/// answer an empty list.
#[utoipa::path(
    get, path = "/admin/review/submissions/{submission_id}/media", operation_id = "admin_review_media",
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = MediaList), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(
    ctx: ReviewerCtx,
    Path(submission_id): Path<String>,
) -> ApiResult<Json<MediaList>> {
    let refs = ctx
        .svc()
        .media_for_review(ctx.caller(), id(&submission_id, "submission")?)
        .await
        .map_err(map_err)?;
    let now_s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let mut items = Vec::with_capacity(refs.len());
    for r in refs {
        let (state, url, duration_ms) = match &ctx.review.media {
            Some(m) => m.view(r.asset_id, now_s).await,
            None => ("none".into(), None, None),
        };
        items.push(MediaItem {
            ref_id: r.ref_id,
            label: r.label,
            state,
            url,
            duration_ms,
        });
    }
    Ok(Json(MediaList { items }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_keys_map_to_signed_cdn_paths_and_unknown_prefixes_do_not() {
        let signer = CdnSigner::new(vec![5u8; 32]);
        let url = proxy_url(&signer, "transcode/12/1/proxy/proxy.mp4", 1000).unwrap();
        assert!(
            url.starts_with("/cdn/transcode/12/1/proxy/proxy.mp4?ex=8200&sig="),
            "{url}"
        );
        // `video/` objects are served under the `hls` kind.
        let hls = proxy_url(&signer, "video/9/v/proxy.mp4", 0).unwrap();
        assert!(hls.starts_with("/cdn/hls/9/v/proxy.mp4?"), "{hls}");
        assert!(proxy_url(&signer, "secrets/key.bin", 0).is_none());
        assert!(proxy_url(&signer, "transcode", 0).is_none());
    }
}
