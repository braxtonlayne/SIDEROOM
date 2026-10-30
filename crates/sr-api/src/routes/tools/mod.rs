//! The utility drawer's backend (wave 32).
//!
//! * `fx`: `GET /tools/fx`, the cached European Central Bank reference rates for the currency converter
//!   (UTL-CALC-001). The browser never calls a third party; the server fetches the feed once a day.
//!
//! utl-dev adds `notes`, `loot` and `timer` to this module (UTL-DEV-001); the integrator merges the two
//! `mod.rs` files (the `pub mod`, the `.route(...)` lines and the `paths(...)` and `schemas(...)` lists).

pub mod fx;

use crate::state::AppState;
use axum::{Router, routing::get};
use utoipa::OpenApi;

/// All routes of this module (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new().route("/tools/fx", get(fx::rates))
}

#[derive(OpenApi)]
#[openapi(paths(fx::rates), components(schemas(fx::FxRatesObject)))]
pub struct ToolsApiDoc;

/// OpenAPI fragment.
pub fn openapi() -> utoipa::openapi::OpenApi {
    ToolsApiDoc::openapi()
}
