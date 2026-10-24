//! Member routes of submissions that are not tied to the review module's own kinds.
//!
//! * `cinema`: the draft behind a Cinema submission and the member's video uploads
//!   (`/submissions/{id}/cinema/*`, RDY-MED-001, spec `docs/specs/cinema-media.md` section 8.7).

mod cinema;

pub use cinema::{CinemaSubmitDeps, SubmissionVideoSlot};

use crate::state::AppState;
use axum::{
    Router,
    routing::{get, post, put},
};
use utoipa::OpenApi;

/// Routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/submissions/{submission_id}/cinema/draft",
            get(cinema::draft),
        )
        .route(
            "/submissions/{submission_id}/cinema/title",
            put(cinema::update_title),
        )
        .route(
            "/submissions/{submission_id}/cinema/episodes",
            post(cinema::add_episode),
        )
        .route(
            "/submissions/{submission_id}/cinema/episodes/{episode_id}",
            put(cinema::update_episode).delete(cinema::delete_episode),
        )
        .route(
            "/submissions/{submission_id}/cinema/files",
            post(cinema::begin_upload),
        )
        .route(
            "/submissions/{submission_id}/cinema/uploads/{asset_id}",
            get(cinema::upload_status).delete(cinema::abort_upload),
        )
        .route(
            "/submissions/{submission_id}/cinema/uploads/{asset_id}/parts",
            get(cinema::upload_parts),
        )
        .route(
            "/submissions/{submission_id}/cinema/uploads/{asset_id}/complete",
            post(cinema::complete_upload),
        )
}

/// OpenAPI fragment of these routes.
#[derive(OpenApi)]
#[openapi(
    paths(
        cinema::draft,
        cinema::update_title,
        cinema::add_episode,
        cinema::update_episode,
        cinema::delete_episode,
        cinema::begin_upload,
        cinema::upload_status,
        cinema::upload_parts,
        cinema::complete_upload,
        cinema::abort_upload,
    ),
    components(schemas(
        cinema::SubmissionDraftView,
        cinema::SubmissionVideo,
        cinema::SubmissionPendingUpload,
        cinema::SubmissionVideoRequest,
        cinema::SubmissionVideoSlot,
        cinema::SubmissionVideoItem,
        cinema::SubmissionEpisodeInput,
        sr_cinema::drafts::SubmissionDraft,
    ))
)]
pub struct SubmissionsApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    SubmissionsApiDoc::openapi()
}

#[cfg(test)]
mod tests_pg;
