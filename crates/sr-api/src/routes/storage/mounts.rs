//! Storage mounts: the member's and the server's own buckets (BYOS-MNT-001).
//!
//! * `GET|POST /users/@me/storage/mounts`, `PATCH|DELETE /users/@me/storage/mounts/{mount_id}`
//! * the same under `/guilds/{guild_id}/storage/mounts` (the caller must manage the server)
//!
//! A mount that is not the caller's (or the server's) is 404, never 403, so ids leak nothing. A server
//! route needs membership (403 otherwise) and `MANAGE_GUILD` (owner and `ADMINISTRATOR` bypass included; the
//! storage-grants table of BYOS-SCP-001 adds the `manage` grant on top). No response carries the secret access
//! key: the view has the access key id and a four-character hint of it. Sessions only: bots and OAuth bearer
//! tokens get 403.
//!
//! Errors (409, the field error code is the stable one clients read at `errors.mount._errors[0].code`):
//! `MOUNT_CAP_REACHED`, `MOUNT_LABEL_TAKEN`.

use std::sync::Arc;

use axum::{Json, extract::Path, http::StatusCode};
use serde::{Deserialize, Serialize};
use sr_core::permissions::Permissions;
use sr_core::{ApiError, ErrorCode, GuildId, UserId};
use sr_storage::mount_cache::CachedMountResolver;
use sr_storage::mounts::{
    Mount, MountError, MountInput, MountOwner, MountPatch, MountRepo, MountResolver, MountStatus,
    NewMount, Provider, SealedSecret, Sealer, Secret,
};
use utoipa::{OpenApi, ToSchema};

use crate::routes::admin::AdminDeps;
use crate::routes::developer::iso;
use crate::routes::messages::error::{ApiJson, MsgError, MsgResult, invalid, parse_id};
use crate::routes::settings::deps::SettingsDeps;
use crate::routes::{Ext, MaybeExt};
use crate::{AuthKind, Principal, openapi::ErrorBody, state::AppState};

/// Default most mounts one member may keep (`byos.max_mounts_per_user`).
pub const DEFAULT_MAX_MOUNTS_PER_USER: u32 = 3;
/// Default most mounts one server may keep (`byos.max_mounts_per_guild`).
pub const DEFAULT_MAX_MOUNTS_PER_GUILD: u32 = 5;

/// Collaborators of the mount routes; registered on the composition root.
pub struct MountsDeps {
    /// The mount rows.
    pub repo: Arc<dyn MountRepo>,
    /// Reads through a 60 s cache; every write here invalidates it.
    pub resolver: Arc<CachedMountResolver>,
    /// Seals secrets under `server.secret_key`.
    pub sealer: Arc<dyn Sealer>,
    /// Snowflake source.
    pub next_id: Arc<dyn Fn() -> u64 + Send + Sync>,
    /// The SSRF policy an endpoint is checked against.
    pub net_policy: sr_net::Policy,
    /// `byos.max_mounts_per_user`.
    pub max_per_user: u32,
    /// `byos.max_mounts_per_guild`.
    pub max_per_guild: u32,
    /// The origin browsers read mounts from (`server.public_url`); the CORS rule names exactly this.
    pub public_origin: String,
    /// How long the connection test may run (20 s).
    pub test_budget: std::time::Duration,
    /// The storage-scope facts: who may manage a server's mounts (`sr_storage::scopes::may_manage_guild`,
    /// so a role with the `manage` grant counts). Unset only in unit tests, which fall back to `MANAGE_GUILD`.
    pub facts: Option<Arc<dyn sr_storage::scopes::ScopeFacts>>,
}

impl MountsDeps {
    /// Set the origin the CORS rule allows (`server.public_url`).
    pub fn with_public_origin(mut self, origin: &str) -> Self {
        self.public_origin = origin.trim_end_matches('/').to_owned();
        self
    }

    /// Decide server management through the storage grants.
    pub fn with_facts(mut self, facts: Arc<dyn sr_storage::scopes::ScopeFacts>) -> Self {
        self.facts = Some(facts);
        self
    }

    /// Share the composition root's cached resolver, so the routes' invalidations reach every reader.
    pub fn with_resolver(mut self, resolver: Arc<CachedMountResolver>) -> Self {
        self.resolver = resolver;
        self
    }

    /// Wire the repo with the default caps.
    pub fn new(
        repo: Arc<dyn MountRepo>,
        sealer: Arc<dyn Sealer>,
        next_id: Arc<dyn Fn() -> u64 + Send + Sync>,
        net_policy: sr_net::Policy,
    ) -> Self {
        let resolver = Arc::new(CachedMountResolver::new(repo_as_resolver(&repo)));
        Self {
            repo,
            resolver,
            sealer,
            next_id,
            net_policy,
            max_per_user: DEFAULT_MAX_MOUNTS_PER_USER,
            max_per_guild: DEFAULT_MAX_MOUNTS_PER_GUILD,
            public_origin: String::new(),
            test_budget: sr_storage::probe::PROBE_BUDGET,
            facts: None,
        }
    }
}

/// A `MountRepo` as the `MountResolver` the cache wraps (trait upcasting).
fn repo_as_resolver(repo: &Arc<dyn MountRepo>) -> Arc<dyn MountResolver> {
    repo.clone()
}

/// The routes.
pub fn router() -> axum::Router<AppState> {
    use axum::routing::{get, patch};
    axum::Router::new()
        .route(
            "/users/@me/storage/mounts",
            get(list_user).post(create_user),
        )
        .route(
            "/users/@me/storage/mounts/{mount_id}",
            patch(patch_user).delete(delete_user),
        )
        .route(
            "/guilds/{guild_id}/storage/mounts",
            get(list_guild).post(create_guild),
        )
        .route(
            "/guilds/{guild_id}/storage/mounts/{mount_id}",
            patch(patch_guild).delete(delete_guild),
        )
}

// ------------------------------------------------------------------ wire shapes

/// A storage mount. The secret access key is never returned.
#[derive(Debug, Serialize, ToSchema)]
pub struct MountObject {
    /// Snowflake.
    pub id: String,
    /// `user`, `guild` or `platform`.
    pub owner_kind: String,
    /// The owner's snowflake; `null` for the platform.
    pub owner_id: Option<String>,
    pub label: String,
    /// `s3`, `r2`, `b2`, `minio`, `wasabi` or `garage`.
    pub provider: String,
    pub endpoint: Option<String>,
    pub region: Option<String>,
    pub bucket: String,
    pub prefix: String,
    pub path_style: bool,
    pub access_key_id: String,
    /// The last four characters of the access key id.
    pub key_hint: String,
    /// `active`, `failing` or `disabled`.
    pub status: String,
    /// When the CORS check last passed (ISO 8601); presigning is refused until it has.
    pub cors_ok_at: Option<String>,
    pub last_test_at: Option<String>,
    /// The last connection test: `{ at, rows: [{ id, status, message }] }`.
    #[schema(value_type = Option<Object>)]
    pub last_test: Option<serde_json::Value>,
    pub created_at: String,
}

impl MountObject {
    /// The wire view of a mount.
    pub fn of(m: &Mount) -> Self {
        Self {
            id: m.id.to_string(),
            owner_kind: m.owner.kind().to_owned(),
            owner_id: match m.owner {
                MountOwner::Platform => None,
                o => Some(o.id().to_string()),
            },
            label: m.label.clone(),
            provider: m.provider.as_str().to_owned(),
            endpoint: Some(m.endpoint.clone()),
            region: Some(m.region.clone()),
            bucket: m.bucket.clone(),
            prefix: m.prefix.clone(),
            path_style: m.path_style,
            access_key_id: m.access_key_id.clone(),
            key_hint: m.key_hint.clone(),
            status: m.status.as_str().to_owned(),
            cors_ok_at: m.cors_ok_at.map(iso),
            last_test_at: m.last_test_at.map(iso),
            last_test: m
                .last_test
                .as_deref()
                .and_then(|t| serde_json::from_str(t).ok()),
            created_at: iso(m.created_at),
        }
    }
}

/// Connect a bucket. `secret_access_key` is write-only.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateMountBody {
    pub label: String,
    pub provider: String,
    /// Required except for `s3` (default `https://s3.<region>.amazonaws.com`).
    pub endpoint: Option<String>,
    /// Default `us-east-1` for `s3`, `auto` otherwise.
    pub region: Option<String>,
    pub bucket: String,
    pub prefix: Option<String>,
    pub path_style: bool,
    pub access_key_id: String,
    pub secret_access_key: String,
}

/// Change a mount: any create field, plus `status` (`active` or `disabled`). A new `secret_access_key`
/// re-seals the credential; a new `access_key_id` needs one with it.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct PatchMountBody {
    pub label: Option<String>,
    pub provider: Option<String>,
    pub endpoint: Option<String>,
    pub region: Option<String>,
    pub bucket: Option<String>,
    pub prefix: Option<String>,
    pub path_style: Option<bool>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub status: Option<String>,
}

// ------------------------------------------------------------------ plumbing

/// A 409 whose stable string code is the field error under `errors.mount`.
pub(crate) fn conflict(code: &str, message: &str) -> MsgError {
    MsgError::Api(
        ApiError::with_message(ErrorCode::SrConflict, message).field("mount", code, message),
    )
}

pub(crate) fn not_found() -> MsgError {
    MsgError::Api(ApiError::with_message(
        ErrorCode::SrNotFound,
        "Unknown storage mount",
    ))
}

pub(crate) fn repo_failure(e: MountError) -> MsgError {
    match e {
        MountError::CapReached(n) => conflict(
            "MOUNT_CAP_REACHED",
            &format!("You can connect at most {n} buckets here."),
        ),
        MountError::LabelTaken => conflict("MOUNT_LABEL_TAKEN", "A bucket with this label exists."),
        MountError::IdTaken => {
            tracing::error!("storage mount id collision");
            ErrorCode::SrInternal.into()
        }
        other => {
            tracing::error!(error = %other, "storage mounts failure");
            ErrorCode::SrInternal.into()
        }
    }
}

/// The signed-in member. Bots and OAuth bearers have no storage of their own.
fn member(p: &Principal) -> MsgResult<UserId> {
    match p.kind {
        AuthKind::Session { .. } => Ok(UserId::new(u64::try_from(p.user_id).unwrap_or(0))),
        _ => Err(ErrorCode::MissingAccess.into()),
    }
}

/// Whose mounts a request touches, after the caller was checked.
pub(crate) enum Scope {
    User(i64),
    Guild(i64),
}

impl Scope {
    fn owner(&self) -> MountOwner {
        match self {
            Self::User(id) => MountOwner::User(*id),
            Self::Guild(id) => MountOwner::Guild(*id),
        }
    }
}

fn raw(id: UserId) -> i64 {
    i64::try_from(id.get()).unwrap_or(0)
}

/// The caller's own mounts.
pub(crate) fn user_scope(p: &Principal) -> MsgResult<Scope> {
    Ok(Scope::User(raw(member(p)?)))
}

/// The caller must be a member of the server who may manage it: 404 unknown server, 403 otherwise.
pub(crate) async fn guild_scope(
    d: &MountsDeps,
    s: &SettingsDeps,
    p: &Principal,
    guild: &str,
) -> MsgResult<Scope> {
    let user = member(p)?;
    let gid: GuildId = parse_id(guild, ErrorCode::UnknownGuild)?;
    let access = s.guild_access(user, gid).await?;
    let raw_gid = i64::try_from(gid.get()).unwrap_or(0);
    let manages = match &d.facts {
        Some(facts) => sr_storage::scopes::may_manage_guild(
            facts.as_ref(),
            sr_storage::scopes::Viewer {
                id: raw(user),
                staff: false,
            },
            raw_gid,
        )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "storage scope facts failed");
            MsgError::from(ErrorCode::SrInternal)
        })?,
        None => access.perms.has(Permissions::MANAGE_GUILD),
    };
    if !manages {
        return Err(ErrorCode::MissingPermissions.into());
    }
    Ok(Scope::Guild(i64::try_from(gid.get()).unwrap_or(0)))
}

/// The live mount `raw_id` if the scope owns it, else 404.
pub(crate) async fn owned_mount(
    deps: &MountsDeps,
    scope: &Scope,
    raw_id: &str,
) -> MsgResult<Mount> {
    let id: i64 = raw_id
        .parse()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(not_found)?;
    match deps.repo.by_id(id).await.map_err(repo_failure)? {
        Some(m) if m.owner == scope.owner() => Ok(m),
        _ => Err(not_found()),
    }
}

// ------------------------------------------------------------------ validation

const LABEL_MAX: usize = 64;

fn trimmed(field: &str, v: &str, max: usize, min: usize) -> MsgResult<String> {
    let t = v.trim();
    let n = t.chars().count();
    if n < min || n > max || t.chars().any(char::is_control) {
        return Err(invalid(
            field,
            "BASE_TYPE_BAD_LENGTH",
            &format!("Must be between {min} and {max} characters."),
        ));
    }
    Ok(t.to_owned())
}

fn parse_provider(v: &str) -> MsgResult<Provider> {
    Provider::parse(v.trim()).ok_or_else(|| {
        invalid(
            "provider",
            "ENUM_TYPE_INVALID",
            "Provider must be s3, r2, b2, minio, wasabi or garage.",
        )
    })
}

fn bucket_name(v: &str) -> MsgResult<String> {
    let b = trimmed("bucket", v, 255, 1)?;
    if !b
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'_'))
    {
        return Err(invalid(
            "bucket",
            "BUCKET_NAME_INVALID",
            "Bucket names use letters, digits, dots, dashes and underscores.",
        ));
    }
    Ok(b)
}

/// A key prefix with no leading slash, no `..`, no empty segment.
fn key_prefix(v: &str) -> MsgResult<String> {
    let p = v.trim().trim_start_matches('/');
    let bad = p.chars().any(char::is_control)
        || p.chars().count() > 512
        || p.split('/')
            .enumerate()
            .any(|(i, s)| s == ".." || s == "." || (s.is_empty() && i + 1 != p.split('/').count()));
    if bad {
        return Err(invalid(
            "prefix",
            "PREFIX_INVALID",
            "The prefix must be a plain folder path.",
        ));
    }
    Ok(p.to_owned())
}

fn default_region(provider: Provider) -> &'static str {
    if provider == Provider::S3 {
        "us-east-1"
    } else {
        "auto"
    }
}

/// Resolve the endpoint (default for S3) and check it against the SSRF policy.
async fn endpoint_for(
    deps: &MountsDeps,
    provider: Provider,
    region: &str,
    given: Option<&str>,
) -> MsgResult<String> {
    let raw = match given.map(str::trim).filter(|e| !e.is_empty()) {
        Some(e) => e.to_owned(),
        None if provider == Provider::S3 => format!("https://s3.{region}.amazonaws.com"),
        None => {
            return Err(invalid(
                "endpoint",
                "ENDPOINT_REQUIRED",
                "This provider needs the S3 endpoint URL.",
            ));
        }
    };
    let bad = |m: &str| invalid("endpoint", "ENDPOINT_INVALID", m);
    let url = url::Url::parse(&raw).map_err(|_| bad("Not a valid URL."))?;
    if !(url.path() == "/" || url.path().is_empty())
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(bad("Use the bare endpoint URL, without a path or query."));
    }
    let private_ok = deps.net_policy.allow_private_hosts || deps.net_policy.allow_private_for_tests;
    if url.scheme() == "http" && !private_ok {
        return Err(bad("The endpoint must use https."));
    }
    sr_net::resolve_validated(&deps.net_policy, &sr_net::SystemResolver, &url)
        .await
        .map_err(|_| bad("This endpoint is not reachable from the public internet."))?;
    Ok(raw.trim_end_matches('/').to_owned())
}

fn key_id(v: &str) -> MsgResult<String> {
    let k = trimmed("access_key_id", v, 256, 1)?;
    if k.chars().any(char::is_whitespace) {
        return Err(invalid(
            "access_key_id",
            "KEY_ID_INVALID",
            "The access key id has no spaces.",
        ));
    }
    Ok(k)
}

fn secret(v: &str) -> MsgResult<Secret> {
    let t = v.trim();
    if t.is_empty() || t.chars().count() > 512 || t.chars().any(char::is_control) {
        return Err(invalid(
            "secret_access_key",
            "SECRET_INVALID",
            "Enter the secret access key.",
        ));
    }
    Ok(Secret::new(t))
}

// ------------------------------------------------------------------ shared handlers

async fn list(deps: &MountsDeps, scope: Scope) -> MsgResult<Json<Vec<MountObject>>> {
    let rows = deps
        .repo
        .for_owner(scope.owner())
        .await
        .map_err(repo_failure)?;
    Ok(Json(rows.iter().map(MountObject::of).collect()))
}

/// The cap for a new mount: the setting when the admin service is wired, else the deps' default.
async fn cap_for(deps: &MountsDeps, admin: Option<&AdminDeps>, scope: Scope) -> u32 {
    let (key, default) = match scope {
        Scope::User(_) => (
            sr_admin::settings::BYOS_MAX_MOUNTS_PER_USER,
            deps.max_per_user,
        ),
        Scope::Guild(_) => (
            sr_admin::settings::BYOS_MAX_MOUNTS_PER_GUILD,
            deps.max_per_guild,
        ),
    };
    match admin {
        Some(a) => match a.service.setting_value_i64(key).await {
            Ok(v) => u32::try_from(v).unwrap_or(default),
            Err(_) => default,
        },
        None => default,
    }
}

async fn create(
    deps: &MountsDeps,
    admin: Option<&AdminDeps>,
    scope: Scope,
    body: CreateMountBody,
) -> MsgResult<(StatusCode, Json<MountObject>)> {
    let provider = parse_provider(&body.provider)?;
    let region = match body
        .region
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
    {
        Some(r) => trimmed("region", r, 64, 1)?,
        None => default_region(provider).to_owned(),
    };
    let input = MountInput {
        owner: scope.owner(),
        label: trimmed("label", &body.label, LABEL_MAX, 1)?,
        provider,
        endpoint: endpoint_for(deps, provider, &region, body.endpoint.as_deref()).await?,
        region,
        bucket: bucket_name(&body.bucket)?,
        prefix: key_prefix(body.prefix.as_deref().unwrap_or(""))?,
        path_style: body.path_style,
        access_key_id: key_id(&body.access_key_id)?,
    };
    let secret = secret(&body.secret_access_key)?;
    let id = i64::try_from((deps.next_id)()).unwrap_or(0);
    let new = NewMount::seal(deps.sealer.as_ref(), id, input, &secret).map_err(repo_failure)?;
    let cap = cap_for(deps, admin, scope).await;
    let mount = deps
        .repo
        .create(new, Some(cap))
        .await
        .map_err(repo_failure)?;
    Ok((StatusCode::CREATED, Json(MountObject::of(&mount))))
}

async fn patch(
    deps: &MountsDeps,
    scope: Scope,
    mount_id: &str,
    body: PatchMountBody,
) -> MsgResult<Json<MountObject>> {
    let current = owned_mount(deps, &scope, mount_id).await?;
    let mut patch = MountPatch::default();
    let mut connection_changed = false;

    if let Some(l) = &body.label {
        patch.label = Some(trimmed("label", l, LABEL_MAX, 1)?);
    }
    let provider = match &body.provider {
        Some(p) => parse_provider(p)?,
        None => current.provider,
    };
    let region = match body
        .region
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
    {
        Some(r) => trimmed("region", r, 64, 1)?,
        None => current.region.clone(),
    };
    if provider != current.provider {
        return Err(invalid(
            "provider",
            "PROVIDER_IMMUTABLE",
            "Connect a new bucket to change the provider.",
        ));
    }
    if region != current.region {
        patch.region = Some(region.clone());
        connection_changed = true;
    }
    if let Some(e) = body.endpoint.as_deref() {
        let endpoint = endpoint_for(deps, provider, &region, Some(e)).await?;
        if endpoint != current.endpoint {
            patch.endpoint = Some(endpoint);
            connection_changed = true;
        }
    }
    if let Some(b) = &body.bucket {
        let b = bucket_name(b)?;
        if b != current.bucket {
            patch.bucket = Some(b);
            connection_changed = true;
        }
    }
    if let Some(pf) = &body.prefix {
        let pf = key_prefix(pf)?;
        if pf != current.prefix {
            patch.prefix = Some(pf);
            connection_changed = true;
        }
    }
    if let Some(ps) = body.path_style
        && ps != current.path_style
    {
        patch.path_style = Some(ps);
        connection_changed = true;
    }
    match (&body.access_key_id, &body.secret_access_key) {
        (None, None) => {}
        (None, Some(sec)) => {
            let sealed = SealedSecret::seal(deps.sealer.as_ref(), current.id, &secret(sec)?)
                .map_err(repo_failure)?;
            patch.credentials = Some((
                current.access_key_id.clone(),
                sealed,
                current.key_hint.clone(),
            ));
            connection_changed = true;
        }
        (Some(_), None) => {
            return Err(invalid(
                "secret_access_key",
                "SECRET_REQUIRED",
                "Changing the access key id needs its secret access key.",
            ));
        }
        (Some(k), Some(sec)) => {
            let k = key_id(k)?;
            let sealed = SealedSecret::seal(deps.sealer.as_ref(), current.id, &secret(sec)?)
                .map_err(repo_failure)?;
            let hint = Secret::key_hint(&k);
            patch.credentials = Some((k, sealed, hint));
            connection_changed = true;
        }
    }
    if let Some(s) = &body.status {
        patch.status = Some(match MountStatus::parse(s) {
            Some(st @ (MountStatus::Active | MountStatus::Disabled)) => st,
            _ => {
                return Err(invalid(
                    "status",
                    "ENUM_TYPE_INVALID",
                    "Status can be set to active or disabled.",
                ));
            }
        });
    }

    let updated = deps
        .repo
        .update(current.id, patch)
        .await
        .map_err(repo_failure)?
        .ok_or_else(not_found)?;
    if connection_changed {
        // A different bucket or key needs its CORS and connection proven again.
        deps.repo
            .set_cors_ok(current.id, None)
            .await
            .map_err(repo_failure)?;
    }
    deps.resolver.invalidate(current.id);
    let fresh = deps
        .repo
        .by_id(current.id)
        .await
        .map_err(repo_failure)?
        .unwrap_or(updated);
    Ok(Json(MountObject::of(&fresh)))
}

async fn remove(deps: &MountsDeps, scope: Scope, mount_id: &str) -> MsgResult<StatusCode> {
    let m = owned_mount(deps, &scope, mount_id).await?;
    deps.repo.soft_delete(m.id).await.map_err(repo_failure)?;
    deps.resolver.invalidate(m.id);
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------------ user routes

/// The caller's own connected buckets.
#[utoipa::path(
    get,
    path = "/users/@me/storage/mounts",
    responses((status = 200, body = Vec<MountObject>), (status = 403, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_user(p: Principal, Ext(d): Ext<MountsDeps>) -> MsgResult<Json<Vec<MountObject>>> {
    list(&d, user_scope(&p)?).await
}

/// Connect a bucket of your own (at most 3). The secret is sealed at rest and never shown again.
#[utoipa::path(
    post,
    path = "/users/@me/storage/mounts",
    request_body = CreateMountBody,
    responses(
        (status = 201, body = MountObject),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "MOUNT_CAP_REACHED or MOUNT_LABEL_TAKEN"),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create_user(
    p: Principal,
    Ext(d): Ext<MountsDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    ApiJson(body): ApiJson<CreateMountBody>,
) -> MsgResult<(StatusCode, Json<MountObject>)> {
    create(&d, admin.as_deref(), user_scope(&p)?, body).await
}

/// Change one of your buckets (label, address, keys, prefix, or switch it on or off).
#[utoipa::path(
    patch,
    path = "/users/@me/storage/mounts/{mount_id}",
    params(("mount_id" = String, Path, description = "Mount snowflake")),
    request_body = PatchMountBody,
    responses(
        (status = 200, body = MountObject),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn patch_user(
    p: Principal,
    Ext(d): Ext<MountsDeps>,
    Path(mount_id): Path<String>,
    ApiJson(body): ApiJson<PatchMountBody>,
) -> MsgResult<Json<MountObject>> {
    patch(&d, user_scope(&p)?, &mount_id, body).await
}

/// Disconnect one of your buckets. Nothing in the bucket is touched.
#[utoipa::path(
    delete,
    path = "/users/@me/storage/mounts/{mount_id}",
    params(("mount_id" = String, Path, description = "Mount snowflake")),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_user(
    p: Principal,
    Ext(d): Ext<MountsDeps>,
    Path(mount_id): Path<String>,
) -> MsgResult<StatusCode> {
    remove(&d, user_scope(&p)?, &mount_id).await
}

// ------------------------------------------------------------------ guild routes

/// The server's connected buckets (the caller manages the server).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/storage/mounts",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<MountObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_guild(
    p: Principal,
    Ext(d): Ext<MountsDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
) -> MsgResult<Json<Vec<MountObject>>> {
    list(&d, guild_scope(&d, &s, &p, &guild_id).await?).await
}

/// Connect a bucket to the server (at most 5).
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/storage/mounts",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = CreateMountBody,
    responses(
        (status = 201, body = MountObject),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "MOUNT_CAP_REACHED or MOUNT_LABEL_TAKEN"),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create_guild(
    p: Principal,
    Ext(d): Ext<MountsDeps>,
    Ext(s): Ext<SettingsDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<CreateMountBody>,
) -> MsgResult<(StatusCode, Json<MountObject>)> {
    let scope = guild_scope(&d, &s, &p, &guild_id).await?;
    create(&d, admin.as_deref(), scope, body).await
}

/// Change one of the server's buckets.
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/storage/mounts/{mount_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("mount_id" = String, Path, description = "Mount snowflake"),
    ),
    request_body = PatchMountBody,
    responses(
        (status = 200, body = MountObject),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn patch_guild(
    p: Principal,
    Ext(d): Ext<MountsDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path((guild_id, mount_id)): Path<(String, String)>,
    ApiJson(body): ApiJson<PatchMountBody>,
) -> MsgResult<Json<MountObject>> {
    patch(
        &d,
        guild_scope(&d, &s, &p, &guild_id).await?,
        &mount_id,
        body,
    )
    .await
}

/// Disconnect one of the server's buckets.
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/storage/mounts/{mount_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("mount_id" = String, Path, description = "Mount snowflake"),
    ),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_guild(
    p: Principal,
    Ext(d): Ext<MountsDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path((guild_id, mount_id)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    remove(&d, guild_scope(&d, &s, &p, &guild_id).await?, &mount_id).await
}

/// OpenAPI fragment.
#[derive(OpenApi)]
#[openapi(
    paths(
        list_user,
        create_user,
        patch_user,
        delete_user,
        list_guild,
        create_guild,
        patch_guild,
        delete_guild
    ),
    components(schemas(MountObject, CreateMountBody, PatchMountBody))
)]
pub struct MountsApiDoc;
