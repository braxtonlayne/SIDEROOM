//! Recovery codes (ENT-AUTH-009). Consumption happens in
//! [`super::login::second_factor`].

use super::{
    common::{AuthRouteError, AuthService, R, Svc},
    mfa::{PasswordBody, current_account, reauthenticate},
    sessions::session_id,
};
use crate::{auth::UserOnly, state::AppState};
use axum::{Json, extract::State};
use serde_json::{Value, json};
use sr_auth::recovery::generate_batch;
use sr_core::{ApiError, ErrorCode};

/// Generate a fresh batch, replace the stored hashes and return the
/// plaintext codes (formatted) for one-time display.
pub(super) async fn issue_recovery_codes(
    svc: &AuthService,
    user_id: i64,
) -> Result<Vec<String>, AuthRouteError> {
    let hasher = svc.hasher.clone();
    let batch = tokio::task::spawn_blocking(move || generate_batch(&hasher))
        .await
        .map_err(|_| AuthRouteError::internal("recovery batch task"))??;
    svc.store
        .recovery_replace_all(user_id, &batch.hashes)
        .await?;
    Ok(batch.codes.iter().map(|c| c.formatted()).collect())
}

/// `POST /sr/auth/recovery-codes {password}`: regenerate the recovery codes
/// (invalidating the old ones). Requires two-factor to be enabled.
pub async fn regenerate(
    State(state): State<AppState>,
    Svc { svc, .. }: Svc,
    UserOnly(p): UserOnly,
    Json(body): Json<PasswordBody>,
) -> R<Json<Value>> {
    session_id(&p)?;
    let account = current_account(&svc, p.user_id).await?;
    reauthenticate(&svc, state.now_secs(), &account, &body.password).await?;
    if !svc
        .store
        .totp_get(account.id)
        .await?
        .is_some_and(|t| t.confirmed)
    {
        return Err(ApiError::with_message(
            ErrorCode::General,
            "Enable two-factor authentication first.",
        )
        .into());
    }
    let codes = issue_recovery_codes(&svc, account.id).await?;
    Ok(Json(json!({ "recovery_codes": codes })))
}
