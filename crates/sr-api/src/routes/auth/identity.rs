//! Signed-in account edits: change password, username and email
//! (LNG-USET-004/005, doc 06 section 2 re-auth).
//!
//! Every route needs a browser session (bots and bearer tokens are refused) and the
//! current password, which shares the failure lockout with login. The display name
//! is a profile field and is edited with `PATCH /users/@me` (`global_name`).
//!
//! Email change is a two-step flow: `POST /sr/auth/email/change` stores the pending
//! address with a single-use token and mails a link to the *new* address; the
//! account keeps its old address until the link is used. The link points at the
//! same `/verify-email/{token}` page as signup verification, and
//! `POST /sr/auth/verify-email` accepts both token kinds (see `account.rs`).

use super::{
    common::{AuthRouteError, R, Svc},
    mail,
    mfa::{current_account, reauthenticate},
    sessions::session_id,
    store::AccountRecord,
    throttle::Rule,
    validate,
};
use crate::openapi::ErrorBody;
use crate::{auth::UserOnly, state::AppState};
use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use sr_auth::email_token::{EmailToken, EmailTokenPurpose};
use sr_core::ApiError;
use utoipa::{OpenApi, ToSchema};

/// Username changes allowed per account per day.
const USERNAME_CHANGES_PER_DAY: usize = 2;

/// Words that must not appear in a new password: the account's own identifiers.
fn context(account: &AccountRecord) -> Vec<String> {
    let mut ctx = vec![
        "sideroom".to_owned(),
        "side room".to_owned(),
        account.username.clone(),
    ];
    if let Some(local) = account.email.split('@').next() {
        ctx.push(local.to_owned());
    }
    ctx
}

/// Body for `POST /sr/auth/password`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ChangePasswordBody {
    /// The password in use now.
    current_password: String,
    /// At least 10 characters and not easy to guess (zxcvbn score 3 or more).
    new_password: String,
    /// Sign out every other session and trusted device (default `true`). Pass
    /// `false` to keep the other browsers signed in.
    #[serde(default = "yes")]
    revoke_other_sessions: bool,
}

fn yes() -> bool {
    true
}

/// `POST /sr/auth/password {current_password, new_password, revoke_other_sessions?}`:
/// change the password of the signed-in account. The current password is checked
/// first (and counts toward the login lockout). By default every other session and
/// every trusted device is revoked; the current session always stays. A notice goes
/// to the account's email.
#[utoipa::path(
    post,
    path = "/sr/auth/password",
    request_body = ChangePasswordBody,
    responses(
        (status = 204, description = "Password changed"),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 429, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn change_password(
    State(state): State<AppState>,
    Svc { svc, .. }: Svc,
    UserOnly(p): UserOnly,
    Json(body): Json<ChangePasswordBody>,
) -> R<StatusCode> {
    let current_session = session_id(&p)?;
    let now = state.now_secs();
    let account = current_account(&svc, p.user_id).await?;
    reauthenticate(&svc, now, &account, &body.current_password).await?;
    let ctx = context(&account);
    let ctx_refs: Vec<&str> = ctx.iter().map(String::as_str).collect();
    if let Err(m) = validate::password(&body.new_password, &ctx_refs) {
        return Err(ApiError::invalid_form()
            .field("new_password", "weak", m)
            .into());
    }
    if body.new_password == body.current_password {
        return Err(ApiError::invalid_form()
            .field(
                "new_password",
                "unchanged",
                "Choose a password you are not already using.",
            )
            .into());
    }
    let hash = svc.hash_password(&body.new_password).await?;
    svc.store.set_password_hash(account.id, &hash).await?;
    if body.revoke_other_sessions {
        svc.store
            .revoke_sessions_except(account.id, Some(current_session))
            .await?;
        svc.store.trusted_devices_revoke_all(account.id).await?;
    }
    svc.store
        .audit(
            account.id,
            "account.password_change",
            Some(account.id),
            if body.revoke_other_sessions {
                "other sessions revoked"
            } else {
                "other sessions kept"
            },
            now,
        )
        .await?;
    let svc2 = svc.clone();
    let to = account.email;
    tokio::spawn(async move {
        if let Err(e) = svc2.mailer.send(mail::password_changed(&to)).await {
            tracing::warn!("password change notice not sent: {e}");
        }
    });
    Ok(StatusCode::NO_CONTENT)
}

/// Body for `PUT /sr/auth/username`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ChangeUsernameBody {
    /// 2 to 32 characters of `a-z 0-9 _ .` (case-insensitive), not reserved.
    username: String,
    /// Current password.
    password: String,
}

/// `PUT /sr/auth/username {username, password}`: change the login name. Same rules as
/// signup (`^[a-z0-9_.]{2,32}$`, not reserved), two changes per day. Returns the new
/// username. Friends and shared servers receive `USER_UPDATE`.
#[utoipa::path(
    put,
    path = "/sr/auth/username",
    request_body = ChangeUsernameBody,
    responses(
        (status = 200, description = "`{username}`: the new login name"),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 429, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn change_username(
    State(state): State<AppState>,
    Svc { svc, .. }: Svc,
    UserOnly(p): UserOnly,
    crate::routes::MaybeExt(social): crate::routes::MaybeExt<crate::routes::social::SocialDeps>,
    Json(body): Json<ChangeUsernameBody>,
) -> R<Json<Value>> {
    session_id(&p)?;
    let now = state.now_secs();
    let account = current_account(&svc, p.user_id).await?;
    let new = validate::username(&body.username).map_err(|m| {
        AuthRouteError::from(ApiError::invalid_form().field("username", "invalid", m))
    })?;
    reauthenticate(&svc, now, &account, &body.password).await?;
    if new == account.username {
        return Ok(Json(json!({ "username": new })));
    }
    if svc.store.username_exists(&new).await? {
        return Err(ApiError::invalid_form()
            .field("username", "taken", "That username is taken.")
            .into());
    }
    svc.ip_limits
        .hit(
            "username-change",
            &account.id.to_string(),
            now,
            &[Rule {
                limit: USERNAME_CHANGES_PER_DAY,
                secs: 86_400,
            }],
        )
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;
    if !svc.store.set_username(account.id, &new).await? {
        return Err(ApiError::invalid_form()
            .field("username", "taken", "That username is taken.")
            .into());
    }
    svc.store
        .audit(
            account.id,
            "account.username_change",
            Some(account.id),
            &format!("{} -> {new}", account.username),
            now,
        )
        .await?;
    if let Some(social) = social {
        crate::routes::social::profile::broadcast_self_update(&social, p.user_id).await;
    }
    Ok(Json(json!({ "username": new })))
}

/// Body for `POST /sr/auth/email/change`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ChangeEmailBody {
    new_email: String,
    /// Current password.
    password: String,
}

/// `POST /sr/auth/email/change {new_email, password}`: start an email change. A
/// confirmation link goes to the new address; the old one stays until it is used.
/// One request per minute per account. `204` also when the address belongs to
/// someone else, so the endpoint is not an account-existence oracle (the mail is
/// simply not sent).
#[utoipa::path(
    post,
    path = "/sr/auth/email/change",
    request_body = ChangeEmailBody,
    responses(
        (status = 204, description = "Confirmation link sent, if the address is free"),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 429, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn change_email(
    State(state): State<AppState>,
    Svc { svc, .. }: Svc,
    UserOnly(p): UserOnly,
    Json(body): Json<ChangeEmailBody>,
) -> R<StatusCode> {
    session_id(&p)?;
    let now = state.now_secs();
    let account = current_account(&svc, p.user_id).await?;
    let new = validate::email(&body.new_email).map_err(|m| {
        AuthRouteError::from(ApiError::invalid_form().field("new_email", "invalid", m))
    })?;
    reauthenticate(&svc, now, &account, &body.password).await?;
    if new == account.email {
        return Err(ApiError::invalid_form()
            .field("new_email", "unchanged", "That is already your email.")
            .into());
    }
    svc.ip_limits
        .hit(
            "email-change",
            &account.id.to_string(),
            now,
            &[Rule { limit: 1, secs: 60 }],
        )
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;
    if svc.store.find_by_email(&new).await?.is_some() {
        return Ok(StatusCode::NO_CONTENT);
    }
    let token = EmailToken::generate(EmailTokenPurpose::ChangeEmail)?;
    svc.store
        .email_change_put(
            account.id,
            &new,
            &token.hash(),
            token.expires_at_in(now, svc.cfg.verify_ttl_secs),
        )
        .await?;
    let link = svc.link("verify-email", token.expose());
    svc.mailer
        .send(mail::change_email(&new, &link))
        .await
        .map_err(|e| {
            tracing::warn!("email change mail not sent: {e}");
            AuthRouteError::internal("email change mail")
        })?;
    Ok(StatusCode::NO_CONTENT)
}

/// Apply a consumed change-email token: swap the address (verified), audit it and
/// tell the old address. `Ok(false)` when the new address was taken in the meantime.
pub(super) async fn apply_email_change(
    svc: &std::sync::Arc<super::common::AuthService>,
    now: i64,
    user_id: i64,
    new_email: &str,
) -> R<bool> {
    let account = current_account(svc, user_id).await?;
    if !svc.store.set_email(user_id, new_email).await? {
        return Ok(false);
    }
    svc.store
        .audit(
            user_id,
            "account.email_change",
            Some(user_id),
            "email changed",
            now,
        )
        .await?;
    let svc2 = svc.clone();
    let old = account.email;
    tokio::spawn(async move {
        if let Err(e) = svc2.mailer.send(mail::email_changed(&old)).await {
            tracing::warn!("email change notice not sent: {e}");
        }
    });
    Ok(true)
}

/// OpenAPI fragment for the signed-in account edits.
#[derive(OpenApi)]
#[openapi(
    paths(change_password, change_username, change_email),
    components(schemas(ChangePasswordBody, ChangeUsernameBody, ChangeEmailBody))
)]
pub struct IdentityApiDoc;
