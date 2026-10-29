//! [`MediaRepository`]: assets, admin views of jobs, tokens and workers, playback data and
//! retention facts.

use super::{PgMediaRepo, be, ms};
use async_trait::async_trait;
use serde_json::Value;
use sqlx::{Row, postgres::PgRow};
use sr_media::assets::*;
use sr_media::downloads::cdn_path_for_key;
use sr_media::plan::SpritePlan;
use sr_media::playlist::{AudioVariant, MediaSet, Segment, SubtitleVariant, VideoVariant};
use sr_media::retention::{AssetFacts, Tier};
use sr_media::retranscode::AssetSummary;
use sr_media::worker_api::{
    JobKind, JobSpec, JobState, ProgressRequest, SegmentIndex, TokenScope, WorkerCapabilities,
};

fn asset_cols() -> String {
    format!(
        "id, mount_id, title_id, episode_id, uploaded_by, state, error, filename, original_key, original_ext, \
         original_size, original_tier, (original_deleted_at IS NOT NULL) AS original_deleted, \
         original_downloads, auto_ladder, duration_ms, profile_version, hls_prefix, proxy_key, \
         (proxy_key IS NOT NULL) AS has_proxy, {}, {}, {}",
        ms("published_at", "published_ms"),
        ms("created_at", "created_ms"),
        ms("updated_at", "updated_ms"),
    )
}

fn asset_from_row(r: &PgRow) -> MediaResult<Asset> {
    let state: String = r.try_get("state").map_err(be)?;
    let hls_prefix: Option<String> = r.try_get("hls_prefix").map_err(be)?;
    Ok(Asset {
        id: r.try_get("id").map_err(be)?,
        mount_id: r.try_get("mount_id").map_err(be)?,
        title_id: r.try_get("title_id").map_err(be)?,
        episode_id: r.try_get("episode_id").map_err(be)?,
        uploaded_by: r.try_get("uploaded_by").map_err(be)?,
        state: AssetState::parse(&state)
            .ok_or_else(|| MediaError::Backend(format!("unknown asset state {state:?}")))?,
        error: r.try_get("error").map_err(be)?,
        filename: r.try_get("filename").map_err(be)?,
        original_size: r.try_get("original_size").map_err(be)?,
        original_tier: r.try_get("original_tier").map_err(be)?,
        original_deleted: r.try_get("original_deleted").map_err(be)?,
        original_downloads: r.try_get("original_downloads").map_err(be)?,
        auto_ladder: r.try_get("auto_ladder").map_err(be)?,
        duration_ms: r.try_get("duration_ms").map_err(be)?,
        profile_version: r.try_get("profile_version").map_err(be)?,
        has_proxy: r.try_get("has_proxy").map_err(be)?,
        playable: hls_prefix.is_some(),
        published_at_ms: r.try_get("published_ms").map_err(be)?,
        created_at_ms: r.try_get("created_ms").map_err(be)?,
        updated_at_ms: r.try_get("updated_ms").map_err(be)?,
        original_key: r.try_get("original_key").map_err(be)?,
        original_ext: r.try_get("original_ext").map_err(be)?,
        hls_prefix,
        proxy_key: r.try_get("proxy_key").map_err(be)?,
    })
}

fn job_kind(s: &str) -> MediaResult<JobKind> {
    JobKind::parse(s).ok_or_else(|| MediaError::Backend(format!("unknown job kind {s:?}")))
}

fn summary_from_row(r: &PgRow) -> MediaResult<JobSummary> {
    let kind: String = r.try_get("kind").map_err(be)?;
    let state: String = r.try_get("state").map_err(be)?;
    let progress: Option<Value> = r.try_get("progress").map_err(be)?;
    let id: i64 = r.try_get("id").map_err(be)?;
    let priority: i16 = r.try_get("priority").map_err(be)?;
    let attempt: i32 = r.try_get("attempt").map_err(be)?;
    Ok(JobSummary {
        job_id: id.to_string(),
        asset_id: r.try_get("asset_id").map_err(be)?,
        kind: job_kind(&kind)?,
        mount_id: r.try_get("mount_id").map_err(be)?,
        state: super::jobs::job_state(&state).map_err(|e| MediaError::Backend(e.to_string()))?,
        priority: u8::try_from(priority).unwrap_or(0),
        attempt: u32::try_from(attempt).unwrap_or(0),
        worker_id: r.try_get("worker_id").map_err(be)?,
        cancel_requested: r.try_get("cancel_requested").map_err(be)?,
        progress: progress.and_then(|p| serde_json::from_value::<ProgressRequest>(p).ok()),
        error: r.try_get("error").map_err(be)?,
        created_at_ms: r.try_get("created_ms").map_err(be)?,
        started_at_ms: r.try_get("started_ms").map_err(be)?,
        finished_at_ms: r.try_get("finished_ms").map_err(be)?,
    })
}

fn job_summary_cols() -> String {
    format!(
        "id, asset_id, kind, mount_id, state, priority, attempt, worker_id, cancel_requested, progress, error, {}, {}, {}",
        ms("created_at", "created_ms"),
        ms("started_at", "started_ms"),
        ms("finished_at", "finished_ms"),
    )
}

fn json_state(s: JobState) -> &'static str {
    match s {
        JobState::Queued => "queued",
        JobState::Running => "running",
        JobState::Succeeded => "succeeded",
        JobState::Failed => "failed",
        JobState::Cancelled => "cancelled",
    }
}

fn segments_of(idx: &SegmentIndex) -> Vec<Segment> {
    let dir = idx.playlist.rsplit_once('/').map_or("", |(d, _)| d);
    idx.segments
        .iter()
        .map(|s| Segment {
            path: if dir.is_empty() {
                s.name.clone()
            } else {
                format!("{dir}/{}", s.name)
            },
            duration_secs: s.duration_secs,
        })
        .collect()
}

#[async_trait]
impl MediaRepository for PgMediaRepo {
    async fn create_asset(&self, a: &NewAsset) -> MediaResult<Asset> {
        let sql = format!(
            "INSERT INTO video_assets (id, title_id, episode_id, uploaded_by, filename, original_key, \
               original_ext, original_size, auto_ladder) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING {}",
            asset_cols()
        );
        let row = super::dynq(sql)
            .bind(a.id)
            .bind(a.title_id)
            .bind(a.episode_id)
            .bind(a.uploaded_by)
            .bind(&a.filename)
            .bind(&a.original_key)
            .bind(&a.original_ext)
            .bind(a.original_size)
            .bind(a.auto_ladder)
            .fetch_one(&self.pool)
            .await
            .map_err(be)?;
        asset_from_row(&row)
    }

    async fn get_asset(&self, id: i64) -> MediaResult<Option<Asset>> {
        let row = super::dynq(format!(
            "SELECT {} FROM video_assets WHERE id = $1",
            asset_cols()
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(be)?;
        row.as_ref().map(asset_from_row).transpose()
    }

    async fn list_assets(&self, q: &AssetQuery) -> MediaResult<Vec<Asset>> {
        let sql = format!(
            "SELECT {} FROM video_assets \
             WHERE ($1::text IS NULL OR state = $1) AND ($2::bigint IS NULL OR title_id = $2) \
               AND ($3::bigint IS NULL OR uploaded_by = $3) AND ($4::bigint IS NULL OR id < $4) \
             ORDER BY id DESC LIMIT $5",
            asset_cols()
        );
        let rows = super::dynq(sql)
            .bind(q.state.map(|s| s.as_str()))
            .bind(q.title_id)
            .bind(q.uploaded_by)
            .bind(q.before)
            .bind(i64::from(q.limit.clamp(1, 200)))
            .fetch_all(&self.pool)
            .await
            .map_err(be)?;
        rows.iter().map(asset_from_row).collect()
    }

    async fn transition_asset(
        &self,
        id: i64,
        from: &[AssetState],
        to: AssetState,
        error: Option<&str>,
    ) -> MediaResult<Option<Asset>> {
        let from: Vec<&str> = from.iter().map(|s| s.as_str()).collect();
        let sql = format!(
            "UPDATE video_assets SET state = $3, error = $4, updated_at = now(), \
               ended_at = CASE WHEN $3 IN ('rejected', 'removed') THEN COALESCE(ended_at, now()) ELSE ended_at END \
             WHERE id = $1 AND state = ANY($2) RETURNING {}",
            asset_cols()
        );
        let row = super::dynq(sql)
            .bind(id)
            .bind(&from)
            .bind(to.as_str())
            .bind(error)
            .fetch_optional(&self.pool)
            .await
            .map_err(be)?;
        row.as_ref().map(asset_from_row).transpose()
    }

    async fn mark_uploaded(&self, id: i64, size: i64) -> MediaResult<Option<Asset>> {
        let sql = format!(
            "UPDATE video_assets SET state = 'uploaded', original_size = $2, error = NULL, updated_at = now() \
             WHERE id = $1 AND state = 'awaiting_upload' RETURNING {}",
            asset_cols()
        );
        let row = super::dynq(sql)
            .bind(id)
            .bind(size)
            .fetch_optional(&self.pool)
            .await
            .map_err(be)?;
        row.as_ref().map(asset_from_row).transpose()
    }

    async fn record_probe(&self, id: i64, probe: &Value, duration_ms: i64) -> MediaResult<()> {
        sqlx::query(
            "UPDATE video_assets SET probe = $2, duration_ms = $3, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(probe)
        .bind(duration_ms)
        .execute(&self.pool)
        .await
        .map_err(be)?;
        Ok(())
    }

    async fn asset_probe(&self, id: i64) -> MediaResult<Option<Value>> {
        let v: Option<Option<Value>> =
            sqlx::query_scalar("SELECT probe FROM video_assets WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(be)?;
        Ok(v.flatten())
    }

    async fn link_asset(
        &self,
        id: i64,
        title: Option<i64>,
        episode: Option<i64>,
    ) -> MediaResult<Option<Asset>> {
        let sql = format!(
            "UPDATE video_assets SET title_id = $2, episode_id = $3, updated_at = now() WHERE id = $1 RETURNING {}",
            asset_cols()
        );
        let row = super::dynq(sql)
            .bind(id)
            .bind(title)
            .bind(episode)
            .fetch_optional(&self.pool)
            .await
            .map_err(be)?;
        row.as_ref().map(asset_from_row).transpose()
    }

    async fn set_original_downloads(&self, id: i64, on: bool) -> MediaResult<Option<Asset>> {
        let sql = format!(
            "UPDATE video_assets SET original_downloads = $2, updated_at = now() WHERE id = $1 RETURNING {}",
            asset_cols()
        );
        let row = super::dynq(sql)
            .bind(id)
            .bind(on)
            .fetch_optional(&self.pool)
            .await
            .map_err(be)?;
        row.as_ref().map(asset_from_row).transpose()
    }

    async fn expire_slots(&self, before_ms: i64, limit: u32) -> MediaResult<Vec<Asset>> {
        let sql = format!(
            "UPDATE video_assets SET state = 'expired', error = 'The upload was never completed.', updated_at = now() \
             WHERE id IN (SELECT id FROM video_assets WHERE state = 'awaiting_upload' \
                          AND created_at < to_timestamp($1::double precision / 1000.0) \
                          ORDER BY created_at LIMIT $2 FOR UPDATE SKIP LOCKED) \
             RETURNING {}",
            asset_cols()
        );
        let rows = super::dynq(sql)
            .bind(before_ms)
            .bind(i64::from(limit))
            .fetch_all(&self.pool)
            .await
            .map_err(be)?;
        rows.iter().map(asset_from_row).collect()
    }

    async fn list_jobs(&self, q: &JobQuery) -> MediaResult<Vec<JobSummary>> {
        let sql = format!(
            "SELECT {} FROM transcode_jobs \
             WHERE ($1::text IS NULL OR state = $1) AND ($2::text IS NULL OR kind = $2) \
               AND ($3::bigint IS NULL OR asset_id = $3) AND ($4::bigint IS NULL OR id < $4) AND ($6::bigint IS NULL OR mount_id = $6) \
             ORDER BY id DESC LIMIT $5",
            job_summary_cols()
        );
        let rows = super::dynq(sql)
            .bind(q.state.map(json_state))
            .bind(q.kind.map(|k| k.as_str()))
            .bind(q.asset_id)
            .bind(q.before)
            .bind(i64::from(q.limit.clamp(1, 200)))
            .bind(q.mount_id)
            .fetch_all(&self.pool)
            .await
            .map_err(be)?;
        rows.iter().map(summary_from_row).collect()
    }

    async fn job_detail(&self, job_id: i64) -> MediaResult<Option<JobDetail>> {
        let sql = format!(
            "SELECT {}, log_tail, encoder, wall_secs, jsonb_array_length(outputs) AS outputs_n, spec \
             FROM transcode_jobs WHERE id = $1",
            job_summary_cols()
        );
        let row = super::dynq(sql)
            .bind(job_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(be)?;
        let Some(row) = row else { return Ok(None) };
        let spec: Value = row.try_get("spec").map_err(be)?;
        let warnings = match serde_json::from_value::<JobSpec>(spec) {
            Ok(JobSpec::Ladder { plan }) => plan.warnings,
            _ => Vec::new(),
        };
        let n: i32 = row.try_get("outputs_n").map_err(be)?;
        Ok(Some(JobDetail {
            job: summary_from_row(&row)?,
            log_tail: row.try_get("log_tail").map_err(be)?,
            encoder: row.try_get("encoder").map_err(be)?,
            wall_secs: row.try_get("wall_secs").map_err(be)?,
            output_count: u32::try_from(n).unwrap_or(0),
            warnings,
        }))
    }

    async fn retry_job(&self, job_id: i64) -> MediaResult<bool> {
        let mut tx = self.pool.begin().await.map_err(be)?;
        let row = sqlx::query(
            "UPDATE transcode_jobs SET state = 'queued', attempt = 0, worker_id = NULL, lease_expires_at = NULL, \
               cancel_requested = false, error = NULL, finished_at = NULL, progress = NULL, updated_at = now() \
             WHERE id = $1 AND state IN ('failed', 'cancelled') RETURNING kind, asset_id",
        )
        .bind(job_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(be)?;
        let Some(row) = row else { return Ok(false) };
        let kind: String = row.try_get("kind").map_err(be)?;
        let asset_id: i64 = row.try_get("asset_id").map_err(be)?;
        if kind == "ladder" {
            sqlx::query(
                "UPDATE video_assets SET state = 'transcoding', error = NULL, updated_at = now() \
                 WHERE id = $1 AND state IN ('failed', 'published', 'probing')",
            )
            .bind(asset_id)
            .execute(&mut *tx)
            .await
            .map_err(be)?;
        }
        if kind == "remux" {
            // Failing back to queued can collide with a newer live entry; then the old one stays failed.
            let _ = sqlx::query(
                "UPDATE cinema_download_cache SET state = 'queued', error = NULL WHERE job_id = $1 AND state = 'failed'",
            )
            .bind(job_id)
            .execute(&mut *tx)
            .await;
        }
        tx.commit().await.map_err(be)?;
        Ok(true)
    }

    async fn queue_counts(&self) -> MediaResult<QueueCounts> {
        let row = sqlx::query(
            "SELECT \
               count(*) FILTER (WHERE state = 'queued') AS queued, \
               count(*) FILTER (WHERE state = 'running') AS running, \
               count(*) FILTER (WHERE state = 'failed' AND finished_at > now() - interval '1 day') AS failed, \
               count(*) FILTER (WHERE state = 'succeeded' AND finished_at > now() - interval '1 day') AS ok \
             FROM transcode_jobs",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(be)?;
        let n = |name: &str| -> MediaResult<u32> {
            let v: i64 = row.try_get(name).map_err(be)?;
            Ok(u32::try_from(v).unwrap_or(u32::MAX))
        };
        Ok(QueueCounts {
            queued: n("queued")?,
            running: n("running")?,
            failed_last_day: n("failed")?,
            succeeded_last_day: n("ok")?,
        })
    }

    async fn create_token(
        &self,
        id: i64,
        name: &str,
        sha256: &str,
        created_by: i64,
        scope: TokenScope,
    ) -> MediaResult<WorkerToken> {
        let sql = format!(
            "INSERT INTO worker_tokens (id, name, token_sha256, created_by, scope) VALUES ($1, $2, $3, $4, $5) \
             RETURNING id, name, scope, created_by, {}, {}, {}",
            ms("created_at", "created_ms"),
            ms("last_used_at", "used_ms"),
            ms("revoked_at", "revoked_ms")
        );
        let row = super::dynq(sql)
            .bind(id)
            .bind(name)
            .bind(sha256)
            .bind(created_by)
            .bind(scope.to_column())
            .fetch_one(&self.pool)
            .await
            .map_err(be)?;
        token_from_row(&row)
    }

    async fn list_mount_tokens(&self, mount_id: i64) -> MediaResult<Vec<WorkerToken>> {
        let sql = format!(
            "SELECT id, name, scope, created_by, {}, {}, {} FROM worker_tokens WHERE scope = $1 ORDER BY id DESC",
            ms("created_at", "created_ms"),
            ms("last_used_at", "used_ms"),
            ms("revoked_at", "revoked_ms")
        );
        let rows = super::dynq(sql)
            .bind(TokenScope::Mount(mount_id).to_column())
            .fetch_all(&self.pool)
            .await
            .map_err(be)?;
        rows.iter().map(token_from_row).collect()
    }

    async fn revoke_mount_token(&self, mount_id: i64, id: i64) -> MediaResult<bool> {
        let mut tx = self.pool.begin().await.map_err(be)?;
        let r = sqlx::query(
            "UPDATE worker_tokens SET revoked_at = now() WHERE id = $1 AND scope = $2 AND revoked_at IS NULL",
        )
        .bind(id)
        .bind(TokenScope::Mount(mount_id).to_column())
        .execute(&mut *tx)
        .await
        .map_err(be)?;
        if r.rows_affected() == 0 {
            return Ok(false);
        }
        sqlx::query("UPDATE transcode_workers SET drain = true WHERE token_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(be)?;
        tx.commit().await.map_err(be)?;
        Ok(true)
    }

    async fn list_tokens(&self) -> MediaResult<Vec<WorkerToken>> {
        let sql = format!(
            "SELECT id, name, scope, created_by, {}, {}, {} FROM worker_tokens ORDER BY id DESC",
            ms("created_at", "created_ms"),
            ms("last_used_at", "used_ms"),
            ms("revoked_at", "revoked_ms")
        );
        let rows = super::dynq(sql).fetch_all(&self.pool).await.map_err(be)?;
        rows.iter().map(token_from_row).collect()
    }

    async fn revoke_token(&self, id: i64) -> MediaResult<bool> {
        let mut tx = self.pool.begin().await.map_err(be)?;
        let r = sqlx::query(
            "UPDATE worker_tokens SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(be)?;
        if r.rows_affected() == 0 {
            return Ok(false);
        }
        sqlx::query("UPDATE transcode_workers SET drain = true WHERE token_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(be)?;
        tx.commit().await.map_err(be)?;
        Ok(true)
    }

    async fn list_workers(&self) -> MediaResult<Vec<WorkerView>> {
        let sql = format!(
            "SELECT w.id, w.name, w.version, w.token_id, t.name AS token_name, (t.revoked_at IS NOT NULL) AS revoked, \
               w.capabilities, {}, w.drain, \
               (SELECT count(*) FROM transcode_jobs j WHERE j.worker_id = w.id AND j.state = 'running') AS active \
             FROM transcode_workers w JOIN worker_tokens t ON t.id = w.token_id \
             ORDER BY w.last_seen_at DESC, w.id",
            ms("w.last_seen_at", "seen_ms")
        );
        let rows = super::dynq(sql).fetch_all(&self.pool).await.map_err(be)?;
        rows.iter()
            .map(|r| {
                let caps: Value = r.try_get("capabilities").map_err(be)?;
                let active: i64 = r.try_get("active").map_err(be)?;
                Ok(WorkerView {
                    id: r.try_get("id").map_err(be)?,
                    name: r.try_get("name").map_err(be)?,
                    version: r.try_get("version").map_err(be)?,
                    token_id: r.try_get("token_id").map_err(be)?,
                    token_name: r.try_get("token_name").map_err(be)?,
                    token_revoked: r.try_get("revoked").map_err(be)?,
                    capabilities: serde_json::from_value::<WorkerCapabilities>(caps)
                        .unwrap_or_default(),
                    last_seen_ms: r.try_get("seen_ms").map_err(be)?,
                    online: false,
                    drain: r.try_get("drain").map_err(be)?,
                    active_jobs: u32::try_from(active).unwrap_or(0),
                })
            })
            .collect()
    }

    async fn set_worker_drain(&self, worker_id: &str, drain: bool) -> MediaResult<bool> {
        let r = sqlx::query("UPDATE transcode_workers SET drain = $2 WHERE id = $1")
            .bind(worker_id)
            .bind(drain)
            .execute(&self.pool)
            .await
            .map_err(be)?;
        Ok(r.rows_affected() > 0)
    }

    async fn playback(&self, asset_id: i64) -> MediaResult<Option<AssetPlayback>> {
        let Some(asset) = self.get_asset(asset_id).await? else {
            return Ok(None);
        };
        let Some(prefix) = asset.hls_prefix.clone() else {
            return Ok(None);
        };
        let Some(base_path) = cdn_path_for_key(&format!("{prefix}/x"))
            .map(|p| p.strip_suffix("/x").map(str::to_owned).unwrap_or(p))
        else {
            return Ok(None);
        };
        let duration_secs = asset.duration_ms.map_or(0.0, |d| d as f64 / 1000.0);
        let rows = sqlx::query(
            "SELECT name, width, height, bandwidth_bps, frame_rate, codecs, playlist_path, init_path, \
               segment_index, profile_version FROM video_renditions WHERE asset_id = $1 \
             ORDER BY bandwidth_bps",
        )
        .bind(asset_id)
        .fetch_all(&self.pool)
        .await
        .map_err(be)?;
        let mut media = MediaSet::default();
        let mut profile_version = 0u32;
        for r in &rows {
            let idx: SegmentIndex = serde_json::from_value(r.try_get("segment_index").map_err(be)?)
                .map_err(|e| MediaError::Backend(e.to_string()))?;
            let (w, h, bw): (i32, i32, i64) = (
                r.try_get("width").map_err(be)?,
                r.try_get("height").map_err(be)?,
                r.try_get("bandwidth_bps").map_err(be)?,
            );
            let pv: i32 = r.try_get("profile_version").map_err(be)?;
            profile_version = u32::try_from(pv).unwrap_or(0);
            media.videos.push(VideoVariant {
                name: r.try_get("name").map_err(be)?,
                bandwidth_bps: u64::try_from(bw).unwrap_or(0),
                average_bandwidth_bps: None,
                width: u32::try_from(w).unwrap_or(0),
                height: u32::try_from(h).unwrap_or(0),
                frame_rate: r.try_get("frame_rate").map_err(be)?,
                codecs: r.try_get("codecs").map_err(be)?,
                playlist_path: r.try_get("playlist_path").map_err(be)?,
                init_path: r.try_get("init_path").map_err(be)?,
                segments: segments_of(&idx),
            });
        }
        let rows = sqlx::query(
            "SELECT name, language_tag, channels, is_default, playlist_path, init_path, segment_index \
             FROM video_audio_tracks WHERE asset_id = $1 ORDER BY ordinal",
        )
        .bind(asset_id)
        .fetch_all(&self.pool)
        .await
        .map_err(be)?;
        for r in &rows {
            let idx: SegmentIndex = serde_json::from_value(r.try_get("segment_index").map_err(be)?)
                .map_err(|e| MediaError::Backend(e.to_string()))?;
            let ch: i32 = r.try_get("channels").map_err(be)?;
            media.audios.push(AudioVariant {
                name: r.try_get("name").map_err(be)?,
                language_tag: r.try_get("language_tag").map_err(be)?,
                channels: u32::try_from(ch).unwrap_or(2),
                is_default: r.try_get("is_default").map_err(be)?,
                playlist_path: r.try_get("playlist_path").map_err(be)?,
                init_path: r.try_get("init_path").map_err(be)?,
                segments: segments_of(&idx),
            });
        }
        let rows = sqlx::query(
            "SELECT name, language_tag, forced, is_default, vtt_path, ass_path FROM video_subtitle_tracks \
             WHERE asset_id = $1 ORDER BY ordinal",
        )
        .bind(asset_id)
        .fetch_all(&self.pool)
        .await
        .map_err(be)?;
        let mut ass_paths = Vec::new();
        for r in &rows {
            let vtt: String = r.try_get("vtt_path").map_err(be)?;
            if let Some(ass) = r.try_get::<Option<String>, _>("ass_path").map_err(be)? {
                ass_paths.push((vtt.clone(), ass));
            }
            media.subtitles.push(SubtitleVariant {
                name: r.try_get("name").map_err(be)?,
                language_tag: r.try_get("language_tag").map_err(be)?,
                forced: r.try_get("forced").map_err(be)?,
                is_default: r.try_get("is_default").map_err(be)?,
                playlist_path: match vtt.strip_suffix(".vtt") {
                    Some(stem) => format!("{stem}.m3u8"),
                    None => format!("{vtt}.m3u8"),
                },
                vtt_path: vtt,
                duration_secs,
            });
        }
        // The sprite plan is part of the ladder job that produced the published files.
        let spec: Option<Value> = sqlx::query_scalar(
            "SELECT spec FROM transcode_jobs WHERE asset_id = $1 AND kind = 'ladder' AND state = 'succeeded' \
             ORDER BY id DESC LIMIT 1",
        )
        .bind(asset_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(be)?;
        let sprite: Option<SpritePlan> = spec
            .and_then(|s| serde_json::from_value::<JobSpec>(s).ok())
            .and_then(|s| match s {
                JobSpec::Ladder { plan } => Some(plan.sprite),
                _ => None,
            });
        Ok(Some(AssetPlayback {
            asset_id,
            mount_id: asset.mount_id,
            base_path,
            duration_secs,
            profile_version,
            media,
            sprite,
            ass_paths,
        }))
    }

    async fn playback_by_prefix(&self, prefix: &str) -> MediaResult<Option<AssetPlayback>> {
        let id: Option<i64> =
            sqlx::query_scalar("SELECT id FROM video_assets WHERE hls_prefix = $1")
                .bind(prefix)
                .fetch_optional(&self.pool)
                .await
                .map_err(be)?;
        match id {
            Some(id) => self.playback(id).await,
            None => Ok(None),
        }
    }

    async fn retention_facts(&self, limit: u32) -> MediaResult<Vec<AssetFacts>> {
        let rows = sqlx::query(
            "SELECT a.id, a.state, a.original_tier, (a.original_deleted_at IS NULL) AS has_original, \
               a.original_size, (extract(epoch FROM a.published_at))::bigint AS published, \
               (extract(epoch FROM a.ended_at))::bigint AS ended, (a.proxy_key IS NOT NULL) AS has_proxy, \
               (extract(epoch FROM a.proxy_created_at))::bigint AS proxy_at, a.original_downloads, \
               EXISTS (SELECT 1 FROM transcode_jobs j WHERE j.asset_id = a.id AND j.state IN ('queued', 'running')) AS active \
             FROM video_assets a WHERE a.state NOT IN ('awaiting_upload', 'expired') \
             ORDER BY a.id LIMIT $1",
        )
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(be)?;
        let mut out = Vec::new();
        for r in &rows {
            let state: String = r.try_get("state").map_err(be)?;
            let Some(state) = AssetState::parse(&state).and_then(AssetState::retention) else {
                continue;
            };
            let tier: String = r.try_get("original_tier").map_err(be)?;
            let id: i64 = r.try_get("id").map_err(be)?;
            let size: i64 = r.try_get("original_size").map_err(be)?;
            let secs = |name: &str| -> MediaResult<Option<u64>> {
                let v: Option<i64> = r.try_get(name).map_err(be)?;
                Ok(v.and_then(|v| u64::try_from(v).ok()))
            };
            out.push(AssetFacts {
                asset_id: u64::try_from(id).unwrap_or_default(),
                state,
                tier: if tier == "cold" {
                    Tier::Cold
                } else {
                    Tier::Hot
                },
                has_original: r.try_get("has_original").map_err(be)?,
                original_size: u64::try_from(size).unwrap_or_default(),
                published_at: secs("published")?,
                ended_at: secs("ended")?,
                has_proxy: r.try_get("has_proxy").map_err(be)?,
                proxy_created_at: secs("proxy_at")?,
                has_active_job: r.try_get("active").map_err(be)?,
                retranscode_pending: false,
                original_downloads_enabled: r.try_get("original_downloads").map_err(be)?,
            });
        }
        Ok(out)
    }

    async fn retranscode_summaries(
        &self,
        asset: Option<i64>,
        title: Option<i64>,
        limit: u32,
    ) -> MediaResult<Vec<AssetSummary>> {
        let rows = sqlx::query(
            "SELECT a.id, a.state, (a.original_deleted_at IS NULL) AS has_original, \
               COALESCE(a.profile_version, 0) AS profile, COALESCE(a.duration_ms, 0) AS dur, a.original_size, \
               COALESCE(a.title_id, (SELECT t.id FROM video_titles t WHERE t.asset_id = a.id LIMIT 1), \
                        (SELECT e.title_id FROM video_episodes e WHERE e.asset_id = a.id LIMIT 1), 0) AS title, \
               EXISTS (SELECT 1 FROM transcode_jobs j WHERE j.asset_id = a.id AND j.state IN ('queued', 'running')) AS active \
             FROM video_assets a \
             WHERE a.state NOT IN ('awaiting_upload', 'expired') \
               AND ($1::bigint IS NULL OR a.id = $1) \
               AND ($2::bigint IS NULL OR a.title_id = $2 \
                    OR a.id = (SELECT t.asset_id FROM video_titles t WHERE t.id = $2) \
                    OR a.id IN (SELECT e.asset_id FROM video_episodes e WHERE e.title_id = $2 AND e.asset_id IS NOT NULL)) \
             ORDER BY a.id LIMIT $3",
        )
        .bind(asset)
        .bind(title)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(be)?;
        let mut out = Vec::new();
        for r in &rows {
            let state: String = r.try_get("state").map_err(be)?;
            let Some(state) = AssetState::parse(&state).and_then(AssetState::retention) else {
                continue;
            };
            let (id, title, dur, size, profile): (i64, i64, i64, i64, i32) = (
                r.try_get("id").map_err(be)?,
                r.try_get("title").map_err(be)?,
                r.try_get("dur").map_err(be)?,
                r.try_get("original_size").map_err(be)?,
                r.try_get("profile").map_err(be)?,
            );
            out.push(AssetSummary {
                asset_id: u64::try_from(id).unwrap_or_default(),
                title_id: u64::try_from(title).unwrap_or_default(),
                state,
                has_original: r.try_get("has_original").map_err(be)?,
                profile_version: u32::try_from(profile).unwrap_or(0),
                has_active_job: r.try_get("active").map_err(be)?,
                duration_secs: dur as f64 / 1000.0,
                original_size: u64::try_from(size).unwrap_or_default(),
            });
        }
        Ok(out)
    }

    async fn mark_original_deleted(&self, id: i64) -> MediaResult<()> {
        sqlx::query(
            "UPDATE video_assets SET original_deleted_at = now(), updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(be)?;
        Ok(())
    }

    async fn mark_proxy_deleted(&self, id: i64) -> MediaResult<()> {
        sqlx::query("UPDATE video_assets SET proxy_key = NULL, proxy_created_at = NULL, updated_at = now() WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(be)?;
        Ok(())
    }

    async fn mark_cold(&self, id: i64) -> MediaResult<()> {
        sqlx::query(
            "UPDATE video_assets SET original_tier = 'cold', updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(be)?;
        Ok(())
    }

    async fn queue_gc(&self, prefixes: &[String], asset: Option<i64>) -> MediaResult<()> {
        for p in prefixes {
            sqlx::query("INSERT INTO cinema_media_gc (prefix, asset_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
                .bind(p)
                .bind(asset)
                .execute(&self.pool)
                .await
                .map_err(be)?;
        }
        Ok(())
    }

    async fn gc_due(&self, before_ms: i64, limit: u32) -> MediaResult<Vec<String>> {
        // A prefix an asset points at again is dropped from the queue instead of collected.
        sqlx::query(
            "DELETE FROM cinema_media_gc g WHERE EXISTS (SELECT 1 FROM video_assets a WHERE a.hls_prefix = g.prefix)",
        )
        .execute(&self.pool)
        .await
        .map_err(be)?;
        sqlx::query_scalar(
            "SELECT prefix FROM cinema_media_gc WHERE queued_at < to_timestamp($1::double precision / 1000.0) \
             ORDER BY queued_at LIMIT $2",
        )
        .bind(before_ms)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(be)
    }

    async fn gc_done(&self, prefixes: &[String]) -> MediaResult<()> {
        sqlx::query("DELETE FROM cinema_media_gc WHERE prefix = ANY($1)")
            .bind(prefixes)
            .execute(&self.pool)
            .await
            .map_err(be)?;
        Ok(())
    }
}

fn token_from_row(r: &PgRow) -> MediaResult<WorkerToken> {
    Ok(WorkerToken {
        id: r.try_get("id").map_err(be)?,
        name: r.try_get("name").map_err(be)?,
        scope: r.try_get("scope").map_err(be)?,
        created_by: r.try_get("created_by").map_err(be)?,
        created_at_ms: r.try_get("created_ms").map_err(be)?,
        last_used_at_ms: r.try_get("used_ms").map_err(be)?,
        revoked_at_ms: r.try_get("revoked_ms").map_err(be)?,
    })
}

/// What the Cinema review kinds need to know about an uploaded video.
#[async_trait]
impl sr_cinema::submissions::AssetLookup for PgMediaRepo {
    async fn asset(&self, id: i64) -> Result<Option<sr_cinema::submissions::AssetFacts>, String> {
        let row = sqlx::query(
            "SELECT uploaded_by, (hls_prefix IS NOT NULL AND state = 'published') AS playable \
             FROM video_assets WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        let Some(row) = row else { return Ok(None) };
        Ok(Some(sr_cinema::submissions::AssetFacts {
            playable: row.try_get("playable").map_err(|e| e.to_string())?,
            uploaded_by: row.try_get("uploaded_by").map_err(|e| e.to_string())?,
        }))
    }

    async fn discard(&self, ids: &[i64]) -> Result<(), String> {
        // Transcode output of unpublished assets goes to the collection queue; the original and
        // the proxy follow through the retention sweep once the state is `removed`.
        sqlx::query(
            "INSERT INTO cinema_media_gc (prefix, asset_id)              SELECT hls_prefix, id FROM video_assets              WHERE id = ANY($1) AND hls_prefix IS NOT NULL AND state <> 'published'              ON CONFLICT DO NOTHING",
        )
        .bind(ids)
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        sqlx::query(
            "UPDATE video_assets SET state = 'removed', ended_at = COALESCE(ended_at, now()),              updated_at = now() WHERE id = ANY($1) AND state <> 'published'",
        )
        .bind(ids)
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}
