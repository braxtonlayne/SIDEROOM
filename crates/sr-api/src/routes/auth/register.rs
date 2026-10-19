//! `POST /sr/register` (ENT-INV-007, ENT-INV-005, ENT-AUTH-001/004/012).

use super::{
    common::{AuthRouteError, AuthService, R, Svc, user_agent},
    discord::{LINK_COOKIE, clear_oauth_cookie},
    mail, mail_text,
    store::{InviteState, RegisterRejection, RegisterTx},
    throttle::Rule,
    validate,
};
use crate::{
    auth::parse_cookie,
    routes::{MaybeExt, settings::features::GuildFeatures},
    state::AppState,
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{AppendHeaders, IntoResponse},
};
use serde::Deserialize;
use serde_json::json;
use sr_auth::{
    email_token::{EmailToken, EmailTokenPurpose},
    invite::InviteCode,
};
use sr_core::{ApiError, ErrorCode};
use std::sync::Arc;

/// Request body (matches `RegisterRequest` in `apps/web/src/api/entrance.ts`).
#[derive(Debug, Deserialize)]
pub struct RegisterBody {
    code: String,
    email: String,
    username: String,
    #[serde(default)]
    display_name: Option<String>,
    password: String,
    #[serde(default)]
    accepted_house_notes: bool,
    /// "I am at least {minimum age}" (D-294): the age the person affirmed, which must reach the
    /// platform's minimum (`platform.compliance` `accounts.minimum_age`, default 13).
    #[serde(default)]
    confirmed_min_age: Option<u8>,
}

fn closed() -> AuthRouteError {
    ApiError::with_message(ErrorCode::SrInviteRequired, "The door's closed for now.").into()
}

/// One message for unknown, used and revoked codes.
fn unusable() -> AuthRouteError {
    ApiError::new(ErrorCode::SrInviteExhausted).into()
}

/// Create the account for a valid invite, log the new member in and send the
/// verification email. The invite is consumed atomically with the cap check
/// and the credit ledger writes (see [`super::store::AuthStore::register`]).
pub async fn register(
    State(state): State<AppState>,
    Svc { svc, ip }: Svc,
    MaybeExt(features): MaybeExt<GuildFeatures>,
    headers: HeaderMap,
    Json(body): Json<RegisterBody>,
) -> R<impl IntoResponse> {
    let now = state.now_secs();
    svc.ip_limits
        .hit(
            "register",
            &ip,
            now,
            &[
                Rule { limit: 5, secs: 60 },
                Rule {
                    limit: 20,
                    secs: 86_400,
                },
            ],
        )
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;

    // Field validation (reported together).
    let mut problems: Vec<(&str, &str)> = Vec::new();
    let username = validate::username(&body.username).unwrap_or_else(|m| {
        problems.push(("username", m));
        String::new()
    });
    let email = validate::email(&body.email).unwrap_or_else(|m| {
        problems.push(("email", m));
        String::new()
    });
    let display_name = validate::display_name(body.display_name.as_deref().unwrap_or(""))
        .unwrap_or_else(|m| {
            problems.push(("display_name", m));
            None
        });
    if !body.accepted_house_notes {
        problems.push(("rules", "Please confirm you have read the house notes."));
    }
    let min_age = match &features {
        Some(gf) => {
            crate::routes::settings::features::compliance_settings(gf)
                .await
                .minimum_age
        }
        None => sr_features::ComplianceSettings::default().minimum_age,
    };
    let age_message = format!("Please confirm you are at least {min_age}.");
    if body.confirmed_min_age.is_none_or(|a| a < min_age) {
        problems.push(("age", age_message.as_str()));
    }
    let local = email.split('@').next().unwrap_or("");
    let context = [username.as_str(), local, "sideroom", "side room"];
    if let Err(m) = validate::password(&body.password, &context) {
        problems.push(("password", m));
    }
    if !problems.is_empty() {
        let mut err = ApiError::invalid_form();
        for (field, msg) in problems {
            err = err.field(field, "invalid", msg);
        }
        return Err(err.into());
    }

    // Cheap rejection before spending an argon2 hash.
    let Ok(code) = InviteCode::parse(&body.code) else {
        return Err(unusable());
    };
    if svc.store.counted_accounts().await? >= svc.account_cap().await? {
        return Err(closed());
    }
    if svc.store.invite_state(&code).await? != Some(InviteState::Open) {
        return Err(unusable());
    }

    let password_hash = svc.hash_password(&body.password).await?;
    // A Discord identity proven on /sr/auth/discord/callback is linked to the
    // new account (open question 78); it is consumed only on success.
    let link_token = parse_cookie(&headers, LINK_COOKIE).map(str::to_owned);
    let link = link_token
        .as_deref()
        .and_then(|t| svc.discord_links.get(t, now));
    let outcome = svc
        .store
        .register(RegisterTx {
            discord_id: link.as_ref().map(|l| l.discord_id.clone()),
            identity_id: svc.next_id(),
            account_id: svc.next_id(),
            new_person_id: svc.next_id(),
            ledger_ids: [svc.next_id(), svc.next_id()],
            code,
            username,
            display_name,
            email: email.clone(),
            password_hash,
            account_cap: svc.account_cap().await?,
            initial_credits: svc.cfg.initial_credits,
            now,
        })
        .await?;
    let registered = match outcome {
        Ok(r) => r,
        Err(RegisterRejection::AtCap) => return Err(closed()),
        Err(RegisterRejection::PersonBanned) => {
            // Generic to the client; staff are alerted (ENT-INV-008).
            svc.staff_alert(
                sr_events::StaffAlertKind::BannedPersonAttempt,
                "registration refused: banned Person",
            )
            .await;
            return Err(unusable());
        }
        Err(RegisterRejection::InviteUnavailable | RegisterRejection::IssuerOutOfCredits) => {
            return Err(unusable());
        }
        Err(RegisterRejection::UsernameTaken) => {
            return Err(ApiError::invalid_form()
                .field("username", "taken", "That username is taken.")
                .into());
        }
        Err(RegisterRejection::EmailTaken) => {
            return Err(ApiError::invalid_form()
                .field("email", "unavailable", "That email can't be used.")
                .into());
        }
    };

    let (_, cookie) = svc
        .start_session(&state, registered.account_id, user_agent(&headers))
        .await?;
    let mut cookies = vec![(header::SET_COOKIE, cookie)];
    if let Some(token) = &link_token {
        svc.discord_links.take(token, now);
        let secure = state.settings().secure_cookies;
        cookies.push((header::SET_COOKIE, clear_oauth_cookie(LINK_COOKIE, secure)));
    }
    if let Some(age) = body.confirmed_min_age
        && let Err(e) = svc
            .store
            .set_declared_min_age(registered.account_id, age.min(18))
            .await
    {
        tracing::warn!(error = %e, "could not record the declared age");
    }
    spawn_verification(svc.clone(), state.now_secs(), registered.account_id, email);
    announce_new_member(&svc, registered.account_id, registered.issuer_id).await;
    svc.notify_invite_credits(registered.issuer_id).await;

    Ok((
        StatusCode::CREATED,
        AppendHeaders(cookies),
        Json(json!({ "id": registered.account_id.to_string() })),
    ))
}

/// Tell the Staff Server's `#new-members` feed who joined and who invited them (ADM-ALR-002).
/// Usernames only: no email, no IP.
async fn announce_new_member(svc: &AuthService, account_id: i64, issuer_id: i64) {
    let name = |id: i64| {
        let store = svc.store.clone();
        async move {
            store
                .find_by_id(id)
                .await
                .ok()
                .flatten()
                .map(|a| a.username)
        }
    };
    let (Some(new), issuer) = (name(account_id).await, name(issuer_id).await) else {
        return;
    };
    let detail = match issuer {
        Some(i) => format!("@{new} joined, invited by @{i}."),
        None => format!("@{new} joined."),
    };
    svc.staff_feed(
        sr_events::StaffFeedKind::NewMember,
        "New member",
        &detail,
        Some(format!("/admin/invite-graph?account={account_id}")),
    )
    .await;
}

/// Mint and email a verification token in the background; failures are
/// logged and never surface to the caller.
pub(super) fn spawn_verification(svc: Arc<AuthService>, now: i64, user_id: i64, email: String) {
    tokio::spawn(async move {
        if let Err(e) = send_verification(&svc, now, user_id, &email).await {
            tracing::warn!(user_id, "could not send verification email: {e:?}");
        }
    });
}

pub(super) async fn send_verification(
    svc: &AuthService,
    now: i64,
    user_id: i64,
    email: &str,
) -> Result<(), String> {
    let token = EmailToken::generate(EmailTokenPurpose::VerifyEmail).map_err(|e| e.to_string())?;
    svc.store
        .email_token_put(
            user_id,
            EmailTokenPurpose::VerifyEmail,
            &token.hash(),
            token.expires_at_in(now, svc.cfg.verify_ttl_secs),
        )
        .await
        .map_err(|e| e.to_string())?;
    let link = svc.link("verify-email", token.expose());
    // The mail is in the account's language; an unreadable account falls back to English.
    let locale = match svc.store.find_by_id(user_id).await {
        Ok(Some(account)) => account.locale,
        _ => mail_text::DEFAULT_LOCALE.to_owned(),
    };
    svc.mailer
        .send(mail::verify_email(email, &link, &locale))
        .await
        .map_err(|e| e.to_string())
}
