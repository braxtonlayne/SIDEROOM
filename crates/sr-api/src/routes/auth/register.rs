//! `POST /sr/register` (ENT-INV-007, ENT-INV-005, ENT-AUTH-001/004/012).

use super::{
    common::{AuthRouteError, AuthService, R, Svc, user_agent},
    mail,
    store::{InviteState, RegisterRejection, RegisterTx},
    throttle::Rule,
    validate,
};
use crate::state::AppState;
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
    if svc.store.counted_accounts().await? >= svc.cfg.account_cap {
        return Err(closed());
    }
    if svc.store.invite_state(&code).await? != Some(InviteState::Open) {
        return Err(unusable());
    }

    let password_hash = svc.hash_password(&body.password).await?;
    let outcome = svc
        .store
        .register(RegisterTx {
            account_id: svc.next_id(),
            new_person_id: svc.next_id(),
            ledger_ids: [svc.next_id(), svc.next_id()],
            discord_id: None,
            identity_id: svc.next_id(),
            code,
            username,
            display_name,
            email: email.clone(),
            password_hash,
            account_cap: svc.cfg.account_cap,
            initial_credits: svc.cfg.initial_credits,
            now,
        })
        .await?;
    let registered = match outcome {
        Ok(r) => r,
        Err(RegisterRejection::AtCap) => return Err(closed()),
        Err(RegisterRejection::PersonBanned) => {
            // Generic to the client; staff are alerted (ENT-INV-008).
            tracing::warn!(target: "staff_alert", "registration refused: banned Person");
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
    spawn_verification(svc.clone(), state.now_secs(), registered.account_id, email);

    Ok((
        StatusCode::CREATED,
        AppendHeaders([(header::SET_COOKIE, cookie)]),
        Json(json!({ "id": registered.account_id.to_string() })),
    ))
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
    svc.mailer
        .send(mail::verify_email(email, &link))
        .await
        .map_err(|e| e.to_string())
}
