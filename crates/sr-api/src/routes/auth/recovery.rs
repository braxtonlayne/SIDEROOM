//! Recovery codes (ENT-AUTH-009). Consumption happens in
//! [`super::login::second_factor`].

use super::{
    common::{AuthRouteError, AuthService, R, Svc},
    identity::require_second_factor,
    mfa::{PasswordBody, current_account, reauthenticate},
    sessions::session_id,
};
use crate::{auth::UserOnly, state::AppState};
use axum::{Json, extract::State};
use serde_json::{Value, json};
use sr_auth::recovery::generate_batch;
use sr_core::{ApiError, ErrorCode};

/// Generate a fresh batch, replace the stored digests and return the
/// plaintext codes (formatted) for one-time display.
pub(super) async fn issue_recovery_codes(
    svc: &AuthService,
    user_id: i64,
) -> Result<Vec<String>, AuthRouteError> {
    let batch = generate_batch()?;
    svc.store
        .recovery_replace_all(user_id, &batch.hashes)
        .await?;
    Ok(batch.codes.iter().map(|c| c.formatted()).collect())
}

/// `POST /sr/auth/recovery-codes {password, mfa?}`: regenerate the recovery codes
/// (invalidating the old ones). Needs the password, a second factor (a
/// confirmed TOTP or at least one passkey, D-580) and its proof (D-573).
pub async fn regenerate(
    State(state): State<AppState>,
    UserOnly(p): UserOnly,
    Svc { svc, .. }: Svc,
    Json(body): Json<PasswordBody>,
) -> R<Json<Value>> {
    let sid = session_id(&p)?;
    let now = state.now_secs();
    let account = current_account(&svc, p.user_id).await?;
    reauthenticate(&svc, now, &account, &body.password).await?;
    let has_totp = svc
        .store
        .totp_get(account.id)
        .await?
        .is_some_and(|t| t.confirmed);
    if !has_totp && !svc.has_passkeys(account.id).await? {
        return Err(ApiError::with_message(
            ErrorCode::General,
            "Enable two-factor authentication first.",
        )
        .into());
    }
    require_second_factor(&svc, now, &account, sid, body.mfa.as_ref()).await?;
    let codes = issue_recovery_codes(&svc, account.id).await?;
    svc.notify_security(account.id, "Your recovery codes were regenerated")
        .await;
    Ok(Json(json!({ "recovery_codes": codes })))
}
