//! Shared pieces of the admin handlers: dependencies, the staff extractor, and
//! the mapping of [`AdminError`] onto Discord-style errors.

use crate::{
    auth::{AuthKind, Principal},
    error::HttpError,
    routes::{
        Ext,
        auth::{AuthRouteError, AuthService, StaffMfa},
    },
    state::AppState,
};
use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::{HeaderMap, request::Parts},
};
use serde::Deserialize;
use sr_admin::{AdminError, AdminService, Caller};
use sr_core::{ApiError, ErrorCode};
use sr_features::FeatureService;
use std::{net::SocketAddr, sync::Arc};
use utoipa::ToSchema;

/// Header that may carry the reason of a body-less request.
pub const REASON_HEADER: &str = "x-audit-log-reason";

/// Collaborators of the admin endpoints.
pub struct AdminDeps {
    pub service: AdminService,
    pub features: Option<FeatureService>,
    pub gateway_sessions: Option<Arc<dyn Fn() -> usize + Send + Sync>>,
    /// Live voice counts (rooms, egress bytes) when the SFU runs in this process.
    pub voice_stats: Option<Arc<dyn Fn() -> (u64, u64) + Send + Sync>>,
    /// Sensitive actions need a second factor checked within the last 15 minutes
    /// (ADM-STF-009). Off by default so tests and development need no authenticator.
    pub require_reauth: bool,
}

impl AdminDeps {
    pub fn new(service: AdminService) -> Self {
        Self {
            service,
            features: None,
            gateway_sessions: None,
            voice_stats: None,
            require_reauth: false,
        }
    }

    /// Feature toggles are exposed through this service.
    pub fn with_features(mut self, f: FeatureService) -> Self {
        self.features = Some(f);
        self
    }

    /// Demand a fresh second factor for the sensitive admin routes (private content,
    /// account email and password, 2FA reset, staff changes, announcements).
    pub fn with_reauth(mut self, on: bool) -> Self {
        self.require_reauth = on;
        self
    }

    /// Source of the live voice room count and egress bytes for the dashboard.
    pub fn with_voice_stats(mut self, f: Arc<dyn Fn() -> (u64, u64) + Send + Sync>) -> Self {
        self.voice_stats = Some(f);
        self
    }

    /// Source of the live gateway session count for the System page.
    pub fn with_gateway_sessions(mut self, f: Arc<dyn Fn() -> usize + Send + Sync>) -> Self {
        self.gateway_sessions = Some(f);
        self
    }
}

/// A staff caller: a browser session whose account currently has platform standing.
/// Everything else is refused (401 without credentials, 403 `SrStaffOnly` otherwise).
pub struct StaffCtx {
    pub deps: Arc<AdminDeps>,
    pub caller: Caller,
    /// The browser session the request came in on.
    pub session_id: i64,
    auth: Option<Arc<AuthService>>,
    now_secs: i64,
}

impl StaffCtx {
    pub fn svc(&self) -> &AdminService {
        &self.deps.service
    }

    pub fn now_secs(&self) -> i64 {
        self.now_secs
    }

    /// Refuse unless this session passed a second-factor check in the last 15 minutes
    /// (ADM-STF-009). The client answers the `SrReauthRequired` error by calling
    /// `POST /admin/reauth` and retrying. A no-op when [`AdminDeps::require_reauth`] is off.
    pub async fn require_reauth(&self) -> Result<(), HttpError> {
        if !self.deps.require_reauth {
            return Ok(());
        }
        let auth = self.auth.as_ref().ok_or_else(|| {
            tracing::error!("admin re-auth is on but no AuthService is registered");
            HttpError::from(ErrorCode::SrInternal)
        })?;
        match crate::routes::auth::staff_mfa_state(
            auth,
            self.now_secs,
            self.caller.user_id,
            self.session_id,
        )
        .await
        .map_err(auth_err)?
        {
            StaffMfa::Fresh => Ok(()),
            StaffMfa::Stale => Err(ApiError::with_message(
                ErrorCode::SrReauthRequired,
                "Confirm your second factor to continue.",
            )
            .into()),
            StaffMfa::NotEnrolled => Err(ApiError::with_message(
                ErrorCode::SrReauthRequired,
                "Set up two-factor authentication before using this admin action.",
            )
            .into()),
        }
    }

    /// Open the 15-minute window with a TOTP, recovery or passkey proof.
    pub async fn prove_second_factor(&self, method: &str, code: &str) -> Result<(), HttpError> {
        let auth = self
            .auth
            .as_ref()
            .ok_or_else(|| HttpError::from(ErrorCode::SrInternal))?;
        crate::routes::auth::staff_mfa_prove(
            auth,
            self.now_secs,
            self.caller.user_id,
            self.session_id,
            method,
            code,
        )
        .await
        .map_err(auth_err)
    }

    /// Where the session stands, for `GET /admin/reauth`.
    pub async fn mfa_state(&self) -> Result<StaffMfa, HttpError> {
        let Some(auth) = &self.auth else {
            return Ok(StaffMfa::NotEnrolled);
        };
        crate::routes::auth::staff_mfa_state(
            auth,
            self.now_secs,
            self.caller.user_id,
            self.session_id,
        )
        .await
        .map_err(auth_err)
    }
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

impl FromRequestParts<AppState> for StaffCtx {
    type Rejection = HttpError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let p = Principal::from_request_parts(parts, state).await?;
        // Admin work needs a human browser session (CSRF-protected); bots and OAuth
        // tokens never carry platform standing.
        let AuthKind::Session { session_id } = p.kind else {
            return Err(ErrorCode::SrStaffOnly.into());
        };
        let Ext(deps) = Ext::<AdminDeps>::from_request_parts(parts, state).await?;
        let ip = client_ip(parts, state.settings().trust_forwarded_for);
        let caller = deps.service.resolve(p.user_id, ip).await.map_err(map_err)?;
        Ok(Self {
            deps,
            caller,
            session_id,
            auth: state.extension::<AuthService>(),
            now_secs: state.now_secs(),
        })
    }
}

fn client_ip(parts: &Parts, trust_xff: bool) -> Option<String> {
    if trust_xff
        && let Some(ip) = parts
            .headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit(',').next())
            .map(str::trim)
            .filter(|s| !s.is_empty())
    {
        return Some(ip.to_owned());
    }
    parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip().to_string())
}

/// Body of actions that only need a reason.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ReasonBody {
    /// Why. Required for account, Person, invite and staff actions; also accepted in the
    /// `X-Audit-Log-Reason` header.
    pub reason: Option<String>,
}

/// The reason from the body, else from the header.
pub fn reason(headers: &HeaderMap, body: Option<&str>) -> Option<String> {
    body.map(str::to_owned).or_else(|| {
        headers
            .get(REASON_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    })
}

/// Parse a snowflake path segment.
pub fn id(raw: &str) -> Result<i64, HttpError> {
    raw.parse::<i64>().ok().filter(|v| *v > 0).ok_or_else(|| {
        ApiError::invalid_form()
            .field("id", "SNOWFLAKE_TYPE_INVALID", "Not a valid id.")
            .into()
    })
}

pub fn map_err(e: AdminError) -> HttpError {
    match e {
        AdminError::NotStaff => ErrorCode::SrStaffOnly.into(),
        AdminError::MissingPermission(p) => ApiError::with_message(
            ErrorCode::MissingPermissions,
            format!("Missing platform permission {p}"),
        )
        .into(),
        AdminError::Forbidden(m) => ApiError::with_message(ErrorCode::MissingAccess, m).into(),
        AdminError::ReasonRequired => ErrorCode::SrReasonRequired.into(),
        AdminError::NotFound(what) => {
            let code = match what {
                "account" => ErrorCode::UnknownAccount,
                "invite" => ErrorCode::UnknownInvite,
                "staff role" => ErrorCode::UnknownRole,
                _ => ErrorCode::SrNotFound,
            };
            ApiError::with_message(code, format!("Unknown {what}")).into()
        }
        AdminError::Invalid(m) => ApiError::with_message(ErrorCode::InvalidFormBody, m).into(),
        AdminError::Conflict(m) => ApiError::with_message(ErrorCode::SrConflict, m).into(),
        AdminError::Storage(m) => {
            tracing::error!(error = %m, "admin storage failure");
            ErrorCode::SrInternal.into()
        }
    }
}
