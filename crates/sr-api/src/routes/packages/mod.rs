//! Declarative packages and customised profiles (EXT-PKG, EXT-PRF): a creator submits a package zip, staff
//! review it, people and servers turn approved packages on, and a person's profile page is a validated
//! document of blocks. The logic lives in `sr-packages`; this module is the HTTP layer.
//!
//! * creator: `POST /packages`, `GET /packages/mine`, `POST /packages/versions/{id}/withdraw`;
//! * browse: `GET /packages`, `GET /packages/{id}`, `GET /packages/versions/{id}/files/{path}`;
//! * people: `GET /users/@me/packages`, `PUT|DELETE /users/@me/packages/{id}` (themes);
//! * servers (`MANAGE_GUILD`): `GET /guilds/{id}/packages`, `PUT|DELETE .../{package}`, `POST .../{package}/apply`
//!   (recipes), `POST /packages/{id}/create-guild` (server templates);
//! * profiles: `GET|PUT /users/@me/profile-doc`, `POST /users/@me/profile-doc/layout`,
//!   `GET /profile-pages/{username}`, `GET /users/{id}/profile-page`, the guestbook under `/users/{id}/guestbook`;
//! * staff (`REVIEW_PACKAGES`): `/admin/developers/packages*`.

mod handlers;

use crate::{routes::messages::error::MsgError, state::AppState};
use axum::{
    Router,
    routing::{get, post, put},
};
use sr_core::{ApiError, ErrorCode};
use sr_packages::{PackageError, service::PackageService};
use utoipa::OpenApi;

/// Collaborators of the packages endpoints.
pub struct PackagesDeps {
    pub service: PackageService,
}

/// Publishes a package version's review-queue card event (D-258); the Staff Server card poster reads it.
pub struct BusPackageEvents {
    bus: std::sync::Arc<dyn sr_events::EventBus>,
}

impl BusPackageEvents {
    pub fn new(bus: std::sync::Arc<dyn sr_events::EventBus>) -> Self {
        Self { bus }
    }
}

#[async_trait::async_trait]
impl sr_packages::service::PackageEvents for BusPackageEvents {
    async fn card(&self, version_id: i64, outcome: Option<&str>) {
        let ev = sr_events::DomainEvent::PackageCard {
            version_id,
            outcome: outcome.map(str::to_owned),
        };
        if let Err(e) = self.bus.publish(sr_events::Topic::Platform, ev).await {
            tracing::warn!(error = %e, "publishing a package card event failed");
        }
    }
}

/// A [`PackageError`] as an API error.
pub(crate) fn pkg(e: PackageError) -> MsgError {
    let api = match e {
        PackageError::NotFound(w) => {
            ApiError::with_message(ErrorCode::SrNotFound, format!("Unknown {w}"))
        }
        PackageError::Forbidden => ApiError::new(ErrorCode::MissingPermissions),
        PackageError::SelfReview => {
            ApiError::with_message(ErrorCode::MissingPermissions, e.to_string())
        }
        PackageError::Invalid(field, m) => {
            ApiError::invalid_form().field(field, "INVALID_VALUE", m)
        }
        PackageError::Refused(problems) => {
            let mut e = ApiError::invalid_form();
            for (i, p) in problems.into_iter().take(40).enumerate() {
                e = e.field(format!("problems.{i}"), "INVALID_VALUE", p);
            }
            e
        }
        PackageError::Conflict(m) => ApiError::with_message(ErrorCode::SrConflict, m),
        PackageError::Storage(m) => {
            tracing::error!(error = %m, "packages failure");
            ApiError::new(ErrorCode::SrInternal)
        }
    };
    MsgError::Api(api)
}

/// Largest package upload.
pub const MAX_UPLOAD: usize = sr_packages::zipfiles::MAX_BYTES as usize + 16 * 1024;

/// Packages routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    use handlers as h;
    Router::new()
        .route(
            "/packages",
            get(h::directory)
                .post(h::submit)
                .layer(axum::extract::DefaultBodyLimit::max(MAX_UPLOAD)),
        )
        .route("/packages/mine", get(h::mine))
        .route(
            "/packages/{package_id}",
            get(h::get_package).delete(h::delete_package),
        )
        .route("/packages/{package_id}/create-guild", post(h::create_guild))
        .route(
            "/packages/versions/{version_id}/withdraw",
            post(h::withdraw),
        )
        .route(
            "/packages/versions/{version_id}/files/{*path}",
            get(h::version_file),
        )
        .route("/users/@me/packages", get(h::my_enabled))
        .route(
            "/users/@me/packages/{package_id}",
            put(h::enable_for_me).delete(h::disable_for_me),
        )
        .route("/guilds/{guild_id}/packages", get(h::guild_enabled))
        .route(
            "/guilds/{guild_id}/packages/{package_id}",
            put(h::enable_for_guild).delete(h::disable_for_guild),
        )
        .route(
            "/guilds/{guild_id}/packages/{package_id}/apply",
            post(h::apply_recipe),
        )
        .route("/users/@me/profile-doc", get(h::own_doc).put(h::set_doc))
        .route("/users/@me/profile-doc/layout", post(h::use_layout))
        .route("/profile-pages/{username}", get(h::page_by_name))
        .route("/users/{user_id}/profile-page", get(h::page_by_id))
        .route(
            "/users/{user_id}/guestbook",
            get(h::guestbook).post(h::sign),
        )
        .route(
            "/users/{user_id}/guestbook/{entry_id}",
            axum::routing::delete(h::remove_entry),
        )
        .route("/admin/developers/packages", get(h::admin_pending))
        .route(
            "/admin/developers/packages/versions/{version_id}/download",
            get(h::admin_download),
        )
        .route(
            "/admin/developers/packages/versions/{version_id}/decision",
            post(h::admin_decide),
        )
}

/// OpenAPI fragment for the packages endpoints.
#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::directory,
        handlers::submit,
        handlers::mine,
        handlers::get_package,
        handlers::create_guild,
        handlers::withdraw,
        handlers::delete_package,
        handlers::version_file,
        handlers::my_enabled,
        handlers::enable_for_me,
        handlers::disable_for_me,
        handlers::guild_enabled,
        handlers::enable_for_guild,
        handlers::disable_for_guild,
        handlers::apply_recipe,
        handlers::own_doc,
        handlers::set_doc,
        handlers::use_layout,
        handlers::page_by_name,
        handlers::page_by_id,
        handlers::guestbook,
        handlers::sign,
        handlers::remove_entry,
        handlers::admin_pending,
        handlers::admin_download,
        handlers::admin_decide,
    ),
    components(schemas(
        sr_packages::PackageKind,
        sr_packages::validate::FileInfo,
        sr_packages::service::VersionView,
        sr_packages::service::PackageView,
        sr_packages::service::MyPackage,
        sr_packages::service::PendingVersion,
        sr_packages::service::Enablement,
        sr_packages::service::Decision,
        sr_packages::service::PageUser,
        sr_packages::service::ProfilePage,
        sr_packages::service::GuestbookEntry,
        sr_packages::profile_doc::ProfileDoc,
        sr_packages::profile_doc::Style,
        sr_packages::profile_doc::Palette,
        sr_packages::profile_doc::Block,
        sr_packages::profile_doc::BlockKind,
        handlers::EnableBody,
        handlers::ApplyResult,
        handlers::AppliedFeature,
        handlers::LayoutBody,
        handlers::SignBody,
        handlers::DecisionBody,
        handlers::CreateGuildBody,
    ))
)]
pub struct PackagesApiDoc;

/// The OpenAPI document fragment for the packages endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    PackagesApiDoc::openapi()
}
