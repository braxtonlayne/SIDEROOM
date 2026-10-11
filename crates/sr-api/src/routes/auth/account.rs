//! Password reset, email verification and the age gate
//! (ENT-AUTH-012/013, doc 06 section 7).

use super::{
    common::{AuthRouteError, R, Svc},
    mail,
    mfa::current_account,
    register::send_verification,
    sessions::session_id,
    store::AccountState,
    throttle::Rule,
    validate,
};
use crate::{auth::UserOnly, state::AppState};
use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;
use sr_auth::email_token::{EmailToken, EmailTokenPurpose};
use sr_core::ApiError;

fn invalid_link() -> AuthRouteError {
    ApiError::invalid_form()
        .field("token", "invalid", "This link is invalid or has expired.")
        .into()
}

/// Body for `/sr/auth/forgot`.
#[derive(Debug, Deserialize)]
pub struct ForgotBody {
    email: String,
}

/// `POST /sr/auth/forgot {email}`. Always `204` for a well-formed request
/// (the copy "If that address is on the list, a link is on its way." lives in
/// the UI). Work happens in a background task so response time does not
/// reveal whether the address exists.
pub async fn forgot(
    State(state): State<AppState>,
    Svc { svc, ip }: Svc,
    Json(body): Json<ForgotBody>,
) -> R<StatusCode> {
    let now = state.now_secs();
    svc.ip_limits
        .hit(
            "forgot",
            &ip,
            now,
            &[
                Rule {
                    limit: 5,
                    secs: 600,
                },
                Rule {
                    limit: 20,
                    secs: 86_400,
                },
            ],
        )
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;
    let Ok(email) = validate::email(&body.email) else {
        // Malformed input says nothing about which addresses exist.
        return Ok(StatusCode::NO_CONTENT);
    };
    // One email per address per minute; excess requests are silently dropped.
    if svc
        .ip_limits
        .hit("forgot-email", &email, now, &[Rule { limit: 1, secs: 60 }])
        .is_err()
    {
        return Ok(StatusCode::NO_CONTENT);
    }
    tokio::spawn(async move {
        let result: Result<(), String> = async {
            let Some(account) = svc
                .store
                .find_by_email(&email)
                .await
                .map_err(|e| e.to_string())?
                .filter(|a| a.state == AccountState::Active)
            else {
                return Ok(());
            };
            let token = EmailToken::generate(EmailTokenPurpose::ResetPassword)
                .map_err(|e| e.to_string())?;
            svc.store
                .email_token_put(
                    account.id,
                    EmailTokenPurpose::ResetPassword,
                    &token.hash(),
                    token.expires_at_in(now, svc.cfg.reset_ttl_secs),
                )
                .await
                .map_err(|e| e.to_string())?;
            let link = svc.link("reset", token.expose());
            svc.mailer
                .send(mail::reset_password(&account.email, &link))
                .await
                .map_err(|e| e.to_string())
        }
        .await;
        if let Err(e) = result {
            tracing::warn!("password reset email not sent: {e}");
        }
    });
    Ok(StatusCode::NO_CONTENT)
}

/// Body for `/sr/auth/reset`.
#[derive(Debug, Deserialize)]
pub struct ResetBody {
    token: String,
    password: String,
}

/// `POST /sr/auth/reset {token, password}`: set a new password with a
/// single-use token (30 minutes), then revoke every session. Two-factor, if
/// enabled, is still required at the next login.
pub async fn reset(
    State(state): State<AppState>,
    Svc { svc, ip }: Svc,
    Json(body): Json<ResetBody>,
) -> R<StatusCode> {
    let now = state.now_secs();
    svc.ip_limits
        .hit(
            "reset",
            &ip,
            now,
            &[Rule {
                limit: 10,
                secs: 3600,
            }],
        )
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;
    let token = EmailToken::parse(EmailTokenPurpose::ResetPassword, &body.token)
        .ok_or_else(invalid_link)?;
    // Check strength before the token is consumed so a weak password does not
    // burn the link. (Personal context is unavailable until then.)
    if let Err(m) = validate::password(&body.password, &["sideroom", "side room"]) {
        return Err(ApiError::invalid_form().field("password", "weak", m).into());
    }
    let user_id = svc
        .store
        .email_token_consume(EmailTokenPurpose::ResetPassword, &token.hash(), now)
        .await?
        .ok_or_else(invalid_link)?;
    let hash = svc.hash_password(&body.password).await?;
    svc.store.set_password_hash(user_id, &hash).await?;
    svc.store.revoke_sessions_except(user_id, None).await?;
    // A changed password also ends "trust this device" on every browser.
    svc.store.trusted_devices_revoke_all(user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Body for `/sr/auth/verify-email`.
#[derive(Debug, Deserialize)]
pub struct VerifyBody {
    token: String,
}

/// `POST /sr/auth/verify-email {token}`: confirm the address. No login needed.
pub async fn verify_email(
    State(state): State<AppState>,
    Svc { svc, ip }: Svc,
    Json(body): Json<VerifyBody>,
) -> R<StatusCode> {
    let now = state.now_secs();
    svc.ip_limits
        .hit(
            "verify",
            &ip,
            now,
            &[Rule {
                limit: 20,
                secs: 600,
            }],
        )
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;
    let token =
        EmailToken::parse(EmailTokenPurpose::VerifyEmail, &body.token).ok_or_else(invalid_link)?;
    let user_id = svc
        .store
        .email_token_consume(EmailTokenPurpose::VerifyEmail, &token.hash(), now)
        .await?
        .ok_or_else(invalid_link)?;
    svc.store.mark_email_verified(user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /sr/auth/verify-email/resend`: send a new verification email (60 s
/// cooldown per account). A no-op once verified.
pub async fn resend_verification(
    State(state): State<AppState>,
    Svc { svc, .. }: Svc,
    UserOnly(p): UserOnly,
) -> R<StatusCode> {
    session_id(&p)?;
    let now = state.now_secs();
    let account = current_account(&svc, p.user_id).await?;
    if account.email_verified {
        return Ok(StatusCode::NO_CONTENT);
    }
    svc.ip_limits
        .hit(
            "verify-resend",
            &account.id.to_string(),
            now,
            &[Rule {
                limit: 1,
                secs: svc.cfg.verify_resend_cooldown_secs,
            }],
        )
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;
    send_verification(&svc, now, account.id, &account.email)
        .await
        .map_err(|e| {
            tracing::warn!("verification email not sent: {e}");
            AuthRouteError::internal("verification mail")
        })?;
    Ok(StatusCode::NO_CONTENT)
}

/// Body for the age gate.
#[derive(Debug, Deserialize)]
pub struct AgeGateBody {
    over_18: bool,
}

/// `POST /sr/age-gate {over_18}`: record the answer to "Are you 18 or
/// older?" (D-032). `true` sets `age_confirmed_at`; `false` hides restricted
/// content. The Settings re-ask path calls the same endpoint.
pub async fn age_gate(
    State(state): State<AppState>,
    Svc { svc, .. }: Svc,
    UserOnly(p): UserOnly,
    Json(body): Json<AgeGateBody>,
) -> R<StatusCode> {
    session_id(&p)?;
    svc.store
        .set_age_confirmed(p.user_id, body.over_18, state.now_secs())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
