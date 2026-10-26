//! The BYOS storage routes (wave 27).
//!
//! * `mounts`: `/users/@me/storage/mounts*` and `/guilds/{id}/storage/mounts*` (BYOS-MNT-001..003).
//!
//! byos-scopes adds `scopes` and `objects` here; each module exposes `router()` and an OpenAPI fragment.

pub mod mounts;
#[cfg(test)]
mod mounts_tests;

use crate::state::AppState;
use axum::Router;
use utoipa::OpenApi;

/// All storage routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new().merge(mounts::router())
}

/// OpenAPI fragment.
pub fn openapi() -> utoipa::openapi::OpenApi {
    let mut doc = mounts::MountsApiDoc::openapi();
    doc.merge(utoipa::openapi::OpenApi::default());
    doc
}
