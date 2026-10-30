//! The Worker API's [`JobStore`] over Postgres.

use super::{PgMediaRepo, se};
use async_trait::async_trait;
use serde_json::Value;
use sqlx::{PgConnection, Row, postgres::PgRow};
use sr_media::plan::{ExtractFormat, JobPlan};
use sr_media::worker_api::{
    CompleteRequest, FailOutcome, FailRequest, JobKind, JobRecord, JobSpec, JobState, JobStore,
    NewJob, OutputRef, ProgressRequest, RegisterRequest, SegmentIndex, StoreError, TokenInfo,
    TokenScope, UploadRecord, WorkerCapabilities, WorkerRecord,
};
use std::collections::HashMap;

pub(super) const JOB_COLS: &str = "id, asset_id, kind, spec, source_key, source_size, source_ext, \
    state, priority, attempt, worker_id, \
    (extract(epoch FROM lease_expires_at))::bigint AS lease, cancel_requested, progress, error, \
    log_tail, outputs, segment_index, encoder";

pub(super) fn job_state(s: &str) -> Result<JobState, StoreError> {
    Ok(match s {
        "queued" => JobState::Queued,
        "running" => JobState::Running,
        "succeeded" => JobState::Succeeded,
        "failed" => JobState::Failed,
        "cancelled" => JobState::Cancelled,
        other => return Err(StoreError::Backend(format!("unknown job state {other:?}"))),
    })
}

fn json_err(e: serde_json::Error) -> StoreError {
    StoreError::Backend(format!("stored job data is unreadable: {e}"))
}

fn job_from_row(r: &PgRow) -> Result<JobRecord, StoreError> {
    let get = se;
    let spec: Value = r.try_get("spec").map_err(get)?;
    let progress: Option<Value> = r.try_get("progress").map_err(get)?;
    let outputs: Value = r.try_get("outputs").map_err(get)?;
    let segment_index: Value = r.try_get("segment_index").map_err(get)?;
    let state: String = r.try_get("state").map_err(get)?;
    let id: i64 = r.try_get("id").map_err(get)?;
    let asset_id: i64 = r.try_get("asset_id").map_err(get)?;
    let source_size: i64 = r.try_get("source_size").map_err(get)?;
    let priority: i16 = r.try_get("priority").map_err(get)?;
    let attempt: i32 = r.try_get("attempt").map_err(get)?;
    let lease: Option<i64> = r.try_get("lease").map_err(get)?;
    Ok(JobRecord {
        job_id: id.to_string(),
        asset_id: u64::try_from(asset_id).unwrap_or_default(),
        spec: serde_json::from_value::<JobSpec>(spec).map_err(json_err)?,
        source_key: r.try_get("source_key").map_err(get)?,
        source_size: u64::try_from(source_size).unwrap_or_default(),
        source_extension: r.try_get("source_ext").map_err(get)?,
        priority: u8::try_from(priority).unwrap_or(0),
        state: job_state(&state)?,
        attempt: u32::try_from(attempt).unwrap_or_default(),
        worker_id: r.try_get("worker_id").map_err(get)?,
        lease_expires_at: lease.and_then(|l| u64::try_from(l).ok()),
        cancel_requested: r.try_get("cancel_requested").map_err(get)?,
        progress: progress
            .map(serde_json::from_value::<ProgressRequest>)
            .transpose()
            .map_err(json_err)?,
        error: r.try_get("error").map_err(get)?,
        log_tail: r.try_get("log_tail").map_err(get)?,
        outputs: serde_json::from_value::<Vec<OutputRef>>(outputs).map_err(json_err)?,
        segment_index: serde_json::from_value::<Vec<SegmentIndex>>(segment_index)
            .map_err(json_err)?,
        encoder: r.try_get("encoder").map_err(get)?,
    })
}

fn parse_id(job_id: &str) -> Result<i64, StoreError> {
    job_id.parse::<i64>().map_err(|_| StoreError::NotFound)
}

fn secs_f64(v: u64) -> f64 {
    v as f64
}

/// Insert a job row (also used by the download cache, inside its transaction).
pub(super) async fn insert_job(
    tx: &mut PgConnection,
    id: i64,
    job: &NewJob,
) -> Result<(), sqlx::Error> {
    let spec = serde_json::to_value(&job.spec).unwrap_or(Value::Null);
    sqlx::query(
        "INSERT INTO transcode_jobs (id, asset_id, kind, spec, source_key, source_size, source_ext, priority, mount_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(i64::try_from(job.asset_id).unwrap_or(i64::MAX))
    .bind(job.spec.kind().as_str())
    .bind(spec)
    .bind(&job.source_key)
    .bind(i64::try_from(job.source_size).unwrap_or(i64::MAX))
    .bind(&job.source_extension)
    .bind(i16::from(job.priority))
    .bind(job.spec.mount_id())
    .execute(tx)
    .await?;
    Ok(())
}

/// What a job that ended without success does to the things waiting on it.
async fn failure_effects(
    tx: &mut PgConnection,
    job_id: i64,
    kind: &str,
    asset_id: i64,
    message: &str,
    cancelled: bool,
) -> Result<(), sqlx::Error> {
    match kind {
        "ladder" => {
            let text = if cancelled {
                "The transcode was cancelled."
            } else {
                message
            };
            // A re-transcode that fails leaves the previous ladder playable.
            sqlx::query(
                "UPDATE video_assets SET \
                   state = CASE WHEN hls_prefix IS NOT NULL THEN 'published' ELSE 'failed' END, \
                   error = $2, updated_at = now() \
                 WHERE id = $1 AND state = 'transcoding'",
            )
            .bind(asset_id)
            .bind(text)
            .execute(&mut *tx)
            .await?;
        }
        "remux" => {
            sqlx::query(
                "UPDATE cinema_download_cache SET state = 'failed', error = $2 \
                 WHERE job_id = $1 AND state IN ('queued', 'preparing')",
            )
            .bind(job_id)
            .bind("The download could not be prepared.")
            .execute(&mut *tx)
            .await?;
        }
        _ => {}
    }
    Ok(())
}

/// Requeue, fail or cancel a running job row that is locked by `tx`.
async fn settle_running(
    tx: &mut PgConnection,
    job_id: i64,
    retryable: bool,
    message: &str,
    log_tail: Option<&str>,
    mark_cancel: bool,
    max_attempts: u32,
) -> Result<FailOutcome, sqlx::Error> {
    let row = sqlx::query(
        "SELECT kind, asset_id, attempt, cancel_requested FROM transcode_jobs WHERE id = $1",
    )
    .bind(job_id)
    .fetch_one(&mut *tx)
    .await?;
    let kind: String = row.try_get("kind")?;
    let asset_id: i64 = row.try_get("asset_id")?;
    let attempt: i32 = row.try_get("attempt")?;
    let cancel = row.try_get::<bool, _>("cancel_requested")? || mark_cancel;
    let outcome = if cancel {
        FailOutcome::Cancelled
    } else if retryable && u32::try_from(attempt).unwrap_or(u32::MAX) < max_attempts {
        FailOutcome::Requeued
    } else {
        FailOutcome::Failed
    };
    let (state, terminal) = match outcome {
        FailOutcome::Requeued => ("queued", false),
        FailOutcome::Failed => ("failed", true),
        FailOutcome::Cancelled => ("cancelled", true),
    };
    sqlx::query(
        "UPDATE transcode_jobs SET state = $2, error = $3, log_tail = COALESCE($4, log_tail), \
           cancel_requested = $5, worker_id = NULL, lease_expires_at = NULL, updated_at = now(), \
           finished_at = CASE WHEN $6 THEN now() ELSE NULL END \
         WHERE id = $1",
    )
    .bind(job_id)
    .bind(state)
    .bind(message)
    .bind(log_tail)
    .bind(cancel)
    .bind(terminal)
    .execute(&mut *tx)
    .await?;
    if terminal {
        failure_effects(
            tx,
            job_id,
            &kind,
            asset_id,
            message,
            matches!(outcome, FailOutcome::Cancelled),
        )
        .await?;
    }
    Ok(outcome)
}

#[async_trait]
impl JobStore for PgMediaRepo {
    async fn authenticate(&self, token_sha256: &str) -> Result<Option<TokenInfo>, StoreError> {
        let row = sqlx::query(
            "SELECT id, name, scope, (last_used_at IS NULL OR last_used_at < now() - interval '1 minute') AS stale \
             FROM worker_tokens WHERE token_sha256 = $1 AND revoked_at IS NULL",
        )
        .bind(token_sha256)
        .fetch_optional(&self.pool)
        .await
        .map_err(se)?;
        let Some(row) = row else { return Ok(None) };
        let id: i64 = row.try_get("id").map_err(se)?;
        if row.try_get::<bool, _>("stale").map_err(se)? {
            sqlx::query("UPDATE worker_tokens SET last_used_at = now() WHERE id = $1")
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(se)?;
        }
        // A scope this code does not know authenticates nobody (fail closed).
        let scope: String = row.try_get("scope").map_err(se)?;
        let Some(scope) = TokenScope::parse(&scope) else {
            return Ok(None);
        };
        Ok(Some(TokenInfo {
            token_id: u64::try_from(id).unwrap_or_default(),
            name: row.try_get("name").map_err(se)?,
            scope,
        }))
    }

    async fn upsert_worker(
        &self,
        token: &TokenInfo,
        reg: &RegisterRequest,
        now: u64,
    ) -> Result<WorkerRecord, StoreError> {
        let caps = serde_json::to_value(&reg.capabilities).map_err(json_err)?;
        let new_id = format!("w{}", (self.ids)());
        let row = sqlx::query(
            "INSERT INTO transcode_workers (id, token_id, name, version, capabilities, last_seen_at) \
             VALUES ($1, $2, $3, $4, $5, to_timestamp($6)) \
             ON CONFLICT (token_id, name) DO UPDATE SET version = EXCLUDED.version, \
               capabilities = EXCLUDED.capabilities, last_seen_at = EXCLUDED.last_seen_at \
             RETURNING id, drain",
        )
        .bind(&new_id)
        .bind(i64::try_from(token.token_id).unwrap_or(i64::MAX))
        .bind(&reg.name)
        .bind(&reg.version)
        .bind(caps)
        .bind(secs_f64(now))
        .fetch_one(&self.pool)
        .await
        .map_err(se)?;
        Ok(WorkerRecord {
            worker_id: row.try_get("id").map_err(se)?,
            token_id: token.token_id,
            name: reg.name.clone(),
            version: reg.version.clone(),
            capabilities: reg.capabilities.clone(),
            last_seen: now,
            drain: row.try_get("drain").map_err(se)?,
        })
    }

    async fn get_worker(&self, worker_id: &str) -> Result<Option<WorkerRecord>, StoreError> {
        let row = sqlx::query(
            "SELECT id, token_id, name, version, capabilities, \
               (extract(epoch FROM last_seen_at))::bigint AS seen, drain \
             FROM transcode_workers WHERE id = $1",
        )
        .bind(worker_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(se)?;
        let Some(row) = row else { return Ok(None) };
        let caps: Value = row.try_get("capabilities").map_err(se)?;
        let token_id: i64 = row.try_get("token_id").map_err(se)?;
        let seen: i64 = row.try_get("seen").map_err(se)?;
        Ok(Some(WorkerRecord {
            worker_id: row.try_get("id").map_err(se)?,
            token_id: u64::try_from(token_id).unwrap_or_default(),
            name: row.try_get("name").map_err(se)?,
            version: row.try_get("version").map_err(se)?,
            capabilities: serde_json::from_value::<WorkerCapabilities>(caps).unwrap_or_default(),
            last_seen: u64::try_from(seen).unwrap_or_default(),
            drain: row.try_get("drain").map_err(se)?,
        }))
    }

    async fn touch_worker(&self, worker_id: &str, now: u64) -> Result<(), StoreError> {
        let r = sqlx::query(
            "UPDATE transcode_workers SET last_seen_at = to_timestamp($2) WHERE id = $1",
        )
        .bind(worker_id)
        .bind(secs_f64(now))
        .execute(&self.pool)
        .await
        .map_err(se)?;
        if r.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    async fn cancel_requests_for(&self, worker_id: &str) -> Result<Vec<String>, StoreError> {
        let rows = sqlx::query(
            "SELECT id FROM transcode_jobs WHERE worker_id = $1 AND state = 'running' AND cancel_requested",
        )
        .bind(worker_id)
        .fetch_all(&self.pool)
        .await
        .map_err(se)?;
        rows.iter()
            .map(|r| Ok(r.try_get::<i64, _>("id").map_err(se)?.to_string()))
            .collect()
    }

    async fn enqueue(&self, job: NewJob) -> Result<String, StoreError> {
        let id = (self.ids)();
        let mut conn = self.pool.acquire().await.map_err(se)?;
        insert_job(&mut conn, id, &job).await.map_err(se)?;
        Ok(id.to_string())
    }

    async fn claim_next(
        &self,
        worker_id: &str,
        kinds: &[JobKind],
        scope: TokenScope,
        now: u64,
        lease_secs: u64,
    ) -> Result<Option<JobRecord>, StoreError> {
        // A mount-scoped token sees only the mount_transcode jobs of its mount; the instance scope sees all.
        let only_mount: Option<i64> = match scope {
            TokenScope::Instance => None,
            TokenScope::Mount(id) => Some(id),
        };
        let kinds: Vec<String> = kinds.iter().map(|k| k.as_str().to_owned()).collect();
        let mut tx = self.pool.begin().await.map_err(se)?;
        let sql = format!(
            "UPDATE transcode_jobs SET state = 'running', attempt = attempt + 1, worker_id = $1, \
               lease_expires_at = to_timestamp($2), started_at = COALESCE(started_at, now()), \
               updated_at = now(), progress = NULL, error = NULL, finished_at = NULL \
             WHERE id = ( \
               SELECT id FROM transcode_jobs WHERE state = 'queued' AND kind = ANY($3) AND (($4::bigint IS NULL AND (kind <> 'mount_transcode' OR spec->>'operator_worker' = 'true')) OR ($4::bigint IS NOT NULL AND kind = 'mount_transcode' AND mount_id = $4)) \
               ORDER BY priority DESC, id FOR UPDATE SKIP LOCKED LIMIT 1) \
             RETURNING {JOB_COLS}"
        );
        let row = super::dynq(sql)
            .bind(worker_id)
            .bind(secs_f64(now.saturating_add(lease_secs)))
            .bind(&kinds)
            .bind(only_mount)
            .fetch_optional(&mut *tx)
            .await
            .map_err(se)?;
        let Some(row) = row else {
            tx.commit().await.map_err(se)?;
            return Ok(None);
        };
        let job = job_from_row(&row)?;
        // A new attempt starts from nothing: leftovers of the previous one are not outputs.
        sqlx::query("DELETE FROM transcode_uploads WHERE job_id = $1")
            .bind(parse_id(&job.job_id)?)
            .execute(&mut *tx)
            .await
            .map_err(se)?;
        // A remux claimed by a worker is being prepared.
        sqlx::query(
            "UPDATE cinema_download_cache SET state = 'preparing' WHERE job_id = $1 AND state = 'queued'",
        )
        .bind(parse_id(&job.job_id)?)
        .execute(&mut *tx)
        .await
        .map_err(se)?;
        tx.commit().await.map_err(se)?;
        Ok(Some(job))
    }

    async fn get_job(&self, job_id: &str) -> Result<Option<JobRecord>, StoreError> {
        let Ok(id) = job_id.parse::<i64>() else {
            return Ok(None);
        };
        let row = super::dynq(format!(
            "SELECT {JOB_COLS} FROM transcode_jobs WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(se)?;
        row.as_ref().map(job_from_row).transpose()
    }

    async fn report_progress(
        &self,
        job_id: &str,
        worker_id: &str,
        progress: &ProgressRequest,
        now: u64,
        lease_secs: u64,
    ) -> Result<JobRecord, StoreError> {
        let id = parse_id(job_id)?;
        let row = super::dynq(format!(
            "UPDATE transcode_jobs SET progress = $3, lease_expires_at = to_timestamp($4), updated_at = now() \
             WHERE id = $1 AND worker_id = $2 AND state = 'running' RETURNING {JOB_COLS}"
        ))
        .bind(id)
        .bind(worker_id)
        .bind(serde_json::to_value(progress).map_err(json_err)?)
        .bind(secs_f64(now.saturating_add(lease_secs)))
        .fetch_optional(&self.pool)
        .await
        .map_err(se)?;
        match row {
            Some(r) => job_from_row(&r),
            None => Err(StoreError::Conflict("job is not running on this worker")),
        }
    }

    async fn add_upload(&self, job_id: &str, upload: UploadRecord) -> Result<(), StoreError> {
        let id = parse_id(job_id)?;
        let r = sqlx::query(
            "INSERT INTO transcode_uploads (job_id, upload_id, path, key, size, finished) \
             VALUES ($1, $2, $3, $4, $5, false) \
             ON CONFLICT (job_id, path) DO UPDATE SET upload_id = EXCLUDED.upload_id, \
               key = EXCLUDED.key, size = EXCLUDED.size, finished = false",
        )
        .bind(id)
        .bind(&upload.upload_id)
        .bind(&upload.path)
        .bind(&upload.key)
        .bind(i64::try_from(upload.size).unwrap_or(i64::MAX))
        .execute(&self.pool)
        .await;
        match r {
            Ok(_) => Ok(()),
            Err(e) if super::is_unique_violation(&e) => Ok(()),
            Err(sqlx::Error::Database(d)) if d.code().is_some_and(|c| c == "23503") => {
                Err(StoreError::NotFound)
            }
            Err(e) => Err(se(e)),
        }
    }

    async fn finish_upload(
        &self,
        job_id: &str,
        upload_id: &str,
    ) -> Result<UploadRecord, StoreError> {
        let id = parse_id(job_id)?;
        let row = sqlx::query(
            "UPDATE transcode_uploads SET finished = true WHERE job_id = $1 AND upload_id = $2 \
             RETURNING upload_id, path, key, size, finished",
        )
        .bind(id)
        .bind(upload_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(se)?
        .ok_or(StoreError::NotFound)?;
        upload_from_row(&row)
    }

    async fn uploads(&self, job_id: &str) -> Result<Vec<UploadRecord>, StoreError> {
        let Ok(id) = job_id.parse::<i64>() else {
            return Ok(Vec::new());
        };
        let rows = sqlx::query(
            "SELECT upload_id, path, key, size, finished FROM transcode_uploads WHERE job_id = $1 ORDER BY path",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(se)?;
        rows.iter().map(upload_from_row).collect()
    }

    async fn complete_job(
        &self,
        job_id: &str,
        worker_id: &str,
        req: &CompleteRequest,
    ) -> Result<(), StoreError> {
        let id = parse_id(job_id)?;
        let mut tx = self.pool.begin().await.map_err(se)?;
        let row = sqlx::query(
            "UPDATE transcode_jobs SET state = 'succeeded', outputs = $3, segment_index = $4, \
               encoder = $5, log_tail = $6, wall_secs = $7, lease_expires_at = NULL, error = NULL, \
               cancel_requested = false, finished_at = now(), updated_at = now(), \
               progress = jsonb_build_object('stage', 'upload', 'percent', 100) \
             WHERE id = $1 AND worker_id = $2 AND state = 'running' \
             RETURNING kind, asset_id, attempt, spec",
        )
        .bind(id)
        .bind(worker_id)
        .bind(serde_json::to_value(&req.outputs).map_err(json_err)?)
        .bind(serde_json::to_value(&req.segment_index).map_err(json_err)?)
        .bind(&req.encoder)
        .bind(req.log_tail.as_deref())
        .bind(req.wall_secs)
        .fetch_optional(&mut *tx)
        .await
        .map_err(se)?
        .ok_or(StoreError::Conflict("job is not running on this worker"))?;
        let asset_id: i64 = row.try_get("asset_id").map_err(se)?;
        let attempt: i32 = row.try_get("attempt").map_err(se)?;
        let spec: JobSpec =
            serde_json::from_value(row.try_get("spec").map_err(se)?).map_err(json_err)?;
        let prefix = format!("transcode/{id}/{attempt}");
        match &spec {
            JobSpec::Proxy { plan } => {
                sqlx::query(
                    "UPDATE video_assets SET proxy_key = $2, proxy_created_at = now(), \
                       state = CASE WHEN state IN ('probing', 'uploaded') AND NOT auto_ladder \
                                    THEN 'proxy_ready' ELSE state END, \
                       updated_at = now() WHERE id = $1",
                )
                .bind(asset_id)
                .bind(format!("{prefix}/{}", plan.output))
                .execute(&mut *tx)
                .await
                .map_err(se)?;
            }
            JobSpec::Ladder { plan } => {
                apply_ladder(&mut tx, &self.ids, id, asset_id, &prefix, plan, req)
                    .await
                    .map_err(se)?;
            }
            JobSpec::MountTranscode {
                mount_id,
                key,
                format,
                preset,
                ..
            } => {
                // The outputs are already in the member bucket under transcoded/<key>/; record what a
                // player opens. The catalog (playable_key, wave 29 conv-remux) reads this row.
                let paths: Vec<&str> = req.outputs.iter().map(|o| o.path.as_str()).collect();
                let entry = sr_media::mount_job::entry_of(*format, &paths).ok_or(
                    StoreError::Conflict("mount job finished without an entry file"),
                )?;
                let total: u64 = req.outputs.iter().map(|o| o.size).sum();
                sqlx::query(
                    "INSERT INTO storage_object_transcodes (mount_id, key, format, preset, prefix, entry, outputs, size_bytes, job_id) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) ON CONFLICT (mount_id, key) DO UPDATE SET format = EXCLUDED.format, preset = EXCLUDED.preset, prefix = EXCLUDED.prefix, entry = EXCLUDED.entry, outputs = EXCLUDED.outputs, size_bytes = EXCLUDED.size_bytes, job_id = EXCLUDED.job_id, finished_at = now()",
                )
                .bind(mount_id)
                .bind(key)
                .bind(format.as_str())
                .bind(preset.as_str())
                .bind(sr_media::mount_job::output_object_key(key, ""))
                .bind(entry.clone())
                .bind(serde_json::to_value(&req.outputs).map_err(json_err)?)
                .bind(i64::try_from(total).unwrap_or(i64::MAX))
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(se)?;
                // A single MP4 is the original's playable copy at once (migration 1480); the indexer keeps it on
                // later runs. A copy the member made in the browser wins; an HLS ladder is not a direct key.
                if format.as_str() == "mp4" {
                    sqlx::query(
                        "UPDATE storage_objects SET playable_key = $3, converter = 'worker', converted_at = now() \
                         WHERE mount_id = $1 AND key = $2 AND (playable_key IS NULL OR converter = 'worker')",
                    )
                    .bind(mount_id)
                    .bind(key)
                    .bind(format!(
                        "{}{entry}",
                        sr_media::mount_job::output_object_key(key, "")
                    ))
                    .execute(&mut *tx)
                    .await
                    .map_err(se)?;
                }
            }
            JobSpec::Remux { plan } => {
                let size = req
                    .outputs
                    .iter()
                    .find(|o| o.path == plan.output)
                    .map_or(0, |o| i64::try_from(o.size).unwrap_or(i64::MAX));
                sqlx::query(
                    "UPDATE cinema_download_cache SET state = 'ready', object_key = $2, size_bytes = $3, \
                       ready_at = now(), expires_at = now() + make_interval(days => 7), error = NULL \
                     WHERE job_id = $1",
                )
                .bind(id)
                .bind(format!("{prefix}/{}", plan.output))
                .bind(size)
                .execute(&mut *tx)
                .await
                .map_err(se)?;
            }
        }
        tx.commit().await.map_err(se)?;
        Ok(())
    }

    async fn fail_job(
        &self,
        job_id: &str,
        worker_id: &str,
        req: &FailRequest,
        max_attempts: u32,
    ) -> Result<FailOutcome, StoreError> {
        let id = parse_id(job_id)?;
        let mut tx = self.pool.begin().await.map_err(se)?;
        let locked = sqlx::query(
            "SELECT id FROM transcode_jobs WHERE id = $1 AND worker_id = $2 AND state = 'running' FOR UPDATE",
        )
        .bind(id)
        .bind(worker_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(se)?;
        if locked.is_none() {
            return Err(StoreError::Conflict("job is not running on this worker"));
        }
        let outcome = settle_running(
            &mut tx,
            id,
            req.retryable,
            &req.message,
            req.log_tail.as_deref(),
            req.cancelled,
            max_attempts,
        )
        .await
        .map_err(se)?;
        tx.commit().await.map_err(se)?;
        Ok(outcome)
    }

    async fn request_cancel(&self, job_id: &str) -> Result<(), StoreError> {
        let id = parse_id(job_id)?;
        let mut tx = self.pool.begin().await.map_err(se)?;
        let row = sqlx::query(
            "SELECT state, kind, asset_id FROM transcode_jobs WHERE id = $1 FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(se)?
        .ok_or(StoreError::NotFound)?;
        let state: String = row.try_get("state").map_err(se)?;
        match state.as_str() {
            "queued" => {
                let kind: String = row.try_get("kind").map_err(se)?;
                let asset_id: i64 = row.try_get("asset_id").map_err(se)?;
                sqlx::query(
                    "UPDATE transcode_jobs SET state = 'cancelled', error = 'cancelled by an admin', \
                       finished_at = now(), updated_at = now() WHERE id = $1",
                )
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(se)?;
                failure_effects(&mut tx, id, &kind, asset_id, "cancelled", true)
                    .await
                    .map_err(se)?;
            }
            "running" => {
                sqlx::query("UPDATE transcode_jobs SET cancel_requested = true, updated_at = now() WHERE id = $1")
                    .bind(id)
                    .execute(&mut *tx)
                    .await
                    .map_err(se)?;
            }
            _ => return Err(StoreError::Conflict("job already finished")),
        }
        tx.commit().await.map_err(se)?;
        Ok(())
    }

    async fn reap_expired(&self, now: u64, max_attempts: u32) -> Result<Vec<String>, StoreError> {
        let mut tx = self.pool.begin().await.map_err(se)?;
        let rows = sqlx::query(
            "SELECT id FROM transcode_jobs WHERE state = 'running' AND lease_expires_at <= to_timestamp($1) \
             ORDER BY id FOR UPDATE SKIP LOCKED",
        )
        .bind(secs_f64(now))
        .fetch_all(&mut *tx)
        .await
        .map_err(se)?;
        let mut hit = Vec::new();
        for r in &rows {
            let id: i64 = r.try_get("id").map_err(se)?;
            settle_running(
                &mut tx,
                id,
                true,
                "worker lease expired",
                None,
                false,
                max_attempts,
            )
            .await
            .map_err(se)?;
            hit.push(id.to_string());
        }
        tx.commit().await.map_err(se)?;
        Ok(hit)
    }
}

fn upload_from_row(r: &PgRow) -> Result<UploadRecord, StoreError> {
    let size: i64 = r.try_get("size").map_err(se)?;
    Ok(UploadRecord {
        upload_id: r.try_get("upload_id").map_err(se)?,
        path: r.try_get("path").map_err(se)?,
        key: r.try_get("key").map_err(se)?,
        size: u64::try_from(size).unwrap_or_default(),
        finished: r.try_get("finished").map_err(se)?,
    })
}

/// Replace the asset's renditions and tracks with the finished ladder and point the asset at the
/// new storage prefix; the previous prefix is queued for collection.
async fn apply_ladder(
    tx: &mut PgConnection,
    ids: &super::IdSource,
    job_id: i64,
    asset_id: i64,
    prefix: &str,
    plan: &JobPlan,
    req: &CompleteRequest,
) -> Result<(), sqlx::Error> {
    let old: Option<String> =
        sqlx::query_scalar("SELECT hls_prefix FROM video_assets WHERE id = $1 FOR UPDATE")
            .bind(asset_id)
            .fetch_optional(&mut *tx)
            .await?
            .flatten();
    for table in [
        "video_renditions",
        "video_audio_tracks",
        "video_subtitle_tracks",
    ] {
        super::dynq(format!("DELETE FROM {table} WHERE asset_id = $1"))
            .bind(asset_id)
            .execute(&mut *tx)
            .await?;
    }
    let sizes: HashMap<&str, u64> = req
        .outputs
        .iter()
        .map(|o| (o.path.as_str(), o.size))
        .collect();
    let index_of = |playlist: &str| req.segment_index.iter().find(|s| s.playlist == playlist);
    let join = |dir: &str, name: &str| format!("{dir}/{name}");
    let profile = i32::try_from(plan.profile_version).unwrap_or(i32::MAX);
    for rung in &plan.rungs {
        let Some(idx) = index_of(&rung.playlist) else {
            return Err(sqlx::Error::Protocol(format!(
                "no segment index for {}",
                rung.name
            )));
        };
        let init = join(&rung.dir, &idx.init);
        let size: u64 = idx.segments.iter().map(|s| s.size_bytes).sum::<u64>()
            + sizes.get(init.as_str()).copied().unwrap_or(0);
        sqlx::query(
            "INSERT INTO video_renditions (id, asset_id, job_id, name, width, height, bandwidth_bps, \
               frame_rate, codecs, playlist_path, init_path, segment_index, size_bytes, profile_version) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(ids())
        .bind(asset_id)
        .bind(job_id)
        .bind(&rung.name)
        .bind(i32::try_from(rung.width).unwrap_or(i32::MAX))
        .bind(i32::try_from(rung.height).unwrap_or(i32::MAX))
        .bind(i64::from(rung.maxrate_kbps) * 1000)
        .bind(plan.video.fps())
        .bind(&rung.codecs)
        .bind(&rung.playlist)
        .bind(&init)
        .bind(serde_json::to_value(idx).unwrap_or(Value::Null))
        .bind(i64::try_from(size).unwrap_or(i64::MAX))
        .bind(profile)
        .execute(&mut *tx)
        .await?;
    }
    for (i, a) in plan.audio.iter().enumerate() {
        let Some(idx) = index_of(&a.playlist) else {
            return Err(sqlx::Error::Protocol(format!(
                "no segment index for {}",
                a.playlist
            )));
        };
        sqlx::query(
            "INSERT INTO video_audio_tracks (id, asset_id, job_id, ordinal, name, language, language_tag, \
               channels, is_default, playlist_path, init_path, segment_index, profile_version) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(ids())
        .bind(asset_id)
        .bind(job_id)
        .bind(i32::try_from(i).unwrap_or(i32::MAX))
        .bind(&a.label)
        .bind(a.language.as_deref())
        .bind(a.language_tag.as_deref())
        .bind(i32::try_from(a.channels).unwrap_or(2))
        .bind(a.is_default)
        .bind(&a.playlist)
        .bind(join(&a.dir, &idx.init))
        .bind(serde_json::to_value(idx).unwrap_or(Value::Null))
        .bind(profile)
        .execute(&mut *tx)
        .await?;
    }
    for (i, s) in plan.subtitles.iter().enumerate() {
        let ass = (s.keep_raw && s.extract == ExtractFormat::Ass).then(|| s.raw_path.clone());
        sqlx::query(
            "INSERT INTO video_subtitle_tracks (id, asset_id, job_id, ordinal, name, language, language_tag, \
               forced, hearing_impaired, is_default, vtt_path, ass_path) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(ids())
        .bind(asset_id)
        .bind(job_id)
        .bind(i32::try_from(i).unwrap_or(i32::MAX))
        .bind(&s.label)
        .bind(s.language.as_deref())
        .bind(s.language_tag.as_deref())
        .bind(s.forced)
        .bind(s.hearing_impaired)
        .bind(s.is_default)
        .bind(&s.vtt_path)
        .bind(ass)
        .execute(&mut *tx)
        .await?;
    }
    let posters: Vec<String> = plan.posters.iter().map(|p| p.path.clone()).collect();
    sqlx::query(
        "UPDATE video_assets SET hls_prefix = $2, sprite_vtt_path = $3, poster_paths = $4, \
           profile_version = $5, \
           state = CASE WHEN state IN ('rejected', 'removed') THEN state ELSE 'published' END, \
           error = NULL, published_at = COALESCE(published_at, now()), updated_at = now() \
         WHERE id = $1",
    )
    .bind(asset_id)
    .bind(prefix)
    .bind(&plan.sprite.vtt_path)
    .bind(&posters)
    .bind(profile)
    .execute(&mut *tx)
    .await?;
    if let Some(old) = old.filter(|o| o != prefix) {
        sqlx::query(
            "INSERT INTO cinema_media_gc (prefix, asset_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(old)
        .bind(asset_id)
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}
