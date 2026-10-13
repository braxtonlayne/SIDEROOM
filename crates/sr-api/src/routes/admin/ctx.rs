//! Shared pieces of the admin handlers: dependencies, the staff extractor, and
//! the mapping of [`AdminError`] onto Discord-style errors.

use crate::{
    auth::{AuthKind, Principal},
    error::HttpError,
    routes::Ext,
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
}

impl AdminDeps {
    pub fn new(service: AdminService) -> Self {
        Self {
            service,
            features: None,
            gateway_sessions: None,
        }
    }

    /// Feature toggles are exposed through this service.
    pub fn with_features(mut self, f: FeatureService) -> Self {
        self.features = Some(f);
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
}

impl StaffCtx {
    pub fn svc(&self) -> &AdminService {
        &self.deps.service
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
        if !matches!(p.kind, AuthKind::Session { .. }) {
            return Err(ErrorCode::SrStaffOnly.into());
        }
        let Ext(deps) = Ext::<AdminDeps>::from_request_parts(parts, state).await?;
        let ip = client_ip(parts, state.settings().trust_forwarded_for);
        let caller = deps.service.resolve(p.user_id, ip).await.map_err(map_err)?;
        Ok(Self { deps, caller })
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
