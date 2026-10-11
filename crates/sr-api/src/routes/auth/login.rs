//! `POST /sr/auth/login` and `POST /sr/auth/2fa` (ENT-AUTH-007/008/009).

use super::{
    common::{AuthRouteError, MFA_COOKIE, R, Svc, clear_mfa_cookie, mfa_cookie, user_agent},
    store::AccountState,
    throttle::Rule,
};
use crate::{auth::parse_cookie, state::AppState};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, header},
    response::{AppendHeaders, IntoResponse},
};
use serde::Deserialize;
use serde_json::json;
use sr_auth::{
    recovery,
    session::SessionToken,
    totp::{self, TotpSecret},
};
use sr_core::{ApiError, ErrorCode};

/// Copy shown for any failed credential check (no oracle).
const NO_MATCH: &str = "That doesn't match our list.";

fn no_match() -> AuthRouteError {
    ApiError::with_message(ErrorCode::Unauthorized, NO_MATCH).into()
}

/// Request body for login.
#[derive(Debug, Deserialize)]
pub struct LoginBody {
    login: String,
    password: String,
}

/// Username-or-email plus password. On success sets the session cookie and
/// answers `{status:"ok"}`; if a second factor is enrolled it sets the
/// short-lived `sr_mfa` cookie and answers
/// `{status:"mfa_required", methods:[..]}`.
pub async fn login(
    State(state): State<AppState>,
    Svc { svc, ip }: Svc,
    headers: HeaderMap,
    Json(body): Json<LoginBody>,
) -> R<impl IntoResponse> {
    let now = state.now_secs();
    let login = body.login.trim().to_ascii_lowercase();
    if login.is_empty() || login.len() > 254 || body.password.len() > 1024 {
        return Err(no_match());
    }
    svc.ip_limits
        .hit(
            "login",
            &ip,
            now,
            &[Rule {
                limit: 30,
                secs: 60,
            }],
        )
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;
    // Keyed by the submitted name, not the account, so existence is not an oracle.
    let acct_key = format!("login:{login}");
    svc.account_lock
        .check(&acct_key, now)
        .and_then(|()| svc.ip_lock.check(&ip, now))
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;

    let account = svc
        .store
        .find_by_login(&login)
        .await?
        .filter(|a| a.state != AccountState::Deleted);
    let verified = match &account {
        Some(a) => svc.verify_password(&body.password, &a.password_hash).await,
        None => {
            svc.dummy_verify(&body.password).await;
            false
        }
    };
    let Some(account) = account.filter(|_| verified) else {
        svc.account_lock.fail(&acct_key, now);
        svc.ip_lock.fail(&ip, now);
        tracing::info!(target: "auth", "failed login");
        return Err(no_match());
    };
    // The password was right, so naming the account state leaks nothing.
    match account.state {
        AccountState::Deactivated => return Err(ErrorCode::SrAccountDeactivated.into()),
        AccountState::PlatformBanned => return Err(ErrorCode::SrPlatformBanned.into()),
        AccountState::Deleted => return Err(no_match()),
        AccountState::Active => {}
    }
    svc.account_lock.clear(&acct_key);

    if svc.hasher.needs_rehash(&account.password_hash)
        && let Ok(h) = svc.hash_password(&body.password).await
        && let Err(e) = svc.store.set_password_hash(account.id, &h).await
    {
        tracing::warn!(error = %e, "password rehash not stored");
    }

    let secure = state.settings().secure_cookies;
    let has_totp = svc
        .store
        .totp_get(account.id)
        .await?
        .is_some_and(|t| t.confirmed);
    if has_totp {
        let token = SessionToken::generate()?;
        svc.pending
            .insert(&token.hash(), account.id, now + svc.cfg.mfa_ttl_secs, now);
        let cookie = mfa_cookie(&token, secure, svc.cfg.mfa_ttl_secs);
        return Ok((
            AppendHeaders([(header::SET_COOKIE, cookie)]),
            Json(json!({ "status": "mfa_required", "methods": ["totp", "recovery"] })),
        ));
    }
    let (_, cookie) = svc
        .start_session(&state, account.id, user_agent(&headers))
        .await?;
    Ok((
        AppendHeaders([(header::SET_COOKIE, cookie)]),
        Json(json!({ "status": "ok" })),
    ))
}

/// Request body for the second factor.
#[derive(Debug, Deserialize)]
pub struct SecondFactorBody {
    method: String,
    code: String,
    #[serde(default)]
    #[allow(dead_code)]
    trust_device: bool,
}

/// Complete a login that answered `mfa_required`, with a TOTP code or a
/// recovery code. `trust_device` is accepted and currently ignored (open
/// question).
pub async fn second_factor(
    State(state): State<AppState>,
    Svc { svc, ip }: Svc,
    headers: HeaderMap,
    Json(body): Json<SecondFactorBody>,
) -> R<impl IntoResponse> {
    let now = state.now_secs();
    let secure = state.settings().secure_cookies;
    let token = parse_cookie(&headers, MFA_COOKIE)
        .and_then(SessionToken::parse)
        .ok_or_else(no_match)?;
    let hash = token.hash();
    let pending = svc.pending.get(&hash, now).ok_or_else(no_match)?;
    let lock_key = format!("mfa:{}", pending.user_id);
    svc.account_lock
        .check(&lock_key, now)
        .and_then(|()| svc.ip_lock.check(&ip, now))
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;
    if body.code.len() > 64 {
        return Err(no_match());
    }

    let account = svc
        .store
        .find_by_id(pending.user_id)
        .await?
        .filter(|a| a.state == AccountState::Active)
        .ok_or_else(no_match)?;

    let mut remaining_recovery = None;
    let ok = match body.method.as_str() {
        "totp" => {
            let rec = svc
                .store
                .totp_get(account.id)
                .await?
                .filter(|t| t.confirmed);
            match rec {
                Some(rec) => {
                    let secret = TotpSecret::from_bytes(&rec.secret)?;
                    let step = totp::verify(
                        &secret,
                        &svc.cfg.issuer,
                        &account.username,
                        &body.code,
                        u64::try_from(now).unwrap_or(0),
                    )?;
                    match step {
                        Some(step) => svc.store.totp_consume_step(account.id, step).await?,
                        None => false,
                    }
                }
                None => false,
            }
        }
        "recovery" => {
            let unused = svc.store.recovery_list_unused(account.id).await?;
            let hashes: Vec<String> = unused.iter().map(|(_, h)| h.clone()).collect();
            let (hasher, code) = (svc.hasher.clone(), body.code.clone());
            let found =
                tokio::task::spawn_blocking(move || recovery::find_match(&hasher, &code, &hashes))
                    .await
                    .map_err(|_| AuthRouteError::internal("recovery task"))??;
            match found {
                Some(i) => {
                    let used = svc
                        .store
                        .recovery_consume(account.id, unused[i].0, now)
                        .await?;
                    if used {
                        remaining_recovery = Some(unused.len() - 1);
                    }
                    used
                }
                None => false,
            }
        }
        _ => {
            return Err(ApiError::invalid_form()
                .field("method", "invalid", "Unknown second-factor method.")
                .into());
        }
    };

    if !ok {
        svc.pending.fail(&hash, svc.cfg.mfa_max_attempts);
        svc.account_lock.fail(&lock_key, now);
        svc.ip_lock.fail(&ip, now);
        return Err(no_match());
    }
    svc.pending.remove(&hash);
    svc.account_lock.clear(&lock_key);
    let (_, cookie) = svc
        .start_session(&state, account.id, user_agent(&headers))
        .await?;
    let mut out = json!({ "status": "ok" });
    if let Some(n) = remaining_recovery {
        out["recovery_codes_remaining"] = json!(n);
    }
    Ok((
        AppendHeaders([
            (header::SET_COOKIE, cookie),
            (header::SET_COOKIE, clear_mfa_cookie(secure)),
        ]),
        Json(out),
    ))
}
