//! Staff removal of a reported guestbook note (PRF-PRO-001, D-507).

use super::{Actor, id, staff_result};
use crate::{error::ApiResult, openapi::ErrorBody, routes::messages::error::ApiQuery};
use axum::{extract::Path, http::StatusCode};
use serde::Deserialize;
use sr_moderation::model::Who;

/// Query of the staff removal.
#[derive(Debug, Default, Deserialize)]
pub struct RemoveQuery {
    /// Why the note is removed (required, up to 500 characters; written to the access log). Missing or blank is a 400.
    #[serde(default)]
    pub reason: String,
}

/// Remove a guestbook note (platform staff with `MANAGE_CASES`). The access-log row
/// `moderation.guestbook_note.delete` is written before the note is deleted. 404 when it is already gone.
#[utoipa::path(
    delete,
    path = "/moderation/guestbook-notes/{note_id}",
    params(
        ("note_id" = String, Path, description = "Guestbook note snowflake"),
        ("reason" = String, Query, description = "Why the note is removed (required)"),
    ),
    responses(
        (status = 204),
        (status = 400, body = ErrorBody),
        (status = 401, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn remove_note(
    a: Actor,
    Path(note): Path<String>,
    ApiQuery(q): ApiQuery<RemoveQuery>,
) -> ApiResult<StatusCode> {
    let note = id(&note, "note")?;
    let ctx = a.staff_only()?;
    let r = a
        .svc()
        .remove_guestbook_note(&Who::Staff(&ctx.caller), note, &q.reason)
        .await;
    staff_result(
        ctx,
        "moderation.guestbook_note.delete",
        Some(("guestbook_note", note)),
        r,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
