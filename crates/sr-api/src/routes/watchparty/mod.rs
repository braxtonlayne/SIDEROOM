//! Watch Party API (`/watch-parties/*`, `/channels/{id}/watch-party`): WP-BE-002 to WP-BE-009. The contract
//! is `docs/specs/watch-parties.md` ("HTTP and gateway API as built"); the implementation lives in
//! `sr-watchparty`, this module is the HTTP layer.
//!
//! Wiring, in the composition root:
//!
//! ```ignore
//! let service = PartyService::new(pool, clock, ids)
//!     .with_play_gate(gate).with_channels(access).with_events(Arc::new(BusEvents::new(bus)));
//! let state = AppState::builder().extension(Arc::new(WatchPartyDeps::new(service))).build();
//! ```

mod handlers;
#[cfg(test)]
mod tests;

use crate::{auth::UserOnly, error::HttpError, openapi::ErrorBody, routes::Ext, state::AppState};
use axum::{
    Json, Router,
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use sr_core::{ApiError, ErrorCode};
use sr_watchparty::{PartyError, PartyService, Viewer};
use std::sync::Arc;
use utoipa::OpenApi;

/// Collaborators of the Watch Party endpoints.
pub struct WatchPartyDeps {
    pub service: PartyService,
}

impl WatchPartyDeps {
    pub fn new(service: PartyService) -> Self {
        Self { service }
    }
}

/// A signed-in member using Watch Parties.
pub(crate) struct Ctx {
    pub deps: Arc<WatchPartyDeps>,
    pub viewer: Viewer,
}

impl Ctx {
    pub fn svc(&self) -> &PartyService {
        &self.deps.service
    }
}

impl FromRequestParts<AppState> for Ctx {
    type Rejection = HttpError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let UserOnly(p) = UserOnly::from_request_parts(parts, state).await?;
        let Ext(deps) = Ext::<WatchPartyDeps>::from_request_parts(parts, state).await?;
        let viewer = deps.service.viewer(p.user_id).await;
        Ok(Self { deps, viewer })
    }
}

/// Parse a snowflake path segment; a malformed one names nothing.
pub(crate) fn id(raw: &str, what: &'static str) -> Result<i64, HttpError> {
    raw.parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| not_found(what))
}

fn not_found(what: &str) -> HttpError {
    ApiError::with_message(ErrorCode::SrNotFound, format!("Unknown {what}")).into()
}

/// A [`PartyError`] as an HTTP response. A stale `expected_version` answers 409 with the current playback so
/// the client can re-sync in one round trip.
pub(crate) struct WpError(pub PartyError);

impl From<PartyError> for WpError {
    fn from(e: PartyError) -> Self {
        Self(e)
    }
}

impl From<HttpError> for WpError {
    fn from(e: HttpError) -> Self {
        let code = e.0.code;
        Self(match code {
            ErrorCode::SrNotFound => PartyError::NotFound("watch party"),
            _ => PartyError::Invalid(e.0.message),
        })
    }
}

impl IntoResponse for WpError {
    fn into_response(self) -> Response {
        let err = match self.0 {
            PartyError::NotFound(w) => {
                ApiError::with_message(ErrorCode::SrNotFound, format!("Unknown {w}"))
            }
            PartyError::Forbidden => ApiError::new(ErrorCode::MissingPermissions),
            PartyError::AgeConfirmationRequired => {
                ApiError::new(ErrorCode::SrAgeConfirmationRequired)
            }
            PartyError::Invalid(m) => ApiError::with_message(ErrorCode::InvalidFormBody, m),
            PartyError::Conflict(m) => ApiError::with_message(ErrorCode::SrConflict, m),
            PartyError::RateLimited => ApiError::new(ErrorCode::TooManyRequests),
            PartyError::VersionConflict { current } => {
                let mut body = ApiError::with_message(
                    ErrorCode::SrConflict,
                    "The playback state changed; re-sync from `playback`.",
                )
                .body();
                if let (Some(obj), Ok(cur)) =
                    (body.as_object_mut(), serde_json::to_value(&*current))
                {
                    obj.insert("playback".to_owned(), cur);
                }
                return (StatusCode::CONFLICT, Json(body)).into_response();
            }
            PartyError::Storage(m) => {
                tracing::error!(error = %m, "watch party failure");
                ApiError::new(ErrorCode::SrInternal)
            }
        };
        HttpError(err).into_response()
    }
}

pub(crate) type WpResult<T> = Result<T, WpError>;

/// Watch Party routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    use handlers as h;
    Router::new()
        .route("/watch-parties/time", get(h::time))
        .route("/watch-parties/me", get(h::mine))
        .route("/watch-parties", post(h::create))
        .route(
            "/watch-parties/{party_id}",
            get(h::get_party).patch(h::update_settings).delete(h::end),
        )
        .route("/channels/{channel_id}/watch-party", get(h::in_channel))
        .route("/watch-parties/{party_id}/join", post(h::join))
        .route("/watch-parties/{party_id}/leave", post(h::leave))
        .route("/watch-parties/{party_id}/host", post(h::transfer_host))
        .route(
            "/watch-parties/{party_id}/controllers/{user_id}",
            put(h::set_controller),
        )
        .route(
            "/watch-parties/{party_id}/members/{user_id}",
            delete(h::kick),
        )
        .route("/watch-parties/{party_id}/playback", post(h::command))
        .route("/watch-parties/{party_id}/media", put(h::change_media))
        .route("/watch-parties/{party_id}/queue", post(h::queue_add))
        .route(
            "/watch-parties/{party_id}/queue/{item_id}",
            axum::routing::patch(h::queue_move).delete(h::queue_remove),
        )
        .route("/watch-parties/{party_id}/next", post(h::next))
        .route("/watch-parties/{party_id}/heartbeat", post(h::heartbeat))
        .route("/watch-parties/{party_id}/reactions", post(h::react))
        .route("/watch-parties/{party_id}/messages", post(h::say))
        .route("/watch-parties/{party_id}/audit", get(h::audit))
}

/// OpenAPI fragment for the Watch Party endpoints.
#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::time,
        handlers::mine,
        handlers::create,
        handlers::get_party,
        handlers::update_settings,
        handlers::end,
        handlers::in_channel,
        handlers::join,
        handlers::leave,
        handlers::transfer_host,
        handlers::set_controller,
        handlers::kick,
        handlers::command,
        handlers::change_media,
        handlers::queue_add,
        handlers::queue_move,
        handlers::queue_remove,
        handlers::next,
        handlers::heartbeat,
        handlers::react,
        handlers::say,
        handlers::audit,
    ),
    components(schemas(
        ErrorBody,
        sr_watchparty::model::PlaybackView,
        sr_watchparty::model::MediaView,
        sr_watchparty::model::ParticipantView,
        sr_watchparty::model::QueueItemView,
        sr_watchparty::model::ControlMode,
        sr_watchparty::model::SettingsView,
        sr_watchparty::model::Binding,
        sr_watchparty::model::MediaKind,
        sr_watchparty::model::ExternalInput,
        sr_watchparty::model::PartyView,
        sr_watchparty::model::YouView,
        sr_watchparty::model::CreateInput,
        sr_watchparty::model::SettingsInput,
        sr_watchparty::model::JoinInput,
        sr_watchparty::model::Action,
        sr_watchparty::model::CommandInput,
        sr_watchparty::model::ChangeMediaInput,
        sr_watchparty::model::QueueAddInput,
        sr_watchparty::model::QueueMoveInput,
        sr_watchparty::model::TransferInput,
        sr_watchparty::model::ControllerInput,
        sr_watchparty::model::HeartbeatInput,
        sr_watchparty::model::HeartbeatResult,
        sr_watchparty::model::TimeView,
        sr_watchparty::model::ReactionInput,
        sr_watchparty::model::MessageInput,
        sr_watchparty::model::AuditEntry,
        handlers::VersionConflictBody,
    ))
)]
pub struct WatchPartyApiDoc;

/// The OpenAPI document fragment for the Watch Party endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    WatchPartyApiDoc::openapi()
}
