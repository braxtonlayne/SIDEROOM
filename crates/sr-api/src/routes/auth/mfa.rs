//! TOTP enrolment, confirmation and removal (ENT-AUTH-008).

use super::{
    common::{AuthRouteError, AuthService, R, Svc},
    identity::{MfaProof, require_second_factor},
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
/// re-auth). Uses its own per-account key (`reauth:<id>`, separate from the login
/// and second-factor keys) with the same back-off policy, so wrong passwords here
/// do not lock sign-in and the reverse.
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

/// D-573: a self-service change to how the account signs in takes the password and, when the
/// account has a second factor (TOTP or a passkey), the D-167 proof (`mfa`, or a check on this
/// session in the last 15 minutes; a recovery code counts). A trusted device skips the second
/// factor at sign-in, so the password alone must not remove a factor or add a sign-in method.
pub(super) async fn reauthenticate_with_factor(
    svc: &AuthService,
    now: i64,
    account: &AccountRecord,
    session: i64,
    password: &str,
    mfa: Option<&MfaProof>,
) -> R<()> {
    reauthenticate(svc, now, account, password).await?;
    require_second_factor(svc, now, account, session, mfa).await
}

/// `POST /sr/auth/totp/enroll {password}`: create a pending secret and return the
/// provisioning URI (render as a QR code) and the manual-entry key. The password is
/// re-checked first (SEC-AUD-022): a session alone must not be able to put its own
/// authenticator on the account and lock the owner out. An account that already has a
/// passkey proves it as well (`mfa`, D-573): adding a second factor needs the existing one.
pub async fn enroll(
    State(state): State<AppState>,
    UserOnly(p): UserOnly,
    Svc { svc, .. }: Svc,
    Json(body): Json<PasswordBody>,
) -> R<Json<Value>> {
    let sid = session_id(&p)?;
    let now = state.now_secs();
    let account = current_account(&svc, p.user_id).await?;
    reauthenticate(&svc, now, &account, &body.password).await?;
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
    require_second_factor(&svc, now, &account, sid, body.mfa.as_ref()).await?;
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
    UserOnly(p): UserOnly,
    Svc { svc, .. }: Svc,
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
    svc.notify_security(account.id, "An authenticator app was added")
        .await;
    Ok(Json(json!({ "recovery_codes": codes })))
}

/// Body for actions that re-confirm the password (and, D-573, the second factor).
#[derive(Debug, Deserialize)]
pub struct PasswordBody {
    pub(super) password: String,
    /// Second factor, when the account has one and this session has not passed a check in
    /// the last 15 minutes (D-167, D-573).
    #[serde(default)]
    pub(super) mfa: Option<MfaProof>,
}

/// `DELETE /sr/auth/totp {password, mfa?}`: disable two-factor and drop the
/// recovery codes. The second factor is proved too (D-573).
pub async fn disable(
    State(state): State<AppState>,
    UserOnly(p): UserOnly,
    Svc { svc, .. }: Svc,
    Json(body): Json<PasswordBody>,
) -> R<StatusCode> {
    let sid = session_id(&p)?;
    let account = current_account(&svc, p.user_id).await?;
    reauthenticate_with_factor(
        &svc,
        state.now_secs(),
        &account,
        sid,
        &body.password,
        body.mfa.as_ref(),
    )
    .await?;
    svc.store.totp_delete(account.id).await?;
    // Passkeys still need the recovery codes.
    if !svc.has_passkeys(account.id).await? {
        svc.store.recovery_replace_all(account.id, &[]).await?;
    }
    svc.notify_security(account.id, "The authenticator app was removed")
        .await;
    Ok(StatusCode::NO_CONTENT)
}
