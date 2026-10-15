//! Live TV REST API (docs/specs/cinema-live-tv.md section 8): the channel list, the guide, now/next and the
//! live HLS playlists for members (`/tv/*`), and the staff controls (`/admin/tv/*`, `MANAGE_TV`).
//!
//! * Members read enabled channels. A restricted programme on air is listed like any other, but opening the
//!   stream (`tune`, the live playlists) answers 403 code 900011 until the account confirmed it is 18 or
//!   older (D-184).
//! * Staff are sessions holding `MANAGE_TV` (the Media Curator role); every change is written to the staff
//!   access log (`tv.channel.create`, `tv.schedule.publish`, ...).
//!
//! Wiring, in the composition root:
//!
//! ```ignore
//! let tv = TvService::new(Arc::new(PgTvRepo::new(pool)), catalog, clock, ids)
//!     .with_live(LiveDeps { source: playback_source, signer: cdn_signer });
//! let deps = TvDeps::new(tv).with_age_gate(Arc::new(PgAgeGate::new(pool)));
//! let state = AppState::builder().extension(Arc::new(deps)).build();
//! ```

mod admin;
mod member;

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
    routing::{delete, get, post},
};
use sr_core::{ApiError, ErrorCode, UserId};
use sr_tv::{Actor, TvError, TvService, Viewer};
use std::sync::Arc;
use utoipa::OpenApi;

/// Collaborators of the Live TV endpoints.
pub struct TvDeps {
    /// Shared: the watch party gate for Live TV is this same service (`sr_tv::gate`).
    pub service: Arc<TvService>,
    age: Arc<dyn AgeGate>,
}

impl TvDeps {
    pub fn new(service: TvService) -> Self {
        Self {
            service: Arc::new(service),
            age: Arc::new(NoAgeConfirmed),
        }
    }

    /// Source of the 18+ confirmation (`accounts.age_confirmed_at`).
    pub fn with_age_gate(mut self, age: Arc<dyn AgeGate>) -> Self {
        self.age = age;
        self
    }
}

/// A signed-in member.
pub(crate) struct MemberCtx {
    pub deps: Arc<TvDeps>,
    pub viewer: Viewer,
}

impl FromRequestParts<AppState> for MemberCtx {
    type Rejection = HttpError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let UserOnly(p) = UserOnly::from_request_parts(parts, state).await?;
        let Ext(deps) = Ext::<TvDeps>::from_request_parts(parts, state).await?;
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

/// A staff session, with the Live TV service next to it.
pub(crate) struct StaffTv {
    pub staff: StaffCtx,
    pub tv: Arc<TvDeps>,
}

impl StaffTv {
    pub fn svc(&self) -> &TvService {
        &self.tv.service
    }

    pub fn actor(&self) -> Actor {
        Actor {
            user_id: self.staff.caller.user_id,
            permissions: self.staff.caller.permissions,
        }
    }

    /// Record a successful change in the staff access log. The change has already happened, so a logging
    /// failure is reported but does not fail the request.
    pub async fn audit(&self, action: &str, target: (&str, i64), detail: serde_json::Value) {
        let admin: &AdminDeps = &self.staff.deps;
        if let Err(e) = admin
            .service
            .record_action(&self.staff.caller, action, Some(target), detail)
            .await
        {
            tracing::error!(error = %e, action, "tv change was not written to the access log");
        }
    }
}

impl FromRequestParts<AppState> for StaffTv {
    type Rejection = HttpError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let staff = StaffCtx::from_request_parts(parts, state).await?;
        let Ext(tv) = Ext::<TvDeps>::from_request_parts(parts, state).await?;
        Ok(Self { staff, tv })
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

pub(crate) fn map_err(e: TvError) -> HttpError {
    match e {
        TvError::NotFound(w) => not_found(w),
        TvError::OffAir => {
            ApiError::with_message(ErrorCode::SrNotFound, "Channel is off air").into()
        }
        TvError::AgeConfirmationRequired => ErrorCode::SrAgeConfirmationRequired.into(),
        TvError::MissingPermission(p) => ApiError::with_message(
            ErrorCode::MissingPermissions,
            format!("Missing platform permission {p}"),
        )
        .into(),
        TvError::Invalid(m) => ApiError::with_message(ErrorCode::InvalidFormBody, m).into(),
        TvError::Conflict(m) => ApiError::with_message(ErrorCode::SrConflict, m).into(),
        TvError::Storage(m) | TvError::Catalog(m) => {
            tracing::error!(error = %m, "live tv failure");
            ErrorCode::SrInternal.into()
        }
    }
}

/// Live TV routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        // Members.
        .route("/tv/channels", get(member::channels))
        .route("/tv/timezones", get(member::timezones))
        .route("/tv/guide", get(member::guide))
        .route(
            "/tv/reminders/{block_id}",
            axum::routing::put(member::set_reminder).delete(member::clear_reminder),
        )
        .route("/tv/channels/{channel_id}", get(member::channel))
        .route("/tv/channels/{channel_id}/now", get(member::now))
        .route("/tv/channels/{channel_id}/tune", get(member::tune))
        .route(
            "/tv/channels/{channel_id}/live/master.m3u8",
            get(member::live_master),
        )
        .route(
            "/tv/channels/{channel_id}/live/audio.m3u8",
            get(member::live_audio),
        )
        .route(
            "/tv/channels/{channel_id}/live/v/{file}",
            get(member::live_video),
        )
        // Staff.
        .route(
            "/admin/tv/channels",
            get(admin::channels).post(admin::create_channel),
        )
        .route(
            "/admin/tv/channels/{channel_id}",
            get(admin::channel)
                .put(admin::update_channel)
                .delete(admin::delete_channel),
        )
        .route(
            "/admin/tv/channels/{channel_id}/schedule",
            get(admin::schedule),
        )
        .route(
            "/admin/tv/channels/{channel_id}/preview",
            post(admin::preview),
        )
        .route(
            "/admin/tv/channels/{channel_id}/publish",
            post(admin::publish),
        )
        .route(
            "/admin/tv/channels/{channel_id}/blocks",
            post(admin::place_block),
        )
        .route(
            "/admin/tv/channels/{channel_id}/now",
            get(admin::now).put(admin::set_now),
        )
        .route("/admin/tv/blocks/{block_id}", delete(admin::remove_block))
        .route("/admin/tv/report", get(admin::report))
}

/// OpenAPI fragment for the Live TV endpoints.
#[derive(OpenApi)]
#[openapi(
    paths(
        member::channels,
        member::timezones,
        member::guide,
        member::set_reminder,
        member::clear_reminder,
        member::channel,
        member::now,
        member::tune,
        member::live_master,
        member::live_audio,
        member::live_video,
        admin::channels,
        admin::create_channel,
        admin::channel,
        admin::update_channel,
        admin::delete_channel,
        admin::schedule,
        admin::preview,
        admin::publish,
        admin::place_block,
        admin::now,
        admin::set_now,
        admin::remove_block,
        admin::report,
    ),
    components(schemas(
        ErrorBody,
        member::ChannelList,
        admin::ChannelListBody,
        member::TimezoneList,
        admin::PreviewBody,
        admin::PublishBody,
        admin::BlockList,
        sr_tv::model::ChannelKind,
        sr_tv::model::FillerPolicy,
        sr_tv::model::BlockKind,
        sr_tv::model::BlockSource,
        sr_tv::model::MediaKind,
        sr_tv::model::PickMode,
        sr_tv::model::Fit,
        sr_tv::model::Restricted,
        sr_tv::model::PoolFilter,
        sr_tv::model::Daypart,
        sr_tv::model::ChannelRules,
        sr_tv::model::ChannelInput,
        sr_tv::model::Channel,
        sr_tv::model::Block,
        sr_tv::service::NowView,
        sr_tv::service::Guide,
        sr_tv::service::GuideChannel,
        sr_tv::service::Tune,
        sr_tv::service::TuneState,
        sr_tv::service::Preview,
        sr_tv::service::Published,
        sr_tv::service::PlaceInput,
        sr_tv::service::SetNowInput,
        sr_tv::service::Report,
        sr_tv::service::ReportChannel,
        sr_tv::service::ReportItem,
    ))
)]
pub struct TvApiDoc;

/// The OpenAPI document fragment for the Live TV endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    TvApiDoc::openapi()
}
