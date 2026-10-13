//! TOTP enrolment, confirmation and removal (ENT-AUTH-008).

use super::{
    common::{AuthRouteError, AuthService, R, Svc},
    recovery::issue_recovery_codes,
    sessions::session_id,
    store::AccountRecord,
};
use crate::{auth::UserOnly, state::AppState};
use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use sr_auth::totp::{self, TotpSecret};
use sr_core::{ApiError, ErrorCode};

/// Load the caller's active account.
pub(super) async fn current_account(svc: &AuthService, user_id: i64) -> R<AccountRecord> {
    svc.store
        .find_by_id(user_id)
        .await?
        .ok_or_else(|| ErrorCode::UnknownAccount.into())
}

/// Re-check the caller's password before a sensitive change (doc 06 section 2:
/// re-auth). Shares the failure lockout with login.
pub(super) async fn reauthenticate(
    svc: &AuthService,
    now: i64,
    account: &AccountRecord,
    password: &str,
) -> R<()> {
    let key = format!("reauth:{}", account.id);
    svc.account_lock
        .check(&key, now)
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;
    if password.len() <= 1024 && svc.verify_password(password, &account.password_hash).await {
        svc.account_lock.clear(&key);
        return Ok(());
    }
    svc.account_lock.fail(&key, now);
    Err(ApiError::with_message(ErrorCode::SrReauthRequired, "That password doesn't match.").into())
}

/// `POST /sr/auth/totp/enroll`: create a pending secret and return the
/// provisioning URI (render as a QR code) and the manual-entry key.
pub async fn enroll(Svc { svc, .. }: Svc, UserOnly(p): UserOnly) -> R<Json<Value>> {
    session_id(&p)?;
    let account = current_account(&svc, p.user_id).await?;
    if svc
        .store
        .totp_get(account.id)
        .await?
        .is_some_and(|t| t.confirmed)
    {
        return Err(ApiError::with_message(
            ErrorCode::General,
            "Two-factor authentication is already enabled.",
        )
        .into());
    }
    let e = totp::enroll(&svc.cfg.issuer, &account.username)?;
    svc.store
        .totp_put_pending(account.id, e.secret.as_bytes())
        .await?;
    Ok(Json(json!({
        "otpauth_uri": e.otpauth_uri,
        "manual_entry_key": e.manual_entry_key,
    })))
}

/// Body carrying a one-time code.
#[derive(Debug, Deserialize)]
pub struct CodeBody {
    code: String,
}

/// `POST /sr/auth/totp/verify {code}`: confirm enrolment. Returns the ten
/// recovery codes, shown once.
pub async fn verify(
    State(state): State<AppState>,
    Svc { svc, .. }: Svc,
    UserOnly(p): UserOnly,
    Json(body): Json<CodeBody>,
) -> R<Json<Value>> {
    session_id(&p)?;
    let now = state.now_secs();
    let account = current_account(&svc, p.user_id).await?;
    let key = format!("mfa-enroll:{}", account.id);
    svc.account_lock
        .check(&key, now)
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;
    let rec = svc
        .store
        .totp_get(account.id)
        .await?
        .filter(|t| !t.confirmed)
        .ok_or_else(|| {
            AuthRouteError::from(ApiError::with_message(
                ErrorCode::General,
                "Start enrolment first.",
            ))
        })?;
    let secret = TotpSecret::from_bytes(&rec.secret)?;
    let step = if body.code.len() <= 16 {
        totp::verify(
            &secret,
            &svc.cfg.issuer,
            &account.username,
            &body.code,
            u64::try_from(now).unwrap_or(0),
        )?
    } else {
        None
    };
    let Some(step) = step else {
        svc.account_lock.fail(&key, now);
        return Err(ApiError::invalid_form()
            .field(
                "code",
                "invalid",
                "That code didn't match. Try the next one.",
            )
            .into());
    };
    svc.account_lock.clear(&key);
    svc.store.totp_confirm(account.id, step).await?;
    let codes = issue_recovery_codes(&svc, account.id).await?;
    Ok(Json(json!({ "recovery_codes": codes })))
}

/// Body for actions that re-confirm the password.
#[derive(Debug, Deserialize)]
pub struct PasswordBody {
    pub(super) password: String,
}

/// `DELETE /sr/auth/totp {password}`: disable two-factor and drop the
/// recovery codes.
pub async fn disable(
    State(state): State<AppState>,
    Svc { svc, .. }: Svc,
    UserOnly(p): UserOnly,
    Json(body): Json<PasswordBody>,
) -> R<StatusCode> {
    session_id(&p)?;
    let account = current_account(&svc, p.user_id).await?;
    reauthenticate(&svc, state.now_secs(), &account, &body.password).await?;
    svc.store.totp_delete(account.id).await?;
    // Passkeys still need the recovery codes.
    if !svc.has_passkeys(account.id).await? {
        svc.store.recovery_replace_all(account.id, &[]).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}
