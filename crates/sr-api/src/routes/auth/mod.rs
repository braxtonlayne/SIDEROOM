//! Entrance/auth REST endpoints (ENT-AUTH-*, ENT-INV-002/007). Paths are
//! listed in docs/05-api-and-gateway.md section 5.1.
//!
//! The service is registered on the composition root with
//! `AppStateBuilder::extension::<AuthService>` and read by handlers through
//! the [`crate::routes::Ext`] extractor (open question 104).
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
mod discord;
mod identity;
mod invites;
mod login;
mod mfa;
mod recovery;
mod register;
mod sessions;
mod store;
pub mod throttle;
mod trusted;
pub mod validate;

pub mod mail;
pub mod memory;

pub use common::{AuthConfig, AuthRouteError, AuthService, IdSource};
pub use discord::DiscordSettings;
pub use store::{
    AccountRecord, AccountState, AuthStore, InviteState, RegisterRejection, RegisterTx, Registered,
    StoreError, StoreResult,
};

use crate::state::AppState;
use axum::{
    Router,
    routing::{delete, get, post, put},
};

/// OpenAPI fragment for the documented auth routes (the signed-in account edits).
pub fn openapi() -> utoipa::openapi::OpenApi {
    use utoipa::OpenApi;
    identity::IdentityApiDoc::openapi()
}

/// The auth routes (relative to `/api/v1`). Handlers need an `AuthService`
/// registered on `AppState`; without one they answer 500.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sr/invites/{code}", get(invites::check_invite))
        .route("/sr/usernames/check", get(invites::check_username))
        .route("/sr/register", post(register::register))
        .route("/sr/age-gate", post(account::age_gate))
        .route("/sr/auth/login", post(login::login))
        .route("/sr/auth/login-link", post(login::login_link))
        .route("/sr/auth/2fa", post(login::second_factor))
        .route("/sr/auth/logout", post(sessions::logout))
        .route("/sr/auth/logout-all", post(sessions::logout_all))
        .route("/sr/auth/sessions", get(sessions::list))
        .route("/sr/auth/sessions/{id}", delete(sessions::revoke))
        .route("/sr/auth/discord/start", get(discord::start))
        .route("/sr/auth/discord/callback", get(discord::callback))
        .route(
            "/sr/auth/discord/pending",
            get(discord::pending).delete(discord::forget),
        )
        .route("/sr/auth/trusted-devices", get(trusted::list))
        .route("/sr/auth/trusted-devices/{id}", delete(trusted::revoke))
        .route("/sr/auth/totp/enroll", post(mfa::enroll))
        .route("/sr/auth/totp/verify", post(mfa::verify))
        .route("/sr/auth/totp", delete(mfa::disable))
        .route("/sr/auth/recovery-codes", post(recovery::regenerate))
        .route("/sr/auth/forgot", post(account::forgot))
        .route("/sr/auth/reset", post(account::reset))
        .route("/sr/auth/verify-email", post(account::verify_email))
        .route("/sr/auth/password", post(identity::change_password))
        .route("/sr/auth/username", put(identity::change_username))
        .route("/sr/auth/email/change", post(identity::change_email))
        .route(
            "/sr/auth/verify-email/resend",
            post(account::resend_verification),
        )
}

#[cfg(test)]
mod tests;
