//! Sign-in alerts (ENT-AUTH-015): the account's switch for the "new sign-in" mail. The mail itself is sent
//! from [`AuthService::note_login_device`](super::common) whenever a session starts on a device label the
//! account has not used before.

use super::{
    common::{R, Svc},
    sessions::session_id,
};
use crate::auth::UserOnly;
use axum::{Json, http::StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

/// `GET /sr/auth/login-alerts`: `{"enabled": bool}` (on by default).
pub async fn get(UserOnly(p): UserOnly, Svc { svc, .. }: Svc) -> R<Json<Value>> {
    session_id(&p)?;
    Ok(Json(
        json!({ "enabled": svc.store.login_alerts_enabled(p.user_id).await? }),
    ))
}

/// Body of `PUT /sr/auth/login-alerts`.
#[derive(Debug, Deserialize)]
pub struct AlertsBody {
    enabled: bool,
}

/// `PUT /sr/auth/login-alerts {enabled}`: turn the new-device mail on or off.
pub async fn put(
    UserOnly(p): UserOnly,
    Svc { svc, .. }: Svc,
    Json(body): Json<AlertsBody>,
) -> R<StatusCode> {
    session_id(&p)?;
    svc.store.set_login_alerts(p.user_id, body.enabled).await?;
    Ok(StatusCode::NO_CONTENT)
}
