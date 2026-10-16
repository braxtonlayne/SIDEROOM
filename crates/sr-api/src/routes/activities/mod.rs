//! Activities API (DEV-ACT-001/002): the owner's settings and submission (`/applications/{id}/activity*`),
//! staff review (`/admin/developers/activities*`), the launcher (`/activities`, `/channels/{id}/activity`) and
//! instances (`/activity-instances/{id}*`). The proxy for `<application-id>.activities.<host>` is
//! [`proxy::activity_host_layer`]. Everything except the owner's settings is behind `[activities] enabled`.
//! The logic lives in `sr-activities`; this module is the HTTP layer.

mod handlers;
pub mod proxy;
#[cfg(test)]
mod tests;

use crate::{routes::messages::error::MsgError, state::AppState};
use async_trait::async_trait;
use axum::{
    Router,
    routing::{get, post},
};
use sr_activities::{ActivityError, ActivityService};
use sr_core::{ApiError, ErrorCode};
use std::sync::Arc;
use utoipa::OpenApi;

pub use proxy::activity_host_layer;

/// A document fetched from an application's own server.
#[derive(Debug, Clone)]
pub struct Fetched {
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

/// Fetches an application's pages for the proxy and for submission. The production client is the SSRF-safe
/// one (public addresses only, no redirects out of policy); the reason on failure never contains the URL.
#[async_trait]
pub trait ActivityFetcher: Send + Sync {
    async fn get(&self, url: &str, max_bytes: usize) -> Result<Fetched, String>;
}

/// Largest document or file the proxy relays.
pub const MAX_PROXIED_BYTES: usize = 8 * 1024 * 1024;

/// Collaborators of the Activities endpoints.
pub struct ActivitiesDeps {
    pub service: ActivityService,
    /// `[activities] enabled`.
    pub enabled: bool,
    /// `https://chat.example.org`: the app shell's origin, the only one allowed to frame an activity.
    pub app_origin: String,
    /// `chat.example.org` (with a port, if any): the activity hosts are `<id>.activities.<this>`.
    pub base_host: String,
    scheme: String,
    pub fetcher: Option<Arc<dyn ActivityFetcher>>,
}

impl ActivitiesDeps {
    /// `public_url` is `server.public_url`; an unparseable one disables nothing but makes hosts unmatchable.
    pub fn new(
        service: ActivityService,
        enabled: bool,
        public_url: &str,
        fetcher: Option<Arc<dyn ActivityFetcher>>,
    ) -> Self {
        let parsed = url::Url::parse(public_url).ok();
        let scheme = parsed.as_ref().map_or("https", |u| u.scheme()).to_owned();
        let base_host = parsed
            .as_ref()
            .and_then(|u| {
                u.host_str().map(|h| match u.port() {
                    Some(p) => format!("{h}:{p}"),
                    None => h.to_owned(),
                })
            })
            .unwrap_or_default();
        Self {
            service,
            enabled,
            app_origin: format!("{scheme}://{base_host}"),
            base_host,
            scheme,
            fetcher,
        }
    }

    /// `https://<application-id>.activities.<host>`.
    pub fn activity_origin(&self, app: i64) -> String {
        format!("{}://{app}.activities.{}", self.scheme, self.base_host)
    }

    /// 404 for everything but the owner's settings when the operator has not turned Activities on.
    pub(crate) fn require_enabled(&self) -> Result<(), MsgError> {
        if self.enabled {
            Ok(())
        } else {
            Err(ApiError::with_message(
                ErrorCode::SrNotFound,
                "Activities are not enabled on this server.",
            )
            .into())
        }
    }
}

/// An [`ActivityError`] as an API error.
pub(crate) fn act(e: ActivityError) -> MsgError {
    let api = match e {
        ActivityError::NotFound(w) => {
            ApiError::with_message(ErrorCode::SrNotFound, format!("Unknown {w}"))
        }
        ActivityError::Forbidden => ApiError::new(ErrorCode::MissingPermissions),
        ActivityError::Invalid(field, m) => {
            ApiError::invalid_form().field(field, "INVALID_VALUE", m)
        }
        ActivityError::Conflict(m) => ApiError::with_message(ErrorCode::SrConflict, m),
        ActivityError::Storage(m) => {
            tracing::error!(error = %m, "activities failure");
            ApiError::new(ErrorCode::SrInternal)
        }
    };
    MsgError::Api(api)
}

/// Activities routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    use handlers as h;
    Router::new()
        .route(
            "/applications/{application_id}/activity",
            get(h::get_config).put(h::put_config),
        )
        .route(
            "/applications/{application_id}/activity/submit",
            post(h::submit),
        )
        .route("/admin/developers/activities", get(h::admin_pending))
        .route(
            "/admin/developers/activities/{application_id}/decision",
            post(h::admin_decide),
        )
        .route("/activities", get(h::shelf))
        .route(
            "/channels/{channel_id}/activity",
            get(h::in_channel).post(h::launch),
        )
        .route(
            "/activity-instances/{instance_id}",
            get(h::get_instance).delete(h::end),
        )
        .route("/activity-instances/{instance_id}/join", post(h::join))
        .route("/activity-instances/{instance_id}/leave", post(h::leave))
}

/// OpenAPI fragment for the Activities endpoints.
#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::get_config,
        handlers::put_config,
        handlers::submit,
        handlers::admin_pending,
        handlers::admin_decide,
        handlers::shelf,
        handlers::in_channel,
        handlers::launch,
        handlers::get_instance,
        handlers::join,
        handlers::leave,
        handlers::end,
    ),
    components(schemas(
        sr_activities::model::UrlMapping,
        sr_activities::model::ReviewState,
        sr_activities::model::ConfigView,
        sr_activities::model::ConfigInput,
        sr_activities::model::PendingView,
        sr_activities::model::ShelfItem,
        sr_activities::model::ParticipantView,
        sr_activities::model::InstanceView,
        handlers::ConfigResponse,
        handlers::DecisionBody,
        handlers::LaunchBody,
        handlers::ShelfResponse,
        handlers::ChannelActivity,
    ))
)]
pub struct ActivitiesApiDoc;

/// The OpenAPI document fragment for the Activities endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    ActivitiesApiDoc::openapi()
}
