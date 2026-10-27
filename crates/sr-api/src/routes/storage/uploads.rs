//! Browser uploads into a bucket mount through the ledger (BYOS-BRW-001).
//!
//! The browser uploads each 16 MiB part straight to the bucket with a presigned `PUT`; the server only
//! signs, lists parts and completes. The flow:
//!
//! 1. `POST /storage/mounts/{mount_id}/uploads` (key, size, content type) starts a multipart upload at the
//!    bucket and records it in the ledger; the answer carries the part size and count.
//! 2. `POST .../uploads/{upload_id}/parts/{n}` answers a presigned `PUT` for part `n` (an hour), counted
//!    against `byos.upload_parts_per_hour`.
//! 3. `GET .../uploads/{upload_id}` lists the parts the bucket holds, which is how a reload resumes.
//! 4. `POST .../uploads/{upload_id}/complete` checks the parts against the declared size and assembles the
//!    object; `DELETE .../uploads/{upload_id}` aborts. `GET .../uploads` lists the caller's open uploads.
//!
//! Who may upload is `sr_storage::scopes::may_upload` (the owner of a user mount; on a server shelf the
//! owner, administrators and roles with the `upload` or `manage` grant). A friend with a share cannot. A
//! mount that has not passed its CORS check cannot start an upload (409 `CORS_NOT_READY`). Another
//! member's upload id is 404. Abandoned uploads are aborted by the `storage.uploads_sweep` job after 24 hours.

use super::objects::{Need, bucket_error, key_error, within, within_for, writer};
use super::{ScopeDeps, UploadDeps, cors_not_ready, mounts::conflict, parse_row, uid};
use crate::auth::SessionOnly;
use crate::routes::admin::AdminDeps;
use crate::routes::messages::error::{ApiJson, MsgError, MsgResult, invalid};
use crate::routes::messages::model::iso;
use crate::routes::{Ext, MaybeExt};
use crate::{openapi::ErrorBody, state::AppState};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use sr_core::ErrorCode;
use sr_storage::mounts::Mount;
use sr_storage::path::safe_key;
use sr_storage::uploads::{
    MAX_OPEN_UPLOADS, MAX_PARTS, MAX_UPLOAD_BYTES, MountUpload, PART_SIZE, PART_URL_TTL,
    UploadState, part_count,
};
use std::time::Duration;
use utoipa::ToSchema;

/// Body of `POST /storage/mounts/{mount_id}/uploads`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct BeginUploadBody {
    /// Mount-relative key of the object to create (an existing object is replaced).
    pub key: String,
    /// Size of the file in bytes (at least 1, at most 156 GiB).
    pub size: u64,
    /// Content type the object is stored with; defaults to `application/octet-stream`.
    #[serde(default)]
    pub content_type: Option<String>,
}

/// One upload in the ledger.
#[derive(Debug, Serialize, ToSchema)]
pub struct UploadView {
    /// Upload id (snowflake).
    pub id: String,
    /// Mount-relative key.
    pub key: String,
    /// Declared size in bytes.
    pub size: u64,
    /// Bytes per part (the last part is smaller).
    pub part_size: u64,
    /// How many parts the file needs.
    pub part_count: u32,
    /// Stored content type.
    pub content_type: String,
    /// Started (ISO 8601).
    pub created_at: String,
}

/// A part the bucket holds.
#[derive(Debug, Serialize, ToSchema)]
pub struct PartView {
    /// Part number, from 1.
    pub number: u32,
    /// Bytes the bucket stored.
    pub size: u64,
    /// The bucket's entity tag for the part.
    pub etag: String,
}

/// An upload with the parts that already arrived.
#[derive(Debug, Serialize, ToSchema)]
pub struct UploadStatusView {
    /// The ledger row.
    pub upload: UploadView,
    /// Parts in the bucket, in order.
    pub parts: Vec<PartView>,
}

/// A presigned part `PUT`.
#[derive(Debug, Serialize, ToSchema)]
pub struct PartUrlView {
    /// Part number.
    pub part_number: u32,
    /// Bytes this part must have.
    pub size: u64,
    /// Send the part body with `PUT` to this URL; never stored or logged by the server.
    pub url: String,
    /// When it stops working (ISO 8601).
    pub expires_at: String,
}

/// One part the client says it sent.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CompletePart {
    /// Part number.
    pub number: u32,
    /// The `ETag` the bucket answered.
    pub etag: String,
}

/// Body of `POST .../uploads/{upload_id}/complete`.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct CompleteUploadBody {
    /// Optional: the parts and etags the client holds. When given they must match what the bucket lists.
    #[serde(default)]
    pub parts: Vec<CompletePart>,
}

/// The finished object.
#[derive(Debug, Serialize, ToSchema)]
pub struct CompletedUploadView {
    /// Mount-relative key.
    pub key: String,
    /// Size in bytes.
    pub size: u64,
}

fn view(u: &MountUpload) -> UploadView {
    UploadView {
        id: u.id.to_string(),
        key: u.key.clone(),
        size: u.size,
        part_size: u.part_size,
        part_count: u.part_count,
        content_type: u.content_type.clone(),
        created_at: iso(u.created_at),
    }
}

fn ledger_failed(e: sr_storage::uploads::LedgerError) -> MsgError {
    tracing::error!(error = %e, "upload ledger failed");
    ErrorCode::SrInternal.into()
}

fn clean_content_type(raw: Option<&str>) -> MsgResult<String> {
    let t = raw.map(str::trim).filter(|t| !t.is_empty());
    match t {
        None => Ok("application/octet-stream".to_owned()),
        Some(t) if t.len() <= 255 && t.chars().all(|c| c.is_ascii_graphic() || c == ' ') => {
            Ok(t.to_owned())
        }
        Some(_) => Err(invalid(
            "content_type",
            "CONTENT_TYPE_INVALID",
            "That content type is not valid.",
        )),
    }
}

/// The caller's open upload on this mount; anything else is 404.
async fn load(d: &UploadDeps, mount: &Mount, upload_id: &str, user: i64) -> MsgResult<MountUpload> {
    let id = parse_row(upload_id)?;
    match d.ledger.get(mount.id, id).await.map_err(ledger_failed)? {
        Some(u) if u.user_id == user && u.state == UploadState::Open => Ok(u),
        _ => Err(ErrorCode::SrNotFound.into()),
    }
}

/// Bytes part `n` must have.
fn expected_size(u: &MountUpload, n: u32) -> u64 {
    if n < u.part_count {
        u.part_size
    } else {
        u.size
            .saturating_sub(u.part_size.saturating_mul(u64::from(u.part_count - 1)))
    }
}

/// Start an upload of one file.
#[utoipa::path(
    post,
    path = "/storage/mounts/{mount_id}/uploads",
    params(("mount_id" = String, Path, description = "Mount snowflake")),
    request_body = BeginUploadBody,
    responses(
        (status = 201, body = UploadView),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "CORS_NOT_READY, TOO_MANY_UPLOADS or BUCKET_UNREACHABLE"),
    ),
    security(("session" = []))
)]
pub async fn begin(
    State(state): State<AppState>,
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(u): Ext<UploadDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    headers: HeaderMap,
    Path(mount_id): Path<String>,
    ApiJson(body): ApiJson<BeginUploadBody>,
) -> MsgResult<(StatusCode, Json<UploadView>)> {
    safe_key(&body.key, false).map_err(|e| key_error("key", e))?;
    if body.size == 0 {
        return Err(invalid(
            "size",
            "EMPTY_FILE",
            "An empty file cannot be uploaded.",
        ));
    }
    if body.size > MAX_UPLOAD_BYTES {
        return Err(invalid(
            "size",
            "FILE_TOO_LARGE",
            "That file is larger than 156 GiB.",
        ));
    }
    let content_type = clean_content_type(body.content_type.as_deref())?;
    let mount = writer(
        &d,
        admin.as_deref(),
        &state,
        &headers,
        uid(&p),
        &mount_id,
        Need::Upload,
    )
    .await?;
    if mount.cors_ok_at.is_none() {
        return Err(cors_not_ready());
    }
    if u.ledger.count_open(uid(&p)).await.map_err(ledger_failed)? >= MAX_OPEN_UPLOADS {
        return Err(conflict(
            "TOO_MANY_UPLOADS",
            "Finish or cancel some uploads before starting more.",
        ));
    }
    let s3_id = within(u.io.begin(&mount, &body.key, &content_type)).await?;
    let now = state.now_secs().saturating_mul(1000);
    let id = i64::try_from((u.next_id)()).unwrap_or(0);
    let row = MountUpload {
        id,
        mount_id: mount.id,
        user_id: uid(&p),
        key: body.key,
        s3_upload_id: s3_id.clone(),
        size: body.size,
        part_size: PART_SIZE,
        part_count: part_count(body.size).min(MAX_PARTS),
        content_type,
        state: UploadState::Open,
        created_at: now,
        updated_at: now,
    };
    if let Err(e) = u.ledger.insert(row.clone()).await {
        // Do not leave an upload at the bucket that nothing records.
        let _ = within(u.io.abort(&mount, &row.key, &s3_id)).await;
        return Err(ledger_failed(e));
    }
    Ok((StatusCode::CREATED, Json(view(&row))))
}

/// The caller's open uploads on this mount, newest first (what a reload resumes).
#[utoipa::path(
    get,
    path = "/storage/mounts/{mount_id}/uploads",
    params(("mount_id" = String, Path, description = "Mount snowflake")),
    responses(
        (status = 200, body = Vec<UploadView>),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(u): Ext<UploadDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    headers: HeaderMap,
    Path(mount_id): Path<String>,
) -> MsgResult<Json<Vec<UploadView>>> {
    let mount = writer(
        &d,
        admin.as_deref(),
        &state,
        &headers,
        uid(&p),
        &mount_id,
        Need::Upload,
    )
    .await?;
    let rows = u
        .ledger
        .list_open(mount.id, uid(&p))
        .await
        .map_err(ledger_failed)?;
    Ok(Json(rows.iter().map(view).collect()))
}

/// One upload with the parts the bucket already holds.
#[utoipa::path(
    get,
    path = "/storage/mounts/{mount_id}/uploads/{upload_id}",
    params(
        ("mount_id" = String, Path, description = "Mount snowflake"),
        ("upload_id" = String, Path, description = "Upload id"),
    ),
    responses(
        (status = 200, body = UploadStatusView),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "BUCKET_UNREACHABLE"),
    ),
    security(("session" = []))
)]
pub async fn status(
    State(state): State<AppState>,
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(u): Ext<UploadDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    headers: HeaderMap,
    Path((mount_id, upload_id)): Path<(String, String)>,
) -> MsgResult<Json<UploadStatusView>> {
    let mount = writer(
        &d,
        admin.as_deref(),
        &state,
        &headers,
        uid(&p),
        &mount_id,
        Need::Upload,
    )
    .await?;
    let up = load(&u, &mount, &upload_id, uid(&p)).await?;
    let parts = within(u.io.parts(&mount, &up.key, &up.s3_upload_id)).await?;
    Ok(Json(UploadStatusView {
        upload: view(&up),
        parts: parts
            .into_iter()
            .map(|p| PartView {
                number: p.number,
                size: p.size,
                etag: p.etag,
            })
            .collect(),
    }))
}

/// A presigned `PUT` for one part, valid for an hour. Counted against `byos.upload_parts_per_hour`.
#[utoipa::path(
    post,
    path = "/storage/mounts/{mount_id}/uploads/{upload_id}/parts/{part_number}",
    params(
        ("mount_id" = String, Path, description = "Mount snowflake"),
        ("upload_id" = String, Path, description = "Upload id"),
        ("part_number" = u32, Path, description = "Part number, from 1"),
    ),
    responses(
        (status = 200, body = PartUrlView),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "CORS_NOT_READY"),
        (status = 429, body = ErrorBody, description = "byos.upload_parts_per_hour reached"),
    ),
    security(("session" = []))
)]
pub async fn part_url(
    State(state): State<AppState>,
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(u): Ext<UploadDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    headers: HeaderMap,
    Path((mount_id, upload_id, number)): Path<(String, String, String)>,
) -> MsgResult<Json<PartUrlView>> {
    let mount = writer(
        &d,
        admin.as_deref(),
        &state,
        &headers,
        uid(&p),
        &mount_id,
        Need::Upload,
    )
    .await?;
    if mount.cors_ok_at.is_none() {
        return Err(cors_not_ready());
    }
    let up = load(&u, &mount, &upload_id, uid(&p)).await?;
    let n: u32 = number
        .parse()
        .ok()
        .filter(|n| *n >= 1)
        .ok_or_else(|| MsgError::from(ErrorCode::SrNotFound))?;
    if n > up.part_count {
        return Err(invalid(
            "part_number",
            "PART_OUT_OF_RANGE",
            "That part number is past the end of the file.",
        ));
    }
    let now = state.now_secs().saturating_mul(1000);
    let limit = match admin.as_deref() {
        Some(a) => a
            .service
            .setting_value_i64(sr_admin::settings::BYOS_UPLOAD_PARTS_PER_HOUR)
            .await
            .unwrap_or(2_000),
        None => 2_000,
    };
    let used = u
        .ledger
        .parts_since(uid(&p), now - 3_600_000)
        .await
        .map_err(ledger_failed)?;
    if i64::try_from(used.saturating_add(1)).unwrap_or(i64::MAX) > limit {
        return Err(ErrorCode::TooManyRequests.into());
    }
    let url =
        u.io.part_url(&mount, &up.key, &up.s3_upload_id, n, PART_URL_TTL)
            .map_err(|e| bucket_error(&e))?;
    u.ledger
        .note_parts(uid(&p), up.id, 1, now)
        .await
        .map_err(ledger_failed)?;
    Ok(Json(PartUrlView {
        part_number: n,
        size: expected_size(&up, n),
        url,
        expires_at: iso(now.saturating_add(PART_URL_TTL.as_secs() as i64 * 1000)),
    }))
}

/// Finish an upload: the parts must all be in the bucket with the declared sizes.
#[utoipa::path(
    post,
    path = "/storage/mounts/{mount_id}/uploads/{upload_id}/complete",
    params(
        ("mount_id" = String, Path, description = "Mount snowflake"),
        ("upload_id" = String, Path, description = "Upload id"),
    ),
    request_body = CompleteUploadBody,
    responses(
        (status = 200, body = CompletedUploadView),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "UPLOAD_INCOMPLETE or BUCKET_UNREACHABLE"),
    ),
    security(("session" = []))
)]
pub async fn complete(
    State(state): State<AppState>,
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(u): Ext<UploadDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    headers: HeaderMap,
    Path((mount_id, upload_id)): Path<(String, String)>,
    ApiJson(body): ApiJson<CompleteUploadBody>,
) -> MsgResult<Json<CompletedUploadView>> {
    let mount = writer(
        &d,
        admin.as_deref(),
        &state,
        &headers,
        uid(&p),
        &mount_id,
        Need::Upload,
    )
    .await?;
    let up = load(&u, &mount, &upload_id, uid(&p)).await?;
    let have = within(u.io.parts(&mount, &up.key, &up.s3_upload_id)).await?;
    let incomplete = |what: String| conflict("UPLOAD_INCOMPLETE", &what);
    let missing: Vec<u32> = (1..=up.part_count)
        .filter(|n| !have.iter().any(|h| h.number == *n))
        .collect();
    if !missing.is_empty() {
        return Err(incomplete(format!(
            "{} of {} parts have not arrived.",
            missing.len(),
            up.part_count
        )));
    }
    if have.len() != up.part_count as usize {
        return Err(incomplete("The bucket holds unexpected parts.".into()));
    }
    for h in &have {
        if h.size != expected_size(&up, h.number) {
            return Err(incomplete(format!(
                "Part {} has the wrong size; send it again.",
                h.number
            )));
        }
    }
    for c in &body.parts {
        if !have
            .iter()
            .any(|h| h.number == c.number && h.etag == c.etag)
        {
            return Err(incomplete(format!(
                "Part {} does not match what the bucket holds.",
                c.number
            )));
        }
    }
    let parts: Vec<(u32, String)> = have.iter().map(|h| (h.number, h.etag.clone())).collect();
    within_for(
        Duration::from_secs(60),
        u.io.complete(&mount, &up.key, &up.s3_upload_id, &parts),
    )
    .await?;
    let now = state.now_secs().saturating_mul(1000);
    u.ledger
        .finish(mount.id, up.id, UploadState::Completed, now)
        .await
        .map_err(ledger_failed)?;
    Ok(Json(CompletedUploadView {
        key: up.key,
        size: up.size,
    }))
}

/// Abandon an upload and free its parts at the bucket.
#[utoipa::path(
    delete,
    path = "/storage/mounts/{mount_id}/uploads/{upload_id}",
    params(
        ("mount_id" = String, Path, description = "Mount snowflake"),
        ("upload_id" = String, Path, description = "Upload id"),
    ),
    responses(
        (status = 204),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "BUCKET_UNREACHABLE"),
    ),
    security(("session" = []))
)]
pub async fn abort(
    State(state): State<AppState>,
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(u): Ext<UploadDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    headers: HeaderMap,
    Path((mount_id, upload_id)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let mount = writer(
        &d,
        admin.as_deref(),
        &state,
        &headers,
        uid(&p),
        &mount_id,
        Need::Upload,
    )
    .await?;
    let up = load(&u, &mount, &upload_id, uid(&p)).await?;
    match within(u.io.abort(&mount, &up.key, &up.s3_upload_id)).await {
        Ok(()) => {}
        // Already gone at the bucket (swept or aborted elsewhere): the ledger row still closes.
        Err(e) if is_missing(&e) => {}
        Err(e) => return Err(e),
    }
    let now = state.now_secs().saturating_mul(1000);
    u.ledger
        .finish(mount.id, up.id, UploadState::Aborted, now)
        .await
        .map_err(ledger_failed)?;
    Ok(StatusCode::NO_CONTENT)
}

fn is_missing(e: &MsgError) -> bool {
    matches!(e, MsgError::Api(a) if a.code == ErrorCode::SrNotFound)
}
