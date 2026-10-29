//! The transcoding admin (`/admin/transcode/*`, `MANAGE_TRANSCODE`): worker tokens, workers,
//! the queue, job logs, cancel and retry, re-transcode and the retention sweep.

use super::{MediaCtx, id, map_err};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::json;
use sr_core::PlatformPermissions as P;
use sr_media::assets::{
    JobDetail, JobQuery, JobSummary, MediaError, QueueCounts, WorkerToken, WorkerView,
};
use sr_media::retranscode::{RetranscodeRequest, RetranscodeScope};
use sr_media::service::{CreatedToken, RetranscodeOutcome, SweepReport};
use sr_media::worker_api::{JobKind, JobState};
use utoipa::{IntoParams, ToSchema};

fn need(ctx: &MediaCtx) -> Result<(), crate::error::HttpError> {
    ctx.require(P::MANAGE_TRANSCODE, "MANAGE_TRANSCODE")
}

/// Worker tokens, newest first (`MANAGE_TRANSCODE`). Only hashes are stored: the secret is shown
/// once, when the token is created.
#[utoipa::path(
    get, path = "/admin/transcode/tokens", operation_id = "admin_transcode_tokens",
    responses((status = 200, body = Vec<WorkerToken>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn tokens(ctx: MediaCtx) -> ApiResult<Json<Vec<WorkerToken>>> {
    need(&ctx)?;
    ctx.media().tokens().await.map(Json).map_err(map_err)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTokenBody {
    /// A label for the machine, 1 to 64 characters.
    pub name: String,
}

/// Create a worker token (`MANAGE_TRANSCODE`). The response carries the secret; it cannot be
/// shown again.
#[utoipa::path(
    post, path = "/admin/transcode/tokens", operation_id = "admin_transcode_create_token", request_body = CreateTokenBody,
    responses((status = 201, body = CreatedToken), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_token(
    ctx: MediaCtx,
    Json(b): Json<CreateTokenBody>,
) -> ApiResult<(StatusCode, Json<CreatedToken>)> {
    need(&ctx)?;
    let t = ctx
        .media()
        .create_token(ctx.user(), &b.name)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "transcode.token.create",
        ("token", t.token.id),
        json!({ "name": t.token.name }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(t)))
}

/// Revoke a token (`MANAGE_TRANSCODE`): it stops authenticating at once and the workers
/// registered with it are told to drain.
#[utoipa::path(
    delete, path = "/admin/transcode/tokens/{token_id}", operation_id = "admin_transcode_revoke_token",
    params(("token_id" = String, Path, description = "Token snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn revoke_token(ctx: MediaCtx, Path(token_id): Path<String>) -> ApiResult<StatusCode> {
    need(&ctx)?;
    let tid = id(&token_id, "token")?;
    ctx.media().revoke_token(tid).await.map_err(map_err)?;
    ctx.audit("transcode.token.revoke", ("token", tid), json!({}))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Workers that registered, with capabilities, last contact and the jobs they run
/// (`MANAGE_TRANSCODE`).
#[utoipa::path(
    get, path = "/admin/transcode/workers", operation_id = "admin_transcode_workers",
    responses((status = 200, body = Vec<WorkerView>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn workers(ctx: MediaCtx) -> ApiResult<Json<Vec<WorkerView>>> {
    need(&ctx)?;
    ctx.media().workers().await.map(Json).map_err(map_err)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DrainBody {
    pub drain: bool,
}

/// Tell a worker to finish its jobs and claim no more, or lift that (`MANAGE_TRANSCODE`). The
/// worker learns it on its next heartbeat.
#[utoipa::path(
    put, path = "/admin/transcode/workers/{worker_id}/drain", operation_id = "admin_transcode_drain", request_body = DrainBody,
    params(("worker_id" = String, Path, description = "Worker id")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn drain(
    ctx: MediaCtx,
    Path(worker_id): Path<String>,
    Json(b): Json<DrainBody>,
) -> ApiResult<StatusCode> {
    need(&ctx)?;
    ctx.media()
        .set_drain(&worker_id, b.drain)
        .await
        .map_err(map_err)?;
    ctx.audit(
        "transcode.worker.drain",
        ("worker", 0),
        json!({ "worker": worker_id, "drain": b.drain }),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Queue depth: queued, running, and the last day's outcomes (`MANAGE_TRANSCODE`).
#[utoipa::path(
    get, path = "/admin/transcode/queue", operation_id = "admin_transcode_queue",
    responses((status = 200, body = QueueCounts), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn queue(ctx: MediaCtx) -> ApiResult<Json<QueueCounts>> {
    need(&ctx)?;
    ctx.media().queue_counts().await.map(Json).map_err(map_err)
}

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct JobListParams {
    pub state: Option<JobState>,
    pub kind: Option<JobKind>,
    pub asset_id: Option<String>,
    /// 1 to 200, default 50.
    pub limit: Option<u32>,
    /// A job id: only older jobs.
    pub before: Option<String>,
}

/// Jobs, newest first (`MANAGE_TRANSCODE`).
#[utoipa::path(
    get, path = "/admin/transcode/jobs", operation_id = "admin_transcode_jobs", params(JobListParams),
    responses((status = 200, body = Vec<JobSummary>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn jobs(
    ctx: MediaCtx,
    Query(p): Query<JobListParams>,
) -> ApiResult<Json<Vec<JobSummary>>> {
    need(&ctx)?;
    ctx.media()
        .jobs(JobQuery {
            state: p.state,
            kind: p.kind,
            asset_id: p.asset_id.as_deref().map(|a| id(a, "asset")).transpose()?,
            mount_id: None,
            limit: p.limit.unwrap_or(50),
            before: p.before.as_deref().map(|b| id(b, "job")).transpose()?,
        })
        .await
        .map(Json)
        .map_err(map_err)
}

/// One job with its log tail, encoder and warnings (`MANAGE_TRANSCODE`).
#[utoipa::path(
    get, path = "/admin/transcode/jobs/{job_id}", operation_id = "admin_transcode_job",
    params(("job_id" = String, Path, description = "Job id")),
    responses((status = 200, body = JobDetail), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn job(ctx: MediaCtx, Path(job_id): Path<String>) -> ApiResult<Json<JobDetail>> {
    need(&ctx)?;
    ctx.media()
        .job(id(&job_id, "job")?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Cancel a job (`MANAGE_TRANSCODE`): a queued job stops at once, a running one when its worker
/// next reports.
#[utoipa::path(
    post, path = "/admin/transcode/jobs/{job_id}/cancel", operation_id = "admin_transcode_cancel",
    params(("job_id" = String, Path, description = "Job id")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn cancel(ctx: MediaCtx, Path(job_id): Path<String>) -> ApiResult<StatusCode> {
    need(&ctx)?;
    let jid = id(&job_id, "job")?;
    ctx.media().cancel_job(jid).await.map_err(map_err)?;
    ctx.audit("transcode.job.cancel", ("job", jid), json!({}))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Queue a failed or cancelled job again with a fresh attempt counter (`MANAGE_TRANSCODE`).
#[utoipa::path(
    post, path = "/admin/transcode/jobs/{job_id}/retry", operation_id = "admin_transcode_retry",
    params(("job_id" = String, Path, description = "Job id")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn retry(ctx: MediaCtx, Path(job_id): Path<String>) -> ApiResult<StatusCode> {
    need(&ctx)?;
    let jid = id(&job_id, "job")?;
    ctx.media().retry_job(jid).await.map_err(map_err)?;
    ctx.audit("transcode.job.retry", ("job", jid), json!({}))
        .await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RetranscodeScopeName {
    Asset,
    Title,
    All,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RetranscodeBody {
    pub scope: RetranscodeScopeName,
    /// The asset or title id; required unless the scope is `all`.
    #[serde(default)]
    pub id: Option<String>,
    /// Only assets rendered with an older profile than this.
    #[serde(default)]
    pub only_older_than_profile: Option<u32>,
    #[serde(default)]
    pub reason: Option<String>,
}

/// Re-transcode one asset, a whole title or everything (`MANAGE_TRANSCODE`): the ladder is
/// planned again from the stored probe and queued (a batch is capped at 500 assets; repeat for
/// the rest). The old files stay playable until the new ones are published.
#[utoipa::path(
    post, path = "/admin/transcode/retranscode", operation_id = "admin_transcode_retranscode", request_body = RetranscodeBody,
    responses((status = 200, body = RetranscodeOutcome), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn retranscode(
    ctx: MediaCtx,
    Json(b): Json<RetranscodeBody>,
) -> ApiResult<Json<RetranscodeOutcome>> {
    need(&ctx)?;
    let target = |what: &'static str| -> Result<u64, crate::error::HttpError> {
        let raw = b
            .id
            .as_deref()
            .ok_or_else(|| map_err(MediaError::Invalid("id is required for this scope".into())))?;
        Ok(u64::try_from(id(raw, what)?).unwrap_or_default())
    };
    let scope = match b.scope {
        RetranscodeScopeName::Asset => RetranscodeScope::Asset(target("asset")?),
        RetranscodeScopeName::Title => RetranscodeScope::Title(target("title")?),
        RetranscodeScopeName::All => RetranscodeScope::All,
    };
    let reason: String = b
        .reason
        .clone()
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect();
    let out = ctx
        .media()
        .retranscode(RetranscodeRequest {
            scope,
            only_older_than_profile: b.only_older_than_profile,
            reason: reason.clone(),
        })
        .await
        .map_err(map_err)?;
    ctx.audit(
        "transcode.retranscode",
        ("scope", 0),
        json!({
            "scope": format!("{scope:?}"),
            "enqueued": out.enqueued.len(),
            "skipped": out.skipped.len(),
            "reason": reason,
        }),
    )
    .await;
    Ok(Json(out))
}

/// Run the retention sweep now (`MANAGE_TRANSCODE`): expired upload slots, originals and
/// proxies the retention policy lets go, unreferenced transcode prefixes, expired downloads. It
/// also runs hourly.
#[utoipa::path(
    post, path = "/admin/transcode/sweep", operation_id = "admin_transcode_sweep",
    responses((status = 200, body = SweepReport), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn sweep(ctx: MediaCtx) -> ApiResult<Json<SweepReport>> {
    need(&ctx)?;
    let report = ctx.media().sweep().await.map_err(map_err)?;
    ctx.deps.downloads.sweep().await.map_err(map_err)?;
    ctx.audit("transcode.sweep", ("sweep", 0), json!({ "report": report }))
        .await;
    Ok(Json(report))
}
