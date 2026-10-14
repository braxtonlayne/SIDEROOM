//! Cinema catalog REST API: the member-facing catalog (`/cinema/*`) and the publisher
//! endpoints (`/admin/cinema/*`). CIN-DATA-002..006. The shapes are documented in
//! docs/specs/cinema-media.md section 8.
//!
//! * Members read published titles. Age-restricted titles are visible to everyone (D-184);
//!   `age=hide|only` narrows a listing and the user setting `cinema.hide_age_restricted`
//!   flips the default to `hide`. Opening a restricted title (`/play`) answers 403
//!   `AGE_CONFIRMATION_REQUIRED` (code 900011) until the account answered yes to
//!   "Are you 18 or older?" through `POST /sr/age-gate` (D-032); the answer is permanent.
//! * Publishers are staff sessions holding `PUBLISH_MEDIA` (create, edit, publish),
//!   `DELETE_CONTENT` (delete, merge) or `MANAGE_COLLECTIONS` (curated rows). Every change is
//!   also written to the staff access log.
//! * Artwork URLs are signed `/cdn/cinema/...` links.
//!
//! Wiring, in the composition root:
//!
//! ```ignore
//! let cinema = CinemaService::new(Arc::new(PgCinemaRepo::new(pool)), cdn_signer, clock, ids)
//!     .with_index(Arc::new(TitleIndex::open_in_dir(&dir)?));
//! cinema.reindex_all().await?;
//! let deps = CinemaDeps::new(cinema).with_age_gate(Arc::new(PgAgeGate::new(pool)));
//! let state = AppState::builder().extension(Arc::new(deps)).build();
//! ```
//! Publishers additionally need [`AdminDeps`](crate::routes::admin::AdminDeps).

mod admin;
mod member;
mod playback;

pub use playback::{
    AssetPlayback, AudioTrack, PlaybackDeps, PlaybackGrant, SpriteInfo, SubtitleTrack,
};

#[cfg(test)]
mod playback_tests;
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
    routing::{get, post, put},
};
use sr_cinema::{Actor, CinemaError, CinemaService, Viewer};
use sr_core::{ApiError, ErrorCode, UserId};
use std::sync::Arc;
use utoipa::OpenApi;

/// Per-user Cinema preferences, read from the user settings blob.
#[async_trait::async_trait]
pub trait CinemaPrefs: Send + Sync {
    /// `cinema.hide_age_restricted` (default false): leave restricted titles out of listings,
    /// rows and search unless a listing asks for them (D-184).
    async fn hide_age_restricted(&self, user: UserId) -> bool;
}

/// Nobody hides restricted titles (tests, tools).
pub struct NoCinemaPrefs;

#[async_trait::async_trait]
impl CinemaPrefs for NoCinemaPrefs {
    async fn hide_age_restricted(&self, _: UserId) -> bool {
        false
    }
}

/// Collaborators of the Cinema endpoints.
pub struct CinemaDeps {
    pub service: CinemaService,
    playback: Option<Arc<playback::PlaybackDeps>>,
    age: Arc<dyn AgeGate>,
    prefs: Arc<dyn CinemaPrefs>,
}

impl CinemaDeps {
    pub fn new(service: CinemaService) -> Self {
        Self {
            service,
            playback: None,
            age: Arc::new(NoAgeConfirmed),
            prefs: Arc::new(NoCinemaPrefs),
        }
    }

    /// Renditions and the signer for `/cinema/assets/{id}/playback` and `refresh`. Register the same value on
    /// the CDN (`CdnDeps::with_playlists`) so the playlists it names can be fetched.
    pub fn with_playback(mut self, playback: Arc<playback::PlaybackDeps>) -> Self {
        self.playback = Some(playback);
        self
    }

    /// Source of the per-user preferences (`cinema.hide_age_restricted`).
    pub fn with_prefs(mut self, prefs: Arc<dyn CinemaPrefs>) -> Self {
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
    pub deps: Arc<CinemaDeps>,
    pub viewer: Viewer,
}

impl FromRequestParts<AppState> for MemberCtx {
    type Rejection = HttpError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let UserOnly(p) = UserOnly::from_request_parts(parts, state).await?;
        let Ext(deps) = Ext::<CinemaDeps>::from_request_parts(parts, state).await?;
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

/// A staff session, with the cinema service next to it.
pub(crate) struct PublisherCtx {
    pub staff: StaffCtx,
    pub cinema: Arc<CinemaDeps>,
}

impl PublisherCtx {
    pub fn svc(&self) -> &CinemaService {
        &self.cinema.service
    }

    pub fn actor(&self) -> Actor {
        Actor {
            user_id: self.staff.caller.user_id,
            permissions: self.staff.caller.permissions,
        }
    }

    /// Record a successful change in the staff access log. The change has already happened,
    /// so a logging failure is reported but does not fail the request.
    pub async fn audit(&self, action: &str, target: (&str, i64), detail: serde_json::Value) {
        let admin: &AdminDeps = &self.staff.deps;
        if let Err(e) = admin
            .service
            .record_action(&self.staff.caller, action, Some(target), detail)
            .await
        {
            tracing::error!(error = %e, action, "cinema change was not written to the access log");
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
        let Ext(cinema) = Ext::<CinemaDeps>::from_request_parts(parts, state).await?;
        Ok(Self { staff, cinema })
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

pub(crate) fn map_err(e: CinemaError) -> HttpError {
    match e {
        CinemaError::NotFound(w) => not_found(w),
        CinemaError::AgeConfirmationRequired => ErrorCode::SrAgeConfirmationRequired.into(),
        CinemaError::MissingPermission(p) => ApiError::with_message(
            ErrorCode::MissingPermissions,
            format!("Missing platform permission {p}"),
        )
        .into(),
        CinemaError::Invalid(m) => ApiError::with_message(ErrorCode::InvalidFormBody, m).into(),
        CinemaError::Conflict(m) => ApiError::with_message(ErrorCode::SrConflict, m).into(),
        CinemaError::Storage(m) | CinemaError::Search(m) => {
            tracing::error!(error = %m, "cinema failure");
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

/// Cinema routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        // Members.
        .route("/cinema/discover", get(member::discover))
        .route("/cinema/rows/{key}", get(member::row))
        .route("/cinema/collections", get(member::collections))
        .route("/cinema/titles", get(member::browse))
        .route("/cinema/facets", get(member::facets))
        .route("/cinema/titles/{title_id}", get(member::title))
        .route("/cinema/titles/{title_id}/episodes", get(member::episodes))
        .route(
            "/cinema/titles/{title_id}/markers",
            get(member::title_markers),
        )
        .route("/cinema/titles/{title_id}/play", get(member::play_title))
        .route("/cinema/episodes/{episode_id}", get(member::episode))
        .route(
            "/cinema/episodes/{episode_id}/markers",
            get(member::episode_markers),
        )
        .route(
            "/cinema/episodes/{episode_id}/play",
            get(member::play_episode),
        )
        .route("/cinema/search", get(member::search))
        .route(
            "/cinema/assets/{asset_id}/playback",
            get(playback::asset_playback),
        )
        .route(
            "/cinema/assets/{asset_id}/refresh",
            post(playback::asset_refresh),
        )
        // Publishers: titles.
        .route(
            "/admin/cinema/titles",
            get(admin::list).post(admin::create_title),
        )
        .route(
            "/admin/cinema/titles/{title_id}",
            get(admin::title)
                .put(admin::update_title)
                .delete(admin::delete_title),
        )
        .route(
            "/admin/cinema/titles/{title_id}/publish",
            post(admin::publish_title),
        )
        .route(
            "/admin/cinema/titles/{title_id}/unpublish",
            post(admin::unpublish_title),
        )
        .route(
            "/admin/cinema/titles/{title_id}/merge",
            post(admin::merge_title),
        )
        .route(
            "/admin/cinema/titles/{title_id}/asset",
            put(admin::link_title_asset),
        )
        .route(
            "/admin/cinema/titles/{title_id}/artwork",
            post(admin::artwork_upload),
        )
        .route(
            "/admin/cinema/titles/{title_id}/seasons",
            post(admin::create_season),
        )
        .route(
            "/admin/cinema/titles/{title_id}/variants",
            post(admin::create_variant),
        )
        .route(
            "/admin/cinema/titles/{title_id}/trailers",
            post(admin::create_trailer),
        )
        .route(
            "/admin/cinema/titles/{title_id}/markers",
            put(admin::replace_title_markers),
        )
        // Publishers: seasons, episodes, variants, trailers.
        .route(
            "/admin/cinema/seasons/{season_id}",
            put(admin::update_season).delete(admin::delete_season),
        )
        .route(
            "/admin/cinema/seasons/{season_id}/episodes",
            post(admin::create_episode),
        )
        .route(
            "/admin/cinema/episodes/{episode_id}",
            put(admin::update_episode).delete(admin::delete_episode),
        )
        .route(
            "/admin/cinema/episodes/{episode_id}/publish",
            post(admin::publish_episode),
        )
        .route(
            "/admin/cinema/episodes/{episode_id}/unpublish",
            post(admin::unpublish_episode),
        )
        .route(
            "/admin/cinema/episodes/{episode_id}/asset",
            put(admin::link_episode_asset),
        )
        .route(
            "/admin/cinema/episodes/{episode_id}/markers",
            get(admin::episode_markers).put(admin::replace_episode_markers),
        )
        .route(
            "/admin/cinema/variants/{variant_id}",
            put(admin::update_variant).delete(admin::delete_variant),
        )
        .route(
            "/admin/cinema/trailers/{trailer_id}",
            put(admin::update_trailer).delete(admin::delete_trailer),
        )
        // Publishers: collections.
        .route(
            "/admin/cinema/collections",
            get(admin::collections).post(admin::create_collection),
        )
        .route(
            "/admin/cinema/collections/{collection_id}",
            get(admin::collection)
                .put(admin::update_collection)
                .delete(admin::delete_collection),
        )
        .route(
            "/admin/cinema/collections/{collection_id}/items",
            put(admin::set_collection_items),
        )
}

/// OpenAPI fragment for the Cinema endpoints.
#[derive(OpenApi)]
#[openapi(
    paths(
        member::discover,
        member::row,
        member::collections,
        member::browse,
        member::facets,
        member::title,
        member::episodes,
        member::title_markers,
        member::play_title,
        member::episode,
        member::episode_markers,
        member::play_episode,
        member::search,
        playback::asset_playback,
        playback::asset_refresh,
        admin::list,
        admin::create_title,
        admin::title,
        admin::update_title,
        admin::delete_title,
        admin::publish_title,
        admin::unpublish_title,
        admin::merge_title,
        admin::link_title_asset,
        admin::artwork_upload,
        admin::create_season,
        admin::update_season,
        admin::delete_season,
        admin::create_episode,
        admin::update_episode,
        admin::delete_episode,
        admin::publish_episode,
        admin::unpublish_episode,
        admin::link_episode_asset,
        admin::episode_markers,
        admin::replace_title_markers,
        admin::replace_episode_markers,
        admin::create_variant,
        admin::update_variant,
        admin::delete_variant,
        admin::create_trailer,
        admin::update_trailer,
        admin::delete_trailer,
        admin::collections,
        admin::create_collection,
        admin::collection,
        admin::update_collection,
        admin::delete_collection,
        admin::set_collection_items,
    ),
    components(schemas(
        ErrorBody,
        playback::AssetPlayback,
        playback::PlaybackGrant,
        playback::SubtitleTrack,
        playback::AudioTrack,
        playback::SpriteInfo,
        admin::MergeBody,
        admin::AssetBody,
        sr_cinema::model::TitleKind,
        sr_cinema::model::TrailerKind,
        sr_cinema::model::MarkerKind,
        sr_cinema::model::CollectionDisplay,
        sr_cinema::model::Surface,
        sr_cinema::model::SortKey,
        sr_cinema::model::AgeFilter,
        sr_cinema::model::RowKind,
        sr_cinema::model::TitleInput,
        sr_cinema::model::Title,
        sr_cinema::model::TitlePage,
        sr_cinema::model::SeasonInput,
        sr_cinema::model::Season,
        sr_cinema::model::EpisodeInput,
        sr_cinema::model::Episode,
        sr_cinema::model::EpisodeList,
        sr_cinema::model::VariantInput,
        sr_cinema::model::Variant,
        sr_cinema::model::TrailerInput,
        sr_cinema::model::Trailer,
        sr_cinema::model::MarkerInput,
        sr_cinema::model::MarkersBody,
        sr_cinema::model::Marker,
        sr_cinema::model::MarkerList,
        sr_cinema::model::TitleDetail,
        sr_cinema::model::AdminTitleDetail,
        sr_cinema::model::PlayTarget,
        sr_cinema::model::CollectionInput,
        sr_cinema::model::Collection,
        sr_cinema::model::CollectionList,
        sr_cinema::model::CollectionDetail,
        sr_cinema::model::CollectionItemsBody,
        sr_cinema::model::RowItem,
        sr_cinema::model::Row,
        sr_cinema::model::Discover,
        sr_cinema::model::FacetCount,
        sr_cinema::model::Facets,
        sr_cinema::model::InstantSearch,
        sr_cinema::model::ArtworkSlot,
        sr_cinema::model::ArtworkRequest,
        sr_cinema::model::ArtworkUpload,
    ))
)]
pub struct CinemaApiDoc;

/// The OpenAPI document fragment for the Cinema endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    CinemaApiDoc::openapi()
}
