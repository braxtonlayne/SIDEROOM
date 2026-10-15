//! Server safety settings that mirror Discord's guild fields (G-36, P10-SFE-3):
//!
//! * `POST /guilds/{id}/mfa`: the 2FA requirement for moderation (`mfa_level`), owner only.
//! * `GET|PATCH /guilds/{id}/widget`: widget settings (`MANAGE_GUILD`).
//! * `GET|POST /sr/auth/reauth`: prove a second factor to open the 15-minute window that
//!   [`MfaGate`] checks.
//!
//! With `mfa_level = 1` every elevated route (D-223: moderator actions, webhooks, expressions,
//! AutoMod, community settings, pins, other members' messages) needs a browser session whose
//! second factor was checked in the last 15 minutes; the refusal is `900004 SR_REAUTH_REQUIRED`,
//! which clients answer by calling `POST /sr/auth/reauth` and retrying, or `900123
//! MFA_ENROLLMENT_REQUIRED` when the account has no second factor at all. Bots are not
//! session-bound: they pass when their application owner has a second factor, else `900123`.

use super::backend::GuildBackend;
use super::util::{Api, actor, ce, id};
use crate::routes::auth::{
    AuthRouteError, AuthService, STAFF_MFA_WINDOW_SECS, StaffMfa, has_second_factor,
    staff_mfa_prove, staff_mfa_state,
};
use crate::routes::developer::DevDeps;
use crate::routes::messages::error::ApiJson;
use crate::{
    AuthKind, HttpError, Principal, error::ApiResult, openapi::ErrorBody, state::AppState,
};
use axum::{
    Json, Router,
    extract::{FromRequestParts, Path},
    http::request::Parts,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sr_chat::admin_rules::GuildPatch;
use sr_core::{ApiError, ChannelId, ErrorCode, GuildId, UserId};
use std::sync::Arc;
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/guilds/{guild_id}/mfa", post(set_mfa_level))
        .route(
            "/guilds/{guild_id}/widget",
            get(get_widget).patch(set_widget),
        )
        .route("/sr/auth/reauth", get(reauth_status).post(reauth))
}

/// Checks the second-factor requirement of a server for the acting session.
pub struct MfaGate {
    auth: Option<Arc<AuthService>>,
    dev: Option<Arc<DevDeps>>,
    now_secs: i64,
    session: Option<i64>,
    bot_app: Option<i64>,
}

impl FromRequestParts<AppState> for MfaGate {
    type Rejection = HttpError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let p = Principal::from_request_parts(parts, state).await?;
        Ok(Self {
            auth: state.extension::<AuthService>(),
            dev: state.extension::<DevDeps>(),
            now_secs: state.now_secs(),
            session: match p.kind {
                AuthKind::Session { session_id } => Some(session_id),
                _ => None,
            },
            bot_app: match p.kind {
                AuthKind::Bot { application_id } => Some(application_id),
                _ => None,
            },
        })
    }
}

fn reauth_required(msg: &str) -> HttpError {
    ApiError::with_message(ErrorCode::SrReauthRequired, msg).into()
}

fn enrollment_required(msg: &str) -> HttpError {
    ApiError::with_message(ErrorCode::SrMfaEnrollmentRequired, msg).into()
}

fn auth_err(e: AuthRouteError) -> HttpError {
    match e {
        AuthRouteError::Api(h) => h,
        AuthRouteError::Throttled { retry_after } => ApiError::with_message(
            ErrorCode::TooManyRequests,
            format!(
                "Too many attempts. Try again in {} seconds.",
                retry_after.max(1)
            ),
        )
        .into(),
    }
}

impl MfaGate {
    /// Refuse a moderator action in `guild` unless the server does not demand 2FA or this
    /// session proved a second factor in the last 15 minutes.
    pub async fn require(
        &self,
        api: &dyn GuildBackend,
        user: UserId,
        guild: GuildId,
    ) -> Result<(), HttpError> {
        if self.session.is_none() && self.bot_app.is_none() {
            return Ok(());
        }
        let (g, _) = api.get_guild(user, guild).await.map_err(ce)?;
        self.check_level(user, g.mfa_level).await
    }

    /// The check of [`Self::require`] when the caller already knows the server's `mfa_level`
    /// (routes outside the guild backend). A session needs a second factor proved in the
    /// last 15 minutes (`900004`; `900123` with none enrolled); a bot passes when its
    /// application's owner has a second factor enrolled and gets `900123` otherwise (D-223).
    pub async fn check_level(&self, user: UserId, mfa_level: u8) -> Result<(), HttpError> {
        if mfa_level == 0 {
            return Ok(());
        }
        if let Some(app) = self.bot_app {
            return self.check_bot_owner(app).await;
        }
        let Some(session) = self.session else {
            return Ok(());
        };
        let Some(auth) = &self.auth else {
            tracing::error!("a server demands 2FA but no AuthService is registered");
            return Err(ErrorCode::SrInternal.into());
        };
        let uid = i64::try_from(user.get()).unwrap_or(0);
        match staff_mfa_state(auth, self.now_secs, uid, session)
            .await
            .map_err(auth_err)?
        {
            StaffMfa::Fresh => Ok(()),
            StaffMfa::Stale => Err(reauth_required(
                "This server requires two-factor authentication for moderator actions. \
                 Confirm your second factor to continue.",
            )),
            StaffMfa::NotEnrolled => Err(enrollment_required(
                "This server requires two-factor authentication for moderator actions. \
                 Set up two-factor authentication in your account settings first.",
            )),
        }
    }

    /// Does the gate apply to this caller at all (a browser session or a bot)?
    pub fn applies(&self) -> bool {
        self.session.is_some() || self.bot_app.is_some()
    }

    async fn check_bot_owner(&self, app: i64) -> Result<(), HttpError> {
        let (Some(dev), Some(auth)) = (&self.dev, &self.auth) else {
            tracing::error!("a server demands 2FA but the bot owner cannot be resolved");
            return Err(ErrorCode::SrInternal.into());
        };
        let owner = dev
            .apps
            .application(app)
            .await
            .map_err(|_| HttpError::from(ErrorCode::SrInternal))?
            .owner_id;
        if has_second_factor(auth, owner).await.map_err(auth_err)? {
            Ok(())
        } else {
            Err(enrollment_required(
                "This server requires two-factor authentication. The owner of this \
                 application must set up two-factor authentication first.",
            ))
        }
    }
}

impl MfaGate {
    /// [`Self::require`] for a route addressed by channel id (DM channels are exempt).
    pub async fn require_channel(
        &self,
        api: &dyn GuildBackend,
        user: UserId,
        channel: ChannelId,
    ) -> Result<(), HttpError> {
        if self.session.is_none() && self.bot_app.is_none() {
            return Ok(());
        }
        let view = api.get_channel(user, channel).await.map_err(ce)?;
        match view.channel.guild_id {
            Some(g) => self.require(api, user, g).await,
            None => Ok(()),
        }
    }
}

// ------------------------------------------------------------------ reauth

/// Where the session stands on the second-factor window.
#[derive(Debug, Serialize, ToSchema)]
pub struct MfaStatus {
    /// A second factor was checked on this session in the last 15 minutes.
    pub fresh: bool,
    /// The account has a second factor (TOTP or passkey).
    pub enrolled: bool,
    /// Length of the window in seconds.
    pub window_secs: i64,
}

/// Body of `POST /sr/auth/reauth`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ReauthBody {
    /// `totp`, `recovery` or `passkey` (then `code` is the JSON of the passkey assertion).
    pub method: String,
    pub code: String,
}

async fn status_of(gate: &MfaGate, p: &Principal) -> ApiResult<MfaStatus> {
    let (Some(auth), Some(session)) = (&gate.auth, gate.session) else {
        return Ok(MfaStatus {
            fresh: false,
            enrolled: false,
            window_secs: STAFF_MFA_WINDOW_SECS,
        });
    };
    let state = staff_mfa_state(auth, gate.now_secs, p.user_id, session)
        .await
        .map_err(auth_err)?;
    Ok(MfaStatus {
        fresh: state == StaffMfa::Fresh,
        enrolled: state != StaffMfa::NotEnrolled,
        window_secs: STAFF_MFA_WINDOW_SECS,
    })
}

/// Is this session's second-factor window open? (browser sessions only)
#[utoipa::path(
    get, path = "/sr/auth/reauth",
    responses((status = 200, body = MfaStatus), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn reauth_status(p: Principal, gate: MfaGate) -> ApiResult<Json<MfaStatus>> {
    Ok(Json(status_of(&gate, &p).await?))
}

/// Prove a second factor to open the 15-minute window for this session.
#[utoipa::path(
    post, path = "/sr/auth/reauth", request_body = ReauthBody,
    responses((status = 200, body = MfaStatus), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 429, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn reauth(
    p: Principal,
    gate: MfaGate,
    Json(b): Json<ReauthBody>,
) -> ApiResult<Json<MfaStatus>> {
    let (Some(auth), Some(session)) = (&gate.auth, gate.session) else {
        return Err(ErrorCode::MissingAccess.into());
    };
    staff_mfa_prove(auth, gate.now_secs, p.user_id, session, &b.method, &b.code)
        .await
        .map_err(auth_err)?;
    Ok(Json(status_of(&gate, &p).await?))
}

// ------------------------------------------------------------------ mfa level

/// Body of `POST /guilds/{id}/mfa`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct MfaLevelBody {
    /// 0 = none, 1 = elevated (moderator actions need a second factor).
    pub level: u8,
}

/// Result of `POST /guilds/{id}/mfa`.
#[derive(Debug, Serialize, ToSchema)]
pub struct MfaLevelObject {
    pub level: u8,
}

/// Set the 2FA requirement for moderation (owner only; Discord parity). Switching it on
/// needs the owner to have a second factor of their own. Emits `GUILD_UPDATE` and writes a
/// `GUILD_UPDATE` audit entry with the `mfa_level` change.
#[utoipa::path(
    post, path = "/guilds/{guild_id}/mfa",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = MfaLevelBody,
    responses((status = 200, body = MfaLevelObject), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn set_mfa_level(
    p: Principal,
    Api(api): Api,
    gate: MfaGate,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<MfaLevelBody>,
) -> ApiResult<Json<MfaLevelObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    // Only the owner may change it; the enrolment check is theirs alone (others get the 403 below).
    let is_owner = api.get_guild(user, gid).await.map_err(ce)?.0.owner_id == user;
    if body.level == 1
        && is_owner
        && let (Some(auth), Some(session)) = (&gate.auth, gate.session)
    {
        let st = staff_mfa_state(auth, gate.now_secs, p.user_id, session)
            .await
            .map_err(auth_err)?;
        if st == StaffMfa::NotEnrolled {
            return Err(HttpError(ApiError::invalid_form().field(
                "level",
                "MFA_REQUIRED",
                "Set up two-factor authentication on your own account before requiring it.",
            )));
        }
    }
    let g = api
        .edit_guild(
            user,
            gid,
            GuildPatch {
                mfa_level: Some(body.level),
                ..GuildPatch::default()
            },
        )
        .await
        .map_err(ce)?;
    Ok(Json(MfaLevelObject { level: g.mfa_level }))
}

// ------------------------------------------------------------------ widget

/// The server widget settings.
#[derive(Debug, Serialize, ToSchema)]
pub struct WidgetSettings {
    pub enabled: bool,
    /// The channel the widget invites to.
    pub channel_id: Option<String>,
}

/// Body of `PATCH /guilds/{id}/widget`.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ModifyWidgetBody {
    pub enabled: Option<bool>,
    #[serde(default, deserialize_with = "super::dto::nullable")]
    #[schema(value_type = Option<String>)]
    pub channel_id: Option<Option<String>>,
}

fn widget_of(g: &sr_chat::Guild) -> WidgetSettings {
    WidgetSettings {
        enabled: g.widget_enabled,
        channel_id: g.widget_channel_id.map(|c| c.to_string()),
    }
}

/// The widget settings (any member; they are part of the server object anyway).
#[utoipa::path(
    get, path = "/guilds/{guild_id}/widget",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = WidgetSettings), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_widget(
    p: Principal,
    Api(api): Api,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<WidgetSettings>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let (g, _) = api.get_guild(user, gid).await.map_err(ce)?;
    Ok(Json(widget_of(&g)))
}

/// Enable or disable the widget and pick its invite channel (`MANAGE_GUILD`). Emits
/// `GUILD_UPDATE`; the audit entry carries `widget_enabled` / `widget_channel_id` changes.
#[utoipa::path(
    patch, path = "/guilds/{guild_id}/widget",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = ModifyWidgetBody,
    responses((status = 200, body = WidgetSettings), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn set_widget(
    p: Principal,
    Api(api): Api,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<ModifyWidgetBody>,
) -> ApiResult<Json<WidgetSettings>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let channel = match body.channel_id {
        None => None,
        Some(None) => Some(None),
        Some(Some(c)) => Some(Some(id::<ChannelId>("channel_id", &c)?)),
    };
    let g = api
        .edit_guild(
            user,
            gid,
            GuildPatch {
                widget_enabled: body.enabled,
                widget_channel_id: channel,
                ..GuildPatch::default()
            },
        )
        .await
        .map_err(ce)?;
    Ok(Json(widget_of(&g)))
}
