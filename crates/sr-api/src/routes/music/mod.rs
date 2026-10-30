//! The music library routes (wave 30).
//!
//! * `library` (MUS-IDX-001): `GET /music/library` (what the caller reaches), `POST /music/library/rescan`.
//! * `tracks` (MUS-IDX-001): `GET|PATCH /music/tracks/{id}` and `POST /music/tracks/{id}/presign`.
//!
//! Every read goes through `sr_storage::scopes` (the viewer is never resolved as staff); a write needs the
//! mount's owner or, on a server's shelf, the `upload` grant. The server never streams or opens a file.

pub mod library;
pub mod tracks;

use crate::routes::messages::error::{MsgError, MsgResult};
use crate::routes::storage::shelves::store_failed;
use crate::routes::storage::{ScopeDeps, facts_failed};
use crate::state::AppState;
use axum::{
    Router,
    routing::{get, post},
};
use sr_core::ErrorCode;
use sr_storage::index::audio::MusicStore;
use sr_storage::index::shelf::{Reach, Section};
use sr_storage::mounts::Mount;
use sr_storage::scopes::{ResolveError, ScopeFacts, Viewer, access};
use std::sync::Arc;
use utoipa::OpenApi;

/// Collaborators of the music routes (mounts, facts and sealer come from [`ScopeDeps`]).
pub struct MusicDeps {
    /// Track rows.
    pub store: Arc<dyn MusicStore>,
}

/// The mounts with listed tracks that the caller may see, with how. A mount the caller cannot reach is absent.
pub(crate) async fn reaches(d: &ScopeDeps, m: &MusicDeps, user: i64) -> MsgResult<Vec<Reach>> {
    let facts: &dyn ScopeFacts = &*d.store;
    let viewer = Viewer {
        id: user,
        staff: false,
    };
    let mut out = Vec::new();
    for id in m.store.mounts().await.map_err(store_failed)? {
        let mount = match d.mounts.by_id(id).await {
            Ok(Some(mt)) => mt,
            Ok(None) => continue,
            Err(e) => return Err(store_failed(e)),
        };
        match access(facts, viewer, &mount).await {
            Ok(a) => {
                if a.unrestricted || a.kinds.admits(Some(Section::Music.kind())) {
                    out.push(Reach { mount, access: a });
                }
            }
            Err(ResolveError::Refused(_)) => {}
            Err(ResolveError::Facts(e)) => return Err(facts_failed(e)),
        }
    }
    Ok(out)
}

/// Whether the caller may write tags on this mount: its owner, or on a server's shelf `upload` or `manage`.
pub(crate) async fn may_write(d: &ScopeDeps, user: i64, mount: &Mount) -> MsgResult<bool> {
    let facts: &dyn ScopeFacts = &*d.store;
    sr_storage::scopes::may_upload(
        facts,
        Viewer {
            id: user,
            staff: false,
        },
        mount,
    )
    .await
    .map_err(facts_failed)
}

pub(crate) fn not_found() -> MsgError {
    ErrorCode::SrNotFound.into()
}

/// The music routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/music/library", get(library::list))
        .route("/music/library/rescan", post(library::rescan))
        .route(
            "/music/tracks/{track_id}",
            get(tracks::get_track).patch(tracks::patch_track),
        )
        .route("/music/tracks/{track_id}/presign", post(tracks::presign))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        library::list,
        library::rescan,
        tracks::get_track,
        tracks::patch_track,
        tracks::presign,
    ),
    components(schemas(
        library::TrackListView,
        library::RescanBody,
        library::RescanView,
        tracks::TrackView,
        tracks::TrackPatchBody,
        tracks::TrackPresignBody,
    ))
)]
pub struct MusicApiDoc;

/// The OpenAPI fragment of the music routes.
pub fn openapi() -> utoipa::openapi::OpenApi {
    MusicApiDoc::openapi()
}
