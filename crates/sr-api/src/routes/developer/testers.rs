//! Application testers (G-01, D-204): the owner invites accounts; the invitee answers the
//! system DM (buttons `sr:tester-accept:<id>` / `sr:tester-decline:<id>`, which call the
//! respond route); accepted testers may install a `development` application in a small
//! guild they manage.

use super::{
    DevDeps, SystemButton, SystemCard, TesterBody, TesterObject, TesterRespondBody,
    TesterRespondResult, app_err, invalid, parse_id,
};
use crate::{
    auth::SessionOnly,
    openapi::ErrorBody,
    routes::{
        Ext,
        messages::error::{ApiJson, MsgResult},
    },
};
use axum::{Json, extract::Path, http::StatusCode};
use sr_apps::{
    model::{Application, Tester},
    repo::AccountBrief,
    service::TesterTarget,
};
use sr_core::ErrorCode;

const APP: ErrorCode = ErrorCode::UnknownApplication;
const USER: ErrorCode = ErrorCode::UnknownUser;

/// Discord button styles used on the invitation card.
const STYLE_SUCCESS: u8 = 3;
const STYLE_DANGER: u8 = 4;

fn object(t: &Tester, who: Option<&AccountBrief>) -> TesterObject {
    TesterObject {
        id: t.id.to_string(),
        user_id: t.account_id.to_string(),
        username: who.map(|a| a.username.clone()),
        global_name: who.and_then(|a| a.display_name.clone()),
        state: t.state.as_str().to_owned(),
        created_at: super::iso(t.created_at_ms),
    }
}

/// The DM that asks the invitee (system user, Accept / Decline).
fn invite_card(app: &Application, owner_name: &str, invite_id: i64) -> SystemCard {
    SystemCard {
        content: "You've been invited to test an application.".to_owned(),
        title: format!("Tester invitation: {}", app.name),
        description: format!(
            "@{owner_name} invited you to test their application {}. As a tester you can add it \
             to servers with up to 25 members where you can manage the server, before it is \
             released. You can leave at any time.",
            app.name
        ),
        buttons: vec![
            SystemButton {
                style: STYLE_SUCCESS,
                label: "Accept".to_owned(),
                custom_id: format!("sr:tester-accept:{invite_id}"),
            },
            SystemButton {
                style: STYLE_DANGER,
                label: "Decline".to_owned(),
                custom_id: format!("sr:tester-decline:{invite_id}"),
            },
        ],
    }
}

/// Testers of my application, invited and accepted.
#[utoipa::path(
    get, path = "/applications/{application_id}/testers", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake")),
    responses((status = 200, body = Vec<TesterObject>), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    Path(id): Path<String>,
) -> MsgResult<Json<Vec<TesterObject>>> {
    let id = parse_id(&id, APP)?;
    let rows = deps
        .apps
        .list_testers(p.user_id, id)
        .await
        .map_err(app_err)?;
    Ok(Json(
        rows.iter()
            .map(|(t, who)| object(t, who.as_ref()))
            .collect(),
    ))
}

/// Invite an account to test my application (by username or id). At most 25 testers.
#[utoipa::path(
    post, path = "/applications/{application_id}/testers", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake")),
    request_body = TesterBody,
    responses((status = 201, body = TesterObject), (status = 400, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn invite(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<TesterBody>,
) -> MsgResult<(StatusCode, Json<TesterObject>)> {
    let id = parse_id(&id, APP)?;
    let target = match (body.user_id.as_deref(), body.username.as_deref()) {
        (Some(raw), _) => TesterTarget::Id(parse_id(raw, USER)?),
        (None, Some(name)) if !name.trim().is_empty() => {
            TesterTarget::Username(name.trim().trim_start_matches('@').to_owned())
        }
        _ => {
            return Err(invalid(
                "username",
                "BASE_TYPE_REQUIRED",
                "Name the person to invite by username.",
            ));
        }
    };
    let (t, app) = deps
        .apps
        .invite_tester(p.user_id, id, target)
        .await
        .map_err(app_err)?;
    let users = deps.users(&[p.user_id, t.account_id]).await?;
    let owner_name = users
        .get(&sr_core::UserId::new(p.user_id as u64))
        .map_or_else(|| "someone".to_owned(), |u| u.username.clone());
    deps.dm_user(t.account_id, invite_card(&app, &owner_name, t.id))
        .await;
    let who = users
        .get(&sr_core::UserId::new(t.account_id as u64))
        .map(|u| AccountBrief {
            id: t.account_id,
            username: u.username.clone(),
            display_name: u.display_name.clone(),
            is_bot: false,
        });
    Ok((StatusCode::CREATED, Json(object(&t, who.as_ref()))))
}

/// Remove a tester or withdraw an open invitation.
#[utoipa::path(
    delete, path = "/applications/{application_id}/testers/{account_id}", tag = "developer",
    params(("application_id" = String, Path, description = "Application snowflake"),
           ("account_id" = String, Path, description = "Tester account snowflake")),
    responses((status = 204), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn remove(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    Path((id, account)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let id = parse_id(&id, APP)?;
    let account = parse_id(&account, USER)?;
    deps.apps
        .remove_tester(p.user_id, id, account)
        .await
        .map_err(app_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Answer a tester invitation. This is what the Accept and Decline buttons of the system DM
/// call. Only the invited account may answer.
#[utoipa::path(
    post, path = "/developers/tester-invites/{invite_id}/respond", tag = "developer",
    params(("invite_id" = String, Path, description = "Invitation snowflake")),
    request_body = TesterRespondBody,
    responses((status = 200, body = TesterRespondResult), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn respond(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    Path(invite): Path<String>,
    ApiJson(body): ApiJson<TesterRespondBody>,
) -> MsgResult<Json<TesterRespondResult>> {
    let invite = parse_id(&invite, ErrorCode::SrNotFound)?;
    let ans = deps
        .apps
        .respond_tester(p.user_id, invite, body.accept)
        .await
        .map_err(app_err)?;
    Ok(Json(TesterRespondResult {
        application_id: ans.application.id.to_string(),
        name: ans.application.name,
        accepted: ans.accepted,
    }))
}
