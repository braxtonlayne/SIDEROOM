//! PRF-SCR-001: the scrapbook's part of the data-subject paths (docs/09). Kept in its own file; the export
//! and the erasure each gain one registration line (`data_export::build_package`, `account_deletion`).
//!
//! * [`export_for`] (Art. 15): what the scrapbook holds about the member: entries made from their messages,
//!   entries they sent to a scrapbook, and the copies' file names, sizes and types (never keys or URLs).
//! * [`erase_for`] (Art. 17): inside the deletion transaction, the entries about the member's messages are
//!   deleted and the removal of each copy is queued in the same transaction (`scrapbook.removals` empties the
//!   buckets afterwards; a bucket that refuses stays visible to the server's managers). Entries the member
//!   merely sent to a scrapbook lose the reference to them, and a shelf they delegated `upload` for is
//!   unlinked, so a manager must grant it again.

use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use sr_storage::scrapbook::DeleteScope;

use crate::DbResult;
use crate::scrapbook_repo::delete_in;

/// The data-subject export for `user`.
pub async fn export_for(pool: &PgPool, user: i64) -> DbResult<Value> {
    let about: Value = sqlx::query_scalar(
        "SELECT COALESCE(jsonb_agg(t ORDER BY t.id), '[]'::jsonb) FROM (
            SELECT e.id::text AS id, e.guild_id::text AS guild_id, e.channel_id::text AS channel_id,
                   e.message_id::text AS message_id, e.content, e.sent_at, e.tags, e.via,
                   e.archived_by::text AS archived_by, e.created_at,
                   (SELECT COALESCE(jsonb_agg(jsonb_build_object('filename', o.filename,
                            'content_type', o.content_type, 'size_bytes', o.size_bytes, 'kind', o.kind)
                            ORDER BY o.position), '[]'::jsonb)
                      FROM scrapbook_objects o WHERE o.entry_id = e.id) AS files
            FROM scrapbook_entries e WHERE e.author_id = $1) t",
    )
    .bind(user)
    .fetch_one(pool)
    .await?;
    let sent: Value = sqlx::query_scalar(
        "SELECT COALESCE(jsonb_agg(t ORDER BY t.id), '[]'::jsonb) FROM (
            SELECT e.id::text AS id, e.guild_id::text AS guild_id, e.message_id::text AS message_id,
                   e.via, e.created_at
            FROM scrapbook_entries e WHERE e.archived_by = $1) t",
    )
    .bind(user)
    .fetch_one(pool)
    .await?;
    let shelves: Value = sqlx::query_scalar(
        "SELECT COALESCE(jsonb_agg(t ORDER BY t.guild_id), '[]'::jsonb) FROM (
            SELECT guild_id::text AS guild_id, mount_id::text AS mount_id, granted_at
            FROM scrapbook_targets WHERE granted_by = $1) t",
    )
    .bind(user)
    .fetch_one(pool)
    .await?;
    Ok(json!({
        "entries_about_your_messages": about,
        "entries_you_sent": sent,
        "shelves_you_allowed": shelves,
    }))
}

/// What erasure did to the scrapbook.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Erased {
    /// Entries deleted.
    pub entries: u64,
    /// Copies queued for removal from their bucket.
    pub copies_queued: u64,
}

/// Erase the scrapbook's rows about `user` inside the deletion transaction.
pub async fn erase_for(conn: &mut PgConnection, user: i64) -> DbResult<Erased> {
    let deleted = delete_in(conn, DeleteScope::Author(user)).await?;
    sqlx::query("UPDATE scrapbook_entries SET archived_by = NULL WHERE archived_by = $1")
        .bind(user)
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM scrapbook_targets WHERE granted_by = $1")
        .bind(user)
        .execute(&mut *conn)
        .await?;
    Ok(Erased {
        entries: deleted.entries,
        copies_queued: deleted.queued.len() as u64,
    })
}
