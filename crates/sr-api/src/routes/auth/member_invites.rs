//! Platform invites the signed-in account manages itself (G-57, LNG-USET-025..029):
//! `GET|POST /sr/invites` and `DELETE /sr/invites/{code}`.
//!
//! Browser session only (bots and bearer tokens are refused), ten creations per hour per
//! account. The store re-checks every rule when a code is created: verified email, account age,
//! the account cap (D-014), open codes below remaining credits (D-026) and the suspicion freeze.
//! The frozen state is reported with generic wording only (doc 06 section 5.4).

use super::{
    common::{AuthRouteError, R, Svc},
    sessions::session_id,
    throttle::Rule,
};
use crate::{auth::UserOnly, openapi::ErrorBody, state::AppState};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sr_auth::{
    invite::InviteCode,
    store::{InviteBlock, MemberInviteRow, MemberInvites},
};
use sr_core::{ApiError, ErrorCode};
use utoipa::ToSchema;

/// Longest private note.
const MAX_NOTE_CHARS: usize = 100;

/// Ten creations per hour per account.
const CREATE_RULES: [Rule; 1] = [Rule {
    limit: 10,
    secs: 3_600,
}];

/// One of the account's platform invites.
#[derive(Debug, Serialize, ToSchema)]
pub struct MemberInvite {
    /// Display form, `XXXXX-XXXXX`.
    pub code: String,
    /// Path of the share link (`/join/<code>`); prefix with the site origin.
    pub link: String,
    /// Private note, visible to the issuer and staff only.
    pub note: String,
    pub created_at_ms: i64,
    /// Redeemed codes only: the account that joined, and when.
    pub redeemed_by: Option<RedeemedBy>,
    pub redeemed_at_ms: Option<i64>,
}

/// The account that used a code.
#[derive(Debug, Serialize, ToSchema)]
pub struct RedeemedBy {
    #[schema(value_type = String)]
    pub id: String,
    pub username: String,
}

/// Why a new code cannot be created right now.
#[derive(Debug, Serialize, ToSchema)]
pub struct InviteBlocked {
    /// `unverified`, `too_new`, `at_cap`, `no_credits`, `at_max` or `frozen`.
    pub reason: String,
    /// `too_new` only: the age an account needs, in days.
    pub days: Option<i64>,
}

/// The Invites section of User Settings.
#[derive(Debug, Serialize, ToSchema)]
pub struct MemberInvitesObject {
    /// Credits left: how many more people the account can bring.
    pub credits: i64,
    pub open: Vec<MemberInvite>,
    pub redeemed: Vec<MemberInvite>,
    /// `None` when a code can be created now.
    pub blocked: Option<InviteBlocked>,
}

/// Body of `POST /sr/invites`.
#[derive(Debug, Default, Deserialize, ToSchema)]
#[schema(as = MemberCreateInviteBody)]
pub struct CreateInviteBody {
    /// Private note ("for Sam"), at most 100 characters.
    #[serde(default)]
    pub note: Option<String>,
}

fn row(r: &MemberInviteRow) -> MemberInvite {
    let formatted = InviteCode::parse(&r.code)
        .map(|c| c.formatted())
        .unwrap_or_else(|_| r.code.clone());
    MemberInvite {
        link: format!("/join/{formatted}"),
        code: formatted,
        note: r.note.clone(),
        created_at_ms: r.created_at * 1000,
        redeemed_by: r.redeemed_by.as_ref().map(|(id, name)| RedeemedBy {
            id: id.to_string(),
            username: name.clone(),
        }),
        redeemed_at_ms: r.redeemed_at.map(|s| s * 1000),
    }
}

fn blocked(b: InviteBlock) -> InviteBlocked {
    let (reason, days) = match b {
        InviteBlock::Unverified => ("unverified", None),
        InviteBlock::TooNew { days } => ("too_new", Some(days)),
        InviteBlock::AtCap => ("at_cap", None),
        InviteBlock::NoCredits => ("no_credits", None),
        InviteBlock::Frozen => ("frozen", None),
        InviteBlock::AtOutstandingMax => ("at_max", None),
    };
    InviteBlocked {
        reason: reason.to_owned(),
        days,
    }
}

fn object(m: &MemberInvites) -> MemberInvitesObject {
    MemberInvitesObject {
        credits: m.balance,
        open: m.open.iter().map(row).collect(),
        redeemed: m.redeemed.iter().map(row).collect(),
        blocked: m.blocked.map(blocked),
    }
}

/// `GET /sr/invites`: credits, open and redeemed codes and whether a code can be created.
#[utoipa::path(
    get, path = "/sr/invites",
    responses((status = 200, body = MemberInvitesObject), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    UserOnly(p): UserOnly,
    Svc { svc, .. }: Svc,
) -> R<Json<MemberInvitesObject>> {
    session_id(&p)?;
    let m = svc
        .store
        .member_invites(p.user_id, state.now_secs())
        .await?;
    Ok(Json(object(&m)))
}

/// `POST /sr/invites {note?}`: create a code. 403 `900006`-style refusals are field errors on
/// `invite` (`reason` in the code); 429 after ten in an hour.
#[utoipa::path(
    post, path = "/sr/invites", request_body = CreateInviteBody,
    responses((status = 201, body = MemberInvite), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 429, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create(
    State(state): State<AppState>,
    UserOnly(p): UserOnly,
    Svc { svc, .. }: Svc,
    body: Option<Json<CreateInviteBody>>,
) -> R<(StatusCode, Json<MemberInvite>)> {
    session_id(&p)?;
    let note = body
        .and_then(|Json(b)| b.note)
        .unwrap_or_default()
        .trim()
        .to_owned();
    if note.chars().count() > MAX_NOTE_CHARS {
        return Err(ApiError::invalid_form()
            .field(
                "note",
                "BASE_TYPE_MAX_LENGTH",
                format!("The note is at most {MAX_NOTE_CHARS} characters."),
            )
            .into());
    }
    let now = state.now_secs();
    svc.ip_limits
        .hit(
            "invite-create",
            &format!("acct:{}", p.user_id),
            now,
            &CREATE_RULES,
        )
        .map_err(|retry_after| AuthRouteError::Throttled { retry_after })?;
    // The unique-code collision is astronomically unlikely; retry a few times anyway.
    let mut last = None;
    for _ in 0..4 {
        let code = InviteCode::generate().map_err(|e| {
            tracing::error!(error = %e, "invite code generation failed");
            ApiError::new(ErrorCode::SrInternal)
        })?;
        let id = (svc.ids)();
        match svc
            .store
            .create_member_invite(p.user_id, id, &code, &note, now)
            .await
        {
            Ok(Ok(())) => {
                let r = MemberInviteRow {
                    code: code.canonical().to_owned(),
                    note,
                    created_at: now,
                    redeemed_by: None,
                    redeemed_at: None,
                };
                return Ok((StatusCode::CREATED, Json(row(&r))));
            }
            Ok(Err(block)) => {
                let b = blocked(block);
                return Err(ApiError::invalid_form()
                    .field("invite", &b.reason, refusal_text(block))
                    .into());
            }
            Err(e) => last = Some(e),
        }
    }
    Err(last.map_or_else(|| ApiError::new(ErrorCode::SrInternal).into(), Into::into))
}

fn refusal_text(b: InviteBlock) -> String {
    match b {
        InviteBlock::Unverified => "Verify your email address to create invites.".into(),
        InviteBlock::TooNew { days } => {
            format!("Accounts can create invites after {days} days.")
        }
        InviteBlock::AtCap => "SIDE ROOM is full right now, so invites are paused.".into(),
        InviteBlock::NoCredits => "You have no spare invite credits.".into(),
        InviteBlock::Frozen => "Invite creation is paused for review.".into(),
        InviteBlock::AtOutstandingMax => {
            "You already hold the most unused invites allowed. Use or revoke one first.".into()
        }
    }
}

/// `DELETE /sr/invites/{code}`: revoke an open code of your own. 404 when it is not open or not
/// yours (a redemption in flight wins).
#[utoipa::path(
    delete, path = "/sr/invites/{code}",
    params(("code" = String, Path, description = "Invite code")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn revoke(
    UserOnly(p): UserOnly,
    Svc { svc, .. }: Svc,
    Path(code): Path<String>,
) -> R<StatusCode> {
    session_id(&p)?;
    let Ok(code) = InviteCode::parse(&code) else {
        return Err(ErrorCode::SrNotFound.into());
    };
    if svc.store.revoke_member_invite(p.user_id, &code).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ErrorCode::SrNotFound.into())
    }
}

/// OpenAPI fragment for the member invite routes.
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(list, create, revoke),
    components(schemas(
        MemberInvitesObject,
        MemberInvite,
        RedeemedBy,
        InviteBlocked,
        CreateInviteBody
    ))
)]
pub struct MemberInvitesApiDoc;

#[cfg(test)]
mod tests {
    use super::*;

    /// MOD-SUS-005: a paused inviter reads only that creation is paused for review, never a cause.
    #[test]
    fn the_paused_notice_names_no_cause() {
        let text = refusal_text(InviteBlock::Frozen);
        assert_eq!(text, "Invite creation is paused for review.");
        let lower = text.to_lowercase();
        for word in ["ban", "suspic", "invitee", "serious"] {
            assert!(!lower.contains(word), "mentions {word}");
        }
        assert_eq!(blocked(InviteBlock::Frozen).reason, "frozen");
    }
}
