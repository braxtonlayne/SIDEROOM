//! Library REST API: the member-facing catalog (`/library/*`) and the publisher
//! endpoints (`/admin/library/*`). LIB-DATA-002..006.
//!
//! * Members read published series. Age-restricted series are visible to everyone
//!   (D-184); `age=hide|only` narrows a listing and the user setting
//!   `library.hide_age_restricted` flips the default to `hide`. Opening the pages of
//!   a restricted series (`GET /library/chapters/{id}/pages`) answers 403
//!   `AGE_CONFIRMATION_REQUIRED` (code 900011) until the account answered yes to
//!   "Are you 18 or older?" through `POST /sr/age-gate` (D-032); the answer is permanent.
//! * Publishers are staff sessions holding `PUBLISH_READING` (create, edit, publish)
//!   or `DELETE_CONTENT` (unpublish, delete, merge). Every change is also written to
//!   the staff access log.
//! * Page and cover URLs are signed `/cdn/library/...` links.
//!
//! Wiring, in the composition root:
//!
//! ```ignore
//! let library = LibraryService::new(Arc::new(PgLibraryRepo::new(pool)), cdn_signer, clock, ids)
//!     .with_index(Arc::new(SeriesIndex::open_in_dir(&dir)?));
//! library.reindex_all().await?;
//! let deps = LibraryDeps::new(library).with_age_gate(Arc::new(PgAgeGate::new(pool)));
//! let state = AppState::builder().extension(Arc::new(deps)).build();
//! ```
//! Publishers additionally need [`AdminDeps`](crate::routes::admin::AdminDeps).

mod admin;
mod comments;
mod member;
mod reading;
mod social;
mod uploads;

#[cfg(test)]
mod tests;

use crate::{
    auth::UserOnly,
    error::HttpError,
    openapi::ErrorBody,
    routes::{
        Ext,
        admin::{AdminDeps, StaffCtx},
        search::{AgeGate, NoAgeConfirmed},
    },
    state::AppState,
};
use axum::{
    Router,
    extract::FromRequestParts,
    http::request::Parts,
    routing::{delete, get, patch, post, put},
};
use sr_core::{ApiError, ErrorCode, UserId};
use sr_library::{Actor, LibraryError, LibraryService, Viewer};
use std::sync::Arc;
use utoipa::OpenApi;

/// Per-user Library preferences, read from the user settings blob.
#[async_trait::async_trait]
pub trait LibraryPrefs: Send + Sync {
    /// `library.hide_age_restricted` (default false): leave restricted series out of
    /// listings, search and follows unless a listing asks for them (D-184).
    async fn hide_age_restricted(&self, user: UserId) -> bool;
}

/// Nobody hides restricted series (tests, tools).
pub struct NoLibraryPrefs;

#[async_trait::async_trait]
impl LibraryPrefs for NoLibraryPrefs {
    async fn hide_age_restricted(&self, _: UserId) -> bool {
        false
    }
}

/// Collaborators of the Library endpoints.
pub struct LibraryDeps {
    pub service: LibraryService,
    age: Arc<dyn AgeGate>,
    prefs: Arc<dyn LibraryPrefs>,
    /// Ratings, reviews, lists, rankings (`sr-library-social`); the routes answer 500 without it.
    pub social: Option<sr_library_social::LibrarySocial>,
}

impl LibraryDeps {
    pub fn new(service: LibraryService) -> Self {
        Self {
            service,
            age: Arc::new(NoAgeConfirmed),
            prefs: Arc::new(NoLibraryPrefs),
            social: None,
        }
    }

    /// The social backend (LIB-SOC-001/003/006, LIB-BRW-002).
    pub fn with_social(mut self, social: sr_library_social::LibrarySocial) -> Self {
        self.social = Some(social);
        self
    }

    /// Source of the per-user preferences (`library.hide_age_restricted`).
    pub fn with_prefs(mut self, prefs: Arc<dyn LibraryPrefs>) -> Self {
        self.prefs = prefs;
        self
    }

    /// Source of the 18+ confirmation (`accounts.age_confirmed_at`).
    pub fn with_age_gate(mut self, age: Arc<dyn AgeGate>) -> Self {
        self.age = age;
        self
    }
}

/// A signed-in member reading the catalog.
pub(crate) struct MemberCtx {
    pub deps: Arc<LibraryDeps>,
    pub viewer: Viewer,
}

impl FromRequestParts<AppState> for MemberCtx {
    type Rejection = HttpError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let UserOnly(p) = UserOnly::from_request_parts(parts, state).await?;
        let Ext(deps) = Ext::<LibraryDeps>::from_request_parts(parts, state).await?;
        let user = UserId::new(u64::try_from(p.user_id).unwrap_or(0));
        let adult = deps.age.age_confirmed(user).await;
        let hide_restricted = deps.prefs.hide_age_restricted(user).await;
        Ok(Self {
            viewer: Viewer {
                user_id: p.user_id,
                adult,
                hide_restricted,
            },
            deps,
        })
    }
}

/// A staff session, with the library service next to it.
pub(crate) struct PublisherCtx {
    pub staff: StaffCtx,
    pub library: Arc<LibraryDeps>,
}

impl PublisherCtx {
    pub fn svc(&self) -> &LibraryService {
        &self.library.service
    }

    pub fn actor(&self) -> Actor {
        Actor {
            user_id: self.staff.caller.user_id,
            permissions: self.staff.caller.permissions,
        }
    }

    /// Record a successful change in the staff access log. The change has already
    /// happened, so a logging failure is reported but does not fail the request.
    pub async fn audit(&self, action: &str, target: (&str, i64), detail: serde_json::Value) {
        let admin: &AdminDeps = &self.staff.deps;
        if let Err(e) = admin
            .service
            .record_action(&self.staff.caller, action, Some(target), detail)
            .await
        {
            tracing::error!(error = %e, action, "library change was not written to the access log");
        }
    }
}

impl FromRequestParts<AppState> for PublisherCtx {
    type Rejection = HttpError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let staff = StaffCtx::from_request_parts(parts, state).await?;
        let Ext(library) = Ext::<LibraryDeps>::from_request_parts(parts, state).await?;
        Ok(Self { staff, library })
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

pub(crate) fn map_err(e: LibraryError) -> HttpError {
    match e {
        LibraryError::NotFound(w) => not_found(w),
        LibraryError::AgeConfirmationRequired => ErrorCode::SrAgeConfirmationRequired.into(),
        LibraryError::MissingPermission(p) => ApiError::with_message(
            ErrorCode::MissingPermissions,
            format!("Missing platform permission {p}"),
        )
        .into(),
        LibraryError::Invalid(m) => ApiError::with_message(ErrorCode::InvalidFormBody, m).into(),
        LibraryError::Conflict(m) => ApiError::with_message(ErrorCode::SrConflict, m).into(),
        LibraryError::Storage(m) | LibraryError::Search(m) => {
            tracing::error!(error = %m, "library failure");
            ErrorCode::SrInternal.into()
        }
    }
}

/// Split a comma-separated query value.
pub(crate) fn csv(raw: &Option<String>) -> Vec<String> {
    raw.iter()
        .flat_map(|s| s.split(','))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Delivers `SR_LIBRARY_UPDATE` to member sessions over the event bus.
pub struct BusLibraryEvents {
    bus: Arc<dyn sr_events::EventBus>,
}

impl BusLibraryEvents {
    pub fn new(bus: Arc<dyn sr_events::EventBus>) -> Self {
        Self { bus }
    }
}

#[async_trait::async_trait]
impl sr_library::LibraryEvents for BusLibraryEvents {
    async fn emit(&self, users: Vec<i64>, payload: serde_json::Value) {
        let users: Vec<UserId> = users
            .into_iter()
            .filter_map(|u| u64::try_from(u).ok().map(UserId::new))
            .collect();
        let ev = sr_events::DomainEvent::UserScoped {
            users: users.clone(),
            name: sr_library::reading::EVENT_NAME.to_owned(),
            payload,
        };
        for u in users {
            if let Err(e) = self
                .bus
                .publish(sr_events::Topic::User(u), ev.clone())
                .await
            {
                tracing::warn!(error = %e, "publishing SR_LIBRARY_UPDATE failed");
            }
        }
    }
}

/// Library routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        // Members.
        .route("/library/series", get(member::browse))
        .route("/library/series/{series_id}", get(member::series))
        .route(
            "/library/series/{series_id}/chapters",
            get(member::chapters),
        )
        .route(
            "/library/series/{series_id}/follow",
            put(member::follow).delete(member::unfollow),
        )
        .route("/library/chapters/{chapter_id}/pages", get(member::pages))
        .route("/library/search", get(member::search))
        .route("/library/updates", get(member::updates))
        .route("/library/me/follows", get(member::my_follows))
        // Reading state.
        .route(
            "/library/chapters/{chapter_id}/progress",
            get(reading::get_progress)
                .put(reading::put_progress)
                .delete(reading::delete_progress),
        )
        .route(
            "/library/chapters/{chapter_id}/read",
            post(reading::mark_read),
        )
        .route(
            "/library/chapters/{chapter_id}/bookmarks",
            get(reading::chapter_bookmarks).put(reading::put_bookmark),
        )
        .route(
            "/library/bookmarks/{bookmark_id}",
            delete(reading::delete_bookmark),
        )
        // Social: ratings, reviews, lists, rankings, unread counts.
        .route(
            "/library/series/{series_id}/ratings",
            get(social::series_ratings),
        )
        .route(
            "/library/series/{series_id}/rating",
            put(social::rate_series).delete(social::unrate_series),
        )
        .route(
            "/library/chapters/{chapter_id}/rating",
            get(social::chapter_rating)
                .put(social::rate_chapter)
                .delete(social::unrate_chapter),
        )
        .route("/library/series/{series_id}/reviews", get(social::reviews))
        .route(
            "/library/series/{series_id}/review",
            put(social::put_review).delete(social::delete_review),
        )
        .route(
            "/library/reviews/{review_id}/helpful",
            put(social::helpful).delete(social::unhelpful),
        )
        .route(
            "/library/series/{series_id}/comments",
            get(comments::series_comments).post(comments::post_series_comment),
        )
        .route(
            "/library/chapters/{chapter_id}/comments",
            get(comments::chapter_comments).post(comments::post_chapter_comment),
        )
        .route(
            "/library/comments/{comment_id}",
            patch(comments::edit).delete(comments::delete),
        )
        .route(
            "/library/comments/{comment_id}/replies",
            get(comments::replies),
        )
        .route(
            "/library/comments/{comment_id}/like",
            put(comments::like).delete(comments::unlike),
        )
        .route(
            "/library/comments/{comment_id}/report",
            post(comments::report),
        )
        .route("/library/me/lists", get(social::my_lists))
        .route("/library/lists", post(social::create_list))
        .route("/library/lists/featured", get(social::featured_lists))
        .route(
            "/library/lists/{list_id}",
            get(social::get_list)
                .patch(social::update_list)
                .delete(social::delete_list),
        )
        .route(
            "/library/lists/{list_id}/items/{series_id}",
            put(social::add_item).delete(social::remove_item),
        )
        .route("/library/lists/{list_id}/order", put(social::reorder))
        .route("/library/users/{user_id}/lists", get(social::user_lists))
        .route("/library/rankings", get(social::rankings))
        .route("/library/me/unread", get(social::unread_counts))
        .route("/library/me/progress", get(reading::history))
        .route("/library/me/bookmarks", get(reading::my_bookmarks))
        // Publishers.
        .route(
            "/admin/library/lists/{list_id}/feature",
            post(social::feature_list),
        )
        .route(
            "/admin/library/series",
            get(admin::list).post(admin::create_series),
        )
        .route(
            "/admin/library/series/{series_id}",
            get(admin::series)
                .put(admin::update_series)
                .delete(admin::delete_series),
        )
        .route(
            "/admin/library/series/{series_id}/publish",
            post(admin::publish_series),
        )
        .route(
            "/admin/library/series/{series_id}/unpublish",
            post(admin::unpublish_series),
        )
        .route(
            "/admin/library/series/{series_id}/merge",
            post(admin::merge_series),
        )
        .route(
            "/admin/library/series/{series_id}/chapters",
            get(admin::chapters),
        )
        .route(
            "/admin/library/series/{series_id}/editions",
            post(admin::create_edition),
        )
        .route(
            "/admin/library/editions/{edition_id}",
            put(admin::update_edition).delete(admin::delete_edition),
        )
        .route(
            "/admin/library/editions/{edition_id}/volumes",
            post(admin::create_volume),
        )
        .route(
            "/admin/library/editions/{edition_id}/chapters",
            post(admin::create_chapter),
        )
        .route(
            "/admin/library/volumes/{volume_id}",
            delete(admin::delete_volume),
        )
        .route(
            "/admin/library/chapters/{chapter_id}",
            put(admin::update_chapter).delete(admin::delete_chapter),
        )
        .route(
            "/admin/library/chapters/{chapter_id}/publish",
            post(admin::publish_chapter),
        )
        .route(
            "/admin/library/chapters/{chapter_id}/unpublish",
            post(admin::unpublish_chapter),
        )
        .route(
            "/admin/library/chapters/{chapter_id}/pages",
            get(admin::pages).put(admin::replace_pages),
        )
        .route(
            "/admin/library/chapters/{chapter_id}/uploads",
            get(uploads::list).post(uploads::begin),
        )
        .route("/admin/library/comment-reports", get(comments::reports))
        .route(
            "/admin/library/comment-reports/{report_id}/resolve",
            post(comments::resolve),
        )
        .route("/admin/library/uploads/{upload_id}", get(uploads::status))
        .route(
            "/admin/library/uploads/{upload_id}/complete",
            post(uploads::complete),
        )
}

/// OpenAPI fragment for the Library endpoints.
#[derive(OpenApi)]
#[openapi(
    paths(
        member::browse,
        member::series,
        member::chapters,
        member::follow,
        member::unfollow,
        member::pages,
        member::search,
        member::updates,
        member::my_follows,
        reading::get_progress,
        reading::put_progress,
        reading::delete_progress,
        reading::mark_read,
        reading::history,
        reading::chapter_bookmarks,
        reading::put_bookmark,
        reading::delete_bookmark,
        reading::my_bookmarks,
        social::series_ratings,
        social::rate_series,
        social::unrate_series,
        social::chapter_rating,
        social::rate_chapter,
        social::unrate_chapter,
        social::reviews,
        social::put_review,
        social::delete_review,
        social::helpful,
        social::unhelpful,
        social::my_lists,
        social::create_list,
        social::featured_lists,
        social::get_list,
        social::update_list,
        social::delete_list,
        social::add_item,
        social::remove_item,
        social::reorder,
        social::user_lists,
        social::rankings,
        social::unread_counts,
        social::feature_list,
        admin::list,
        admin::create_series,
        admin::series,
        admin::update_series,
        admin::delete_series,
        admin::publish_series,
        admin::unpublish_series,
        admin::merge_series,
        admin::chapters,
        admin::create_edition,
        admin::update_edition,
        admin::delete_edition,
        admin::create_volume,
        admin::delete_volume,
        admin::create_chapter,
        admin::update_chapter,
        admin::delete_chapter,
        admin::publish_chapter,
        admin::unpublish_chapter,
        admin::pages,
        admin::replace_pages,
        uploads::begin,
        uploads::complete,
        uploads::status,
        uploads::list,
        comments::series_comments,
        comments::post_series_comment,
        comments::chapter_comments,
        comments::post_chapter_comment,
        comments::replies,
        comments::edit,
        comments::delete,
        comments::like,
        comments::unlike,
        comments::report,
        comments::reports,
        comments::resolve,
    ),
    components(schemas(
        ErrorBody,
        member::FollowBody,
        admin::MergeBody,
        sr_library::model::SeriesKind,
        sr_library::model::SeriesStatus,
        sr_library::model::ReadingDirection,
        sr_library::model::ChapterFormat,
        sr_library::model::Shelf,
        sr_library::model::SortKey,
        sr_library::model::AgeFilter,
        sr_library::model::SeriesInput,
        sr_library::model::Series,
        sr_library::model::SeriesPage,
        sr_library::model::SeriesDetail,
        sr_library::model::EditionInput,
        sr_library::model::Edition,
        sr_library::model::VolumeInput,
        sr_library::model::Volume,
        sr_library::model::ChapterInput,
        sr_library::model::Chapter,
        sr_library::model::ChapterProgress,
        sr_library::model::ChapterList,
        sr_library::model::PageInput,
        sr_library::model::PageVariant,
        sr_library::model::PageEntry,
        sr_library::model::PageManifest,
        sr_library::model::Follow,
        sr_library::model::FollowedSeries,
        sr_library::model::FollowPage,
        sr_library::model::UpdateItem,
        sr_library::model::UpdatesPage,
        sr_library::model::InstantSearch,
        sr_library::model::UploadKind,
        sr_library::model::UploadStatus,
        sr_library::model::Upload,
        sr_library::model::UploadRequest,
        sr_library::model::UploadSlot,
        sr_library::model::UploadList,
        sr_library::reading::ProgressInput,
        sr_library::reading::Progress,
        sr_library::reading::ProgressSaved,
        sr_library::reading::ReadMarkInput,
        sr_library::reading::ReadMarkResult,
        sr_library::reading::HistoryItem,
        sr_library::reading::HistoryPage,
        sr_library::reading::BookmarkInput,
        sr_library::reading::Bookmark,
        sr_library::reading::BookmarkList,
        sr_library::reading::BookmarkEntry,
        sr_library::reading::BookmarkPage,
        social::CountBody,
        comments::ReportBody,
        comments::ReportList,
        comments::ResolveBody,
        sr_library_social::model::Comment,
        sr_library_social::model::CommentPage,
        sr_library_social::model::CommentInput,
        sr_library_social::model::CommentPatch,
        sr_library_social::model::CommentReport,
        social::ListCard,
        social::ListsBody,
        social::ListItemView,
        social::ListDetail,
        social::FeatureBody,
        social::RankingEntry,
        social::RankingsBody,
        sr_library_social::model::RatingAggregate,
        sr_library_social::model::SeriesRatings,
        sr_library_social::model::ChapterRating,
        sr_library_social::model::ScoreInput,
        sr_library_social::model::Review,
        sr_library_social::model::ReviewPage,
        sr_library_social::model::ReviewInput,
        sr_library_social::model::ReviewSort,
        sr_library_social::model::Visibility,
        sr_library_social::model::ReadingList,
        sr_library_social::model::ListEntry,
        sr_library_social::model::ListInput,
        sr_library_social::model::ListPatch,
        sr_library_social::model::ListItemInput,
        sr_library_social::model::ListOrderInput,
        sr_library_social::model::RankingPeriod,
        sr_library_social::model::RankingScope,
        sr_library_social::model::UnreadCount,
        sr_library_social::model::UnreadCounts,
    ))
)]
pub struct LibraryApiDoc;

/// The OpenAPI document fragment for the Library endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    LibraryApiDoc::openapi()
}
