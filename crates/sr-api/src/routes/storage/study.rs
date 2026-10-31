//! The Study's file notes (STU-SHF-001, wave 41-B): `GET|PUT /storage/mounts/{mount_id}/notes`.
//!
//! The owner of a mount (on a guild shelf a manager, `scopes::may_manage`) may give any file a custom title, a cover
//! (the key of an image on the same mount) and a note. Everyone who reaches the mount reads them with the files they
//! may already see: `scopes::access` decides the mount, `Access::permits` each key, and delisted keys are dropped.
//! Nothing is fetched from outside; the browser extracts covers from the files themselves.

use super::objects::{capabilities, reach};
use super::{ScopeDeps, ShelfDeps, facts_failed, refusal, uid};
use crate::auth::SessionOnly;
use crate::openapi::ErrorBody;
use crate::routes::Ext;
use crate::routes::messages::error::{ApiJson, MsgError, MsgResult, invalid};
use axum::{Json, extract::Path, http::StatusCode};
use serde::{Deserialize, Serialize};
use sr_core::ErrorCode;
use sr_storage::index::shelf::FileNote;
use sr_storage::path::safe_key;
use sr_storage::scopes::ScopeFacts;
use utoipa::ToSchema;

/// Most notes one mount lists.
const MAX_NOTES: i64 = 5000;
/// Longest custom title, in characters.
const MAX_TITLE: usize = 300;
/// Longest note, in characters.
const MAX_NOTE: usize = 2000;
/// Extensions a cover key may have.
const COVER_EXT: [&str; 5] = ["jpg", "jpeg", "png", "webp", "gif"];

/// One file's notes.
#[derive(Debug, Serialize, ToSchema)]
pub struct FileNoteView {
    /// Mount-relative key of the file.
    pub key: String,
    /// The owner's title for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The owner's note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Key of an image on the same mount used as the cover.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_key: Option<String>,
}

/// The notes of a mount, with what the caller may do about them.
#[derive(Debug, Serialize, ToSchema)]
pub struct FileNoteList {
    /// Notes on files the caller may read.
    pub notes: Vec<FileNoteView>,
    /// The caller may write notes here (owner; a manager on a guild shelf).
    pub can_edit: bool,
}

/// Body of `PUT /storage/mounts/{mount_id}/notes`: the whole note of one file. All three fields empty removes it.
#[derive(Debug, Deserialize, ToSchema)]
pub struct FileNoteBody {
    /// Mount-relative key of the file.
    pub key: String,
    /// A custom title (1 to 300 characters), or empty/absent for none.
    #[serde(default)]
    pub title: Option<String>,
    /// A note (1 to 2000 characters), or empty/absent for none.
    #[serde(default)]
    pub note: Option<String>,
    /// The key of an image on the same mount, or empty/absent for none.
    #[serde(default)]
    pub cover_key: Option<String>,
}

fn store_failed(e: impl std::fmt::Display) -> MsgError {
    tracing::error!(error = %e, "file notes failed");
    ErrorCode::SrInternal.into()
}

fn clean(field: &str, v: &Option<String>, max: usize) -> MsgResult<Option<String>> {
    let Some(t) = v.as_deref().map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if t.chars().count() > max {
        return Err(invalid(
            field,
            "BASE_TYPE_BAD_LENGTH",
            &format!("Use at most {max} characters."),
        ));
    }
    Ok(Some(t.to_owned()))
}

/// Validate a body into the note to store, and whether it is empty (the note is then removed).
fn note_of(b: &FileNoteBody) -> MsgResult<(FileNote, bool)> {
    safe_key(&b.key, false).map_err(|e| invalid("key", "KEY_INVALID", &e.to_string()))?;
    let title = clean("title", &b.title, MAX_TITLE)?;
    let note = clean("note", &b.note, MAX_NOTE)?;
    let cover_key = match b
        .cover_key
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        None => None,
        Some(c) => {
            safe_key(c, false).map_err(|e| invalid("cover_key", "KEY_INVALID", &e.to_string()))?;
            let ext = c.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
            if !ext.is_some_and(|e| COVER_EXT.contains(&e.as_str())) {
                return Err(invalid(
                    "cover_key",
                    "COVER_NOT_IMAGE",
                    "A cover must be a jpg, png, webp or gif image.",
                ));
            }
            Some(c.to_owned())
        }
    };
    let empty = title.is_none() && note.is_none() && cover_key.is_none();
    Ok((
        FileNote {
            key: b.key.clone(),
            title,
            note,
            cover_key,
        },
        empty,
    ))
}

/// The owner's titles, covers and notes for the files of a mount.
#[utoipa::path(
    get,
    path = "/storage/mounts/{mount_id}/notes",
    params(("mount_id" = String, Path, description = "Mount snowflake")),
    responses((status = 200, body = FileNoteList), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list_notes(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(sh): Ext<ShelfDeps>,
    Path(mount_id): Path<String>,
) -> MsgResult<Json<FileNoteList>> {
    let r = reach(&d, None, uid(&p), None, &mount_id).await?;
    let rows = sh
        .store
        .file_notes(r.mount.id, MAX_NOTES)
        .await
        .map_err(store_failed)?;
    let rows: Vec<FileNote> = rows
        .into_iter()
        .filter(|n| r.access.permits(&n.key).is_ok())
        .collect();
    let facts: &dyn ScopeFacts = &*d.store;
    let keys: Vec<String> = rows.iter().map(|n| n.key.clone()).collect();
    let gone: std::collections::HashSet<String> = facts
        .delisted(r.mount.id, &keys)
        .await
        .map_err(facts_failed)?
        .into_iter()
        .collect();
    let notes = rows
        .into_iter()
        .filter(|n| !gone.contains(&n.key))
        .map(|n| FileNoteView {
            key: n.key,
            title: n.title,
            note: n.note,
            cover_key: n.cover_key,
        })
        .collect();
    let (_, can_edit) = capabilities(&d, &r, uid(&p)).await;
    Ok(Json(FileNoteList { notes, can_edit }))
}

/// Set or clear the note of one file. Owner only (a manager on a guild shelf).
#[utoipa::path(
    put,
    path = "/storage/mounts/{mount_id}/notes",
    params(("mount_id" = String, Path, description = "Mount snowflake")),
    request_body = FileNoteBody,
    responses(
        (status = 204),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn put_note(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(sh): Ext<ShelfDeps>,
    Path(mount_id): Path<String>,
    ApiJson(body): ApiJson<FileNoteBody>,
) -> MsgResult<StatusCode> {
    let r = reach(&d, None, uid(&p), None, &mount_id).await?;
    // `capabilities().1` is `scopes::may_manage`: the owner, or a manager of the server.
    let (_, can_edit) = capabilities(&d, &r, uid(&p)).await;
    if !can_edit {
        return Err(ErrorCode::MissingPermissions.into());
    }
    let (note, empty) = note_of(&body)?;
    // A key the caller could not read is not theirs to annotate either.
    r.access.permits(&note.key).map_err(refusal)?;
    if empty {
        sh.store
            .delete_file_note(r.mount.id, &note.key)
            .await
            .map_err(store_failed)?;
    } else {
        sh.store
            .set_file_note(r.mount.id, &note, uid(&p))
            .await
            .map_err(store_failed)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(key: &str, title: &str, note: &str, cover: &str) -> FileNoteBody {
        let o = |s: &str| (!s.is_empty()).then(|| s.to_owned());
        FileNoteBody {
            key: key.into(),
            title: o(title),
            note: o(note),
            cover_key: o(cover),
        }
    }

    #[test]
    fn a_blank_body_removes_the_note() {
        let (_, empty) = note_of(&body("a/b.mp4", "  ", "", "")).unwrap();
        assert!(empty);
    }

    #[test]
    fn fields_are_trimmed_and_kept() {
        let (n, empty) = note_of(&body("a/b.mp4", " My cut ", "seen twice", "a/c.JPG")).unwrap();
        assert!(!empty);
        assert_eq!(n.title.as_deref(), Some("My cut"));
        assert_eq!(n.note.as_deref(), Some("seen twice"));
        assert_eq!(n.cover_key.as_deref(), Some("a/c.JPG"));
    }

    #[test]
    fn a_cover_must_be_an_image_and_a_key_must_be_safe() {
        assert!(note_of(&body("a/b.mp4", "", "", "a/c.mp4")).is_err());
        assert!(note_of(&body("/abs.mp4", "t", "", "")).is_err());
        assert!(note_of(&body("a/b.mp4", &"x".repeat(301), "", "")).is_err());
        assert!(note_of(&body("a/b.mp4", "", &"x".repeat(2001), "")).is_err());
    }
}
