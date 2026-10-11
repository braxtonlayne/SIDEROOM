//! The [`Principal`] extractor.

use super::{BackendError, csrf};
use crate::{error::HttpError, state::AppState};
use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, header, request::Parts},
};
use sr_auth::session::{SessionPolicy, SessionToken};
use sr_core::{ApiError, ErrorCode};

/// Name of the session cookie.
pub const SESSION_COOKIE: &str = "sr_session";

/// How the caller authenticated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthKind {
    /// Browser session (CSRF-checked on mutations).
    Session {
        /// Session row id.
        session_id: i64,
    },
    /// `Authorization: Bot`.
    Bot {
        /// Owning application.
        application_id: i64,
    },
    /// `Authorization: Bearer` (OAuth2).
    Bearer {
        /// Granted scopes.
        scopes: Vec<String>,
    },
}

/// An authenticated caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    /// Acting user (for bots, the bot user).
    pub user_id: i64,
    /// Credential kind.
    pub kind: AuthKind,
}

impl Principal {
    /// Require an OAuth scope when authenticated by bearer token. Sessions and
    /// bots are not scope-limited.
    pub fn require_scope(&self, scope: &str) -> Result<(), HttpError> {
        match &self.kind {
            AuthKind::Bearer { scopes } if !scopes.iter().any(|s| s == scope) => Err(
                ApiError::with_message(ErrorCode::MissingAccess, format!("Missing scope: {scope}"))
                    .into(),
            ),
            _ => Ok(()),
        }
    }
}

/// Extract one cookie value from a header map.
pub fn parse_cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

/// Build the `Set-Cookie` value for a new session.
pub fn session_cookie(token: &SessionToken, secure: bool, max_age_secs: i64) -> String {
    format!(
        "{SESSION_COOKIE}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_secs}{}",
        token.expose(),
        if secure { "; Secure" } else { "" }
    )
}

fn internal(e: BackendError) -> HttpError {
    tracing::error!(error = %e, "auth backend failure");
    ErrorCode::SrInternal.into()
}

fn invalid() -> HttpError {
    ErrorCode::InvalidAuthenticationToken.into()
}

impl FromRequestParts<AppState> for Principal {
    type Rejection = HttpError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(p) = parts.extensions.get::<Principal>() {
            return Ok(p.clone());
        }
        let p = authenticate(parts, state).await?;
        parts.extensions.insert(p.clone());
        Ok(p)
    }
}

async fn authenticate(parts: &Parts, state: &AppState) -> Result<Principal, HttpError> {
    if let Some(h) = parts.headers.get(header::AUTHORIZATION) {
        let h = h.to_str().map_err(|_| invalid())?;
        let (scheme, token) = h.split_once(' ').ok_or_else(invalid)?;
        let token = token.trim();
        return if scheme.eq_ignore_ascii_case("bot") {
            let id = state
                .bots()
                .authenticate(token)
                .await
                .map_err(internal)?
                .ok_or_else(invalid)?;
            Ok(Principal {
                user_id: id.user_id,
                kind: AuthKind::Bot {
                    application_id: id.application_id,
                },
            })
        } else if scheme.eq_ignore_ascii_case("bearer") {
            let id = state
                .bearers()
                .authenticate(token)
                .await
                .map_err(internal)?
                .ok_or_else(invalid)?;
            Ok(Principal {
                user_id: id.user_id,
                kind: AuthKind::Bearer { scopes: id.scopes },
            })
        } else {
            Err(invalid())
        };
    }

    let raw = parse_cookie(&parts.headers, SESSION_COOKIE)
        .ok_or_else(|| HttpError::from(ErrorCode::Unauthorized))?;
    let token = SessionToken::parse(raw).ok_or_else(invalid)?;
    let rec = state
        .sessions()
        .find(&token.hash())
        .await
        .map_err(internal)?
        .ok_or_else(invalid)?;
    let now = state.now_secs();
    if !SessionPolicy::is_valid(rec.expires_at, now) {
        return Err(invalid());
    }
    if !csrf::is_safe_method(&parts.method) {
        let ok = parts
            .headers
            .get(csrf::CSRF_HEADER)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| csrf::verify(token.expose(), v));
        if !ok {
            return Err(ApiError::with_message(
                ErrorCode::MissingAccess,
                "Missing or invalid CSRF token",
            )
            .into());
        }
    }
    // Roll the expiry, at most once a minute to avoid a write per request.
    if now - rec.last_seen_at >= 60 {
        let exp = state.session_policy().expiry(rec.created_at, now);
        if let Err(e) = state.sessions().touch(rec.id, now, exp).await {
            tracing::warn!(error = %e, "failed to touch session");
        }
    }
    Ok(Principal {
        user_id: rec.user_id,
        kind: AuthKind::Session { session_id: rec.id },
    })
}

/// Rejects bot callers (20001).
#[derive(Debug, Clone)]
pub struct UserOnly(pub Principal);

impl FromRequestParts<AppState> for UserOnly {
    type Rejection = HttpError;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let p = Principal::from_request_parts(parts, state).await?;
        match p.kind {
            AuthKind::Bot { .. } => Err(ErrorCode::BotsCannotUseThisEndpoint.into()),
            _ => Ok(Self(p)),
        }
    }
}

/// Accepts only bot callers (20002).
#[derive(Debug, Clone)]
pub struct BotOnly(pub Principal);

impl FromRequestParts<AppState> for BotOnly {
    type Rejection = HttpError;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let p = Principal::from_request_parts(parts, state).await?;
        match p.kind {
            AuthKind::Bot { .. } => Ok(Self(p)),
            _ => Err(ErrorCode::OnlyBotsCanUseThisEndpoint.into()),
        }
    }
}
