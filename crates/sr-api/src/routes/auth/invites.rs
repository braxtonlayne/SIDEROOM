//! Public pre-registration checks: invite validity (ENT-INV-002) and username
//! availability (ENT-AUTH-002).

use super::{
    common::{AuthRouteError, R, Svc},
    store::InviteState,
    throttle::Rule,
    validate,
};
use crate::state::AppState;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sr_auth::invite::InviteCode;

/// Doc 06 section 3: 10 per minute and 50 per day per IP.
const INVITE_RULES: [Rule; 2] = [
    Rule {
        limit: 10,
        secs: 60,
    },
    Rule {
        limit: 50,
        secs: 86_400,
    },
];

/// `GET /sr/invites/{code}`: `{valid: true}`, `{valid: false}`,
/// `{valid: false, reason: "closed"}` (the house is full) or `{valid: false, reason: "locked"}` (Lock the Doors,
/// OPS-INV-001; the answer does not depend on the code). Unknown, malformed, used and revoked
/// codes are indistinguishable. When the cap is reached every code answers
/// `closed`, so the response does not depend on the code at all.
pub async fn check_invite(
    State(state): State<AppState>,
    Svc { svc, ip }: Svc,
    Path(code): Path<String>,
) -> R<Json<Value>> {
    svc.ip_limits
        .hit("invite-check", &ip, state.now_secs(), &INVITE_RULES)
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;

    if svc.store.invite_controls().await?.locked {
        return Ok(Json(json!({ "valid": false, "reason": "locked" })));
    }
    if svc.store.counted_accounts().await? >= svc.account_cap().await? {
        return Ok(Json(json!({ "valid": false, "reason": "closed" })));
    }
    let valid = match InviteCode::parse(&code) {
        Ok(c) => svc.store.invite_state(&c).await? == Some(InviteState::Open),
        Err(_) => false,
    };
    Ok(Json(json!({ "valid": valid })))
}

/// Query for [`check_username`].
#[derive(Debug, Deserialize)]
pub struct UsernameQuery {
    username: String,
}

/// `GET /sr/usernames/check?username=`: `{available}`. Invalid or reserved
/// names are reported unavailable.
pub async fn check_username(
    State(state): State<AppState>,
    Svc { svc, ip }: Svc,
    Query(q): Query<UsernameQuery>,
) -> R<Json<Value>> {
    svc.ip_limits
        .hit(
            "username-check",
            &ip,
            state.now_secs(),
            &[Rule {
                limit: 30,
                secs: 60,
            }],
        )
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;
    let available = match validate::username(&q.username) {
        Ok(u) => !svc.store.username_exists(&u).await?,
        Err(_) => false,
    };
    Ok(Json(json!({ "available": available })))
}
