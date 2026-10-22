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
mod alerts;
mod common;
mod discord;
mod identity;
mod invites;
mod login;
mod member_invites;
mod mfa;
mod passkeys;
mod recovery;
mod register;
mod self_delete;
mod sessions;
mod store;
pub mod throttle;
mod trusted;
pub mod validate;

pub mod mail;
pub mod mail_text;
pub mod memory;

pub use common::{AuthConfig, AuthRouteError, AuthService, IdSource};

/// Re-check a signed-in user's password before a sensitive operation outside `/sr/auth`
/// (the developer portal). Shares the failure lockout with login.
pub async fn confirm_password(
    svc: &AuthService,
    now_secs: i64,
    user_id: i64,
    password: &str,
) -> Result<(), AuthRouteError> {
    let account = mfa::current_account(svc, user_id).await?;
    mfa::reauthenticate(svc, now_secs, &account, password).await
}
/// Second-factor window of a staff session (ADM-STF-009).
pub use identity::{STAFF_MFA_WINDOW_SECS, StaffMfa};
pub use self_delete::{
    SelfDeleteCounts, SelfDeleteDeps, SelfDeleteRefusal, SelfDeleteRun, SelfDeleteScope,
    SelfDeleteStore,
};

/// Is the session's second factor fresh? (admin re-auth gate)
pub async fn staff_mfa_state(
    svc: &AuthService,
    now_secs: i64,
    user_id: i64,
    session_id: i64,
) -> Result<StaffMfa, AuthRouteError> {
    identity::staff_mfa_state(svc, now_secs, user_id, session_id).await
}

/// Does the account have a confirmed TOTP or a passkey?
pub async fn has_second_factor(svc: &AuthService, user_id: i64) -> Result<bool, AuthRouteError> {
    identity::has_second_factor(svc, user_id).await
}

/// Open the 15-minute window with a second-factor proof.
pub async fn staff_mfa_prove(
    svc: &AuthService,
    now_secs: i64,
    user_id: i64,
    session_id: i64,
    method: &str,
    code: &str,
) -> Result<(), AuthRouteError> {
    identity::staff_mfa_prove(svc, now_secs, user_id, session_id, method, code).await
}
pub use discord::DiscordSettings;
pub use store::{
    AccountRecord, AccountState, AuthStore, InviteState, RegisterRejection, RegisterTx, Registered,
    StoreError, StoreResult,
};

use crate::state::AppState;
use axum::{
    Router,
    routing::{delete, get, patch, post, put},
};

/// OpenAPI fragment for the documented auth routes (the signed-in account edits).
pub fn openapi() -> utoipa::openapi::OpenApi {
    use utoipa::OpenApi;
    let mut doc = identity::IdentityApiDoc::openapi();
    doc.merge(passkeys::PasskeysApiDoc::openapi());
    doc.merge(discord::DiscordApiDoc::openapi());
    doc.merge(member_invites::MemberInvitesApiDoc::openapi());
    doc.merge(self_delete::SelfDeleteApiDoc::openapi());
    doc
}

/// The auth routes (relative to `/api/v1`). Handlers need an `AuthService`
/// registered on `AppState`; without one they answer 500.
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/sr/invites",
            get(member_invites::list).post(member_invites::create),
        )
        .route(
            "/sr/invites/{code}",
            get(invites::check_invite).delete(member_invites::revoke),
        )
        .route("/sr/usernames/check", get(invites::check_username))
        .route("/sr/register", post(register::register))
        .route("/sr/age-gate", post(account::age_gate))
        .route(
            "/users/@me/age-confirmation",
            get(account::get_age_confirmation).delete(account::reset_age_confirmation),
        )
        .route("/sr/auth/login", post(login::login))
        .route("/sr/auth/login-link", post(login::login_link))
        .route("/sr/auth/2fa", post(login::second_factor))
        .route("/sr/auth/logout", post(sessions::logout))
        .route("/sr/auth/logout-all", post(sessions::logout_all))
        .route("/sr/auth/sessions", get(sessions::list))
        .route("/sr/auth/sessions/{id}", delete(sessions::revoke))
        .route("/sr/auth/discord", get(discord::status))
        .route("/sr/auth/discord/link", post(discord::link_start))
        .route("/sr/auth/discord/unlink", post(discord::unlink))
        .route("/sr/auth/discord/start", get(discord::start))
        .route("/sr/auth/discord/callback", get(discord::callback))
        .route(
            "/sr/auth/discord/pending",
            get(discord::pending).delete(discord::forget),
        )
        .route("/sr/auth/login-alerts", get(alerts::get).put(alerts::put))
        .route("/sr/auth/trusted-devices", get(trusted::list))
        .route("/sr/auth/trusted-devices/{id}", delete(trusted::revoke))
        .route("/sr/auth/totp/enroll", post(mfa::enroll))
        .route("/sr/auth/totp/verify", post(mfa::verify))
        .route("/sr/auth/totp", delete(mfa::disable))
        .route("/sr/auth/recovery-codes", post(recovery::regenerate))
        .route(
            "/sr/auth/passkeys/register/begin",
            post(passkeys::register_begin),
        )
        .route(
            "/sr/auth/passkeys/register/finish",
            post(passkeys::register_finish),
        )
        .route("/sr/auth/passkeys/login/begin", post(passkeys::login_begin))
        .route(
            "/sr/auth/passkeys/login/finish",
            post(passkeys::login_finish),
        )
        .route(
            "/sr/auth/passkeys/verify/begin",
            post(passkeys::verify_begin),
        )
        .route("/sr/auth/passkeys", get(passkeys::list))
        .route(
            "/sr/auth/passkeys/{id}",
            patch(passkeys::rename).delete(passkeys::remove),
        )
        .route("/sr/auth/forgot", post(account::forgot))
        .route("/sr/auth/reset", post(account::reset))
        .route("/sr/auth/verify-email", post(account::verify_email))
        .route("/sr/auth/password", post(identity::change_password))
        .route("/sr/auth/username", put(identity::change_username))
        .route("/sr/auth/account/delete", post(identity::delete_account))
        .route(
            "/sr/users/@me/messages/delete-all",
            get(self_delete::preview).post(self_delete::delete_all),
        )
        .route("/sr/auth/email/change", post(identity::change_email))
        .route("/sr/auth/email/cancel", post(identity::cancel_change_email))
        .route(
            "/sr/auth/verify-email/resend",
            post(account::resend_verification),
        )
}

#[cfg(test)]
mod tests;
