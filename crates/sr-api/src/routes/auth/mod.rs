//! Entrance/auth REST endpoints (ENT-AUTH-*, ENT-INV-002/007). Paths are
//! listed in docs/05-api-and-gateway.md section 5.1.
//!
//! Mount with [`router_with`] (preferred) or layer an
//! `Extension<Arc<AuthService>>` onto [`router`]:
//!
//! ```ignore
//! let svc = Arc::new(AuthService::new(store, mailer, ids));
//! let app = build_router(state, routes::auth::router_with(svc));
//! ```
//!
//! Security notes:
//! * Pre-auth POSTs (login, 2FA, register, forgot, reset) have no session to
//!   carry a CSRF token. They accept only `application/json` (forcing a CORS
//!   preflight cross-origin), the session cookie is `SameSite=Lax`, and the
//!   `Svc` extractor rejects a browser `Origin` outside the allowed
//!   origins. Authenticated mutations use the normal `X-SR-CSRF` check done by
//!   the `Principal` extractor.
//! * Lockouts and per-IP limits are in memory (single node, like the global
//!   rate limiter).

mod account;
mod common;
mod invites;
mod login;
mod mfa;
mod recovery;
mod register;
mod sessions;
mod store;
pub mod throttle;
pub mod validate;

pub mod mail;
pub mod memory;

pub use common::{AuthConfig, AuthRouteError, AuthService, IdSource};
pub use store::{
    AccountRecord, AccountState, AuthStore, InviteState, RegisterRejection, RegisterTx, Registered,
    StoreError, StoreResult,
};

use crate::state::AppState;
use axum::{
    Extension, Router,
    routing::{delete, get, post},
};
use std::sync::Arc;

/// The auth routes (relative to `/api/v1`). Requires an
/// `Extension<Arc<AuthService>>` layer; without it every handler answers 500.
/// Prefer [`router_with`].
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sr/invites/{code}", get(invites::check_invite))
        .route("/sr/usernames/check", get(invites::check_username))
        .route("/sr/register", post(register::register))
        .route("/sr/age-gate", post(account::age_gate))
        .route("/sr/auth/login", post(login::login))
        .route("/sr/auth/2fa", post(login::second_factor))
        .route("/sr/auth/logout", post(sessions::logout))
        .route("/sr/auth/logout-all", post(sessions::logout_all))
        .route("/sr/auth/sessions", get(sessions::list))
        .route("/sr/auth/sessions/{id}", delete(sessions::revoke))
        .route("/sr/auth/totp/enroll", post(mfa::enroll))
        .route("/sr/auth/totp/verify", post(mfa::verify))
        .route("/sr/auth/totp", delete(mfa::disable))
        .route("/sr/auth/recovery-codes", post(recovery::regenerate))
        .route("/sr/auth/forgot", post(account::forgot))
        .route("/sr/auth/reset", post(account::reset))
        .route("/sr/auth/verify-email", post(account::verify_email))
        .route(
            "/sr/auth/verify-email/resend",
            post(account::resend_verification),
        )
}

/// [`router`] with the service attached.
pub fn router_with(service: Arc<AuthService>) -> Router<AppState> {
    router().layer(Extension(service))
}

#[cfg(test)]
mod tests;
