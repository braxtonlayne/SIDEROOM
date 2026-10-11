//! Logout, logout-all and the device list (ENT-AUTH-005).

use super::{
    common::{R, Svc, clear_session_cookie},
    validate::iso8601,
};
use crate::{
    auth::{AuthKind, Principal, UserOnly},
    state::AppState,
};
use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{AppendHeaders, IntoResponse},
};
use serde_json::{Value, json};
use sr_core::ErrorCode;

/// The calling session id; bots and bearer tokens are refused.
pub(super) fn session_id(p: &Principal) -> Result<i64, ErrorCode> {
    match p.kind {
        AuthKind::Session { session_id } => Ok(session_id),
        _ => Err(ErrorCode::MissingAccess),
    }
}

/// `POST /sr/auth/logout`: revoke the current session and clear the cookie.
pub async fn logout(
    State(state): State<AppState>,
    Svc { svc, .. }: Svc,
    UserOnly(p): UserOnly,
) -> R<impl IntoResponse> {
    let sid = session_id(&p)?;
    svc.store.revoke_session(p.user_id, sid).await?;
    Ok((
        StatusCode::NO_CONTENT,
        AppendHeaders([(
            header::SET_COOKIE,
            clear_session_cookie(state.settings().secure_cookies),
        )]),
    ))
}

/// `POST /sr/auth/logout-all`: revoke every session of the account,
/// including this one.
pub async fn logout_all(
    State(state): State<AppState>,
    Svc { svc, .. }: Svc,
    UserOnly(p): UserOnly,
) -> R<impl IntoResponse> {
    session_id(&p)?;
    svc.store.revoke_sessions_except(p.user_id, None).await?;
    Ok((
        StatusCode::NO_CONTENT,
        AppendHeaders([(
            header::SET_COOKIE,
            clear_session_cookie(state.settings().secure_cookies),
        )]),
    ))
}

/// `GET /sr/auth/sessions`: the account's live sessions, newest first.
pub async fn list(
    State(state): State<AppState>,
    Svc { svc, .. }: Svc,
    UserOnly(p): UserOnly,
) -> R<Json<Value>> {
    let current = session_id(&p)?;
    let mut rows = svc.store.list_sessions(p.user_id, state.now_secs()).await?;
    rows.sort_by_key(|r| std::cmp::Reverse((r.created_at, r.id)));
    let out: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id.to_string(),
                "device_name": r.device_name,
                "created_at": iso8601(r.created_at),
                "last_seen_at": iso8601(r.last_seen_at),
                "expires_at": iso8601(r.expires_at),
                "current": r.id == current,
            })
        })
        .collect();
    Ok(Json(Value::Array(out)))
}

/// `DELETE /sr/auth/sessions/{id}`: revoke one of the caller's sessions.
pub async fn revoke(
    Svc { svc, .. }: Svc,
    UserOnly(p): UserOnly,
    Path(id): Path<String>,
) -> R<StatusCode> {
    session_id(&p)?;
    let id: i64 = id.parse().map_err(|_| ErrorCode::UnknownSession)?;
    if svc.store.revoke_session(p.user_id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ErrorCode::UnknownSession.into())
    }
}
