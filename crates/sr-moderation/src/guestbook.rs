//! Reports about guestbook notes (PRF-PRO-001, D-506, D-507).
//!
//! A note on a profile's guestbook is reported like any other target (`target_kind = "guestbook_note"`). Who may
//! report it: the page's owner, for any note on their page (a held note is visible to the owner alone, so only
//! they can report it); a signed-in viewer the guestbook block admits, for an approved note; never the note's
//! author. Everyone else gets the one 404 a missing note gets, so a pending note is no oracle.
//!
//! The report goes to platform staff (a profile belongs to no server) with a text snapshot as evidence; the
//! visitor's `ip_hash` is never copied. There is no hold: the owner may delete the note at any time. Staff may
//! remove the note through [`Moderation::remove_guestbook_note`], which writes the access-log row first.

use serde_json::{Value, json};
use sqlx::{PgConnection, Row};
use sr_admin::AccessKind;
use sr_core::PlatformPermissions as P;

use crate::error::{ModError, ModResult, invalid};
use crate::model::Who;
use crate::service::{Moderation, log_staff, require_staff};

/// What the report path needs of a note the reporter may see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteFacts {
    /// The page's owner.
    pub profile_id: i64,
    /// The signed-in author; `None` for a visitor without an account.
    pub author_id: Option<i64>,
    pub pending: bool,
}

/// The facts of a note `reporter` may see, or `None` when it is hidden from them or not there: the owner sees
/// every note of their page; another signed-in viewer sees an approved note when the guestbook block admits them.
pub async fn visible_note(
    conn: &mut PgConnection,
    note: i64,
    reporter: i64,
) -> ModResult<Option<NoteFacts>> {
    let Some(r) =
        sqlx::query("SELECT profile_id, author_id, status FROM profile_guestbook WHERE id = $1")
            .bind(note)
            .fetch_optional(&mut *conn)
            .await?
    else {
        return Ok(None);
    };
    let facts = NoteFacts {
        profile_id: r.try_get("profile_id")?,
        author_id: r.try_get("author_id")?,
        pending: r.try_get::<String, _>("status")? == "pending",
    };
    if facts.profile_id == reporter {
        return Ok(Some(facts));
    }
    if facts.pending {
        return Ok(None);
    }
    let sees =
        sr_packages::service::privacy::guestbook_visible_to(conn, reporter, facts.profile_id)
            .await?;
    Ok(sees.then_some(facts))
}

/// Resolve a report of a note: the facts, or the missing-note 404, or the refusal for the author.
pub(crate) async fn resolve_report(
    conn: &mut PgConnection,
    note: i64,
    reporter: i64,
) -> ModResult<NoteFacts> {
    let facts = visible_note(conn, note, reporter)
        .await?
        .ok_or(ModError::NotFound("note"))?;
    if facts.author_id == Some(reporter) {
        return Err(invalid("You cannot report your own note"));
    }
    Ok(facts)
}

/// The evidence payload of a note, taken at report time (D-507). It never carries the visitor's address hash.
pub async fn note_snapshot(conn: &mut PgConnection, note: i64) -> ModResult<Value> {
    let r = sqlx::query(
        "SELECT g.profile_id, g.author_id, g.anonymous, g.status, g.body, \
                (extract(epoch FROM g.created_at) * 1000)::bigint AS created_ms, \
                p.username::text AS profile_username, a.username::text AS author_username \
         FROM profile_guestbook g \
         JOIN accounts p ON p.id = g.profile_id \
         LEFT JOIN accounts a ON a.id = g.author_id \
         WHERE g.id = $1",
    )
    .bind(note)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(ModError::NotFound("note"))?;
    Ok(json!({
        "kind": "guestbook_note",
        "id": note.to_string(),
        "profile_id": r.try_get::<i64, _>("profile_id")?.to_string(),
        "profile_username": r.try_get::<Option<String>, _>("profile_username")?,
        "author_id": r.try_get::<Option<i64>, _>("author_id")?.map(|a| a.to_string()),
        "author_username": r.try_get::<Option<String>, _>("author_username")?,
        "anonymous": r.try_get::<bool, _>("anonymous")?,
        "status": r.try_get::<String, _>("status")?,
        "body": r.try_get::<String, _>("body")?,
        "created_at_ms": r.try_get::<i64, _>("created_ms")?,
    }))
}

impl Moderation {
    /// Staff remove a reported guestbook note (platform staff with `MANAGE_CASES`). The access-log row
    /// `moderation.guestbook_note.delete` is written before the delete, in the same transaction. `NotFound` when
    /// the note is already gone.
    pub async fn remove_guestbook_note(
        &self,
        who: &Who<'_>,
        note: i64,
        reason: &str,
    ) -> ModResult<()> {
        require_staff(who, P::MANAGE_CASES, "MANAGE_CASES")?;
        let mut tx = self.pool.begin().await?;
        let profile: i64 =
            sqlx::query_scalar("SELECT profile_id FROM profile_guestbook WHERE id = $1 FOR UPDATE")
                .bind(note)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(ModError::NotFound("note"))?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.guestbook_note.delete",
            ("guestbook_note", note),
            reason.trim(),
            json!({ "profile_id": profile.to_string() }),
        )
        .await?;
        sqlx::query("DELETE FROM profile_guestbook WHERE id = $1")
            .bind(note)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}
