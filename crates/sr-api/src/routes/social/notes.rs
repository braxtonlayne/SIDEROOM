//! Private notes on other users (Q111): only the author ever sees them.

use super::deps::SocialDeps;
use super::error::{ApiJson, MsgResult};
use super::events::SocialEvent;
use super::model::{NoteBody, NoteObject};
use super::util::{actor, user_id};
use crate::{Principal, openapi::ErrorBody, routes::Ext};
use axum::{Json, extract::Path, http::StatusCode};
use serde_json::json;
use sr_core::ErrorCode;

/// The caller's note on a user; 404 when there is none.
#[utoipa::path(
    get,
    path = "/users/@me/notes/{user_id}",
    params(("user_id" = String, Path, description = "User snowflake")),
    responses((status = 200, body = NoteObject), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_note(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    Path(target): Path<String>,
) -> MsgResult<Json<NoteObject>> {
    let me = actor(&p)?;
    let them = user_id(&target)?;
    let note = deps
        .service
        .get_note(me, them)
        .await?
        .ok_or(ErrorCode::UnknownUser)?;
    Ok(Json(NoteObject {
        user_id: me.to_string(),
        note_user_id: them.to_string(),
        note,
    }))
}

/// Set the note (max 256 characters); an empty or null note deletes it.
#[utoipa::path(
    put,
    path = "/users/@me/notes/{user_id}",
    params(("user_id" = String, Path, description = "User snowflake")),
    request_body = NoteBody,
    responses((status = 204, description = "Saved"), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn put_note(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    Path(target): Path<String>,
    ApiJson(body): ApiJson<NoteBody>,
) -> MsgResult<StatusCode> {
    let me = actor(&p)?;
    let them = user_id(&target)?;
    deps.service
        .set_note(me, them, body.note.as_deref())
        .await?;
    let note = deps.service.get_note(me, them).await?.unwrap_or_default();
    deps.emit(SocialEvent::new(
        "USER_NOTE_UPDATE",
        vec![me],
        json!({ "id": them.to_string(), "note": note }),
    ))
    .await;
    Ok(StatusCode::NO_CONTENT)
}
