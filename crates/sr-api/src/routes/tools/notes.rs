//! The encrypted scratchpad: opaque ciphertext, owner only (UTL-DEV-001).
//!
//! The browser encrypts each note (AES-GCM, a key that never leaves it) and sends the bytes as Base64. The
//! server checks the size and stores them; it has no key, no plaintext and no way to read one.

use crate::routes::Ext;
use crate::routes::messages::error::{ApiJson, MsgError, MsgResult};
use crate::routes::userfeat::{UserFeatDeps, me, raw, uerr};
use crate::{Principal, openapi::ErrorBody};
use axum::{Json, extract::Path, http::StatusCode};
use data_encoding::BASE64;
use serde::{Deserialize, Serialize};
use sr_core::{ApiError, ErrorCode};
use sr_userfeat::tools::{MAX_NOTE_BYTES, NoteRow};
use utoipa::ToSchema;

/// One stored note.
#[derive(Debug, Serialize, ToSchema)]
pub struct NoteObject {
    /// A lowercase UUID the browser chose.
    pub note_id: String,
    /// The encrypted note, standard Base64. Opaque to the server.
    pub ciphertext: String,
    /// Starts at 1, goes up by one on every write.
    pub revision: i64,
    pub updated_at: String,
}

impl From<NoteRow> for NoteObject {
    fn from(r: NoteRow) -> Self {
        Self {
            note_id: r.note_id,
            ciphertext: BASE64.encode(&r.ciphertext),
            revision: r.revision,
            updated_at: r.updated_at,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PutNoteBody {
    /// The encrypted note, standard Base64, 1 to 65536 bytes once decoded.
    pub ciphertext: String,
    /// The revision this edit started from; absent when creating the note. A different current revision is a 409.
    #[serde(default)]
    pub base_revision: Option<i64>,
}

/// All of the caller's encrypted notes, most recently changed first.
#[utoipa::path(
    get,
    path = "/users/@me/tools/notes",
    responses((status = 200, body = Vec<NoteObject>), (status = 403, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list(p: Principal, Ext(d): Ext<UserFeatDeps>) -> MsgResult<Json<Vec<NoteObject>>> {
    let user = raw(me(&p)?);
    let rows = d.uf.list_notes(user).await.map_err(uerr)?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

/// Create a note (no `base_revision`) or replace one (the revision you read). 409 when the note changed
/// somewhere else or already exists; 400 when the ciphertext is empty or over 64 KiB.
#[utoipa::path(
    put,
    path = "/users/@me/tools/notes/{note_id}",
    params(("note_id" = String, Path, description = "Lowercase UUID")),
    request_body = PutNoteBody,
    responses(
        (status = 200, body = NoteObject),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn put(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Path(note_id): Path<String>,
    ApiJson(body): ApiJson<PutNoteBody>,
) -> MsgResult<Json<NoteObject>> {
    let user = raw(me(&p)?);
    // Base64 expands by a third: refuse an obviously oversized body before decoding it.
    if body.ciphertext.len() > MAX_NOTE_BYTES * 4 / 3 + 8 {
        return Err(MsgError::Api(ApiError::with_message(
            ErrorCode::InvalidFormBody,
            "A note is at most 64 KiB once encrypted",
        )));
    }
    let bytes = BASE64.decode(body.ciphertext.as_bytes()).map_err(|_| {
        MsgError::Api(ApiError::with_message(
            ErrorCode::InvalidFormBody,
            "ciphertext must be standard Base64",
        ))
    })?;
    let row =
        d.uf.put_note(user, &note_id, &bytes, body.base_revision)
            .await
            .map_err(uerr)?;
    Ok(Json(row.into()))
}

/// Delete a note. 404 when it does not exist (for the caller).
#[utoipa::path(
    delete,
    path = "/users/@me/tools/notes/{note_id}",
    params(("note_id" = String, Path, description = "Lowercase UUID")),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn remove(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Path(note_id): Path<String>,
) -> MsgResult<StatusCode> {
    let user = raw(me(&p)?);
    d.uf.delete_note(user, &note_id).await.map_err(uerr)?;
    Ok(StatusCode::NO_CONTENT)
}
