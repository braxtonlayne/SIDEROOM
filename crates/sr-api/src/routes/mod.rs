//! Feature route modules. `sr-server` mounts everything through [`all`] and
//! documents it through [`openapi`], so adding a module is one `.merge(...)`
//! line in each.

use crate::state::AppState;
use axum::Router;

/// Every feature router, relative to the `/api/v1` prefix.
pub fn all() -> Router<AppState> {
    Router::new()
}

/// The full OpenAPI document: [`crate::openapi::base`] plus each module's paths.
pub fn openapi() -> utoipa::openapi::OpenApi {
    crate::openapi::base()
}
