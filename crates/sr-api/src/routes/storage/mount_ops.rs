//! Operations on a mount: the connection test (BYOS-MNT-002) and the CORS check and apply (BYOS-MNT-003).
//!
//! * `POST {base}/{mount_id}/test` runs the six probes inside a 20 s budget, stores the result as `last_test`
//!   and sets the mount `failing` on any failed row (back to `active` on a clean run). Rate limited by
//!   `abuse.storage_tests_per_hour`.
//! * `POST {base}/{mount_id}/cors/check` and `.../cors/apply` answer `ok|missing|different|manual`. `ok` stamps
//!   `cors_ok_at` (the presign gate); anything else clears it.
//!
//! `{base}` is `/users/@me/storage/mounts` or `/guilds/{guild_id}/storage/mounts`, authorised exactly as
//! the CRUD routes. The endpoint is re-checked against the SSRF policy before every outbound call.

use std::time::{SystemTime, UNIX_EPOCH};

use axum::{Json, extract::Path};
use serde::Serialize;
use sr_core::{ApiError, ErrorCode};
use sr_storage::cors_check::{self, CorsReport};
use sr_storage::mounts::{Mount, MountStatus};
use sr_storage::probe::{self, ConnectionTest};
use sr_storage::s3admin::S3Admin;
use utoipa::{OpenApi, ToSchema};

use super::mounts::{
    MountsDeps, Scope, guild_scope, not_found, owned_mount, repo_failure, user_scope,
};
use crate::routes::Ext;
use crate::routes::developer::iso;
use crate::routes::messages::error::{MsgError, MsgResult};
use crate::routes::settings::deps::SettingsDeps;
use crate::{Principal, openapi::ErrorBody, state::AppState};

/// The routes.
pub fn router() -> axum::Router<AppState> {
    use axum::routing::post;
    axum::Router::new()
        .route("/users/@me/storage/mounts/{mount_id}/test", post(test_user))
        .route(
            "/users/@me/storage/mounts/{mount_id}/cors/check",
            post(check_user),
        )
        .route(
            "/users/@me/storage/mounts/{mount_id}/cors/apply",
            post(apply_user),
        )
        .route(
            "/guilds/{guild_id}/storage/mounts/{mount_id}/test",
            post(test_guild),
        )
        .route(
            "/guilds/{guild_id}/storage/mounts/{mount_id}/cors/check",
            post(check_guild),
        )
        .route(
            "/guilds/{guild_id}/storage/mounts/{mount_id}/cors/apply",
            post(apply_guild),
        )
}

/// One row of a connection test.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProbeRowObject {
    /// `head_bucket`, `list`, `put`, `multipart`, `cors` or `list_buckets`.
    pub id: String,
    /// `pass`, `warn` or `fail`.
    pub status: String,
    /// One English sentence, shown as it is.
    pub message: String,
}

/// A connection test; the same object is stored as the mount's `last_test`.
#[derive(Debug, Serialize, ToSchema)]
pub struct TestResultObject {
    /// When it ran (ISO 8601).
    pub at: String,
    pub rows: Vec<ProbeRowObject>,
}

/// The state of a bucket's CORS rules against the one this site needs.
#[derive(Debug, Serialize, ToSchema)]
pub struct CorsResultObject {
    /// `ok`, `missing`, `different` or `manual`.
    pub state: String,
    /// The required rule, as lines of text.
    pub required: Vec<String>,
    /// The rules the bucket has now, as lines of text.
    pub found: Vec<String>,
    /// Provider steps when `state` is `manual`.
    pub manual_steps: Option<String>,
    /// When it was checked (ISO 8601).
    pub checked_at: String,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn result_json(at_ms: i64, t: &ConnectionTest) -> (serde_json::Value, TestResultObject) {
    let rows: Vec<ProbeRowObject> = t
        .rows
        .iter()
        .map(|r| ProbeRowObject {
            id: r.id.to_owned(),
            status: r.status.as_str().to_owned(),
            message: r.message.clone(),
        })
        .collect();
    let obj = TestResultObject {
        at: iso(at_ms),
        rows,
    };
    (serde_json::to_value(&obj).unwrap_or_default(), obj)
}

/// Re-check the stored endpoint against the SSRF policy (the DNS answer may have changed since it was saved)
/// and build the admin client with the opened secret.
async fn admin_for(deps: &MountsDeps, m: &Mount) -> MsgResult<S3Admin> {
    let bad = |msg: &str| {
        MsgError::Api(ApiError::with_message(ErrorCode::SrConflict, msg).field(
            "mount",
            "MOUNT_ENDPOINT_BLOCKED",
            msg,
        ))
    };
    let url =
        url::Url::parse(&m.endpoint).map_err(|_| bad("The stored endpoint is not a valid URL."))?;
    sr_net::resolve_validated(&deps.net_policy, &sr_net::SystemResolver, &url)
        .await
        .map_err(|_| bad("This endpoint is not reachable from the public internet."))?;
    let secret = m
        .secret_key_enc
        .open(deps.sealer.as_ref(), m.id)
        .map_err(repo_failure)?;
    S3Admin::new(
        &m.endpoint,
        &m.bucket,
        &m.provider.signing_region(&m.region),
        &m.access_key_id,
        secret.expose(),
    )
    .map_err(|e| {
        tracing::error!(error = %e, "building the S3 admin client failed");
        MsgError::from(ErrorCode::SrInternal)
    })
}

async fn run_test(
    deps: &MountsDeps,
    scope: Scope,
    mount_id: &str,
) -> MsgResult<Json<TestResultObject>> {
    let m = owned_mount(deps, &scope, mount_id).await?;
    let admin = admin_for(deps, &m).await?;
    let result = probe::run(&admin, &m.prefix, deps.test_budget).await;
    let at = now_ms();
    let (json, obj) = result_json(at, &result);
    let status = if result.failed() {
        Some(MountStatus::Failing)
    } else if m.status == MountStatus::Failing {
        Some(MountStatus::Active)
    } else {
        None
    };
    let kept = deps
        .repo
        .record_test(m.id, at, json.to_string(), status)
        .await
        .map_err(repo_failure)?;
    deps.resolver.invalidate(m.id);
    if !kept {
        return Err(not_found());
    }
    Ok(Json(obj))
}

async fn run_cors(
    deps: &MountsDeps,
    scope: Scope,
    mount_id: &str,
    apply: bool,
) -> MsgResult<Json<CorsResultObject>> {
    if deps.public_origin.is_empty() {
        tracing::error!("MountsDeps has no public origin; the CORS routes cannot run");
        return Err(ErrorCode::SrInternal.into());
    }
    let m = owned_mount(deps, &scope, mount_id).await?;
    let admin = admin_for(deps, &m).await?;
    let report: CorsReport = if apply {
        cors_check::apply(&admin, m.provider, &deps.public_origin).await
    } else {
        cors_check::check(&admin, m.provider, &deps.public_origin).await
    }
    .map_err(|e| {
        tracing::warn!(error = %e, mount = m.id, "CORS call failed");
        MsgError::Api(ApiError::with_message(
            ErrorCode::SrConflict,
            "The service refused to read or write the bucket's CORS rules. Check the key's permissions or set the rule in the provider's console.",
        ))
    })?;
    let at = now_ms();
    let ok = report.state == cors_check::CorsState::Ok;
    deps.repo
        .set_cors_ok(m.id, ok.then_some(at))
        .await
        .map_err(repo_failure)?;
    deps.resolver.invalidate(m.id);
    Ok(Json(CorsResultObject {
        state: report.state.as_str().to_owned(),
        required: report.required,
        found: report.found,
        manual_steps: report.manual_steps,
        checked_at: iso(at),
    }))
}

/// Run the connection test on one of your buckets (20 s budget).
#[utoipa::path(
    post,
    path = "/users/@me/storage/mounts/{mount_id}/test",
    params(("mount_id" = String, Path, description = "Mount snowflake")),
    responses((status = 200, body = TestResultObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 429, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn test_user(
    p: Principal,
    Ext(d): Ext<MountsDeps>,
    Path(mount_id): Path<String>,
) -> MsgResult<Json<TestResultObject>> {
    run_test(&d, user_scope(&p)?, &mount_id).await
}

/// Run the connection test on one of the server's buckets (20 s budget).
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/storage/mounts/{mount_id}/test",
    params(("guild_id" = String, Path, description = "Guild snowflake"), ("mount_id" = String, Path, description = "Mount snowflake")),
    responses((status = 200, body = TestResultObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 429, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn test_guild(
    p: Principal,
    Ext(d): Ext<MountsDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path((guild_id, mount_id)): Path<(String, String)>,
) -> MsgResult<Json<TestResultObject>> {
    run_test(&d, guild_scope(&d, &s, &p, &guild_id).await?, &mount_id).await
}

/// Compare one of your bucket's CORS rules with the one this site needs.
#[utoipa::path(
    post,
    path = "/users/@me/storage/mounts/{mount_id}/cors/check",
    params(("mount_id" = String, Path, description = "Mount snowflake")),
    responses((status = 200, body = CorsResultObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn check_user(
    p: Principal,
    Ext(d): Ext<MountsDeps>,
    Path(mount_id): Path<String>,
) -> MsgResult<Json<CorsResultObject>> {
    run_cors(&d, user_scope(&p)?, &mount_id, false).await
}

/// Write the needed CORS rule to one of your buckets, then check it again.
#[utoipa::path(
    post,
    path = "/users/@me/storage/mounts/{mount_id}/cors/apply",
    params(("mount_id" = String, Path, description = "Mount snowflake")),
    responses((status = 200, body = CorsResultObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn apply_user(
    p: Principal,
    Ext(d): Ext<MountsDeps>,
    Path(mount_id): Path<String>,
) -> MsgResult<Json<CorsResultObject>> {
    run_cors(&d, user_scope(&p)?, &mount_id, true).await
}

/// Compare one of the server's bucket's CORS rules with the one this site needs.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/storage/mounts/{mount_id}/cors/check",
    params(("guild_id" = String, Path, description = "Guild snowflake"), ("mount_id" = String, Path, description = "Mount snowflake")),
    responses((status = 200, body = CorsResultObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn check_guild(
    p: Principal,
    Ext(d): Ext<MountsDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path((guild_id, mount_id)): Path<(String, String)>,
) -> MsgResult<Json<CorsResultObject>> {
    run_cors(&d, guild_scope(&d, &s, &p, &guild_id).await?, &mount_id, false).await
}

/// Write the needed CORS rule to one of the server's buckets, then check it again.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/storage/mounts/{mount_id}/cors/apply",
    params(("guild_id" = String, Path, description = "Guild snowflake"), ("mount_id" = String, Path, description = "Mount snowflake")),
    responses((status = 200, body = CorsResultObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn apply_guild(
    p: Principal,
    Ext(d): Ext<MountsDeps>,
    Ext(s): Ext<SettingsDeps>,
    Path((guild_id, mount_id)): Path<(String, String)>,
) -> MsgResult<Json<CorsResultObject>> {
    run_cors(&d, guild_scope(&d, &s, &p, &guild_id).await?, &mount_id, true).await
}

/// OpenAPI fragment.
#[derive(OpenApi)]
#[openapi(
    paths(
        test_user,
        test_guild,
        check_user,
        apply_user,
        check_guild,
        apply_guild
    ),
    components(schemas(ProbeRowObject, TestResultObject, CorsResultObject))
)]
pub struct MountOpsApiDoc;
