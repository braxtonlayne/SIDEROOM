//! The transcoder as an optional worker into a member's own bucket (BYOS-CNV-003).
//!
//! * `POST /storage/mounts/{mount_id}/transcode`: queue a `mount_transcode` job for one object (the key, HLS or a
//!   single MP4, a ladder preset). `GET` lists the latest jobs of the mount (the member's view; Admin >
//!   Transcoding shows the same rows with a Mount column).
//! * `POST|GET /storage/mounts/{mount_id}/worker-tokens` and `DELETE .../{worker_token_id}`: worker tokens
//!   scoped to this one mount (`worker_tokens.scope = mount:<id>`), for a member who runs `sr-transcoder` on
//!   their own machine. The secret is shown once.
//!
//! Who may queue: the owner of a member's mount, and the operator (`MANAGE_TRANSCODE`) for every other mount (a
//! server's shelf, the instance bucket). The operator's own workers serve a member's job only while
//! `byos.worker_for_members` is on; with it off the owner needs a worker of their own, otherwise the answer is
//! 409 `WORKER_NOT_ENABLED`. The server never carries a byte: the worker receives presigned URLs
//! (`sr_media::mount_job`).

use super::objects::{client_ip, storage_for, vet_endpoint, within};
use super::shelves::managed_mount;
use super::{ScopeDeps, facts_failed, parse_row, refusal, uid};
use crate::auth::SessionOnly;
use crate::openapi::ErrorBody;
use crate::routes::admin::AdminDeps;
use crate::routes::messages::error::{ApiJson, MsgError, MsgResult, invalid};
use crate::routes::{Ext, MaybeExt};
use crate::state::AppState;
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sr_admin::Caller;
use sr_core::{ErrorCode, PlatformPermissions};
use sr_media::assets::{JobSummary, MediaError, WorkerToken};
use sr_media::mount_job::{MountFormat, MountPreset};
use sr_media::service::{CreatedToken, MediaService};
use sr_storage::ObjectStorage;
use sr_storage::mounts::{INSTANCE_MOUNT_ID, Mount, MountOwner};
use sr_storage::path::safe_key;
use sr_storage::scopes::{ResolveError, ScopeFacts, Viewer};
use std::sync::Arc;
use utoipa::{OpenApi, ToSchema};

/// Most live worker tokens one mount may hold.
pub const MAX_MOUNT_TOKENS: usize = 5;

/// Collaborators of the conversion routes (the mount lookup and facts come from [`ScopeDeps`]).
pub struct TranscodeDeps {
    /// The media service: the job queue and the worker tokens.
    pub media: Arc<MediaService>,
}

/// Queue a conversion.
#[derive(Debug, Deserialize, ToSchema)]
pub struct QueueTranscodeBody {
    /// Mount-relative key of the video.
    pub key: String,
    /// `hls` (a ladder) or `mp4` (one file).
    pub format: MountFormat,
    /// The tallest output: `low` (480p), `standard` (720p, the default) or `high` (1080p).
    #[serde(default)]
    pub preset: Option<MountPreset>,
}

/// A queued conversion.
#[derive(Debug, Serialize, ToSchema)]
pub struct QueuedTranscodeView {
    /// The job id (Admin > Transcoding shows it too).
    pub job_id: String,
    /// Whether the operator's own workers may take it (`byos.worker_for_members`); false: only a worker
    /// token of this mount does.
    pub operator_worker: bool,
    /// Where the result lands in the bucket, mount-relative.
    pub output_prefix: String,
}

/// A new worker token for a mount.
#[derive(Debug, Deserialize, ToSchema)]
pub struct MountTokenBody {
    /// A label for the machine, 1 to 64 characters.
    pub name: String,
}

fn media_error(e: MediaError) -> MsgError {
    match e {
        MediaError::NotFound(_) => ErrorCode::SrNotFound.into(),
        MediaError::Invalid(m) => invalid("name", "BASE_TYPE_BAD_LENGTH", &m),
        MediaError::Conflict(m) => super::mounts::conflict("CONFLICT", &m),
        other => {
            tracing::error!(error = %other, "mount transcode media call failed");
            ErrorCode::SrInternal.into()
        }
    }
}

/// The caller as an operator allowed to run the transcoder, if they are.
async fn transcode_operator(
    admin: Option<&AdminDeps>,
    user: i64,
    ip: Option<String>,
) -> Option<Caller> {
    let admin = admin?;
    let caller = admin.service.resolve(user, ip).await.ok()?;
    (caller.is_owner
        || caller
            .permissions
            .has(PlatformPermissions::MANAGE_TRANSCODE))
    .then_some(caller)
}

/// Who is acting on the mount.
enum Acting {
    /// The owner of a member's mount.
    Owner,
    /// An operator on a mount that is not a member's (logged).
    Operator(Caller),
}

/// The mount and the right to run conversions in it: the owner of a member's mount, or an operator on any other.
/// A mount that is not the caller's and not reachable is a 404; the instance bucket (mount 1) is the operator's.
async fn conversion_mount(
    d: &ScopeDeps,
    admin: Option<&AdminDeps>,
    user: i64,
    ip: Option<String>,
    mount_id: &str,
) -> MsgResult<(Mount, Acting)> {
    let id = parse_row(mount_id)?;
    let mount = match d.mounts.by_id(id).await {
        Ok(Some(m)) => m,
        Ok(None) => return Err(ErrorCode::SrNotFound.into()),
        Err(e) => {
            tracing::error!(error = %e, "mount lookup failed");
            return Err(ErrorCode::SrInternal.into());
        }
    };
    if mount.status == sr_storage::mounts::MountStatus::Disabled {
        return Err(ErrorCode::SrNotFound.into());
    }
    match mount.owner {
        MountOwner::User(_) if mount.id != INSTANCE_MOUNT_ID => {
            // The owner, or a 403/404 like every other mount-owner route.
            let m = managed_mount(d, user, mount_id).await?;
            if m.owner != MountOwner::User(user) {
                return Err(ErrorCode::MissingPermissions.into());
            }
            Ok((m, Acting::Owner))
        }
        _ => match transcode_operator(admin, user, ip).await {
            Some(c) => Ok((mount, Acting::Operator(c))),
            None => {
                // Hide a shelf the caller cannot reach; a member of it learns only that they may not.
                let facts: &dyn ScopeFacts = &*d.store;
                match sr_storage::scopes::access(
                    facts,
                    Viewer {
                        id: user,
                        staff: false,
                    },
                    &mount,
                )
                .await
                {
                    Ok(_) => Err(ErrorCode::MissingPermissions.into()),
                    Err(ResolveError::Refused(r)) => Err(refusal(r)),
                    Err(ResolveError::Facts(e)) => Err(facts_failed(e)),
                }
            }
        },
    }
}

async fn worker_for_members(admin: Option<&AdminDeps>) -> bool {
    match admin {
        Some(a) => {
            a.service
                .setting_value_i64(sr_admin::settings::BYOS_WORKER_FOR_MEMBERS)
                .await
                .unwrap_or(0)
                == 1
        }
        None => false,
    }
}

/// Queue the conversion of one video of a mount. The worker receives a presigned `GET` of the source and a
/// presigned `PUT` for every output file; the result lands under `transcoded/<key>/` in the same bucket.
#[utoipa::path(
    post,
    path = "/storage/mounts/{mount_id}/transcode",
    params(("mount_id" = String, Path, description = "Mount snowflake")),
    request_body = QueueTranscodeBody,
    responses(
        (status = 202, body = QueuedTranscodeView),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "WORKER_NOT_ENABLED or BUCKET_UNREACHABLE"),
    ),
    security(("session" = []))
)]
pub async fn queue(
    State(state): State<AppState>,
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(t): Ext<TranscodeDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    headers: HeaderMap,
    Path(mount_id): Path<String>,
    ApiJson(body): ApiJson<QueueTranscodeBody>,
) -> MsgResult<(StatusCode, Json<QueuedTranscodeView>)> {
    safe_key(&body.key, false).map_err(|e| super::objects::key_error("key", e))?;
    if sr_storage::scopes::Kind::of_key(&body.key) != Some(sr_storage::scopes::Kind::Cinema) {
        return Err(invalid(
            "key",
            "KEY_INVALID",
            "Only video files can be converted.",
        ));
    }
    let ip = client_ip(&headers, &state);
    let (mount, acting) = conversion_mount(&d, admin.as_deref(), uid(&p), ip, &mount_id).await?;
    // A delisted key binds everyone; the viewer here has already proved the right to the whole mount.
    let facts: &dyn ScopeFacts = &*d.store;
    sr_storage::scopes::resolve(
        facts,
        Viewer {
            id: uid(&p),
            staff: matches!(acting, Acting::Operator(_)),
        },
        &mount,
        &body.key,
    )
    .await
    .map_err(|e| match e {
        ResolveError::Refused(r) => refusal(r),
        ResolveError::Facts(e) => facts_failed(e),
    })?;
    // The operator's workers serve a member only when the operator says so; everything else is the operator's own.
    let operator_worker = match acting {
        Acting::Owner => worker_for_members(admin.as_deref()).await,
        Acting::Operator(_) => true,
    };
    if !operator_worker {
        let tokens = t.media.mount_tokens(mount.id).await.map_err(media_error)?;
        if !tokens.iter().any(|k| k.revoked_at_ms.is_none()) {
            return Err(super::mounts::conflict(
                "WORKER_NOT_ENABLED",
                "This server does not offer its transcoder to members. Run sr-transcoder yourself with a worker token of this bucket.",
            ));
        }
    }
    // The size, from the bucket (a head request; the object is not read).
    vet_endpoint(&d, &mount).await?;
    let storage = storage_for(&d, &mount)?;
    let info = within(storage.head(&mount.bucket_key(&body.key))).await?;
    if info.size == 0 {
        return Err(invalid("key", "KEY_INVALID", "That file is empty."));
    }
    let preset = body.preset.unwrap_or(MountPreset::Standard);
    let job_id = t
        .media
        .queue_mount_transcode(
            mount.id,
            &body.key,
            info.size,
            body.format,
            preset,
            operator_worker,
        )
        .await
        .map_err(media_error)?;
    if let (Acting::Operator(caller), Some(a)) = (&acting, admin.as_deref())
        && let Err(e) = a
            .service
            .record_action(
                caller,
                "storage.mount_transcode",
                Some(("storage_mount", mount.id)),
                json!({ "key": body.key, "job_id": job_id }),
            )
            .await
    {
        tracing::error!(error = %e, "operator mount transcode was not written to the access log");
    }
    Ok((
        StatusCode::ACCEPTED,
        Json(QueuedTranscodeView {
            job_id,
            operator_worker,
            output_prefix: sr_media::mount_job::output_object_key(&body.key, ""),
        }),
    ))
}

/// The latest conversions of a mount, newest first.
#[utoipa::path(
    get,
    path = "/storage/mounts/{mount_id}/transcode",
    params(("mount_id" = String, Path, description = "Mount snowflake")),
    responses(
        (status = 200, body = Vec<JobSummary>),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(t): Ext<TranscodeDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    headers: HeaderMap,
    Path(mount_id): Path<String>,
) -> MsgResult<Json<Vec<JobSummary>>> {
    let ip = client_ip(&headers, &state);
    let (mount, _) = conversion_mount(&d, admin.as_deref(), uid(&p), ip, &mount_id).await?;
    let jobs = t
        .media
        .mount_jobs(mount.id, 50)
        .await
        .map_err(media_error)?;
    Ok(Json(jobs))
}

/// The mount, if it is the caller's own (worker tokens are the owner's, never an operator's).
async fn owned_mount(d: &ScopeDeps, user: i64, mount_id: &str) -> MsgResult<Mount> {
    let mount = managed_mount(d, user, mount_id).await?;
    if mount.owner != MountOwner::User(user) || mount.id == INSTANCE_MOUNT_ID {
        return Err(ErrorCode::MissingPermissions.into());
    }
    Ok(mount)
}

/// Create a worker token scoped to this mount. The response carries the secret; it cannot be shown again.
#[utoipa::path(
    post,
    path = "/storage/mounts/{mount_id}/worker-tokens",
    params(("mount_id" = String, Path, description = "Mount snowflake")),
    request_body = MountTokenBody,
    responses(
        (status = 201, body = CreatedToken),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "Too many worker tokens"),
    ),
    security(("session" = []))
)]
pub async fn create_token(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(t): Ext<TranscodeDeps>,
    Path(mount_id): Path<String>,
    ApiJson(body): ApiJson<MountTokenBody>,
) -> MsgResult<(StatusCode, Json<CreatedToken>)> {
    let mount = owned_mount(&d, uid(&p), &mount_id).await?;
    let live = t
        .media
        .mount_tokens(mount.id)
        .await
        .map_err(media_error)?
        .iter()
        .filter(|k| k.revoked_at_ms.is_none())
        .count();
    if live >= MAX_MOUNT_TOKENS {
        return Err(super::mounts::conflict(
            "TOO_MANY_TOKENS",
            "This bucket already has the most worker tokens it may have. Revoke one first.",
        ));
    }
    let created = t
        .media
        .create_mount_token(uid(&p), &body.name, mount.id)
        .await
        .map_err(media_error)?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// The worker tokens of this mount (never their secrets).
#[utoipa::path(
    get,
    path = "/storage/mounts/{mount_id}/worker-tokens",
    params(("mount_id" = String, Path, description = "Mount snowflake")),
    responses(
        (status = 200, body = Vec<WorkerToken>),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn list_tokens(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(t): Ext<TranscodeDeps>,
    Path(mount_id): Path<String>,
) -> MsgResult<Json<Vec<WorkerToken>>> {
    let mount = owned_mount(&d, uid(&p), &mount_id).await?;
    Ok(Json(
        t.media.mount_tokens(mount.id).await.map_err(media_error)?,
    ))
}

/// Revoke a worker token of this mount; its workers drain.
#[utoipa::path(
    delete,
    path = "/storage/mounts/{mount_id}/worker-tokens/{worker_token_id}",
    params(
        ("mount_id" = String, Path, description = "Mount snowflake"),
        ("worker_token_id" = String, Path, description = "Worker token snowflake"),
    ),
    responses(
        (status = 204),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn revoke_token(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(t): Ext<TranscodeDeps>,
    Path((mount_id, token_id)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let mount = owned_mount(&d, uid(&p), &mount_id).await?;
    let token = parse_row(&token_id)?;
    // A token of another mount (or an instance token) is "not found" here, never revoked.
    t.media
        .revoke_mount_token(mount.id, token)
        .await
        .map_err(media_error)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(OpenApi)]
#[openapi(
    paths(queue, list, create_token, list_tokens, revoke_token),
    components(schemas(
        QueueTranscodeBody,
        QueuedTranscodeView,
        MountTokenBody,
        MountFormat,
        MountPreset,
    ))
)]
pub struct TranscodeApiDoc;

/// The OpenAPI fragment of the conversion routes.
pub fn openapi() -> utoipa::openapi::OpenApi {
    TranscodeApiDoc::openapi()
}
