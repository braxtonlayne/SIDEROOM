//! Evidence snapshots (spec 3.2, D-214): the reported message with the ten before and after, as they were at report
//! time, so deleting or editing the message later does not erase the evidence.

use serde_json::{Value, json};
use sqlx::{PgConnection, Row};

use crate::error::ModResult;
use crate::rules::SNAPSHOT_CONTEXT;

/// The snapshot payload of message `message_id` in `channel_id`: the message itself (`target: true`), up to ten
/// messages before and ten after, each with its attachments index and embeds.
pub async fn message_snapshot(
    conn: &mut PgConnection,
    channel_id: i64,
    message_id: i64,
) -> ModResult<Value> {
    let ctx = i64::try_from(SNAPSHOT_CONTEXT).unwrap_or(10);
    let ids: Vec<i64> = sqlx::query_scalar(
        "(SELECT id FROM messages WHERE channel_id = $1 AND id < $2 ORDER BY id DESC LIMIT $3) \
         UNION ALL (SELECT id FROM messages WHERE channel_id = $1 AND id = $2) \
         UNION ALL (SELECT id FROM messages WHERE channel_id = $1 AND id > $2 ORDER BY id ASC LIMIT $3)",
    )
    .bind(channel_id)
    .bind(message_id)
    .bind(ctx)
    .fetch_all(&mut *conn)
    .await?;

    let rows = sqlx::query(
        "SELECT m.id, m.author_id, a.username, m.type, m.content, m.embeds, m.sticker_ids, \
                m.visibility::text AS visibility, \
                (extract(epoch FROM m.created_at) * 1000)::bigint AS created_ms, \
                (extract(epoch FROM m.edited_at) * 1000)::bigint AS edited_ms \
         FROM messages m LEFT JOIN accounts a ON a.id = m.author_id \
         WHERE m.channel_id = $1 AND m.id = ANY($2) ORDER BY m.id",
    )
    .bind(channel_id)
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;
    let atts = sqlx::query(
        "SELECT message_id, id, filename, content_type, size_bytes FROM attachments \
         WHERE message_id = ANY($1) ORDER BY id",
    )
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;

    let mut messages = Vec::with_capacity(rows.len());
    for r in &rows {
        let id: i64 = r.try_get("id")?;
        let attachments: Vec<Value> = atts
            .iter()
            .filter(|a| a.try_get::<i64, _>("message_id").ok() == Some(id))
            .map(|a| {
                json!({
                    "id": a.try_get::<i64, _>("id").unwrap_or(0).to_string(),
                    "filename": a.try_get::<String, _>("filename").unwrap_or_default(),
                    "content_type": a.try_get::<Option<String>, _>("content_type").unwrap_or(None),
                    "size_bytes": a.try_get::<i64, _>("size_bytes").unwrap_or(0),
                })
            })
            .collect();
        messages.push(json!({
            "id": id.to_string(),
            "target": id == message_id,
            "author_id": r.try_get::<i64, _>("author_id")?.to_string(),
            "author_username": r.try_get::<Option<String>, _>("username")?,
            "type": r.try_get::<i16, _>("type")?,
            "content": r.try_get::<String, _>("content")?,
            "embeds": r.try_get::<Value, _>("embeds")?,
            "sticker_ids": r.try_get::<Vec<i64>, _>("sticker_ids")?
                .iter().map(ToString::to_string).collect::<Vec<_>>(),
            "visibility": r.try_get::<String, _>("visibility")?,
            "created_at_ms": r.try_get::<i64, _>("created_ms")?,
            "edited_at_ms": r.try_get::<Option<i64>, _>("edited_ms")?,
            "attachments": attachments,
        }));
    }
    Ok(json!({
        "channel_id": channel_id.to_string(),
        "message_id": message_id.to_string(),
        "messages": messages,
    }))
}
