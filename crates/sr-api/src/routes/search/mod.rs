//! Message search REST API (LNG-SRC-003, open question 131): guild-wide and
//! single-channel search over the Tantivy index in `sr-search`.
//!
//! Wiring, in the composition root:
//!
//! ```ignore
//! let search = SearchDeps::new(Arc::new(MessageIndex::open_in_dir(&dir)?))
//!     .with_age_gate(Arc::new(PgAgeGate::new(pool)));
//! let state = AppState::builder().extension(Arc::new(search)).build();
//! ```
//!
//! Hits are rendered through the messages module, so [`MessagesDeps`] must be
//! registered too. Keeping the index current is the job of `search.index_message`
//! (`sr-server::search_jobs`).
//!
//! [`MessagesDeps`]: crate::routes::messages::MessagesDeps

pub mod deps;
pub mod messages;

#[cfg(test)]
mod tests;

pub use deps::{AgeGate, ConfirmedUsers, NoAgeConfirmed, SearchDeps};

use crate::{openapi::ErrorBody, state::AppState};
use axum::{Router, routing::get};
use utoipa::OpenApi;

/// Search routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/guilds/{guild_id}/messages/search",
            get(messages::search_guild),
        )
        .route(
            "/channels/{channel_id}/messages/search",
            get(messages::search_channel),
        )
}

/// OpenAPI fragment for these routes; merge into [`crate::openapi::base`].
#[derive(OpenApi)]
#[openapi(
    paths(messages::search_guild, messages::search_channel),
    components(schemas(ErrorBody, messages::SearchResponse, messages::SearchSnippet))
)]
pub struct SearchApiDoc;

/// The OpenAPI document fragment for the search endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    SearchApiDoc::openapi()
}
