//! Signed-in account edits: change password, username and email
//! (LNG-USET-004/005, doc 06 section 2 re-auth).
//!
//! Every route needs a browser session (bots and bearer tokens are refused) and the
//! current password, which shares the failure lockout with login. The display name
//! is a profile field and is edited with `PATCH /users/@me` (`global_name`).
//!
//! Email change is a two-step flow: `POST /sr/auth/email/change` stores the pending
//! address with a single-use token and mails a link to the *new* address; the
//! account keeps its old address until the link is used. The link points at the
//! same `/verify-email/{token}` page as signup verification, and
//! `POST /sr/auth/verify-email` accepts both token kinds (see `account.rs`).

use super::{
    common::{AuthRouteError, AuthService, R, Svc},
    login::check_second_factor,
    mail,
    mfa::{current_account, reauthenticate},
    sessions::session_id,
    store::{AccountRecord, DeleteRefusal},
    throttle::Rule,
    validate,
};
use crate::openapi::ErrorBody;
use crate::{auth::UserOnly, state::AppState};
use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use sr_auth::email_token::{EmailToken, EmailTokenPurpose};
use sr_core::{ApiError, ErrorCode};
use utoipa::{OpenApi, ToSchema};

/// Username changes allowed per account per day.
const USERNAME_CHANGES_PER_DAY: usize = 2;

/// Words that must not appear in a new password: the account's own identifiers.
fn context(account: &AccountRecord) -> Vec<String> {
    let mut ctx = vec![
        "sideroom".to_owned(),
        "side room".to_owned(),
        account.username.clone(),
    ];
    if let Some(local) = account.email.split('@').next() {
        ctx.push(local.to_owned());
    }
    ctx
}

/// Body for `POST /sr/auth/password`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ChangePasswordBody {
    /// The password in use now.
    current_password: String,
    /// At least 10 characters and not easy to guess (zxcvbn score 3 or more).
    new_password: String,
    /// Sign out every other session and trusted device (default `true`). Pass
    /// `false` to keep the other browsers signed in.
    #[serde(default = "yes")]
    revoke_other_sessions: bool,
}

fn yes() -> bool {
    true
}

/// `POST /sr/auth/password {current_password, new_password, revoke_other_sessions?}`:
/// change the password of the signed-in account. The current password is checked
/// first (and counts toward the login lockout). By default every other session and
/// every trusted device is revoked; the current session always stays. A notice goes
/// to the account's email.
#[utoipa::path(
    post,
    path = "/sr/auth/password",
    request_body = ChangePasswordBody,
    responses(
        (status = 204, description = "Password changed"),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 429, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn change_password(
    State(state): State<AppState>,
    UserOnly(p): UserOnly,
    Svc { svc, .. }: Svc,
    Json(body): Json<ChangePasswordBody>,
) -> R<StatusCode> {
    let current_session = session_id(&p)?;
    let now = state.now_secs();
    let account = current_account(&svc, p.user_id).await?;
    reauthenticate(&svc, now, &account, &body.current_password).await?;
    let ctx = context(&account);
    let ctx_refs: Vec<&str> = ctx.iter().map(String::as_str).collect();
    if let Err(m) = validate::password(&body.new_password, &ctx_refs) {
        return Err(ApiError::invalid_form()
            .field("new_password", "weak", m)
            .into());
    }
    if body.new_password == body.current_password {
        return Err(ApiError::invalid_form()
            .field(
                "new_password",
                "unchanged",
                "Choose a password you are not already using.",
            )
            .into());
    }
    let hash = svc.hash_password(&body.new_password).await?;
    svc.store.set_password_hash(account.id, &hash).await?;
    if body.revoke_other_sessions {
        svc.store
            .revoke_sessions_except(account.id, Some(current_session))
            .await?;
        svc.store.trusted_devices_revoke_all(account.id).await?;
    }
    svc.store
        .audit(
            account.id,
            "account.password_change",
            Some(account.id),
            if body.revoke_other_sessions {
                "other sessions revoked"
            } else {
                "other sessions kept"
            },
            now,
        )
        .await?;
    let svc2 = svc.clone();
    let (to, locale) = (account.email, account.locale);
    tokio::spawn(async move {
        if let Err(e) = svc2.mailer.send(mail::password_changed(&to, &locale)).await {
            tracing::warn!("password change notice not sent: {e}");
        }
    });
    Ok(StatusCode::NO_CONTENT)
}

/// Body for `PUT /sr/auth/username`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ChangeUsernameBody {
    /// 2 to 32 characters of `a-z 0-9 _ .` (case-insensitive), not reserved.
    username: String,
    /// Current password.
    password: String,
}

/// `PUT /sr/auth/username {username, password}`: change the login name. Same rules as
/// signup (`^[a-z0-9_.]{2,32}$`, not reserved), two changes per day. Returns the new
/// username. Friends and shared servers receive `USER_UPDATE`.
#[utoipa::path(
    put,
    path = "/sr/auth/username",
    request_body = ChangeUsernameBody,
    responses(
        (status = 200, description = "`{username}`: the new login name"),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 429, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn change_username(
    State(state): State<AppState>,
    UserOnly(p): UserOnly,
    Svc { svc, .. }: Svc,
    crate::routes::MaybeExt(social): crate::routes::MaybeExt<crate::routes::social::SocialDeps>,
    Json(body): Json<ChangeUsernameBody>,
) -> R<Json<Value>> {
    session_id(&p)?;
    let now = state.now_secs();
    let account = current_account(&svc, p.user_id).await?;
    let new = validate::username(&body.username).map_err(|m| {
        AuthRouteError::from(ApiError::invalid_form().field("username", "invalid", m))
    })?;
    reauthenticate(&svc, now, &account, &body.password).await?;
    if new == account.username {
        return Ok(Json(json!({ "username": new })));
    }
    if svc.store.username_exists(&new).await? {
        return Err(ApiError::invalid_form()
            .field("username", "taken", "That username is taken.")
            .into());
    }
    svc.ip_limits
        .hit(
            "username-change",
            &account.id.to_string(),
            now,
            &[Rule {
                limit: USERNAME_CHANGES_PER_DAY,
                secs: 86_400,
            }],
        )
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;
    if !svc.store.set_username(account.id, &new).await? {
        return Err(ApiError::invalid_form()
            .field("username", "taken", "That username is taken.")
            .into());
    }
    svc.store
        .audit(
            account.id,
            "account.username_change",
            Some(account.id),
            &format!("{} -> {new}", account.username),
            now,
        )
        .await?;
    if let Some(social) = social {
        crate::routes::social::profile::broadcast_self_update(&social, p.user_id).await;
    }
    Ok(Json(json!({ "username": new })))
}

/// A second-factor proof (D-167).
#[derive(Debug, Deserialize, ToSchema)]
pub struct MfaProof {
    /// `totp`, `recovery` or `passkey` (then `code` is the JSON text of
    /// `{ceremony_id, credential}` from `POST /sr/auth/passkeys/verify/begin` and
    /// `navigator.credentials.get`).
    method: String,
    /// The authenticator code or an unused recovery code.
    code: String,
}

/// Body for `POST /sr/auth/email/change`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ChangeEmailBody {
    new_email: String,
    /// Current password.
    password: String,
    /// Second factor. Required when the account has two-factor authentication on,
    /// unless this session passed a second-factor check in the last 15 minutes.
    #[serde(default)]
    mfa: Option<MfaProof>,
}

/// A second-factor check counts for this long (D-167).
const MFA_FRESH_SECS: i64 = 15 * 60;

/// Require a second factor for a sensitive change when the account has one
/// (TOTP or passkeys). A check on this session within
/// [`MFA_FRESH_SECS`] counts; otherwise `proof` must be valid, and then the session
/// is marked verified.
pub(super) async fn require_second_factor(
    svc: &AuthService,
    now: i64,
    account: &AccountRecord,
    session: i64,
    proof: Option<&MfaProof>,
) -> R<()> {
    let has_totp = svc
        .store
        .totp_get(account.id)
        .await?
        .is_some_and(|t| t.confirmed);
    if !has_totp && !svc.has_passkeys(account.id).await? {
        return Ok(());
    }
    if let Some(at) = svc.store.session_mfa_verified_at(session).await?
        && now >= at
        && now - at < MFA_FRESH_SECS
    {
        return Ok(());
    }
    let Some(proof) = proof else {
        return Err(ApiError::invalid_form()
            .field(
                "mfa",
                "required",
                "Enter a code from your authenticator app, use a passkey, or enter a recovery code.",
            )
            .into());
    };
    let key = format!("mfa-reauth:{}", account.id);
    svc.account_lock
        .check(&key, now)
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;
    let max_code = if proof.method == "passkey" {
        super::passkeys::MAX_PROOF_LEN
    } else {
        64
    };
    if proof.code.len() > max_code {
        svc.account_lock.fail(&key, now);
        return Err(wrong_code());
    }
    let check = check_second_factor(svc, account, &proof.method, &proof.code, now).await?;
    if !check.ok {
        svc.account_lock.fail(&key, now);
        return Err(wrong_code());
    }
    svc.account_lock.clear(&key);
    svc.store.session_mark_mfa_verified(session, now).await?;
    Ok(())
}

fn wrong_code() -> AuthRouteError {
    ApiError::invalid_form()
        .field(
            "mfa",
            "invalid",
            "That code didn't match. Try the next one.",
        )
        .into()
}

fn invalid_link() -> AuthRouteError {
    ApiError::invalid_form()
        .field("token", "invalid", "This link is invalid or has expired.")
        .into()
}

/// Body for `POST /sr/auth/email/cancel`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CancelEmailChangeBody {
    /// The token from the "This wasn't me" link.
    token: String,
}

/// `POST /sr/auth/email/cancel {token}`: the "This wasn't me" link of an email-change
/// notice. No login needed. While the change is still pending it is cancelled; once it
/// has completed, every session and trusted device is revoked instead. The token works
/// once, within 7 days.
#[utoipa::path(
    post,
    path = "/sr/auth/email/cancel",
    request_body = CancelEmailChangeBody,
    responses(
        (status = 204, description = "Pending change cancelled, or all sessions revoked"),
        (status = 400, body = ErrorBody),
        (status = 429, body = ErrorBody),
    )
)]
pub async fn cancel_change_email(
    State(state): State<AppState>,
    Svc { svc, ip }: Svc,
    Json(body): Json<CancelEmailChangeBody>,
) -> R<StatusCode> {
    let now = state.now_secs();
    svc.ip_limits
        .hit(
            "email-cancel",
            &ip,
            now,
            &[Rule {
                limit: 20,
                secs: 600,
            }],
        )
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;
    let token = EmailToken::parse(EmailTokenPurpose::CancelEmailChange, &body.token)
        .ok_or_else(invalid_link)?;
    let user_id = svc
        .store
        .email_token_consume(EmailTokenPurpose::CancelEmailChange, &token.hash(), now)
        .await?
        .ok_or_else(invalid_link)?;
    if svc.store.email_change_cancel(user_id).await? {
        svc.store
            .audit(
                user_id,
                "account.email_change_cancel",
                Some(user_id),
                "pending email change cancelled",
                now,
            )
            .await?;
    } else {
        svc.store.revoke_sessions_except(user_id, None).await?;
        svc.store.trusted_devices_revoke_all(user_id).await?;
        svc.store
            .audit(
                user_id,
                "account.email_change_revoked",
                Some(user_id),
                "all sessions revoked from the email-change notice",
                now,
            )
            .await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /sr/auth/email/change {new_email, password, mfa?}`: start an email change. A
/// confirmation link goes to the new address; the old one stays until it is used, and
/// receives a notice with a "This wasn't me" link. With two-factor authentication on,
/// `mfa {method, code}` is required unless this session passed a second-factor check
/// in the last 15 minutes. One request per minute per account. `204` also when the address belongs to
/// someone else, so the endpoint is not an account-existence oracle (the mail is
/// simply not sent).
#[utoipa::path(
    post,
    path = "/sr/auth/email/change",
    request_body = ChangeEmailBody,
    responses(
        (status = 204, description = "Confirmation link sent, if the address is free"),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 429, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn change_email(
    State(state): State<AppState>,
    UserOnly(p): UserOnly,
    Svc { svc, .. }: Svc,
    Json(body): Json<ChangeEmailBody>,
) -> R<StatusCode> {
    let sid = session_id(&p)?;
    let now = state.now_secs();
    let account = current_account(&svc, p.user_id).await?;
    let new = validate::email(&body.new_email).map_err(|m| {
        AuthRouteError::from(ApiError::invalid_form().field("new_email", "invalid", m))
    })?;
    reauthenticate(&svc, now, &account, &body.password).await?;
    require_second_factor(&svc, now, &account, sid, body.mfa.as_ref()).await?;
    if new == account.email {
        return Err(ApiError::invalid_form()
            .field("new_email", "unchanged", "That is already your email.")
            .into());
    }
    svc.ip_limits
        .hit(
            "email-change",
            &account.id.to_string(),
            now,
            &[Rule { limit: 1, secs: 60 }],
        )
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;
    if svc.store.find_by_email(&new).await?.is_some() {
        return Ok(StatusCode::NO_CONTENT);
    }
    let token = EmailToken::generate(EmailTokenPurpose::ChangeEmail)?;
    svc.store
        .email_change_put(
            account.id,
            &new,
            &token.hash(),
            token.expires_at_in(now, svc.cfg.verify_ttl_secs),
        )
        .await?;
    let link = svc.link("verify-email", token.expose());
    svc.mailer
        .send(mail::change_email(&new, &link, &account.locale))
        .await
        .map_err(|e| {
            tracing::warn!("email change mail not sent: {e}");
            AuthRouteError::internal("email change mail")
        })?;
    // The old address learns of the request and can cancel it (D-167).
    let cancel = EmailToken::generate(EmailTokenPurpose::CancelEmailChange)?;
    svc.store
        .email_token_put(
            account.id,
            EmailTokenPurpose::CancelEmailChange,
            &cancel.hash(),
            cancel.expires_at(now),
        )
        .await?;
    let notice = mail::change_email_notice(
        &account.email,
        &new,
        &svc.cancel_email_link(cancel.expose()),
        &account.locale,
    );
    if let Err(e) = svc.mailer.send(notice).await {
        tracing::warn!("email change notice not sent: {e}");
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Apply a consumed change-email token: swap the address (verified), audit it and
/// tell the old address. `Ok(false)` when the new address was taken in the meantime.
pub(super) async fn apply_email_change(
    svc: &std::sync::Arc<super::common::AuthService>,
    now: i64,
    user_id: i64,
    new_email: &str,
) -> R<bool> {
    let account = current_account(svc, user_id).await?;
    if !svc.store.set_email(user_id, new_email).await? {
        return Ok(false);
    }
    svc.store
        .audit(
            user_id,
            "account.email_change",
            Some(user_id),
            "email changed",
            now,
        )
        .await?;
    // A fresh token (superseding the one from the request) so the notice's link now
    // signs every device out.
    let cancel = EmailToken::generate(EmailTokenPurpose::CancelEmailChange)?;
    svc.store
        .email_token_put(
            user_id,
            EmailTokenPurpose::CancelEmailChange,
            &cancel.hash(),
            cancel.expires_at(now),
        )
        .await?;
    let notice = mail::email_changed(
        &account.email,
        &svc.cancel_email_link(cancel.expose()),
        &account.locale,
    );
    let svc2 = svc.clone();
    tokio::spawn(async move {
        if let Err(e) = svc2.mailer.send(notice).await {
            tracing::warn!("email change notice not sent: {e}");
        }
    });
    Ok(true)
}

/// Body for `POST /sr/auth/account/delete`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct DeleteAccountBody {
    /// Current password.
    password: String,
    /// The account's username, typed to confirm (case-insensitive).
    username: String,
    /// Second factor. Required when the account has two-factor authentication on,
    /// unless this session passed a second-factor check in the last 15 minutes.
    #[serde(default)]
    mfa: Option<MfaProof>,
}

/// `POST /sr/auth/account/delete {password, username, mfa?}`: delete the signed-in
/// account (LNG-USET-007, D-029). Needs the password, the typed username and, with
/// two-factor on, a second factor. The account can never sign in again; messages stay
/// and show "Deleted User". Sessions, friendships, blocks, server and group memberships
/// are removed, open platform invites are revoked, unspent invite credits are forfeited
/// and the username is held for 30 days. Refused with a `guilds` field error while the
/// account still owns a server (transfer or delete it first). Friends and shared servers
/// are told through `RELATIONSHIP_REMOVE` and `GUILD_MEMBER_REMOVE`.
#[utoipa::path(
    post,
    path = "/sr/auth/account/delete",
    request_body = DeleteAccountBody,
    responses(
        (status = 204, description = "Account deleted; the session cookie is cleared"),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 429, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn delete_account(
    State(state): State<AppState>,
    UserOnly(p): UserOnly,
    Svc { svc, .. }: Svc,
    crate::routes::MaybeExt(social): crate::routes::MaybeExt<crate::routes::social::SocialDeps>,
    crate::routes::MaybeExt(media): crate::routes::MaybeExt<crate::routes::media::MediaDeps>,
    Json(body): Json<DeleteAccountBody>,
) -> R<impl axum::response::IntoResponse> {
    let sid = session_id(&p)?;
    let now = state.now_secs();
    let account = current_account(&svc, p.user_id).await?;
    if !body.username.trim().eq_ignore_ascii_case(&account.username) {
        return Err(ApiError::invalid_form()
            .field(
                "username",
                "mismatch",
                "Type your username exactly to confirm.",
            )
            .into());
    }
    reauthenticate(&svc, now, &account, &body.password).await?;
    require_second_factor(&svc, now, &account, sid, body.mfa.as_ref()).await?;
    let ledger_id = (svc.ids)();
    let done = match svc.store.delete_account(account.id, ledger_id).await? {
        Ok(d) => d,
        Err(DeleteRefusal::NotActive) => return Err(ErrorCode::UnknownAccount.into()),
        Err(DeleteRefusal::Owner) => {
            return Err(ApiError::invalid_form()
                .field(
                    "owner",
                    "is_owner",
                    "The Owner cannot delete the account. Transfer ownership first (sr-cli staff transfer-owner).",
                )
                .into());
        }
        Err(DeleteRefusal::Staff) => {
            return Err(ApiError::invalid_form()
                .field(
                    "staff",
                    "is_staff",
                    "You are on the staff. Leave the Staff Server first, then delete your account.",
                )
                .into());
        }
        Err(DeleteRefusal::OwnsGuilds(ids)) => {
            let list: Vec<String> = ids.iter().map(ToString::to_string).collect();
            return Err(ApiError::invalid_form()
                .field(
                    "guilds",
                    "owns_guilds",
                    format!(
                        "You still own {} server(s) ({}). Transfer ownership or delete them first.",
                        ids.len(),
                        list.join(", ")
                    ),
                )
                .into());
        }
    };
    svc.store
        .audit(
            account.id,
            "account.delete",
            Some(account.id),
            &format!(
                "user deleted the account; {} credits forfeited, {} invites revoked",
                done.credits_forfeited, done.invites_revoked
            ),
            now,
        )
        .await?;
    if let Some(media) = media {
        for (kind, key) in [
            (sr_storage::media::MediaKind::UserAvatar, &done.avatar_key),
            (sr_storage::media::MediaKind::UserBanner, &done.banner_key),
        ] {
            if let (Some(key), Ok(uid)) = (key, u64::try_from(account.id)) {
                media.remove(kind, uid, key).await;
            }
        }
        // Profile page pictures (K-17): `avatars/<user>/<hash>`.
        if let Ok(uid) = u64::try_from(account.id) {
            for key in &done.profile_media_keys {
                if let Some(hash) = key.rsplit('/').next() {
                    media
                        .remove(sr_storage::media::MediaKind::ProfileMedia, uid, hash)
                        .await;
                }
            }
        }
    }
    if let Some(social) = social {
        crate::routes::social::announce_account_deleted(&social, account.id, &done).await;
    }
    Ok((
        StatusCode::NO_CONTENT,
        axum::response::AppendHeaders([(
            axum::http::header::SET_COOKIE,
            super::common::clear_session_cookie(state.settings().secure_cookies),
        )]),
    ))
}

/// OpenAPI fragment for the signed-in account edits.
#[derive(OpenApi)]
#[openapi(
    paths(
        change_password,
        change_username,
        change_email,
        cancel_change_email,
        delete_account,
        super::account::get_age_confirmation,
        super::account::reset_age_confirmation
    ),
    components(schemas(
        ChangePasswordBody,
        ChangeUsernameBody,
        ChangeEmailBody,
        MfaProof,
        CancelEmailChangeBody,
        DeleteAccountBody,
        super::account::AgeConfirmationObject
    ))
)]
pub struct IdentityApiDoc;

// ---- second factor for staff (ADM-STF-009) ----------------------------------------

/// Where a staff session stands on the 15-minute second-factor window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaffMfa {
    /// A check on this session is younger than 15 minutes.
    Fresh,
    /// The account has a second factor but the last check is older (or never happened).
    Stale,
    /// The account has no second factor, so the window can never be opened.
    NotEnrolled,
}

/// The window length in seconds (D-167).
pub const STAFF_MFA_WINDOW_SECS: i64 = MFA_FRESH_SECS;

/// Does the account have a confirmed TOTP or a passkey? (second factor to accept an appointment, D-198)
pub(super) async fn has_second_factor(svc: &AuthService, user_id: i64) -> R<bool> {
    let account = current_account(svc, user_id).await?;
    let has_totp = svc
        .store
        .totp_get(account.id)
        .await?
        .is_some_and(|t| t.confirmed);
    Ok(has_totp || svc.has_passkeys(account.id).await?)
}

pub(super) async fn staff_mfa_state(
    svc: &AuthService,
    now: i64,
    user_id: i64,
    session: i64,
) -> R<StaffMfa> {
    let account = current_account(svc, user_id).await?;
    let has_totp = svc
        .store
        .totp_get(account.id)
        .await?
        .is_some_and(|t| t.confirmed);
    if !has_totp && !svc.has_passkeys(account.id).await? {
        return Ok(StaffMfa::NotEnrolled);
    }
    Ok(match svc.store.session_mfa_verified_at(session).await? {
        Some(at) if now >= at && now - at < MFA_FRESH_SECS => StaffMfa::Fresh,
        _ => StaffMfa::Stale,
    })
}

/// Open the window with a TOTP, recovery or passkey proof; shares the login lockout.
pub(super) async fn staff_mfa_prove(
    svc: &AuthService,
    now: i64,
    user_id: i64,
    session: i64,
    method: &str,
    code: &str,
) -> R<()> {
    if staff_mfa_state(svc, now, user_id, session).await? == StaffMfa::NotEnrolled {
        return Err(ApiError::with_message(
            sr_core::ErrorCode::SrReauthRequired,
            "Set up two-factor authentication before using this admin action.",
        )
        .into());
    }
    let account = current_account(svc, user_id).await?;
    let proof = MfaProof {
        method: method.to_owned(),
        code: code.to_owned(),
    };
    require_second_factor(svc, now, &account, session, Some(&proof)).await
}
