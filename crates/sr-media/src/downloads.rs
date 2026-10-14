//! Patron downloads (CIN-DL-001, spec section 5): an on-demand stream-copy MP4 of a chosen
//! rendition, cached for 7 days and shared between members, or the original when staff allow it.
//!
//! * Entitlement: [`DownloadEntitlements`] says who may download and how many times a day. The
//!   donations work (perks, doc 08) does not exist yet; the Postgres implementation reads the
//!   `cinema_download_grants` table, which that work (or staff) fills.
//! * A request creates a `cinema_downloads` row (that is what the daily limit counts) and, for a
//!   rendition nobody prepared yet, a `remux` transcode job planned by [`crate::remux`]. The
//!   workers run it; when it completes the cache entry turns `ready` (see the Postgres job
//!   store) and expires 7 days later.
//! * The link a member gets is `/cinema/downloads/{id}/file`, which only works for the member
//!   who asked and answers with a redirect to a signed CDN URL valid for a few minutes. The CDN
//!   URL alone is therefore not a shareable credential for long.

use crate::assets::{MediaError, MediaRepository, MediaResult};
use crate::playlist::UrlSigner;
use crate::remux::{RemuxPlan, RemuxRequest, plan_remux};
use crate::service::{Clock, IdSource};
use crate::worker_api::{JobSpec, NewJob};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

/// How long a prepared MP4 stays cached.
pub const CACHE_DAYS: i64 = 7;
/// How long the signed URL behind a download link works.
pub const LINK_TTL_SECS: u64 = 15 * 60;
/// Priority of remux jobs: below the ladder, above bulk re-transcodes.
pub const REMUX_PRIORITY: u8 = 6;
/// The `rendition_id` of the original file.
pub const ORIGINAL: &str = "original";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allowance {
    pub per_day: u32,
}

/// Who may download. `None` means not a Patron (403).
#[async_trait]
pub trait DownloadEntitlements: Send + Sync {
    async fn allowance(&self, user: i64) -> MediaResult<Option<Allowance>>;
}

/// Nobody may download (until the donations work provides perks).
pub struct NoDownloads;

#[async_trait]
impl DownloadEntitlements for NoDownloads {
    async fn allowance(&self, _user: i64) -> MediaResult<Option<Allowance>> {
        Ok(None)
    }
}

/// Everybody gets the same allowance (tests, tools).
pub struct FixedAllowance(pub u32);

#[async_trait]
impl DownloadEntitlements for FixedAllowance {
    async fn allowance(&self, _user: i64) -> MediaResult<Option<Allowance>> {
        Ok(Some(Allowance { per_day: self.0 }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DownloadKind {
    Remux,
    Original,
}

/// One rendition the picker offers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct DownloadRendition {
    /// `1080p`, `720p`, ... or `original`.
    pub id: String,
    pub label: String,
    pub height: Option<u32>,
    pub size_bytes: u64,
    pub kind: DownloadKind,
    /// Prepared and cached: no job is needed.
    pub cached: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct DownloadOptions {
    pub renditions: Vec<DownloadRendition>,
    /// Downloads left today; `null` when unlimited.
    pub daily_remaining: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    Queued,
    Preparing,
    Ready,
    Failed,
}

/// What the download modal polls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct DownloadJob {
    pub id: String,
    pub state: DownloadState,
    /// 0 to 1.
    pub progress: f32,
    /// The link to fetch the file (needs the same session), once `ready`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// When the prepared file is deleted, Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ---- persistence port ----------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheState {
    Queued,
    Preparing,
    Ready,
    Failed,
    Expired,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CacheEntry {
    pub id: i64,
    pub asset_id: i64,
    pub rendition: String,
    pub state: CacheState,
    pub job_id: Option<i64>,
    /// Progress of the remux job, 0..=100.
    pub job_percent: Option<f32>,
    pub object_key: Option<String>,
    pub size_bytes: Option<i64>,
    pub error: Option<String>,
    pub expires_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadRow {
    pub id: i64,
    pub user_id: i64,
    pub title_id: i64,
    pub episode_id: Option<i64>,
    pub asset_id: i64,
    pub rendition: String,
    pub cache_id: Option<i64>,
}

/// A rendition's name, height and size for the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRendition {
    pub name: String,
    pub height: u32,
    pub size_bytes: u64,
}

#[async_trait]
pub trait DownloadRepository: Send + Sync {
    /// Requests the member made since `since_ms`.
    async fn count_since(&self, user: i64, since_ms: i64) -> MediaResult<u32>;
    async fn renditions(&self, asset_id: i64) -> MediaResult<Vec<StoredRendition>>;
    /// The files of one rendition (and the audio tracks) as a remux request; `None` when the
    /// asset has no such rendition.
    async fn remux_request(
        &self,
        asset_id: i64,
        rendition: &str,
    ) -> MediaResult<Option<RemuxRequest>>;
    /// The live (queued, preparing or ready and not expired) cache entry.
    async fn live_cache(&self, asset_id: i64, rendition: &str) -> MediaResult<Option<CacheEntry>>;
    /// Create the cache entry and enqueue its remux job in one transaction; `None` when another
    /// request created a live entry first.
    async fn create_cache(
        &self,
        id: i64,
        asset_id: i64,
        rendition: &str,
        job: NewJob,
    ) -> MediaResult<Option<CacheEntry>>;
    async fn create_download(&self, row: &DownloadRow) -> MediaResult<()>;
    async fn download(&self, id: i64) -> MediaResult<Option<(DownloadRow, Option<CacheEntry>)>>;
    /// Ready entries past their expiry, marked `expired`; the caller deletes the objects.
    async fn expire_cache(&self, now_ms: i64, limit: u32) -> MediaResult<Vec<CacheEntry>>;
}

// ---- service ------------------------------------------------------------------------------

pub struct DownloadService {
    repo: Arc<dyn DownloadRepository>,
    media: Arc<dyn MediaRepository>,
    entitlements: Arc<dyn DownloadEntitlements>,
    signer: Arc<dyn UrlSigner>,
    storage: Arc<dyn sr_storage::ObjectStorage>,
    ids: IdSource,
    clock: Clock,
}

/// `/cdn/...` path serving a storage key, when a CDN kind exists for it.
pub fn cdn_path_for_key(key: &str) -> Option<String> {
    let (prefix, rest) = key.split_once('/')?;
    let kind = match prefix {
        "originals" => "originals",
        "transcode" => "hls",
        _ => return None,
    };
    if rest.is_empty() {
        return None;
    }
    Some(format!("/cdn/{kind}/{rest}"))
}

fn day_ago_ms(now_secs: u64) -> i64 {
    i64::try_from(now_secs.saturating_sub(24 * 3600).saturating_mul(1000)).unwrap_or(0)
}

impl DownloadService {
    pub fn new(
        repo: Arc<dyn DownloadRepository>,
        media: Arc<dyn MediaRepository>,
        entitlements: Arc<dyn DownloadEntitlements>,
        signer: Arc<dyn UrlSigner>,
        storage: Arc<dyn sr_storage::ObjectStorage>,
        ids: IdSource,
        clock: Clock,
    ) -> Self {
        Self {
            repo,
            media,
            entitlements,
            signer,
            storage,
            ids,
            clock,
        }
    }

    fn now(&self) -> u64 {
        (self.clock)()
    }

    async fn allowance(&self, user: i64) -> MediaResult<Allowance> {
        self.entitlements
            .allowance(user)
            .await?
            .ok_or_else(|| MediaError::Forbidden("Downloads are for Patrons.".into()))
    }

    /// Fails with `Forbidden` unless the member holds the download perk.
    pub async fn check_entitled(&self, user: i64) -> MediaResult<()> {
        self.allowance(user).await.map(|_| ())
    }

    /// The picker for an asset the caller may open.
    pub async fn options(&self, user: i64, asset_id: i64) -> MediaResult<DownloadOptions> {
        let allowance = self.allowance(user).await?;
        let used = self.repo.count_since(user, day_ago_ms(self.now())).await?;
        let asset = self
            .media
            .get_asset(asset_id)
            .await?
            .ok_or(MediaError::NotFound("asset"))?;
        let mut renditions = Vec::new();
        let mut stored = self.repo.renditions(asset_id).await?;
        // Biggest first, like a quality menu.
        stored.sort_by_key(|r| std::cmp::Reverse(r.height));
        for r in stored {
            let cached = self
                .repo
                .live_cache(asset_id, &r.name)
                .await?
                .is_some_and(|c| c.state == CacheState::Ready);
            renditions.push(DownloadRendition {
                label: r.name.clone(),
                id: r.name,
                height: Some(r.height),
                size_bytes: r.size_bytes,
                kind: DownloadKind::Remux,
                cached,
            });
        }
        if asset.original_downloads && !asset.original_deleted {
            renditions.push(DownloadRendition {
                id: ORIGINAL.to_owned(),
                label: "Original".to_owned(),
                height: None,
                size_bytes: u64::try_from(asset.original_size).unwrap_or(0),
                kind: DownloadKind::Original,
                cached: true,
            });
        }
        Ok(DownloadOptions {
            renditions,
            daily_remaining: Some(allowance.per_day.saturating_sub(used)),
        })
    }

    /// Start (or reuse) the download of one rendition. `asset_id` is what the catalog resolved
    /// for the title or episode after its own age and visibility checks.
    pub async fn start(
        &self,
        user: i64,
        title_id: i64,
        episode_id: Option<i64>,
        asset_id: i64,
        rendition: &str,
    ) -> MediaResult<DownloadJob> {
        let allowance = self.allowance(user).await?;
        let used = self.repo.count_since(user, day_ago_ms(self.now())).await?;
        if used >= allowance.per_day {
            return Err(MediaError::RateLimited(
                "You have used today's downloads. Try again tomorrow.".into(),
            ));
        }
        let asset = self
            .media
            .get_asset(asset_id)
            .await?
            .ok_or(MediaError::NotFound("asset"))?;
        let mut row = DownloadRow {
            id: (self.ids)(),
            user_id: user,
            title_id,
            episode_id,
            asset_id,
            rendition: rendition.to_owned(),
            cache_id: None,
        };
        if rendition == ORIGINAL {
            if !asset.original_downloads || asset.original_deleted {
                return Err(MediaError::Forbidden(
                    "The original is not available for download.".into(),
                ));
            }
            self.repo.create_download(&row).await?;
            return self.view(&row, None).await;
        }
        let cache = match self.repo.live_cache(asset_id, rendition).await? {
            Some(c) => c,
            None => self.prepare(asset_id, rendition).await?,
        };
        row.cache_id = Some(cache.id);
        self.repo.create_download(&row).await?;
        self.view(&row, Some(&cache)).await
    }

    async fn prepare(&self, asset_id: i64, rendition: &str) -> MediaResult<CacheEntry> {
        let req = self
            .repo
            .remux_request(asset_id, rendition)
            .await?
            .ok_or_else(|| MediaError::Invalid("unknown rendition".into()))?;
        let plan: RemuxPlan = plan_remux(&req)
            .map_err(|e| MediaError::Conflict(format!("this rendition cannot be remuxed: {e}")))?;
        let total = plan.total_input_bytes();
        let job = NewJob {
            asset_id: u64::try_from(asset_id).unwrap_or_default(),
            spec: JobSpec::Remux {
                plan: Box::new(plan),
            },
            source_key: String::new(),
            source_size: total,
            source_extension: "m3u8".to_owned(),
            priority: REMUX_PRIORITY,
        };
        if let Some(c) = self
            .repo
            .create_cache((self.ids)(), asset_id, rendition, job)
            .await?
        {
            return Ok(c);
        }
        // Somebody else prepared it a moment ago.
        self.repo
            .live_cache(asset_id, rendition)
            .await?
            .ok_or_else(|| MediaError::Conflict("the download could not be prepared".into()))
    }

    /// State of a download, for its owner only (anyone else sees 404).
    pub async fn job(&self, user: i64, id: i64) -> MediaResult<DownloadJob> {
        let (row, cache) = self.owned(user, id).await?;
        self.view(&row, cache.as_ref()).await
    }

    async fn owned(&self, user: i64, id: i64) -> MediaResult<(DownloadRow, Option<CacheEntry>)> {
        match self.repo.download(id).await? {
            Some((row, cache)) if row.user_id == user => Ok((row, cache)),
            _ => Err(MediaError::NotFound("download")),
        }
    }

    /// The signed CDN URL of a ready download, for its owner while still entitled.
    pub async fn file_url(&self, user: i64, id: i64) -> MediaResult<String> {
        self.allowance(user).await?;
        let (row, cache) = self.owned(user, id).await?;
        let key = if row.rendition == ORIGINAL {
            let asset = self
                .media
                .get_asset(row.asset_id)
                .await?
                .ok_or(MediaError::NotFound("asset"))?;
            if !asset.original_downloads || asset.original_deleted {
                return Err(MediaError::Forbidden(
                    "The original is not available for download.".into(),
                ));
            }
            asset.original_key
        } else {
            let c = cache.ok_or(MediaError::NotFound("download"))?;
            let fresh = c
                .expires_at_ms
                .is_none_or(|e| e > i64::try_from(self.now() * 1000).unwrap_or(i64::MAX));
            match (c.state, c.object_key) {
                (CacheState::Ready, Some(k)) if fresh => k,
                (CacheState::Expired, _) => {
                    return Err(MediaError::Conflict(
                        "The prepared file expired; request it again.".into(),
                    ));
                }
                _ => {
                    return Err(MediaError::Conflict(
                        "The download is not ready yet.".into(),
                    ));
                }
            }
        };
        let path = cdn_path_for_key(&key)
            .ok_or_else(|| MediaError::Backend("no CDN path for the stored file".into()))?;
        Ok(self.signer.signed_url(&path, self.now() + LINK_TTL_SECS))
    }

    async fn view(
        &self,
        row: &DownloadRow,
        cache: Option<&CacheEntry>,
    ) -> MediaResult<DownloadJob> {
        let id = row.id.to_string();
        let file = format!("/cinema/downloads/{id}/file");
        let Some(c) = cache else {
            // The original: nothing to prepare.
            return Ok(DownloadJob {
                id,
                state: DownloadState::Ready,
                progress: 1.0,
                url: Some(file),
                expires_at_ms: None,
                error: None,
            });
        };
        let (state, progress) = match c.state {
            CacheState::Ready => (DownloadState::Ready, 1.0),
            CacheState::Failed | CacheState::Expired => (DownloadState::Failed, 0.0),
            CacheState::Queued if c.job_percent.is_some() => (
                DownloadState::Preparing,
                (c.job_percent.unwrap_or(0.0) / 100.0).clamp(0.0, 0.99),
            ),
            CacheState::Preparing => (
                DownloadState::Preparing,
                (c.job_percent.unwrap_or(0.0) / 100.0).clamp(0.0, 0.99),
            ),
            CacheState::Queued => (DownloadState::Queued, 0.0),
        };
        let error = match c.state {
            CacheState::Failed => Some(
                c.error
                    .clone()
                    .unwrap_or_else(|| "The download could not be prepared.".to_owned()),
            ),
            CacheState::Expired => Some("The prepared file expired; request it again.".to_owned()),
            _ => None,
        };
        Ok(DownloadJob {
            id,
            state,
            progress,
            url: (state == DownloadState::Ready).then_some(file),
            expires_at_ms: c.expires_at_ms,
            error,
        })
    }

    /// The periodic cleanup: prepared files past their 7 days are deleted from storage.
    pub async fn sweep(&self) -> MediaResult<u32> {
        let now_ms = i64::try_from(self.now().saturating_mul(1000)).unwrap_or(i64::MAX);
        let expired = self.repo.expire_cache(now_ms, 200).await?;
        let mut n = 0;
        for c in expired {
            if let Some(key) = c.object_key {
                match self.storage.delete(&key).await {
                    Ok(()) | Err(sr_storage::StorageError::NotFound(_)) => n += 1,
                    Err(e) => {
                        tracing::warn!(error = %e, key, "could not delete an expired download")
                    }
                }
            }
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_keys_map_to_cdn_paths() {
        assert_eq!(
            cdn_path_for_key("originals/12/source.mkv").as_deref(),
            Some("/cdn/originals/12/source.mkv")
        );
        assert_eq!(
            cdn_path_for_key("transcode/9/1/remux/video.mp4").as_deref(),
            Some("/cdn/hls/9/1/remux/video.mp4")
        );
        assert_eq!(cdn_path_for_key("chat/1/2/a.png"), None);
        assert_eq!(cdn_path_for_key("originals/"), None);
        assert_eq!(cdn_path_for_key("noslash"), None);
    }

    #[test]
    fn day_window() {
        assert_eq!(day_ago_ms(100_000), (100_000 - 86_400) * 1000);
        assert_eq!(day_ago_ms(10), 0);
    }
}
