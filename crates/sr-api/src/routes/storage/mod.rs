//! Storage mounts, read side (wave 27, BYOS-SCP-001 and 002):
//!
//! * `scopes`: Friend Shelf shares (`/users/@me/storage/mounts/{id}/shares*`, `/users/@me/storage/shares`),
//!   guild storage grants (`PUT /guilds/{id}/storage/grants`) and `GET /users/@me/shelves/summary`.
//! * `objects`: the scoped listing, object metadata and presign (`/storage/mounts/{id}/objects*`,
//!   `/storage/mounts/{id}/presign`).
//!
//! Every read goes through `sr_storage::scopes`. The mount CRUD routes (`mounts`) belong to byos-core and are
//! merged here by the integrator.

mod objects;
mod scopes;

use crate::{
    routes::messages::error::{MsgError, MsgResult},
    state::AppState,
};
use axum::{
    Router,
    routing::{get, post, put},
};
use sr_core::{ApiError, ErrorCode, error::FieldError};
use sr_storage::mounts::{MountResolver, Sealer};
use sr_storage::scopes::{Disclosure, FactsError, Refused, ScopeStore};
use std::sync::Arc;
use utoipa::OpenApi;

/// Collaborators of the storage read routes.
pub struct ScopeDeps {
    /// Finds mounts.
    pub mounts: Arc<dyn MountResolver>,
    /// The facts behind `scopes::resolve`, plus share and grant rows.
    pub store: Arc<dyn ScopeStore>,
    /// Opens sealed mount secrets to build the S3 client.
    pub sealer: Arc<dyn Sealer>,
}

/// The acting user id; the routes accept browser sessions only (see `SessionOnly`).
pub(crate) fn uid(p: &crate::Principal) -> i64 {
    p.user_id
}

/// A snowflake path segment or body field; malformed means "no such row".
pub(crate) fn parse_row(raw: &str) -> MsgResult<i64> {
    raw.parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| MsgError::from(ErrorCode::SrNotFound))
}

/// A refusal as the wire convention has it: 403 for a member who lacks only a grant, 404 for the rest.
pub(crate) fn refusal(r: Refused) -> MsgError {
    match r.disclosure() {
        Disclosure::Forbidden => ErrorCode::MissingPermissions.into(),
        Disclosure::NotFound => ErrorCode::SrNotFound.into(),
    }
}

/// The facts store failed: nothing is admitted.
pub(crate) fn facts_failed(e: FactsError) -> MsgError {
    tracing::error!(error = %e, "storage scope facts failed");
    ErrorCode::SrInternal.into()
}

/// 409 `CORS_NOT_READY`: the mount has not passed its CORS check, so it presigns nothing.
pub(crate) fn cors_not_ready() -> MsgError {
    let mut e = ApiError::with_message(ErrorCode::SrConflict, "CORS_NOT_READY");
    e.fields.push((
        "mount".to_owned(),
        FieldError {
            code: "CORS_NOT_READY".to_owned(),
            message: "The bucket has not passed its CORS check yet.".to_owned(),
        },
    ));
    MsgError::Api(e)
}

/// All storage read routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/users/@me/storage/mounts/{mount_id}/shares",
            get(scopes::list_shares),
        )
        .route(
            "/users/@me/storage/mounts/{mount_id}/shares/{user_id}",
            put(scopes::put_share).delete(scopes::delete_share),
        )
        .route("/users/@me/storage/shares", get(scopes::incoming_shares))
        .route("/users/@me/shelves/summary", get(scopes::summary))
        .route("/guilds/{guild_id}/storage/grants", put(scopes::put_grants))
        .route("/storage/mounts/{mount_id}/objects", get(objects::list))
        .route(
            "/storage/mounts/{mount_id}/objects/{*key}",
            get(objects::meta),
        )
        .route("/storage/mounts/{mount_id}/presign", post(objects::presign))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        scopes::list_shares,
        scopes::put_share,
        scopes::delete_share,
        scopes::incoming_shares,
        scopes::summary,
        scopes::put_grants,
        objects::list,
        objects::meta,
        objects::presign,
    ),
    components(schemas(
        crate::openapi::ErrorBody,
        scopes::ShareView,
        scopes::ShareBody,
        scopes::IncomingShare,
        scopes::ShelvesSummaryView,
        scopes::CatalogCounts,
        scopes::GrantsBody,
        objects::ObjectList,
        objects::ObjectEntry,
        objects::ObjectMetaView,
        objects::PresignBody,
        objects::PresignView,
    ))
)]
pub struct StorageApiDoc;

/// The OpenAPI fragment of the storage read routes.
pub fn openapi() -> utoipa::openapi::OpenApi {
    StorageApiDoc::openapi()
}
