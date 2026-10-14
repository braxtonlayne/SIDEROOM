//! Cinema social and progress API (`/cinema/titles/{id}/progress`, `/cinema/me/*`, `/cinema/lists/*`,
//! ratings, reviews, comments, trending): CIN-PLY-007/009, CIN-MY-002, CIN-SOC-001/002.
//! The contract is `docs/specs/cinema-media.md` section 8. The implementation lives in `sr-cinema-social`;
//! this module is the HTTP layer.
//!
//! Wiring, in the composition root:
//!
//! ```ignore
//! let service = CinemaService::new(pool, catalog, ids).with_events(Arc::new(BusEvents::new(bus)));
//! let deps = CinemaSocialDeps::new(service).with_age_gate(Arc::new(PgAgeGate::new(pool)));
//! let state = AppState::builder().extension(Arc::new(deps)).build();
//! ```

mod discover;
mod lists;
mod moderation;
#[cfg(test)]
mod moderation_tests;
mod progress;
mod social;

#[cfg(test)]
mod tests;

use crate::{
    auth::UserOnly,
    error::HttpError,
    openapi::ErrorBody,
    routes::{
        Ext,
        search::{AgeGate, NoAgeConfirmed},
    },
    state::AppState,
};
use axum::{
    Router,
    extract::FromRequestParts,
    http::request::Parts,
    routing::{delete, get, post, put},
};
use serde::Serialize;
use sr_cinema_social::{CinemaService, SocialError, Viewer};
use sr_core::{ApiError, ErrorCode, UserId};
use std::sync::Arc;
use utoipa::{OpenApi, ToSchema};

/// Collaborators of the Cinema social endpoints.
pub struct CinemaSocialDeps {
    pub service: CinemaService,
    age: Arc<dyn AgeGate>,
}

impl CinemaSocialDeps {
    pub fn new(service: CinemaService) -> Self {
        Self {
            service,
            age: Arc::new(NoAgeConfirmed),
        }
    }

    /// Source of the 18+ confirmation (`accounts.age_confirmed_at`).
    pub fn with_age_gate(mut self, age: Arc<dyn AgeGate>) -> Self {
        self.age = age;
        self
    }
}

/// A signed-in member using the Cinema.
pub(crate) struct CinemaCtx {
    pub deps: Arc<CinemaSocialDeps>,
    pub viewer: Viewer,
}

impl CinemaCtx {
    pub fn svc(&self) -> &CinemaService {
        &self.deps.service
    }
}

impl FromRequestParts<AppState> for CinemaCtx {
    type Rejection = HttpError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let UserOnly(p) = UserOnly::from_request_parts(parts, state).await?;
        let Ext(deps) = Ext::<CinemaSocialDeps>::from_request_parts(parts, state).await?;
        let user = UserId::new(u64::try_from(p.user_id).unwrap_or(0));
        let adult = deps.age.age_confirmed(user).await;
        Ok(Self {
            viewer: Viewer {
                user_id: p.user_id,
                adult,
            },
            deps,
        })
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

pub(crate) fn map_err(e: SocialError) -> HttpError {
    match e {
        SocialError::NotFound(w) => not_found(w),
        SocialError::AgeConfirmationRequired => ErrorCode::SrAgeConfirmationRequired.into(),
        SocialError::Invalid(m) => ApiError::with_message(ErrorCode::InvalidFormBody, m).into(),
        SocialError::Conflict(m) => ApiError::with_message(ErrorCode::SrConflict, m).into(),
        SocialError::Storage(m) => {
            tracing::error!(error = %m, "cinema social failure");
            ErrorCode::SrInternal.into()
        }
    }
}

/// A count after a vote.
#[derive(Debug, Serialize, ToSchema)]
pub struct CountBody {
    pub count: i32,
}

/// Cinema social routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        // Progress, watched marks.
        .route(
            "/cinema/titles/{title_id}/progress",
            get(progress::get_movie_progress)
                .put(progress::put_movie_progress)
                .delete(progress::delete_movie_progress),
        )
        .route(
            "/cinema/titles/{title_id}/episodes/{episode_id}/progress",
            get(progress::get_episode_progress)
                .put(progress::put_episode_progress)
                .delete(progress::delete_episode_progress),
        )
        .route(
            "/cinema/titles/{title_id}/watch-state",
            get(progress::watch_state),
        )
        .route("/cinema/titles/{title_id}/watched", post(progress::watched))
        // History, continue watching, presence.
        .route(
            "/cinema/me/history",
            get(progress::history).delete(progress::clear_history),
        )
        .route(
            "/cinema/me/history/settings",
            get(progress::get_history_settings).put(progress::put_history_settings),
        )
        .route(
            "/cinema/me/continue-watching",
            get(progress::continue_watching),
        )
        .route("/cinema/me/watching", delete(progress::stop_watching))
        .route(
            "/cinema/me/friends-watching",
            get(progress::friends_watching),
        )
        // Lists.
        .route("/cinema/me/list", get(lists::my_list))
        .route(
            "/cinema/me/list/{title_id}",
            put(lists::save_title).delete(lists::unsave_title),
        )
        .route("/cinema/lists", get(lists::my_lists).post(lists::create))
        .route(
            "/cinema/lists/{list_id}",
            get(lists::get)
                .patch(lists::update)
                .delete(lists::delete_list),
        )
        .route(
            "/cinema/lists/{list_id}/items/{title_id}",
            put(lists::add_item).delete(lists::remove_item),
        )
        .route("/cinema/lists/{list_id}/order", put(lists::reorder))
        .route("/cinema/users/{user_id}/lists", get(lists::user_lists))
        // Ratings.
        .route(
            "/cinema/titles/{title_id}/ratings",
            get(social::title_ratings),
        )
        .route(
            "/cinema/titles/{title_id}/rating",
            put(social::rate_title).delete(social::unrate_title),
        )
        .route(
            "/cinema/titles/{title_id}/episodes/{episode_id}/ratings",
            get(social::episode_ratings),
        )
        .route(
            "/cinema/titles/{title_id}/episodes/{episode_id}/rating",
            put(social::rate_episode).delete(social::unrate_episode),
        )
        // Reviews.
        .route("/cinema/titles/{title_id}/reviews", get(social::reviews))
        .route(
            "/cinema/titles/{title_id}/review",
            put(social::put_review).delete(social::delete_review),
        )
        .route(
            "/cinema/reviews/{review_id}/helpful",
            put(social::helpful).delete(social::unhelpful),
        )
        .route(
            "/cinema/reviews/{review_id}/report",
            post(social::report_review),
        )
        // Comments.
        .route(
            "/cinema/titles/{title_id}/comments",
            get(social::title_comments).post(social::post_title_comment),
        )
        .route(
            "/cinema/titles/{title_id}/episodes/{episode_id}/comments",
            get(social::episode_comments).post(social::post_episode_comment),
        )
        .route(
            "/cinema/comments/{comment_id}/replies",
            get(social::replies),
        )
        .route(
            "/cinema/comments/{comment_id}",
            axum::routing::patch(social::edit_comment).delete(social::delete_comment),
        )
        .route(
            "/cinema/comments/{comment_id}/like",
            put(social::like).delete(social::unlike),
        )
        .route(
            "/cinema/comments/{comment_id}/report",
            post(social::report_comment),
        )
        // Discovery.
        .route("/cinema/trending", get(discover::trending))
        .route("/cinema/me/recommendations", get(discover::recommendations))
        // Staff moderation (DELETE_CONTENT).
        .route("/admin/cinema/reports", get(moderation::reports))
        .route(
            "/admin/cinema/reports/{report_id}/resolve",
            post(moderation::resolve_report),
        )
        .route(
            "/admin/cinema/reviews/{review_id}/hidden",
            put(moderation::hide_review),
        )
        .route(
            "/admin/cinema/comments/{comment_id}/hidden",
            put(moderation::hide_comment),
        )
}

/// OpenAPI fragment for the Cinema social endpoints.
#[derive(OpenApi)]
#[openapi(
    paths(
        progress::get_movie_progress,
        progress::put_movie_progress,
        progress::delete_movie_progress,
        progress::get_episode_progress,
        progress::put_episode_progress,
        progress::delete_episode_progress,
        progress::watch_state,
        progress::watched,
        progress::history,
        progress::clear_history,
        progress::get_history_settings,
        progress::put_history_settings,
        progress::continue_watching,
        progress::stop_watching,
        progress::friends_watching,
        lists::my_list,
        lists::save_title,
        lists::unsave_title,
        lists::my_lists,
        lists::create,
        lists::get,
        lists::update,
        lists::delete_list,
        lists::add_item,
        lists::remove_item,
        lists::reorder,
        lists::user_lists,
        social::title_ratings,
        social::rate_title,
        social::unrate_title,
        social::episode_ratings,
        social::rate_episode,
        social::unrate_episode,
        social::reviews,
        social::put_review,
        social::delete_review,
        social::helpful,
        social::unhelpful,
        social::report_review,
        social::title_comments,
        social::post_title_comment,
        social::episode_comments,
        social::post_episode_comment,
        social::replies,
        social::edit_comment,
        social::delete_comment,
        social::like,
        social::unlike,
        social::report_comment,
        discover::trending,
        discover::recommendations,
        moderation::reports,
        moderation::resolve_report,
        moderation::hide_review,
        moderation::hide_comment,
    ),
    components(schemas(
        ErrorBody,
        CountBody,
        sr_cinema_social::TitleKind,
        sr_cinema_social::TitleRef,
        sr_cinema_social::EpisodeRef,
        sr_cinema_social::model::ProgressInput,
        sr_cinema_social::model::Progress,
        sr_cinema_social::model::ProgressSaved,
        sr_cinema_social::model::WatchState,
        sr_cinema_social::model::WatchedInput,
        sr_cinema_social::model::WatchedResult,
        sr_cinema_social::model::HistoryItem,
        sr_cinema_social::model::HistoryPage,
        sr_cinema_social::model::HistorySettings,
        sr_cinema_social::model::ContinueReason,
        sr_cinema_social::model::ContinueItem,
        sr_cinema_social::model::ContinueWatching,
        sr_cinema_social::model::FriendWatching,
        sr_cinema_social::model::FriendsWatching,
        sr_cinema_social::model::Visibility,
        sr_cinema_social::model::ListKind,
        sr_cinema_social::model::CinemaList,
        sr_cinema_social::model::ListItem,
        sr_cinema_social::model::ListItemsPage,
        sr_cinema_social::model::ListDetail,
        sr_cinema_social::model::ListsResponse,
        sr_cinema_social::model::ListInput,
        sr_cinema_social::model::ListPatch,
        sr_cinema_social::model::ListItemInput,
        sr_cinema_social::model::ListOrderInput,
        sr_cinema_social::model::RatingInput,
        sr_cinema_social::model::RatingAggregate,
        sr_cinema_social::model::Ratings,
        sr_cinema_social::model::ReviewInput,
        sr_cinema_social::model::Review,
        sr_cinema_social::model::ReviewPage,
        sr_cinema_social::model::ReviewSort,
        sr_cinema_social::model::CommentInput,
        sr_cinema_social::model::CommentPatch,
        sr_cinema_social::model::Comment,
        sr_cinema_social::model::CommentPage,
        sr_cinema_social::model::ReportInput,
        sr_cinema_social::model::TrendScope,
        sr_cinema_social::model::TrendingItem,
        sr_cinema_social::model::Trending,
        sr_cinema_social::model::Recommendation,
        sr_cinema_social::model::RecommendationRow,
        sr_cinema_social::model::Recommendations,
        sr_cinema_social::moderation::ReportStatus,
        sr_cinema_social::moderation::ReportedContent,
        sr_cinema_social::moderation::Report,
        sr_cinema_social::moderation::ReportPage,
        moderation::ResolveBody,
        moderation::HiddenBody,
    ))
)]
pub struct CinemaSocialApiDoc;

/// The OpenAPI document fragment for the Cinema social endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    CinemaSocialApiDoc::openapi()
}
