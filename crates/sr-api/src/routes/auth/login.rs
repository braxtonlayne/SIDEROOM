//! `POST /sr/auth/login`, `POST /sr/auth/login-link` and `POST /sr/auth/2fa`
//! (ENT-AUTH-007/008/009; open questions 43, 143 and 144).

use super::{
    common::{
        AuthRouteError, AuthService, MFA_COOKIE, R, Svc, TRUSTED_COOKIE, clear_mfa_cookie,
        mfa_cookie, trusted_cookie, user_agent,
    },
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
    email_token::{EmailToken, EmailTokenPurpose},
    recovery,
    session::{DeviceToken, SessionToken, TokenHash},
    totp::{self, TotpSecret},
};
use sr_core::{ApiError, ErrorCode};

/// Copy shown for any failed credential check (no oracle).
const NO_MATCH: &str = "That doesn't match our list.";

pub(super) fn no_match() -> AuthRouteError {
    ApiError::with_message(ErrorCode::Unauthorized, NO_MATCH).into()
}

fn invalid_link() -> AuthRouteError {
    ApiError::invalid_form()
        .field("token", "invalid", "This link is invalid or has expired.")
        .into()
}

/// The `methods` a login that needs a second factor offers (open question 83).
fn second_factor_methods(has_totp: bool, has_passkey: bool) -> Vec<&'static str> {
    let mut methods = Vec::new();
    if has_passkey {
        methods.push("passkey");
    }
    if has_totp {
        methods.push("totp");
    }
    methods.push("recovery");
    methods
}

/// Request body for login.
#[derive(Debug, Deserialize)]
pub struct LoginBody {
    login: String,
    password: String,
}

/// Username-or-email plus password. On success sets the session cookie and
/// answers `{status:"ok"}`; if a second factor is enrolled and this browser
/// is not trusted, it sets the short-lived `sr_mfa` cookie and answers
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
    // A ceiling per login name across all addresses (open question 143).
    svc.ip_limits
        .hit(
            "login-name",
            &login,
            now,
            &[Rule {
                limit: 50,
                secs: 3600,
            }],
        )
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;
    // Keyed by (name, address): a victim's name cannot be locked out from
    // elsewhere, and unknown names lock the same way as known ones.
    let acct_key = format!("login:{login}\n{ip}");
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
    let has_passkey = svc.has_passkeys(account.id).await?;
    if (has_totp || has_passkey) && !is_trusted(&svc, &state, &headers, account.id).await? {
        let token = SessionToken::generate()?;
        svc.pending
            .insert(&token.hash(), account.id, now + svc.cfg.mfa_ttl_secs, now);
        let cookie = mfa_cookie(&token, secure, svc.cfg.mfa_ttl_secs);
        return Ok((
            AppendHeaders([(header::SET_COOKIE, cookie)]),
            Json(json!({
                "status": "mfa_required",
                "methods": second_factor_methods(has_totp, has_passkey),
            })),
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

/// Whether the request carries a live "trust this device" cookie for `user_id`.
async fn is_trusted(
    svc: &AuthService,
    state: &AppState,
    headers: &HeaderMap,
    user_id: i64,
) -> R<bool> {
    let Some(token) = parse_cookie(headers, TRUSTED_COOKIE).and_then(DeviceToken::parse) else {
        return Ok(false);
    };
    Ok(svc
        .store
        .trusted_device_matches(user_id, &token.hash(), state.now_secs())
        .await?)
}

/// Request body for the one-time Owner login link (open question 43).
#[derive(Debug, Deserialize)]
pub struct LoginLinkBody {
    token: String,
}

/// `POST /sr/auth/login-link {token}`: exchange the one-time link printed by
/// `sr-cli bootstrap` for a session. Single use, 1 hour. Never resets a
/// password.
pub async fn login_link(
    State(state): State<AppState>,
    Svc { svc, ip }: Svc,
    headers: HeaderMap,
    Json(body): Json<LoginLinkBody>,
) -> R<impl IntoResponse> {
    let now = state.now_secs();
    svc.ip_limits
        .hit(
            "login-link",
            &ip,
            now,
            &[Rule {
                limit: 10,
                secs: 600,
            }],
        )
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;
    let token =
        EmailToken::parse(EmailTokenPurpose::LoginLink, &body.token).ok_or_else(invalid_link)?;
    let user_id = svc
        .store
        .email_token_consume(EmailTokenPurpose::LoginLink, &token.hash(), now)
        .await?
        .ok_or_else(invalid_link)?;
    let account = svc
        .store
        .find_by_id(user_id)
        .await?
        .filter(|a| a.state == AccountState::Active)
        .ok_or_else(invalid_link)?;
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
    /// Remember this browser for 30 days (open question 144).
    #[serde(default)]
    trust_device: bool,
}

/// Result of [`check_second_factor`].
pub(super) struct FactorCheck {
    /// The code was valid (and is now spent).
    pub ok: bool,
    /// Recovery codes left after a recovery code was used.
    pub remaining_recovery: Option<usize>,
}

/// Check a TOTP, recovery or passkey `code` for `account` and spend it. Used by the login
/// second step and by sensitive changes that re-prompt (email change, F-03). The
/// caller owns lockouts. For `passkey`, `code` is the JSON string `{ceremony_id, credential}`.
pub(super) async fn check_second_factor(
    svc: &AuthService,
    account: &super::store::AccountRecord,
    method: &str,
    code: &str,
    now: i64,
) -> R<FactorCheck> {
    let mut remaining_recovery = None;
    let ok = match method {
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
                        code,
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
            let hashes: Vec<TokenHash> = unused.iter().map(|(_, h)| *h).collect();
            match recovery::find_match(code, &hashes) {
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
        "passkey" => super::passkeys::check_proof(svc, account.id, code, now).await?,
        _ => {
            return Err(ApiError::invalid_form()
                .field("method", "invalid", "Unknown second-factor method.")
                .into());
        }
    };
    Ok(FactorCheck {
        ok,
        remaining_recovery,
    })
}

/// Complete a login that answered `mfa_required`, with a TOTP code or a
/// recovery code. With `trust_device` the browser also gets the 30-day
/// `sr_trusted` cookie.
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
    let lock_key = format!("mfa:{}\n{ip}", pending.user_id);
    svc.account_lock
        .check(&lock_key, now)
        .and_then(|()| svc.ip_lock.check(&ip, now))
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;
    let max_code = if body.method == "passkey" {
        super::passkeys::MAX_PROOF_LEN
    } else {
        64
    };
    if body.code.len() > max_code {
        return Err(no_match());
    }

    let account = svc
        .store
        .find_by_id(pending.user_id)
        .await?
        .filter(|a| a.state == AccountState::Active)
        .ok_or_else(no_match)?;

    let FactorCheck {
        ok,
        remaining_recovery,
    } = check_second_factor(&svc, &account, body.method.as_str(), &body.code, now).await?;

    if !ok {
        svc.pending.fail(&hash, svc.cfg.mfa_max_attempts);
        svc.account_lock.fail(&lock_key, now);
        svc.ip_lock.fail(&ip, now);
        return Err(no_match());
    }
    svc.pending.remove(&hash);
    svc.account_lock.clear(&lock_key);

    let trust_cookie = if body.trust_device {
        let device = DeviceToken::generate()?;
        let name = super::validate::device_name(user_agent(&headers));
        svc.store
            .trusted_device_put(
                account.id,
                &device.hash(),
                name.as_deref(),
                now + svc.cfg.trusted_device_secs,
            )
            .await?;
        Some(trusted_cookie(&device, secure, svc.cfg.trusted_device_secs))
    } else {
        None
    };
    let (_, cookie) = svc
        .start_session_mfa(&state, account.id, user_agent(&headers))
        .await?;
    let mut out = json!({ "status": "ok" });
    if let Some(n) = remaining_recovery {
        out["recovery_codes_remaining"] = json!(n);
    }
    let mut cookies = vec![
        (header::SET_COOKIE, cookie),
        (header::SET_COOKIE, clear_mfa_cookie(secure)),
    ];
    if let Some(c) = trust_cookie {
        cookies.push((header::SET_COOKIE, c));
    }
    Ok((AppendHeaders(cookies), Json(out)))
}
