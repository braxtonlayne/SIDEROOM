//! Trusted devices ("Trust this device for 30 days", open question 144).
//! Listed and revocable from the Devices settings page.

use super::{
    common::{R, Svc},
    sessions::session_id,
    validate::iso8601,
};
use crate::{auth::UserOnly, state::AppState};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};
use sr_core::ErrorCode;

/// `GET /sr/auth/trusted-devices`: the account's live trusted devices.
pub async fn list(
    State(state): State<AppState>,
    Svc { svc, .. }: Svc,
    UserOnly(p): UserOnly,
) -> R<Json<Value>> {
    session_id(&p)?;
    let rows = svc
        .store
        .trusted_devices_list(p.user_id, state.now_secs())
        .await?;
    let out: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id.to_string(),
                "device_name": r.device_name,
                "created_at": iso8601(r.created_at),
                "expires_at": iso8601(r.expires_at),
            })
        })
        .collect();
    Ok(Json(Value::Array(out)))
}

/// `DELETE /sr/auth/trusted-devices/{id}`: stop trusting one device.
pub async fn revoke(
    Svc { svc, .. }: Svc,
    UserOnly(p): UserOnly,
    Path(id): Path<String>,
) -> R<StatusCode> {
    session_id(&p)?;
    let id: i64 = id.parse().map_err(|_| ErrorCode::UnknownSession)?;
    if svc.store.trusted_device_revoke(p.user_id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ErrorCode::UnknownSession.into())
    }
}
