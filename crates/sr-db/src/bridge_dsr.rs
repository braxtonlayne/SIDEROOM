//! BRG-UI-001 (D-381): the Discord bridge's part of the data-subject paths (docs/09, lawyer brief Q21). Kept in its
//! own file; the export and the erasure each gain one registration line (`data_export::build_package`,
//! `account_deletion`), the same way the scrapbook and the status rings do.
//!
//! * [`export_for`] (Art. 15): the member's bridged messages as ids only (ours, Discord's, the channel and the
//!   direction), and their per-channel opt-outs. No content, no Discord names.
//! * [`erase_for`] (Art. 17): inside the deletion transaction, the message-map rows of the member's messages are
//!   deleted, and for each message the bridge posted to Discord through a webhook a `delete` is queued on the
//!   outbound queue (`bridge_outbox`, best effort: the bridge's worker logs a failure and the rows are gone
//!   regardless); the member's opt-outs are deleted. The queue row carries everything the worker needs, because the
//!   map row is gone: `{ "discord_message_id", "webhook_id", "reason": "erasure" }`.
//!
//! Tables come from migration 1720 (bridge-core). A server that never switched the bridge on has no rows, and a
//! database without the tables (a branch that predates 1720) is treated as having none.

use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};

use crate::DbResult;

/// Does the bridge's schema exist in this database?
async fn has_tables<'e, E>(exec: E) -> DbResult<bool>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    Ok(
        sqlx::query_scalar("SELECT to_regclass('bridge_message_map') IS NOT NULL")
            .fetch_one(exec)
            .await?,
    )
}

/// The data-subject export for `user` (`bridge.json`): ids only.
pub async fn export_for(pool: &PgPool, user: i64) -> DbResult<Value> {
    if !has_tables(pool).await? {
        return Ok(json!({ "bridged_messages": [], "opt_outs": [] }));
    }
    let messages: Value = sqlx::query_scalar(
        "SELECT COALESCE(jsonb_agg(t ORDER BY t.message_id), '[]'::jsonb) FROM (
            SELECT mm.sr_message_id::text AS message_id, mm.discord_message_id::text AS discord_message_id,
                   m.channel_id::text AS channel_id, mm.direction
            FROM bridge_message_map mm JOIN messages m ON m.id = mm.sr_message_id
            WHERE m.author_id = $1) t",
    )
    .bind(user)
    .fetch_one(pool)
    .await?;
    let opt_outs: Value = sqlx::query_scalar(
        "SELECT COALESCE(jsonb_agg(t ORDER BY t.channel_id), '[]'::jsonb) FROM (
            SELECT channel_id::text AS channel_id, guild_id::text AS guild_id, created_at
            FROM bridge_optouts WHERE user_id = $1) t",
    )
    .bind(user)
    .fetch_one(pool)
    .await?;
    Ok(json!({ "bridged_messages": messages, "opt_outs": opt_outs }))
}

/// What erasure did to the bridge's rows about a member.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Erased {
    /// Message-map rows deleted.
    pub map_rows: u64,
    /// Webhook messages queued for deletion on Discord.
    pub deletions_queued: u64,
    /// Opt-outs deleted.
    pub opt_outs: u64,
}

/// Erase the bridge's rows about `user` inside the deletion transaction.
pub async fn erase_for(conn: &mut PgConnection, user: i64) -> DbResult<Erased> {
    if !has_tables(&mut *conn).await? {
        return Ok(Erased::default());
    }
    // The queue row gets a negative id derived from the message (message ids are positive snowflakes, so they never
    // clash with the worker's own ids); the pending delete is written before the map row is removed.
    let queued = sqlx::query(
        "INSERT INTO bridge_outbox (id, guild_id, map_id, kind, sr_message_id, payload)
         SELECT -mm.sr_message_id, c.guild_id, mm.map_id, 'delete', mm.sr_message_id,
                jsonb_build_object('discord_message_id', mm.discord_message_id::text,
                                   'webhook_id', mm.webhook_id::text, 'reason', 'erasure')
         FROM bridge_message_map mm
           JOIN messages m ON m.id = mm.sr_message_id
           JOIN bridge_channel_maps c ON c.id = mm.map_id
         WHERE m.author_id = $1 AND mm.direction = 'out' AND mm.webhook_id IS NOT NULL
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    let map_rows = sqlx::query(
        "DELETE FROM bridge_message_map mm USING messages m
         WHERE m.id = mm.sr_message_id AND m.author_id = $1",
    )
    .bind(user)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    let opt_outs = sqlx::query("DELETE FROM bridge_optouts WHERE user_id = $1")
        .bind(user)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    Ok(Erased {
        map_rows,
        deletions_queued: queued,
        opt_outs,
    })
}
