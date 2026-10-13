//! Passkeys (WebAuthn, ENT-AUTH-010, D-161, open questions 78 and 83).
//!
//! The verifier is the pure-Rust relying party in `sr_auth::webauthn` (`none`
//! attestation; ES256, EdDSA, RS256). This odule owns the ceremonies, the
//! REST routes and the glue to the login state machine:
//!
//! * `POST /sr/auth/passkeys/register/{begin,finish}`: add a passkey. `begin`
//!   re-checks the password; the ceremony is bound to the session.
//! * `POST /sr/auth/passkeys/login/{begin,finish}`: passkey-only login with a
//!   discoverable credential (user verification required, so it counts as two
//!   factors). `begin` with `second_factor: true` instead starts a ceremony for
//!   the account waiting on the `sr_mfa` cookie, finished through
//!   `POST /sr/auth/2fa` with `method: "passkey"`.
//! * `POST /sr/auth/passkeys/verify/begin`: the same ceremony for a signed-in
//!   session that must re-prove a second factor (email change).
//! * `GET /sr/auth/passkeys`, `PATCH`/`DELETE /sr/auth/passkeys/{id}`: manage
//!   credentials. Deleting re-checks the password.
//!
//! Pending ceremonies live in process memory ([`Ceremonies`]), like the pending
//! 2FA table and the lockouts: this is a single-node deployment today, entries are
//! single use and expire after [`CEREMONY_TTL_SECS`]. A multi-node deployment must
//! move them to Postgres together with those tables.
//!
//! A signature counter that does not increase is recorded as the platform audit
//! event `passkey.counter_regression` (actor and target: the credential's owner and
//! the credential row) and the attempt fails like any other bad assertion.

use super::{
    common::{AuthRouteError, AuthService, MFA_COOKIE, R, Svc, clear_mfa_cookie},
    login::no_match,
    mfa::{PasswordBody, current_account, reauthenticate},
    recovery::issue_recovery_codes,
    sessions::session_id,
    store::{AccountState, PasskeyRecord},
    throttle::Rule,
    validate::iso8601,
};
use crate::{auth::UserOnly, openapi::ErrorBody, state::AppState};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{AppendHeaders, IntoResponse},
};
use data_encoding::BASE64URL_NOPAD;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sr_auth::{
    session::SessionToken,
    webauthn::{
        self, AssertionResponse, CEREMONY_TTL_SECS, CoseAlg, PendingCeremony, RelyingParty,
        StoredCredential, UserVerification, WebauthnError,
    },
};
use sr_core::{ApiError, ErrorCode};
use std::{collections::HashMap, sync::Mutex};
use utoipa::{OpenApi, ToSchema};

/// Most passkeys one account may hold.
const MAX_PASSKEYS: usize = 20;
/// Most pending ceremonies kept in memory at once.
const MAX_CEREMONIES: usize = 20_000;
/// Longest accepted base64url field (an attestation object is the largest).
const MAX_FIELD_B64: usize = 16 * 1024;
/// Longest `code` accepted for the `passkey` second-factor method.
pub(super) const MAX_PROOF_LEN: usize = 48 * 1024;

/// What a pending ceremony may be used for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CeremonyCtx {
    /// Adding a passkey, from this session of this account.
    Register { account_id: i64, session_id: i64 },
    /// Passkey-only login (discoverable credential).
    Login,
    /// Second factor for this account.
    SecondFactor { account_id: i64 },
}

struct Entry {
    ctx: CeremonyCtx,
    ceremony: PendingCeremony,
    expires_at: i64,
}

/// Single-use pending ceremonies, keyed by a random id handed to the client.
#[derive(Default)]
pub struct Ceremonies {
    map: Mutex<HashMap<String, Entry>>,
}

impl Ceremonies {
    /// Remember a ceremony; returns its id, or `None` when the table is full.
    fn insert(&self, ctx: CeremonyCtx, ceremony: PendingCeremony, now: i64) -> Option<String> {
        let id = BASE64URL_NOPAD.encode(&sr_auth::random::bytes::<24>().ok()?);
        let mut map = self.map.lock().unwrap_or_else(|p| p.into_inner());
        if map.len() >= MAX_CEREMONIES {
            map.retain(|_, e| e.expires_at >= now);
            if map.len() >= MAX_CEREMONIES {
                return None;
            }
        }
        map.insert(
            id.clone(),
            Entry {
                ctx,
                ceremony,
                expires_at: now.saturating_add(CEREMONY_TTL_SECS),
            },
        );
        Some(id)
    }

    /// Consume a ceremony (it can never be used twice, whatever the outcome).
    fn take(&self, id: &str, now: i64) -> Option<(CeremonyCtx, PendingCeremony)> {
        let mut map = self.map.lock().unwrap_or_else(|p| p.into_inner());
        let e = map.remove(id)?;
        (e.expires_at >= now).then_some((e.ctx, e.ceremony))
    }
}

impl AuthService {
    /// The relying party: the host of `public_url` (or `webauthn_rp_id`) and the exact
    /// origins in `webauthn_origins` plus the `public_url` origin.
    pub(super) fn relying_party(&self) -> Result<RelyingParty, AuthRouteError> {
        let public = self
            .cfg
            .public_url
            .trim_end_matches('/')
            .to_ascii_lowercase();
        let id = match &self.cfg.webauthn_rp_id {
            Some(id) => id.clone(),
            None => host_of(&public).ok_or_else(|| AuthRouteError::internal("webauthn rp id"))?,
        };
        let mut origins = vec![public.clone()];
        for o in &self.cfg.webauthn_origins {
            let o = o.trim_end_matches('/').to_ascii_lowercase();
            if !origins.contains(&o) {
                origins.push(o);
            }
        }
        Ok(RelyingParty {
            id,
            name: self.cfg.issuer.clone(),
            origins,
        })
    }

    /// Whether the account can use a passkey as a second factor.
    pub(super) async fn has_passkeys(&self, account_id: i64) -> Result<bool, AuthRouteError> {
        Ok(!self.store.passkey_list(account_id).await?.is_empty())
    }
}

/// Host part of an origin or URL, without scheme, userinfo, port or path.
fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    if authority.starts_with('[') {
        return None; // IP literals are not valid RP ids
    }
    let host = authority.split(':').next()?.to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

fn too_busy() -> AuthRouteError {
    AuthRouteError::Throttled { retry_after: 30 }
}

fn bad_ceremony() -> AuthRouteError {
    ApiError::with_message(
        ErrorCode::General,
        "That passkey request expired. Start again.",
    )
    .into()
}

fn throttle(r: Result<(), i64>) -> R<()> {
    r.map_err(|retry_after| AuthRouteError::Throttled { retry_after })
}

/// The base64url-encoded credential a browser produced (`PublicKeyCredential`
/// serialised by the web app).
#[derive(Debug, Deserialize, ToSchema)]
pub struct RegistrationCredential {
    /// base64url credential id.
    #[serde(rename = "rawId")]
    raw_id: String,
    /// The authenticator's response.
    response: RegistrationResponse,
}

/// Registration response fields (all base64url, no padding).
#[derive(Debug, Deserialize, ToSchema)]
pub struct RegistrationResponse {
    /// `clientDataJSON`.
    #[serde(rename = "clientDataJSON")]
    client_data_json: String,
    /// `attestationObject`.
    #[serde(rename = "attestationObject")]
    attestation_object: String,
}

/// The assertion a browser produced for `navigator.credentials.get`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AssertionCredential {
    /// base64url credential id.
    #[serde(rename = "rawId")]
    raw_id: String,
    /// The authenticator's response.
    response: AssertionFields,
}

/// Assertion response fields (all base64url, no padding).
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AssertionFields {
    /// `clientDataJSON`.
    #[serde(rename = "clientDataJSON")]
    client_data_json: String,
    /// `authenticatorData`.
    #[serde(rename = "authenticatorData")]
    authenticator_data: String,
    /// `signature`.
    signature: String,
    /// `userHandle` (present for discoverable credentials).
    #[serde(rename = "userHandle", default)]
    user_handle: Option<String>,
}

/// The `code` of the `passkey` second-factor method, as a JSON string.
#[derive(Debug, Deserialize)]
struct PasskeyProof {
    ceremony_id: String,
    credential: AssertionCredential,
}

fn b64(field: &str) -> Option<Vec<u8>> {
    if field.len() > MAX_FIELD_B64 {
        return None;
    }
    BASE64URL_NOPAD.decode(field.as_bytes()).ok()
}

/// A passkey as the settings page shows it.
#[derive(Debug, Serialize, ToSchema)]
pub struct PasskeyView {
    /// Row id (a snowflake as a string).
    id: String,
    /// Label.
    name: String,
    /// ISO 8601.
    created_at: String,
    /// ISO 8601, null until first used.
    last_used_at: Option<String>,
    /// Synced (multi-device) passkey.
    backup_eligible: bool,
}

fn view(p: &PasskeyRecord) -> PasskeyView {
    PasskeyView {
        id: p.id.to_string(),
        name: p.name.clone(),
        created_at: iso8601(p.created_at),
        last_used_at: p.last_used_at.map(iso8601),
        backup_eligible: p.backup_eligible,
    }
}

fn clean_name(raw: &str) -> Option<String> {
    let name: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let n = name.chars().count();
    (1..=64).contains(&n).then_some(name)
}

fn name_error() -> AuthRouteError {
    ApiError::invalid_form()
        .field(
            "name",
            "invalid",
            "Give the passkey a name of up to 64 characters.",
        )
        .into()
}

// ---------------------------------------------------------------------------
// Assertion verification shared by passkey login and the second factor
// ---------------------------------------------------------------------------

/// What a finished assertion must have been issued for.
pub(super) enum Expect {
    /// Passkey-only login: any account.
    Login,
    /// Second factor for exactly this account.
    Account(i64),
}

/// Verify an assertion against a pending ceremony. `Ok(None)` for every failure
/// caused by the client (unknown or reused ceremony, wrong account, bad signature,
/// counter regression); `Err` only for storage failures. On success the use is
/// recorded and the credential returned.
pub(super) async fn verify_assertion(
    svc: &AuthService,
    ceremony_id: &str,
    cred: &AssertionCredential,
    expect: Expect,
    now: i64,
) -> R<Option<PasskeyRecord>> {
    let Some((ctx, ceremony)) = svc.ceremonies.take(ceremony_id, now) else {
        return Ok(None);
    };
    match (&expect, &ctx) {
        (Expect::Login, CeremonyCtx::Login) => {}
        (Expect::Account(a), CeremonyCtx::SecondFactor { account_id }) if a == account_id => {}
        _ => return Ok(None),
    }
    let (Some(raw_id), Some(client_data), Some(auth_data), Some(signature)) = (
        b64(&cred.raw_id),
        b64(&cred.response.client_data_json),
        b64(&cred.response.authenticator_data),
        b64(&cred.response.signature),
    ) else {
        return Ok(None);
    };
    let user_handle = match cred.response.user_handle.as_deref() {
        None | Some("") => None,
        Some(h) => match b64(h) {
            Some(h) => Some(h),
            None => return Ok(None),
        },
    };
    let Some(rec) = svc.store.passkey_find(&raw_id).await? else {
        return Ok(None);
    };
    if let Expect::Account(a) = expect
        && rec.account_id != a
    {
        return Ok(None);
    }
    // Passkey-only login relies on the discoverable flow: the authenticator must name
    // the user, and the name must be the owner's handle.
    let handle = svc
        .store
        .passkey_user_handle(rec.account_id, &sr_auth::random::bytes::<32>()?)
        .await?;
    if matches!(expect, Expect::Login) && user_handle.is_none() {
        return Ok(None);
    }
    let mut aaguid = [0u8; 16];
    if rec.aaguid.len() == 16 {
        aaguid.copy_from_slice(&rec.aaguid);
    }
    let (Some(alg), Ok(sign_count)) = (
        CoseAlg::from_id(i64::from(rec.alg)),
        u32::try_from(rec.sign_count),
    ) else {
        tracing::error!(passkey = rec.id, "stored passkey unusable");
        return Ok(None);
    };
    let stored = StoredCredential {
        credential_id: rec.credential_id.clone(),
        cose_key: rec.cose_key.clone(),
        alg,
        sign_count,
        backup_eligible: rec.backup_eligible,
        backup_state: rec.backup_state,
        aaguid,
    };
    let rp = svc.relying_party()?;
    let response = AssertionResponse {
        raw_id: &raw_id,
        client_data_json: &client_data,
        authenticator_data: &auth_data,
        signature: &signature,
        user_handle: user_handle.as_deref(),
    };
    let outcome = match webauthn::finish_authentication(
        &rp,
        &ceremony,
        &stored,
        Some(&handle),
        &response,
        now,
    ) {
        Ok(o) => o,
        Err(WebauthnError::CounterRegression) => {
            tracing::warn!(target: "auth", passkey = rec.id, "passkey counter regression");
            svc.store
                .audit(
                    rec.account_id,
                    "passkey.counter_regression",
                    Some(rec.id),
                    &format!(
                        "stored counter {}, authenticator reported a value that did not increase",
                        rec.sign_count
                    ),
                    now,
                )
                .await?;
            return Ok(None);
        }
        Err(e) => {
            tracing::info!(target: "auth", error = %e, "passkey assertion refused");
            return Ok(None);
        }
    };
    let applied = svc
        .store
        .passkey_record_use(
            rec.id,
            rec.sign_count,
            i64::from(outcome.sign_count),
            outcome.backup_state,
            now,
        )
        .await?;
    // Lost a race with a concurrent use of the same counter value: refuse.
    Ok(applied.then_some(rec))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Response of every `.../begin` route.
#[derive(Debug, Serialize, ToSchema)]
pub struct BeginResponse {
    /// Send back with the finish call.
    ceremony_id: String,
    /// `PublicKeyCredentialCreationOptions` / `RequestOptions` JSON with base64url
    /// binary fields; pass to `navigator.credentials.create/get({ publicKey })`
    /// after decoding those fields.
    options: Value,
}

/// `POST /sr/auth/passkeys/register/begin {password}`: re-check the password and
/// start a registration ceremony for the signed-in account.
#[utoipa::path(
    post,
    path = "/sr/auth/passkeys/register/begin",
    request_body = PasswordBodyDoc,
    responses(
        (status = 200, body = BeginResponse),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 429, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn register_begin(
    State(state): State<AppState>,
    Svc { svc, .. }: Svc,
    UserOnly(p): UserOnly,
    Json(body): Json<PasswordBody>,
) -> R<Json<BeginResponse>> {
    let sid = session_id(&p)?;
    let now = state.now_secs();
    throttle(svc.ip_limits.hit(
        "passkey-register",
        &p.user_id.to_string(),
        now,
        &[Rule {
            limit: 10,
            secs: 600,
        }],
    ))?;
    let account = current_account(&svc, p.user_id).await?;
    reauthenticate(&svc, now, &account, &body.password).await?;
    let existing = svc.store.passkey_list(account.id).await?;
    if existing.len() >= MAX_PASSKEYS {
        return Err(ApiError::with_message(
            ErrorCode::General,
            "You have reached the passkey limit. Remove one first.",
        )
        .into());
    }
    let handle = svc
        .store
        .passkey_user_handle(account.id, &sr_auth::random::bytes::<32>()?)
        .await?;
    let rp = svc.relying_party()?;
    let exclude: Vec<Vec<u8>> = existing.into_iter().map(|e| e.credential_id).collect();
    let display = account
        .display_name
        .clone()
        .unwrap_or_else(|| account.username.clone());
    let (ceremony, options) = webauthn::begin_registration(
        &rp,
        &handle,
        &account.username,
        &display,
        &exclude,
        UserVerification::Required,
        now,
    )
    .map_err(|_| AuthRouteError::internal("passkey registration options"))?;
    let ceremony_id = svc
        .ceremonies
        .insert(
            CeremonyCtx::Register {
                account_id: account.id,
                session_id: sid,
            },
            ceremony,
            now,
        )
        .ok_or_else(too_busy)?;
    Ok(Json(BeginResponse {
        ceremony_id,
        options,
    }))
}

/// Body for `POST /sr/auth/passkeys/register/finish`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterFinishBody {
    /// From `register/begin`.
    ceremony_id: String,
    /// Label for the new passkey (default `Passkey`).
    #[serde(default)]
    name: Option<String>,
    /// The result of `navigator.credentials.create`.
    credential: RegistrationCredential,
}

/// Response of `POST /sr/auth/passkeys/register/finish`.
#[derive(Debug, Serialize, ToSchema)]
pub struct RegisterFinishResponse {
    /// The stored passkey.
    passkey: PasskeyView,
    /// Present when the account had no unused recovery codes: ten fresh codes, shown
    /// once. A passkey makes password logins ask for a second factor, so there must
    /// be a way back in without it.
    recovery_codes: Option<Vec<String>>,
}

/// `POST /sr/auth/passkeys/register/finish`: verify the new credential and store it.
#[utoipa::path(
    post,
    path = "/sr/auth/passkeys/register/finish",
    request_body = RegisterFinishBody,
    responses(
        (status = 200, body = RegisterFinishResponse),
        (status = 400, body = ErrorBody),
        (status = 429, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn register_finish(
    State(state): State<AppState>,
    Svc { svc, .. }: Svc,
    UserOnly(p): UserOnly,
    Json(body): Json<RegisterFinishBody>,
) -> R<Json<RegisterFinishResponse>> {
    let sid = session_id(&p)?;
    let now = state.now_secs();
    throttle(svc.ip_limits.hit(
        "passkey-register-finish",
        &p.user_id.to_string(),
        now,
        &[Rule {
            limit: 20,
            secs: 600,
        }],
    ))?;
    let name = match body.name.as_deref() {
        None => "Passkey".to_owned(),
        Some(n) => clean_name(n).ok_or_else(name_error)?,
    };
    let (ctx, ceremony) = svc
        .ceremonies
        .take(&body.ceremony_id, now)
        .ok_or_else(bad_ceremony)?;
    if ctx
        != (CeremonyCtx::Register {
            account_id: p.user_id,
            session_id: sid,
        })
    {
        return Err(bad_ceremony());
    }
    let (Some(raw_id), Some(client_data), Some(attestation)) = (
        b64(&body.credential.raw_id),
        b64(&body.credential.response.client_data_json),
        b64(&body.credential.response.attestation_object),
    ) else {
        return Err(bad_credential());
    };
    let rp = svc.relying_party()?;
    let stored =
        webauthn::finish_registration(&rp, &ceremony, &raw_id, &client_data, &attestation, now)
            .map_err(|e| {
                tracing::info!(target: "auth", error = %e, "passkey registration refused");
                bad_credential()
            })?;
    let existing = svc.store.passkey_list(p.user_id).await?;
    if existing.len() >= MAX_PASSKEYS {
        return Err(bad_credential());
    }
    let rec = PasskeyRecord {
        id: svc.next_id(),
        account_id: p.user_id,
        credential_id: stored.credential_id,
        cose_key: stored.cose_key,
        alg: i16::try_from(stored.alg.id()).map_err(|_| bad_credential())?,
        sign_count: i64::from(stored.sign_count),
        backup_eligible: stored.backup_eligible,
        backup_state: stored.backup_state,
        aaguid: stored.aaguid.to_vec(),
        name,
        created_at: now,
        last_used_at: None,
    };
    if !svc.store.passkey_insert(rec.clone()).await? {
        return Err(ApiError::with_message(
            ErrorCode::General,
            "That passkey is already registered.",
        )
        .into());
    }
    svc.store
        .audit(p.user_id, "passkey.register", Some(rec.id), "", now)
        .await?;
    let recovery_codes = if svc.store.recovery_list_unused(p.user_id).await?.is_empty() {
        Some(issue_recovery_codes(&svc, p.user_id).await?)
    } else {
        None
    };
    Ok(Json(RegisterFinishResponse {
        passkey: view(&rec),
        recovery_codes,
    }))
}

fn bad_credential() -> AuthRouteError {
    ApiError::with_message(
        ErrorCode::General,
        "That passkey couldn't be verified. Try again.",
    )
    .into()
}

// ---------------------------------------------------------------------------
// Login and second factor
// ---------------------------------------------------------------------------

/// Body for `POST /sr/auth/passkeys/login/begin`.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct LoginBeginBody {
    /// Start the second-factor ceremony of the password login waiting on the
    /// `sr_mfa` cookie (finish through `POST /sr/auth/2fa`, method `passkey`)
    /// instead of passkey-only login.
    #[serde(default)]
    second_factor: bool,
}

/// `POST /sr/auth/passkeys/login/begin {second_factor?}`: no login needed.
#[utoipa::path(
    post,
    path = "/sr/auth/passkeys/login/begin",
    request_body = LoginBeginBody,
    responses(
        (status = 200, body = BeginResponse),
        (status = 401, body = ErrorBody),
        (status = 429, body = ErrorBody),
    )
)]
pub async fn login_begin(
    State(state): State<AppState>,
    Svc { svc, ip }: Svc,
    headers: HeaderMap,
    body: Option<Json<LoginBeginBody>>,
) -> R<Json<BeginResponse>> {
    let now = state.now_secs();
    throttle(svc.ip_limits.hit(
        "passkey-login-begin",
        &ip,
        now,
        &[Rule {
            limit: 60,
            secs: 60,
        }],
    ))?;
    let second_factor = body.is_some_and(|b| b.0.second_factor);
    let rp = svc.relying_party()?;
    let (ctx, allow, uv) = if second_factor {
        let token = crate::auth::parse_cookie(&headers, MFA_COOKIE)
            .and_then(SessionToken::parse)
            .ok_or_else(no_match)?;
        let pending = svc.pending.get(&token.hash(), now).ok_or_else(no_match)?;
        let allow: Vec<Vec<u8>> = svc
            .store
            .passkey_list(pending.user_id)
            .await?
            .into_iter()
            .map(|p| p.credential_id)
            .collect();
        if allow.is_empty() {
            return Err(no_match());
        }
        (
            CeremonyCtx::SecondFactor {
                account_id: pending.user_id,
            },
            allow,
            UserVerification::Preferred,
        )
    } else {
        (CeremonyCtx::Login, Vec::new(), UserVerification::Required)
    };
    let (ceremony, options) = webauthn::begin_authentication(&rp, &allow, uv, now)
        .map_err(|_| AuthRouteError::internal("passkey login options"))?;
    let ceremony_id = svc
        .ceremonies
        .insert(ctx, ceremony, now)
        .ok_or_else(too_busy)?;
    Ok(Json(BeginResponse {
        ceremony_id,
        options,
    }))
}

/// `POST /sr/auth/passkeys/verify/begin`: start a second-factor ceremony for the
/// signed-in account (the email-change re-prompt). The result goes into
/// `mfa: {method: "passkey", code}` as a JSON string of
/// `{ceremony_id, credential}`.
#[utoipa::path(
    post,
    path = "/sr/auth/passkeys/verify/begin",
    responses(
        (status = 200, body = BeginResponse),
        (status = 400, body = ErrorBody),
        (status = 429, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn verify_begin(
    State(state): State<AppState>,
    Svc { svc, .. }: Svc,
    UserOnly(p): UserOnly,
) -> R<Json<BeginResponse>> {
    session_id(&p)?;
    let now = state.now_secs();
    throttle(svc.ip_limits.hit(
        "passkey-verify-begin",
        &p.user_id.to_string(),
        now,
        &[Rule {
            limit: 30,
            secs: 600,
        }],
    ))?;
    let allow: Vec<Vec<u8>> = svc
        .store
        .passkey_list(p.user_id)
        .await?
        .into_iter()
        .map(|p| p.credential_id)
        .collect();
    if allow.is_empty() {
        return Err(ApiError::with_message(ErrorCode::General, "You have no passkeys.").into());
    }
    let rp = svc.relying_party()?;
    let (ceremony, options) =
        webauthn::begin_authentication(&rp, &allow, UserVerification::Preferred, now)
            .map_err(|_| AuthRouteError::internal("passkey verify options"))?;
    let ceremony_id = svc
        .ceremonies
        .insert(
            CeremonyCtx::SecondFactor {
                account_id: p.user_id,
            },
            ceremony,
            now,
        )
        .ok_or_else(too_busy)?;
    Ok(Json(BeginResponse {
        ceremony_id,
        options,
    }))
}

/// Body for `POST /sr/auth/passkeys/login/finish`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginFinishBody {
    /// From `login/begin`.
    ceremony_id: String,
    /// The result of `navigator.credentials.get`.
    credential: AssertionCredential,
}

/// `POST /sr/auth/passkeys/login/finish`: complete a passkey-only login. Sets the
/// session cookie and answers `{status: "ok"}`. User verification was required, so no
/// further factor is asked.
#[utoipa::path(
    post,
    path = "/sr/auth/passkeys/login/finish",
    request_body = LoginFinishBody,
    responses(
        (status = 200, description = "`{status: \"ok\"}` and a session cookie"),
        (status = 401, body = ErrorBody),
        (status = 429, body = ErrorBody),
    )
)]
pub async fn login_finish(
    State(state): State<AppState>,
    Svc { svc, ip }: Svc,
    headers: HeaderMap,
    Json(body): Json<LoginFinishBody>,
) -> R<impl IntoResponse> {
    let now = state.now_secs();
    throttle(svc.ip_limits.hit(
        "passkey-login-finish",
        &ip,
        now,
        &[Rule {
            limit: 30,
            secs: 60,
        }],
    ))?;
    throttle(svc.ip_lock.check(&ip, now))?;
    let Some(rec) = verify_assertion(
        &svc,
        &body.ceremony_id,
        &body.credential,
        Expect::Login,
        now,
    )
    .await?
    else {
        svc.ip_lock.fail(&ip, now);
        return Err(no_match());
    };
    let account = svc
        .store
        .find_by_id(rec.account_id)
        .await?
        .ok_or_else(no_match)?;
    match account.state {
        AccountState::Active => {}
        AccountState::Deactivated => return Err(ErrorCode::SrAccountDeactivated.into()),
        AccountState::PlatformBanned => return Err(ErrorCode::SrPlatformBanned.into()),
        AccountState::Deleted => return Err(no_match()),
    }
    let (_, cookie) = svc
        .start_session_mfa(&state, account.id, super::common::user_agent(&headers))
        .await?;
    let secure = state.settings().secure_cookies;
    Ok((
        AppendHeaders([
            (header::SET_COOKIE, cookie),
            (header::SET_COOKIE, clear_mfa_cookie(secure)),
        ]),
        Json(json!({ "status": "ok" })),
    ))
}

/// Verify a `passkey` second-factor `code` (a JSON string of
/// `{ceremony_id, credential}`) for `account_id`. Used by `check_second_factor`.
pub(super) async fn check_proof(
    svc: &AuthService,
    account_id: i64,
    code: &str,
    now: i64,
) -> R<bool> {
    if code.len() > MAX_PROOF_LEN {
        return Ok(false);
    }
    let Ok(proof) = serde_json::from_str::<PasskeyProof>(code) else {
        return Ok(false);
    };
    Ok(verify_assertion(
        svc,
        &proof.ceremony_id,
        &proof.credential,
        Expect::Account(account_id),
        now,
    )
    .await?
    .is_some())
}

// ---------------------------------------------------------------------------
// Manage
// ---------------------------------------------------------------------------

/// `GET /sr/auth/passkeys`: the account's passkeys, oldest first.
#[utoipa::path(
    get,
    path = "/sr/auth/passkeys",
    responses((status = 200, body = Vec<PasskeyView>), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(Svc { svc, .. }: Svc, UserOnly(p): UserOnly) -> R<Json<Vec<PasskeyView>>> {
    session_id(&p)?;
    let rows = svc.store.passkey_list(p.user_id).await?;
    Ok(Json(rows.iter().map(view).collect()))
}

/// Body for `PATCH /sr/auth/passkeys/{id}`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RenameBody {
    /// New label, 1 to 64 characters.
    name: String,
}

fn parse_id(id: &str) -> Result<i64, AuthRouteError> {
    id.parse::<i64>()
        .map_err(|_| ErrorCode::UnknownAccount.into())
}

/// `PATCH /sr/auth/passkeys/{id} {name}`: rename.
#[utoipa::path(
    patch,
    path = "/sr/auth/passkeys/{id}",
    params(("id" = String, Path, description = "Passkey id")),
    request_body = RenameBody,
    responses(
        (status = 200, body = PasskeyView),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn rename(
    Svc { svc, .. }: Svc,
    UserOnly(p): UserOnly,
    Path(id): Path<String>,
    Json(body): Json<RenameBody>,
) -> R<Json<PasskeyView>> {
    session_id(&p)?;
    let id = parse_id(&id)?;
    let name = clean_name(&body.name).ok_or_else(name_error)?;
    if !svc.store.passkey_rename(p.user_id, id, &name).await? {
        return Err(ErrorCode::UnknownAccount.into());
    }
    let row = svc
        .store
        .passkey_list(p.user_id)
        .await?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or(ErrorCode::UnknownAccount)?;
    Ok(Json(view(&row)))
}

/// `DELETE /sr/auth/passkeys/{id} {password}`: remove a passkey (password re-check).
/// Removing the last second factor also drops the recovery codes.
#[utoipa::path(
    delete,
    path = "/sr/auth/passkeys/{id}",
    params(("id" = String, Path, description = "Passkey id")),
    request_body = PasswordBodyDoc,
    responses(
        (status = 204, description = "Removed"),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 429, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn remove(
    State(state): State<AppState>,
    Svc { svc, .. }: Svc,
    UserOnly(p): UserOnly,
    Path(id): Path<String>,
    Json(body): Json<PasswordBody>,
) -> R<StatusCode> {
    session_id(&p)?;
    let now = state.now_secs();
    let id = parse_id(&id)?;
    let account = current_account(&svc, p.user_id).await?;
    reauthenticate(&svc, now, &account, &body.password).await?;
    if !svc.store.passkey_delete(account.id, id).await? {
        return Err(ErrorCode::UnknownAccount.into());
    }
    svc.store
        .audit(account.id, "passkey.delete", Some(id), "", now)
        .await?;
    let totp = svc
        .store
        .totp_get(account.id)
        .await?
        .is_some_and(|t| t.confirmed);
    if !totp && !svc.has_passkeys(account.id).await? {
        svc.store.recovery_replace_all(account.id, &[]).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Documentation stand-in for [`PasswordBody`].
#[derive(Debug, Deserialize, ToSchema)]
pub struct PasswordBodyDoc {
    /// The current password.
    #[allow(dead_code)]
    password: String,
}

/// OpenAPI fragment for the passkey routes.
#[derive(OpenApi)]
#[openapi(
    paths(
        register_begin,
        register_finish,
        login_begin,
        login_finish,
        verify_begin,
        list,
        rename,
        remove
    ),
    components(schemas(
        BeginResponse,
        RegisterFinishBody,
        RegisterFinishResponse,
        RegistrationCredential,
        RegistrationResponse,
        AssertionCredential,
        AssertionFields,
        LoginBeginBody,
        LoginFinishBody,
        PasskeyView,
        RenameBody,
        PasswordBodyDoc
    ))
)]
pub struct PasskeysApiDoc;
