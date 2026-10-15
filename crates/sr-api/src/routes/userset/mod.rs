//! User-settings backend routes (phase 11): the data export (LNG-USET-020) and the
//! connections (LNG-USET-024). Account deletion is in `auth::identity`.

pub mod connections;
pub mod data_export;
pub mod providers;

use crate::state::AppState;
use axum::Router;
use utoipa::OpenApi;

/// All routes of this module (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        .merge(data_export::router())
        .merge(connections::router())
}

/// OpenAPI fragment.
pub fn openapi() -> utoipa::openapi::OpenApi {
    let mut doc = data_export::DataExportApiDoc::openapi();
    doc.merge(connections::ConnectionsApiDoc::openapi());
    doc
}

#[cfg(test)]
mod tests;
