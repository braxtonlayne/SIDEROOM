//! The scoped listing, object metadata and presign routes (BYOS-SCP-002). Every route decides through
//! `sr_storage::scopes`; the server never streams a mount's bytes (a presign is local signing, no S3 round
//! trip). A staff read of a mount that is not theirs is written to the access log before anything returns.

use super::{ScopeDeps, UploadDeps, cors_not_ready, facts_failed, parse_row, refusal, uid};
use crate::auth::{SESSION_COOKIE, SessionOnly, parse_cookie};
use crate::routes::admin::AdminDeps;
use crate::routes::messages::error::{ApiJson, ApiQuery, MsgError, MsgResult, invalid};
use crate::routes::messages::model::iso;
use crate::routes::{Ext, MaybeExt};
use crate::{openapi::ErrorBody, state::AppState};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sr_admin::Caller;
use sr_auth::session::SessionToken;
use sr_core::{ErrorCode, PlatformPermissions};
use sr_storage::browse::PAGE;
use sr_storage::mounts::{Mount, MountStatus};
use sr_storage::path::safe_key;
use sr_storage::scopes::{
    Access, Refused, ResolveError, Scope, ScopeFacts, Viewer, check_range, presign_ttl_secs,
};
use sr_storage::uploads::{FOLDER_OP_CAP, MoveError, plan_move};
use sr_storage::{ObjectStorage, StorageError};
use sr_storage::{PresignMethod, mounts::Sealer};
use std::time::Duration;
use utoipa::ToSchema;

/// Query of `GET /storage/mounts/{mount_id}/objects`.
#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    /// Folder to list (mount-relative, empty for the top, `/` appended when missing).
    #[serde(default)]
    pub prefix: String,
    /// `/` (one level, the default) or empty (everything below the prefix).
    pub delimiter: Option<String>,
    /// The `next_cursor` of the previous page.
    pub cursor: Option<String>,
}

/// An object in a listing.
#[derive(Debug, Serialize, ToSchema)]
pub struct ObjectEntry {
    /// Mount-relative key.
    pub key: String,
    /// Size in bytes.
    pub size: u64,
    /// Last modified (ISO 8601), `null` when the service did not say.
    pub last_modified: Option<String>,
    /// Content type guessed from the extension.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

/// A page of a listing.
#[derive(Debug, Serialize, ToSchema)]
pub struct ObjectList {
    /// Sub-folder prefixes (mount-relative, ending in `/`).
    pub folders: Vec<String>,
    /// Objects the viewer may read.
    pub objects: Vec<ObjectEntry>,
    /// Cursor of the next page, `null` at the end.
    pub next_cursor: Option<String>,
    /// Whether the caller may upload and make folders here (`scopes::may_upload`); the UI shows the write
    /// side only when it is true. The write routes decide again.
    pub can_upload: bool,
    /// Whether the caller may rename, move and delete here (`scopes::may_manage`).
    pub can_manage: bool,
}

/// Metadata of one object.
#[derive(Debug, Serialize, ToSchema)]
pub struct ObjectMetaView {
    /// Mount-relative key.
    pub key: String,
    /// Size in bytes.
    pub size: u64,
    /// Last modified (ISO 8601).
    pub last_modified: Option<String>,
    /// Entity tag.
    pub etag: Option<String>,
    /// Content type guessed from the extension.
    pub content_type: Option<String>,
}

/// Body of `POST /storage/mounts/{mount_id}/presign`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PresignBody {
    /// Mount-relative key.
    pub key: String,
    /// `GET` (the only method; writes arrive with the upload routes).
    pub method: String,
    /// A request range the client will send, `bytes=a-b`; checked for shape.
    #[serde(default)]
    pub range: Option<String>,
}

/// A presigned URL.
#[derive(Debug, Serialize, ToSchema)]
pub struct PresignView {
    /// The URL; never stored or logged by the server.
    pub url: String,
    /// When it stops working (ISO 8601).
    pub expires_at: String,
}

fn content_type(key: &str) -> Option<String> {
    let ext = key.rsplit_once('.')?.1.to_ascii_lowercase();
    let t = match ext.as_str() {
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "m4a" | "aac" => "audio/mp4",
        "wav" => "audio/wav",
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "epub" => "application/epub+zip",
        "cbz" => "application/vnd.comicbook+zip",
        "vtt" => "text/vtt",
        "srt" => "application/x-subrip",
        "txt" => "text/plain",
        "json" => "application/json",
        _ => return None,
    };
    Some(t.to_owned())
}

/// Everything a handler needs after the scope decision.
pub(super) struct Reach {
    pub(super) mount: Mount,
    pub(super) access: Access,
    /// Set when the read is a staff read: the caller to log.
    pub(super) staff: Option<Caller>,
}

/// What the caller may do to the mount beyond reading it. A staff read is read-only; a failed fact is "no".
pub(super) async fn capabilities(d: &ScopeDeps, r: &Reach, user: i64) -> (bool, bool) {
    if r.staff.is_some() {
        return (false, false);
    }
    let facts: &dyn ScopeFacts = &*d.store;
    let viewer = Viewer {
        id: user,
        staff: false,
    };
    let up = sr_storage::scopes::may_upload(facts, viewer, &r.mount)
        .await
        .unwrap_or(false);
    let manage = sr_storage::scopes::may_manage(facts, viewer, &r.mount)
        .await
        .unwrap_or(false);
    (up, manage)
}

/// Look the mount up and decide how the caller reaches it. Refusals are 404 (403 for a missing grant).
pub(super) async fn reach(
    d: &ScopeDeps,
    admin: Option<&AdminDeps>,
    user: i64,
    ip: Option<String>,
    mount_id: &str,
) -> MsgResult<Reach> {
    let id = parse_row(mount_id)?;
    let mount = match d.mounts.by_id(id).await {
        Ok(Some(m)) => m,
        Ok(None) => return Err(ErrorCode::SrNotFound.into()),
        Err(e) => {
            tracing::error!(error = %e, "mount lookup failed");
            return Err(ErrorCode::SrInternal.into());
        }
    };
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
        Ok(access) => Ok(Reach {
            mount,
            access,
            staff: None,
        }),
        Err(ResolveError::Facts(e)) => Err(facts_failed(e)),
        Err(ResolveError::Refused(r)) => {
            // Platform staff with OPERATOR may look; the first reach is retried as staff and logged.
            let overridable = !matches!(r, Refused::Disabled | Refused::OwnerInactive);
            if overridable && let Some(caller) = operator(admin, user, ip).await {
                let access = sr_storage::scopes::access(
                    facts,
                    Viewer {
                        id: user,
                        staff: true,
                    },
                    &mount,
                )
                .await
                .map_err(|e| match e {
                    ResolveError::Refused(r) => refusal(r),
                    ResolveError::Facts(e) => facts_failed(e),
                })?;
                return Ok(Reach {
                    mount,
                    access,
                    staff: Some(caller),
                });
            }
            Err(refusal(r))
        }
    }
}

/// The caller as platform staff holding `OPERATOR`, if they are.
async fn operator(admin: Option<&AdminDeps>, user: i64, ip: Option<String>) -> Option<Caller> {
    let admin = admin?;
    let caller = admin.service.resolve(user, ip).await.ok()?;
    (caller.is_owner || caller.permissions.has(PlatformPermissions::OPERATOR)).then_some(caller)
}

/// Write the staff read to the access log. A failed write refuses the read (fail closed).
async fn log_staff(admin: Option<&AdminDeps>, r: &Reach, op: &str, key: &str) -> MsgResult<()> {
    let (Some(caller), Some(admin)) = (&r.staff, admin) else {
        return Ok(());
    };
    // `Scope::Staff` is the only scope that carries a caller.
    debug_assert_eq!(r.access.scope, Scope::Staff);
    admin
        .service
        .record_read(
            caller,
            "storage.read",
            Some(("storage_mount", r.mount.id)),
            json!({ "op": op, "key": key }),
        )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "staff storage read could not be logged");
            MsgError::from(ErrorCode::SrInternal)
        })
}

/// Per-key check: the kind rule of the scope, then delisting.
async fn key_allowed(d: &ScopeDeps, r: &Reach, key: &str) -> MsgResult<()> {
    r.access.permits(key).map_err(refusal)?;
    let gone = d
        .store
        .delisted(r.mount.id, &[key.to_owned()])
        .await
        .map_err(facts_failed)?;
    if gone.is_empty() {
        Ok(())
    } else {
        Err(refusal(Refused::Delisted))
    }
}

pub(super) fn client_ip(headers: &HeaderMap, state: &AppState) -> Option<String> {
    if state.settings().trust_forwarded_for
        && let Some(ip) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit(',').next())
            .map(str::trim)
            .filter(|s| !s.is_empty())
    {
        return Some(ip.to_owned());
    }
    None
}

pub(super) fn bucket_error(e: &StorageError) -> MsgError {
    match e {
        StorageError::NotFound(_) => ErrorCode::SrNotFound.into(),
        StorageError::InvalidPath(_) => invalid("key", "KEY_INVALID", "That key is not valid."),
        other => {
            tracing::warn!(error = %other, "mount bucket request failed");
            super::mounts::conflict(
                "BUCKET_UNREACHABLE",
                "The bucket did not answer. Check that it is online and reachable.",
            )
        }
    }
}

pub(super) fn key_error(field: &str, e: sr_storage::path::KeyError) -> MsgError {
    invalid(field, "KEY_INVALID", &e.to_string())
}

/// Re-check a member endpoint right before `object_store` calls it. That client resolves the name itself and
/// cannot be pinned, so this narrows the DNS-rebind window to the gap between the check and the connect (the
/// signed clients of the write side are pinned outright, see `sr_storage::guard`). The operator's own bucket
/// (the platform mount) is not vetted.
async fn vet_endpoint(d: &ScopeDeps, mount: &Mount) -> MsgResult<()> {
    let Some(g) = &d.guard else { return Ok(()) };
    if mount.owner == sr_storage::mounts::MountOwner::Platform {
        return Ok(());
    }
    g.validated_addrs(&mount.endpoint).await.map_err(|e| {
        tracing::warn!(mount = mount.id, error = %e, "mount endpoint refused before a bucket call");
        super::mounts::conflict(
            "MOUNT_ENDPOINT_BLOCKED",
            "This endpoint is not reachable from the public internet.",
        )
    })?;
    Ok(())
}

fn storage_for(d: &ScopeDeps, mount: &Mount) -> MsgResult<sr_storage::ObjectStoreStorage> {
    let s: &dyn Sealer = &*d.sealer;
    mount.storage(s).map_err(|e| {
        tracing::error!(error = %e, mount = mount.id, "mount client could not be built");
        MsgError::from(ErrorCode::SrInternal)
    })
}

/// List one folder of a mount.
#[utoipa::path(
    get,
    path = "/storage/mounts/{mount_id}/objects",
    params(
        ("mount_id" = String, Path, description = "Mount snowflake"),
        ("prefix" = Option<String>, Query, description = "Folder, mount-relative"),
        ("delimiter" = Option<String>, Query, description = "`/` (default) or empty"),
        ("cursor" = Option<String>, Query, description = "next_cursor of the previous page"),
    ),
    responses(
        (status = 200, body = ObjectList),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    headers: HeaderMap,
    Path(mount_id): Path<String>,
    ApiQuery(q): ApiQuery<ListQuery>,
) -> MsgResult<Json<ObjectList>> {
    let delimiter = match q.delimiter.as_deref() {
        None | Some("/") => true,
        Some("") => false,
        Some(_) => {
            return Err(invalid(
                "delimiter",
                "ENUM_TYPE_COERCE",
                "Use / or nothing.",
            ));
        }
    };
    let mut prefix = q.prefix;
    if !prefix.is_empty() {
        safe_key(&prefix, true).map_err(|e| key_error("prefix", e))?;
        if !prefix.ends_with('/') {
            prefix.push('/');
        }
    }
    let r = reach(
        &d,
        admin.as_deref(),
        uid(&p),
        client_ip(&headers, &state),
        &mount_id,
    )
    .await?;
    if r.mount.status == MountStatus::Disabled {
        return Err(ErrorCode::SrNotFound.into());
    }
    log_staff(admin.as_deref(), &r, "list", &prefix).await?;
    let base = r.mount.prefix.trim_matches('/').to_owned();
    let bucket_prefix = if prefix.is_empty() {
        if base.is_empty() {
            String::new()
        } else {
            format!("{base}/")
        }
    } else {
        r.mount.bucket_key(&prefix)
    };
    let strip = |k: &str| -> String {
        if base.is_empty() {
            k.to_owned()
        } else {
            k.strip_prefix(&format!("{base}/")).unwrap_or(k).to_owned()
        }
    };
    vet_endpoint(&d, &r.mount).await?;
    let storage = storage_for(&d, &r.mount)?;
    let page =
        within(storage.list_dir(&bucket_prefix, delimiter, q.cursor.as_deref(), PAGE)).await?;
    let rel: Vec<String> = page.objects.iter().map(|o| strip(&o.key)).collect();
    let gone = d
        .store
        .delisted(r.mount.id, &rel)
        .await
        .map_err(facts_failed)?;
    let mut objects = Vec::new();
    for (o, key) in page.objects.iter().zip(rel) {
        if gone.contains(&key) || r.access.permits(&key).is_err() {
            continue;
        }
        objects.push(ObjectEntry {
            content_type: content_type(&key),
            last_modified: Some(iso(o.last_modified_ms)),
            size: o.size,
            key,
        });
    }
    let (can_upload, can_manage) = capabilities(&d, &r, uid(&p)).await;
    Ok(Json(ObjectList {
        folders: page.folders.iter().map(|f| strip(f)).collect(),
        objects,
        next_cursor: page.next_cursor,
        can_upload,
        can_manage,
    }))
}

/// Metadata of one object (size, modification time, etag), without its bytes.
#[utoipa::path(
    get,
    path = "/storage/mounts/{mount_id}/objects/{key}",
    params(
        ("mount_id" = String, Path, description = "Mount snowflake"),
        ("key" = String, Path, description = "Mount-relative key"),
    ),
    responses(
        (status = 200, body = ObjectMetaView),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn meta(
    State(state): State<AppState>,
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    headers: HeaderMap,
    Path((mount_id, key)): Path<(String, String)>,
) -> MsgResult<Json<ObjectMetaView>> {
    safe_key(&key, false).map_err(|e| key_error("key", e))?;
    let r = reach(
        &d,
        admin.as_deref(),
        uid(&p),
        client_ip(&headers, &state),
        &mount_id,
    )
    .await?;
    key_allowed(&d, &r, &key).await?;
    log_staff(admin.as_deref(), &r, "meta", &key).await?;
    vet_endpoint(&d, &r.mount).await?;
    let storage = storage_for(&d, &r.mount)?;
    let m = within(storage.head_meta(&r.mount.bucket_key(&key))).await?;
    Ok(Json(ObjectMetaView {
        content_type: content_type(&key),
        last_modified: Some(iso(m.last_modified_ms)),
        size: m.size,
        etag: m.etag,
        key,
    }))
}

/// A presigned `GET` for one object. The URL lives `byos.presign_ttl_secs` (never past the cap, the hard
/// ceiling of a day, or the caller's session). A mount that has not passed its CORS check answers 409
/// `CORS_NOT_READY`.
#[utoipa::path(
    post,
    path = "/storage/mounts/{mount_id}/presign",
    params(("mount_id" = String, Path, description = "Mount snowflake")),
    request_body = PresignBody,
    responses(
        (status = 200, body = PresignView),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "CORS_NOT_READY"),
    ),
    security(("session" = []))
)]
pub async fn presign(
    State(state): State<AppState>,
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    headers: HeaderMap,
    Path(mount_id): Path<String>,
    ApiJson(body): ApiJson<PresignBody>,
) -> MsgResult<Json<PresignView>> {
    if !body.method.eq_ignore_ascii_case("GET") {
        return Err(invalid(
            "method",
            "ENUM_TYPE_COERCE",
            "Only GET can be presigned.",
        ));
    }
    safe_key(&body.key, false).map_err(|e| key_error("key", e))?;
    if let Some(range) = &body.range
        && !check_range(range)
    {
        return Err(invalid("range", "RANGE_INVALID", "Use bytes=start-end."));
    }
    let r = reach(
        &d,
        admin.as_deref(),
        uid(&p),
        client_ip(&headers, &state),
        &mount_id,
    )
    .await?;
    key_allowed(&d, &r, &body.key).await?;
    if r.mount.cors_ok_at.is_none() {
        return Err(cors_not_ready());
    }
    // Lifetime: the settings, the cap, the hard ceiling, and what the session has left.
    let (ttl, cap) = match admin.as_deref() {
        Some(a) => (
            a.service
                .setting_value_i64(sr_admin::settings::BYOS_PRESIGN_TTL)
                .await
                .unwrap_or(900),
            a.service
                .setting_value_i64(sr_admin::settings::BYOS_PRESIGN_TTL_CAP)
                .await
                .unwrap_or(3600),
        ),
        None => (900, 3600),
    };
    let left = session_left(&state, &headers).await;
    let Some(secs) = presign_ttl_secs(
        u64::try_from(ttl).unwrap_or(900),
        u64::try_from(cap).unwrap_or(3600),
        left,
    ) else {
        // The session is about to end: sign in again.
        return Err(ErrorCode::Unauthorized.into());
    };
    let storage = storage_for(&d, &r.mount)?;
    let url = storage
        .presign(
            PresignMethod::Get,
            &r.mount.bucket_key(&body.key),
            Duration::from_secs(secs),
        )
        .await
        .map_err(|e| bucket_error(&e))?;
    log_staff(admin.as_deref(), &r, "presign", &body.key).await?;
    let expires_ms = state
        .now_secs()
        .saturating_add(secs as i64)
        .saturating_mul(1000);
    Ok(Json(PresignView {
        url,
        expires_at: iso(expires_ms),
    }))
}

/// Seconds the caller's session has left; `None` when it cannot be told (then the other caps apply).
pub(super) async fn session_left(state: &AppState, headers: &HeaderMap) -> Option<i64> {
    let raw = parse_cookie(headers, SESSION_COOKIE)?;
    let token = SessionToken::parse(raw)?;
    let rec = state.sessions().find(&token.hash()).await.ok()??;
    Some(rec.expires_at - state.now_secs())
}

/// How long a request waits for a member's bucket: `object_store` retries a dead endpoint for minutes, and a
/// hung bucket must not hold a request that long.
const BUCKET_WAIT: Duration = Duration::from_secs(8);

/// Run a bucket call under [`BUCKET_WAIT`]; a timeout is `BUCKET_UNREACHABLE` like any other bucket failure.
pub(super) async fn within<T>(
    fut: impl std::future::Future<Output = Result<T, StorageError>>,
) -> MsgResult<T> {
    match tokio::time::timeout(BUCKET_WAIT, fut).await {
        Ok(r) => r.map_err(|e| bucket_error(&e)),
        Err(_) => Err(bucket_error(&StorageError::Backend("timed out".into()))),
    }
}

// ------------------------------------------------------------------ write side (BYOS-BRW-001)

/// Like [`within`] with a longer wait (completing an upload, copying objects).
pub(super) async fn within_for<T>(
    wait: Duration,
    fut: impl std::future::Future<Output = Result<T, StorageError>>,
) -> MsgResult<T> {
    match tokio::time::timeout(wait, fut).await {
        Ok(r) => r.map_err(|e| bucket_error(&e)),
        Err(_) => Err(bucket_error(&StorageError::Backend("timed out".into()))),
    }
}

/// What a write route asks of the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Need {
    /// `scopes::may_upload`: uploads and new folders.
    Upload,
    /// `scopes::may_manage`: rename, move, delete.
    Manage,
}

/// Find the mount and require the right to write to it. A caller who cannot reach the mount at all gets the
/// read side's 404 (403 for a missing grant); one who reaches it (a friend with a share, a server member) but
/// may not write gets 403. Platform staff reach a mount to look, never to write.
pub(super) async fn writer(
    d: &ScopeDeps,
    admin: Option<&AdminDeps>,
    state: &AppState,
    headers: &HeaderMap,
    user: i64,
    mount_id: &str,
    need: Need,
) -> MsgResult<Mount> {
    let r = reach(d, admin, user, client_ip(headers, state), mount_id).await?;
    if r.staff.is_some() {
        return Err(ErrorCode::MissingPermissions.into());
    }
    let facts: &dyn ScopeFacts = &*d.store;
    let viewer = Viewer {
        id: user,
        staff: false,
    };
    let ok = match need {
        Need::Upload => sr_storage::scopes::may_upload(facts, viewer, &r.mount).await,
        Need::Manage => sr_storage::scopes::may_manage(facts, viewer, &r.mount).await,
    }
    .map_err(facts_failed)?;
    if ok {
        Ok(r.mount)
    } else {
        Err(ErrorCode::MissingPermissions.into())
    }
}

/// Body of `POST /storage/mounts/{mount_id}/folders`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct FolderBody {
    /// Mount-relative folder key, ending in `/`.
    pub key: String,
}

/// A created folder.
#[derive(Debug, Serialize, ToSchema)]
pub struct FolderView {
    /// Mount-relative folder key, ending in `/`.
    pub key: String,
}

/// Body of `POST /storage/mounts/{mount_id}/move` (rename and move are the same call).
#[derive(Debug, Deserialize, ToSchema)]
pub struct MoveBody {
    /// Mount-relative key of an object, or of a folder (ending in `/`).
    pub from: String,
    /// The new key: an object stays an object, a folder stays a folder.
    pub to: String,
}

/// What a move or delete touched.
#[derive(Debug, Serialize, ToSchema)]
pub struct CountView {
    /// Objects moved or deleted (folder markers included).
    pub count: u32,
}

/// Create an empty folder: a zero-byte `folder/` marker, which is how S3 clients show an empty folder.
#[utoipa::path(
    post,
    path = "/storage/mounts/{mount_id}/folders",
    params(("mount_id" = String, Path, description = "Mount snowflake")),
    request_body = FolderBody,
    responses(
        (status = 201, body = FolderView),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "BUCKET_UNREACHABLE"),
    ),
    security(("session" = []))
)]
#[allow(clippy::too_many_arguments)]
pub async fn create_folder(
    State(state): State<AppState>,
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(u): Ext<UploadDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    headers: HeaderMap,
    Path(mount_id): Path<String>,
    ApiJson(body): ApiJson<FolderBody>,
) -> MsgResult<(StatusCode, Json<FolderView>)> {
    if !body.key.ends_with('/') {
        return Err(invalid(
            "key",
            "KEY_INVALID",
            "A folder key ends in a slash.",
        ));
    }
    safe_key(&body.key, true).map_err(|e| key_error("key", e))?;
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
    within(u.io.put_marker(&mount, &body.key)).await?;
    Ok((StatusCode::CREATED, Json(FolderView { key: body.key })))
}

fn move_refusal(e: MoveError) -> MsgError {
    match e {
        MoveError::Same => invalid("to", "MOVE_SAME", "That is where it already is."),
        MoveError::IntoItself => invalid(
            "to",
            "MOVE_INTO_ITSELF",
            "A folder cannot move into itself.",
        ),
        MoveError::KindMismatch => invalid(
            "to",
            "KIND_MISMATCH",
            "A folder moves to a folder and a file to a file.",
        ),
        MoveError::TooMany => invalid(
            "from",
            "FOLDER_TOO_LARGE",
            "That folder has more than 500 entries; move it in smaller pieces or at the provider.",
        ),
        MoveError::TooLarge => super::mounts::conflict(
            "OBJECT_TOO_LARGE",
            "A file over 5 GiB cannot be moved here; move it at the provider.",
        ),
        MoveError::Missing => ErrorCode::SrNotFound.into(),
    }
}

/// Rename or move a file or a folder inside the bucket: a server-side copy of each object, then the
/// delete of the originals (nothing is deleted when a copy fails). Refuses a target that already exists,
/// a file over 5 GiB, and a folder with more than 500 entries.
#[utoipa::path(
    post,
    path = "/storage/mounts/{mount_id}/move",
    params(("mount_id" = String, Path, description = "Mount snowflake")),
    request_body = MoveBody,
    responses(
        (status = 200, body = CountView),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "TARGET_EXISTS, OBJECT_TOO_LARGE or BUCKET_UNREACHABLE"),
    ),
    security(("session" = []))
)]
#[allow(clippy::too_many_arguments)]
pub async fn move_objects(
    State(state): State<AppState>,
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(u): Ext<UploadDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    headers: HeaderMap,
    Path(mount_id): Path<String>,
    ApiJson(body): ApiJson<MoveBody>,
) -> MsgResult<Json<CountView>> {
    safe_key(&body.from, true).map_err(|e| key_error("from", e))?;
    safe_key(&body.to, true).map_err(|e| key_error("to", e))?;
    let mount = writer(
        &d,
        admin.as_deref(),
        &state,
        &headers,
        uid(&p),
        &mount_id,
        Need::Manage,
    )
    .await?;
    let folder = body.from.ends_with('/');
    // For a file the first key under the name is the file itself (it sorts before `name.bak`).
    let cap = if folder { FOLDER_OP_CAP } else { 1 };
    let found = within(u.io.list_under(&mount, &body.from, cap)).await?;
    let plan = plan_move(&body.from, &body.to, &found).map_err(move_refusal)?;
    let taken = within(u.io.list_under(&mount, &body.to, 1)).await?;
    let clash = if folder {
        !taken.is_empty()
    } else {
        taken.first().is_some_and(|(k, _)| *k == body.to)
    };
    if clash {
        return Err(super::mounts::conflict(
            "TARGET_EXISTS",
            "Something with that name is already there.",
        ));
    }
    let copy_all = async {
        for (from, to, _) in &plan {
            if from.ends_with('/') {
                u.io.put_marker(&mount, to).await?;
            } else {
                u.io.copy(&mount, from, to).await?;
            }
        }
        Ok::<(), StorageError>(())
    };
    within_for(Duration::from_secs(40), copy_all).await?;
    let delete_all = async {
        for (from, _, _) in &plan {
            u.io.delete(&mount, from).await?;
        }
        Ok::<(), StorageError>(())
    };
    within_for(Duration::from_secs(40), delete_all).await?;
    Ok(Json(CountView {
        count: u32::try_from(plan.len()).unwrap_or(u32::MAX),
    }))
}

/// Delete a file, or a folder with everything in it (a key ending in `/`; at most 500 entries). The bucket
/// is the only copy of the bytes: there is no undo.
#[utoipa::path(
    delete,
    path = "/storage/mounts/{mount_id}/objects/{key}",
    params(
        ("mount_id" = String, Path, description = "Mount snowflake"),
        ("key" = String, Path, description = "Mount-relative key; a trailing slash deletes a folder"),
    ),
    responses(
        (status = 200, body = CountView),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "FOLDER_TOO_LARGE or BUCKET_UNREACHABLE"),
    ),
    security(("session" = []))
)]
pub async fn delete_objects(
    State(state): State<AppState>,
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(u): Ext<UploadDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    headers: HeaderMap,
    Path((mount_id, key)): Path<(String, String)>,
) -> MsgResult<Json<CountView>> {
    safe_key(&key, true).map_err(|e| key_error("key", e))?;
    let mount = writer(
        &d,
        admin.as_deref(),
        &state,
        &headers,
        uid(&p),
        &mount_id,
        Need::Manage,
    )
    .await?;
    let folder = key.ends_with('/');
    let cap = if folder { FOLDER_OP_CAP } else { 1 };
    let found = within(u.io.list_under(&mount, &key, cap)).await?;
    let doomed: Vec<String> = if folder {
        if found.len() > FOLDER_OP_CAP {
            return Err(super::mounts::conflict(
                "FOLDER_TOO_LARGE",
                "That folder has more than 500 entries; delete it in smaller pieces or at the provider.",
            ));
        }
        found.into_iter().map(|(k, _)| k).collect()
    } else {
        found
            .into_iter()
            .map(|(k, _)| k)
            .filter(|k| *k == key)
            .collect()
    };
    if doomed.is_empty() {
        return Err(ErrorCode::SrNotFound.into());
    }
    let delete_all = async {
        for k in &doomed {
            u.io.delete(&mount, k).await?;
        }
        Ok::<(), StorageError>(())
    };
    within_for(Duration::from_secs(40), delete_all).await?;
    Ok(Json(CountView {
        count: u32::try_from(doomed.len()).unwrap_or(u32::MAX),
    }))
}
