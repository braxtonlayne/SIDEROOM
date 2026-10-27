//! The BYOS storage routes (wave 27):
//!
//! * `mounts`: `/users/@me/storage/mounts*` and `/guilds/{id}/storage/mounts*` (BYOS-MNT-001).
//! * `mount_ops`: `.../mounts/{id}/test` and `.../mounts/{id}/cors/{check,apply}` (BYOS-MNT-002, 003).
//! * `scopes`: Friend Shelf shares (`/users/@me/storage/mounts/{id}/shares*`, `/users/@me/storage/shares`),
//!   guild storage grants (`PUT /guilds/{id}/storage/grants`) and `GET /users/@me/shelves/summary`.
//! * `objects`: the scoped listing, object metadata and presign (`/storage/mounts/{id}/objects*`,
//!   `/storage/mounts/{id}/presign`).
//!
//! Every read goes through `sr_storage::scopes`.

pub mod mount_ops;
pub mod mounts;
#[cfg(test)]
mod mounts_tests;
mod objects;
mod scopes;
pub mod shelves;

pub use shelves::ShelfDeps;

use crate::{
    routes::messages::error::{MsgError, MsgResult},
    state::AppState,
};
use axum::{
    Router,
    routing::{get, patch, post, put},
};
use sr_core::ErrorCode;
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
    mounts::conflict(
        "CORS_NOT_READY",
        "The bucket has not passed its CORS check yet.",
    )
}

/// All storage read routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(mounts::router())
        .merge(mount_ops::router())
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
        .route(
            "/storage/mounts/{mount_id}/index",
            get(shelves::index_status).post(shelves::start_index),
        )
        .route("/shelves/{section}", get(shelves::list))
        .route(
            "/shelves/{section}/{item_id}",
            get(shelves::get_item).patch(shelves::patch_item),
        )
        .route(
            "/shelves/{section}/{item_id}/parts/{part_id}",
            patch(shelves::patch_part),
        )
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
        shelves::list,
        shelves::get_item,
        shelves::patch_item,
        shelves::patch_part,
        shelves::start_index,
        shelves::index_status,
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
        shelves::ShelfSourceView,
        shelves::ShelfItemView,
        shelves::ShelfPartView,
        shelves::ShelfListView,
        shelves::ShelfDetailView,
        shelves::DetailsBody,
        shelves::ShelfPatchBody,
        shelves::PartPatchBody,
        shelves::IndexStatusView,
    ))
)]
pub struct StorageApiDoc;

/// The OpenAPI fragment of the storage read routes.
pub fn openapi() -> utoipa::openapi::OpenApi {
    let mut doc = StorageApiDoc::openapi();
    doc.merge(mounts::MountsApiDoc::openapi());
    doc.merge(mount_ops::MountOpsApiDoc::openapi());
    doc
}
