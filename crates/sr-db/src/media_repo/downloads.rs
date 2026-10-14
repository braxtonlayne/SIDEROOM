//! [`DownloadRepository`] (migration 0422) and the Postgres entitlement source.

use super::{PgMediaRepo, be, is_unique_violation, jobs::insert_job};
use async_trait::async_trait;
use serde_json::Value;
use sqlx::{PgPool, Row, postgres::PgRow};
use sr_media::assets::{MediaError, MediaResult};
use sr_media::downloads::*;
use sr_media::remux::{RemuxFile, RemuxRequest, RemuxTrack};
use sr_media::worker_api::{NewJob, OutputRef, SegmentIndex};
use std::collections::HashMap;

fn cache_state(s: &str, job_state: Option<&str>) -> CacheState {
    match s {
        "ready" => CacheState::Ready,
        "failed" => CacheState::Failed,
        "expired" => CacheState::Expired,
        "preparing" => CacheState::Preparing,
        _ if job_state == Some("running") => CacheState::Preparing,
        _ => CacheState::Queued,
    }
}

const CACHE_SELECT: &str = "SELECT c.id, c.asset_id, c.rendition, c.state, c.job_id, c.object_key, \
    c.size_bytes, c.error, (extract(epoch FROM c.expires_at) * 1000)::bigint AS expires_ms, \
    j.state AS job_state, (j.progress->>'percent')::real AS percent \
    FROM cinema_download_cache c LEFT JOIN transcode_jobs j ON j.id = c.job_id";

fn cache_from_row(r: &PgRow) -> MediaResult<CacheEntry> {
    let state: String = r.try_get("state").map_err(be)?;
    let job_state: Option<String> = r.try_get("job_state").map_err(be)?;
    Ok(CacheEntry {
        id: r.try_get("id").map_err(be)?,
        asset_id: r.try_get("asset_id").map_err(be)?,
        rendition: r.try_get("rendition").map_err(be)?,
        state: cache_state(&state, job_state.as_deref()),
        job_id: r.try_get("job_id").map_err(be)?,
        job_percent: r.try_get("percent").map_err(be)?,
        object_key: r.try_get("object_key").map_err(be)?,
        size_bytes: r.try_get("size_bytes").map_err(be)?,
        error: r.try_get("error").map_err(be)?,
        expires_at_ms: r.try_get("expires_ms").map_err(be)?,
    })
}

fn dir_of(p: &str) -> &str {
    p.rsplit_once('/').map_or("", |(d, _)| d)
}

fn track(idx: &SegmentIndex, init_path: &str, sizes: &HashMap<String, u64>) -> RemuxTrack {
    let dir = dir_of(&idx.playlist);
    let size = |p: &str| sizes.get(p).copied().unwrap_or(0);
    RemuxTrack {
        playlist: idx.playlist.clone(),
        init: init_path.to_owned(),
        segments: idx
            .segments
            .iter()
            .map(|s| RemuxFile {
                path: format!("{dir}/{}", s.name),
                size_bytes: s.size_bytes,
            })
            .collect(),
        playlist_size: size(&idx.playlist),
        init_size: size(init_path),
    }
}

#[async_trait]
impl DownloadRepository for PgMediaRepo {
    async fn count_since(&self, user: i64, since_ms: i64) -> MediaResult<u32> {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM cinema_downloads WHERE user_id = $1 \
               AND created_at > to_timestamp($2::double precision / 1000.0)",
        )
        .bind(user)
        .bind(since_ms)
        .fetch_one(&self.pool)
        .await
        .map_err(be)?;
        Ok(u32::try_from(n).unwrap_or(u32::MAX))
    }

    async fn renditions(&self, asset_id: i64) -> MediaResult<Vec<StoredRendition>> {
        let rows = sqlx::query(
            "SELECT name, height, size_bytes FROM video_renditions r \
             JOIN video_assets a ON a.id = r.asset_id WHERE r.asset_id = $1 AND a.hls_prefix IS NOT NULL \
             ORDER BY height DESC",
        )
        .bind(asset_id)
        .fetch_all(&self.pool)
        .await
        .map_err(be)?;
        rows.iter()
            .map(|r| {
                let h: i32 = r.try_get("height").map_err(be)?;
                let s: i64 = r.try_get("size_bytes").map_err(be)?;
                Ok(StoredRendition {
                    name: r.try_get("name").map_err(be)?,
                    height: u32::try_from(h).unwrap_or(0),
                    size_bytes: u64::try_from(s).unwrap_or(0),
                })
            })
            .collect()
    }

    async fn remux_request(
        &self,
        asset_id: i64,
        rendition: &str,
    ) -> MediaResult<Option<RemuxRequest>> {
        let Some(a) = sqlx::query("SELECT hls_prefix, duration_ms FROM video_assets WHERE id = $1")
            .bind(asset_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(be)?
        else {
            return Ok(None);
        };
        let prefix: Option<String> = a.try_get("hls_prefix").map_err(be)?;
        let duration_ms: Option<i64> = a.try_get("duration_ms").map_err(be)?;
        let Some(prefix) = prefix else {
            return Ok(None);
        };
        let Some(v) = sqlx::query(
            "SELECT job_id, playlist_path, init_path, segment_index FROM video_renditions \
             WHERE asset_id = $1 AND name = $2",
        )
        .bind(asset_id)
        .bind(rendition)
        .fetch_optional(&self.pool)
        .await
        .map_err(be)?
        else {
            return Ok(None);
        };
        let job_id: i64 = v.try_get("job_id").map_err(be)?;
        // Sizes of playlists and init segments come from the ladder job's output list.
        let outputs: Value = sqlx::query_scalar("SELECT outputs FROM transcode_jobs WHERE id = $1")
            .bind(job_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(be)?
            .unwrap_or(Value::Null);
        let sizes: HashMap<String, u64> = serde_json::from_value::<Vec<OutputRef>>(outputs)
            .unwrap_or_default()
            .into_iter()
            .map(|o| (o.path, o.size))
            .collect();
        let idx: SegmentIndex = serde_json::from_value(v.try_get("segment_index").map_err(be)?)
            .map_err(|e| MediaError::Backend(e.to_string()))?;
        let init: String = v.try_get("init_path").map_err(be)?;
        let video = track(&idx, &init, &sizes);
        let rows = sqlx::query(
            "SELECT init_path, segment_index FROM video_audio_tracks WHERE asset_id = $1 \
             ORDER BY is_default DESC, ordinal LIMIT 4",
        )
        .bind(asset_id)
        .fetch_all(&self.pool)
        .await
        .map_err(be)?;
        let mut audio = Vec::new();
        for r in &rows {
            let idx: SegmentIndex = serde_json::from_value(r.try_get("segment_index").map_err(be)?)
                .map_err(|e| MediaError::Backend(e.to_string()))?;
            let init: String = r.try_get("init_path").map_err(be)?;
            audio.push(track(&idx, &init, &sizes));
        }
        let base = format!("/cdn/transcode/{}", prefix.trim_start_matches("transcode/"));
        Ok(Some(RemuxRequest {
            rendition: rendition.to_owned(),
            cdn_base: base,
            video,
            audio,
            duration_secs: duration_ms.map_or(0.0, |d| d as f64 / 1000.0),
        }))
    }

    async fn live_cache(&self, asset_id: i64, rendition: &str) -> MediaResult<Option<CacheEntry>> {
        let sql = format!(
            "{CACHE_SELECT} WHERE c.asset_id = $1 AND c.rendition = $2 \
               AND (c.state IN ('queued', 'preparing') OR (c.state = 'ready' AND c.expires_at > now())) \
             ORDER BY c.id DESC LIMIT 1"
        );
        let row = super::dynq(sql)
            .bind(asset_id)
            .bind(rendition)
            .fetch_optional(&self.pool)
            .await
            .map_err(be)?;
        row.as_ref().map(cache_from_row).transpose()
    }

    async fn create_cache(
        &self,
        id: i64,
        asset_id: i64,
        rendition: &str,
        job: NewJob,
    ) -> MediaResult<Option<CacheEntry>> {
        // A ready entry past its expiry blocks the unique index until the sweep marks it.
        sqlx::query(
            "UPDATE cinema_download_cache SET state = 'expired' \
             WHERE asset_id = $1 AND rendition = $2 AND state = 'ready' AND expires_at <= now()",
        )
        .bind(asset_id)
        .bind(rendition)
        .execute(&self.pool)
        .await
        .map_err(be)?;
        let mut tx = self.pool.begin().await.map_err(be)?;
        let job_id = (self.ids)();
        let inserted = sqlx::query(
            "INSERT INTO cinema_download_cache (id, asset_id, rendition, state) VALUES ($1, $2, $3, 'queued')",
        )
        .bind(id)
        .bind(asset_id)
        .bind(rendition)
        .execute(&mut *tx)
        .await;
        match inserted {
            Ok(_) => {}
            Err(e) if is_unique_violation(&e) => return Ok(None),
            Err(e) => return Err(be(e)),
        }
        insert_job(&mut tx, job_id, &job).await.map_err(be)?;
        sqlx::query("UPDATE cinema_download_cache SET job_id = $2 WHERE id = $1")
            .bind(id)
            .bind(job_id)
            .execute(&mut *tx)
            .await
            .map_err(be)?;
        tx.commit().await.map_err(be)?;
        self.live_cache(asset_id, rendition).await
    }

    async fn create_download(&self, row: &DownloadRow) -> MediaResult<()> {
        sqlx::query(
            "INSERT INTO cinema_downloads (id, user_id, title_id, episode_id, asset_id, rendition, cache_id) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(row.id)
        .bind(row.user_id)
        .bind(row.title_id)
        .bind(row.episode_id)
        .bind(row.asset_id)
        .bind(&row.rendition)
        .bind(row.cache_id)
        .execute(&self.pool)
        .await
        .map_err(be)?;
        Ok(())
    }

    async fn download(&self, id: i64) -> MediaResult<Option<(DownloadRow, Option<CacheEntry>)>> {
        let Some(r) = sqlx::query(
            "SELECT id, user_id, title_id, episode_id, asset_id, rendition, cache_id FROM cinema_downloads WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(be)?
        else {
            return Ok(None);
        };
        let row = DownloadRow {
            id: r.try_get("id").map_err(be)?,
            user_id: r.try_get("user_id").map_err(be)?,
            title_id: r.try_get("title_id").map_err(be)?,
            episode_id: r.try_get("episode_id").map_err(be)?,
            asset_id: r.try_get("asset_id").map_err(be)?,
            rendition: r.try_get("rendition").map_err(be)?,
            cache_id: r.try_get("cache_id").map_err(be)?,
        };
        let cache = match row.cache_id {
            Some(cid) => {
                let sql = format!("{CACHE_SELECT} WHERE c.id = $1");
                let c = super::dynq(sql)
                    .bind(cid)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(be)?;
                c.as_ref().map(cache_from_row).transpose()?
            }
            None => None,
        };
        Ok(Some((row, cache)))
    }

    async fn expire_cache(&self, now_ms: i64, limit: u32) -> MediaResult<Vec<CacheEntry>> {
        let rows = sqlx::query(
            "UPDATE cinema_download_cache SET state = 'expired' \
             WHERE id IN (SELECT id FROM cinema_download_cache WHERE state = 'ready' \
                          AND expires_at <= to_timestamp($1::double precision / 1000.0) \
                          ORDER BY expires_at LIMIT $2 FOR UPDATE SKIP LOCKED) \
             RETURNING id, asset_id, rendition, state, job_id, object_key, size_bytes, error, \
               (extract(epoch FROM expires_at) * 1000)::bigint AS expires_ms, \
               NULL::text AS job_state, NULL::real AS percent",
        )
        .bind(now_ms)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(be)?;
        rows.iter().map(cache_from_row).collect()
    }
}

/// Reads the download perk from `cinema_download_grants`.
pub struct PgDownloadEntitlements {
    pool: PgPool,
}

impl PgDownloadEntitlements {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DownloadEntitlements for PgDownloadEntitlements {
    async fn allowance(&self, user: i64) -> MediaResult<Option<Allowance>> {
        let per_day: Option<i32> = sqlx::query_scalar(
            "SELECT per_day FROM cinema_download_grants \
             WHERE user_id = $1 AND (expires_at IS NULL OR expires_at > now())",
        )
        .bind(user)
        .fetch_optional(&self.pool)
        .await
        .map_err(be)?;
        Ok(per_day.map(|p| Allowance {
            per_day: u32::try_from(p).unwrap_or(0),
        }))
    }
}
