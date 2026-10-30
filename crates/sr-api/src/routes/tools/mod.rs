//! HTTP layer of the utility drawer's server-backed tools (wave 32, roadmap-p27 section 10 and 11).
//! Every tool owns one submodule with its own `router()` and `openapi()`; this file only merges them.

pub mod cards;

use crate::state::AppState;
use axum::Router;

/// All tool routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new().merge(cards::router())
}

/// The OpenAPI fragment of the tool routes.
pub fn openapi() -> utoipa::openapi::OpenApi {
    let mut doc = cards::openapi();
    // Other tools: `doc.merge(<tool>::openapi());`
    let _ = &mut doc;
    doc
}
