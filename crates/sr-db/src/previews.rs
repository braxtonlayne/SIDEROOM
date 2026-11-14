//! The one query behind the two previews (D-688 Message Requests, D-689 the thread row): the latest
//! not-deleted message of each of several channels, read for all of them at once, with its attachment count.
//! Messages are indexed by channel and id, so each channel is one index probe (a lateral join over the ids).

use sqlx::{PgPool, Row};
use sr_chat::preview::MessageGist;
use sr_core::{ChannelId, MessageId, UserId};
use std::collections::HashMap;

/// Message types a person wrote (default, reply, slash command, context menu): a system line such as
/// "a thread was started" is never a preview.
const WRITTEN: &str = "m.type IN (0, 19, 20, 23)";

/// For each of `channels`, its latest visible message written by a person; with `not_by`, the latest one
/// that user did not write (a Message Request shows what the sender wrote). A channel with none is absent.
pub(crate) async fn latest_gists(
    pool: &PgPool,
    channels: &[ChannelId],
    not_by: Option<UserId>,
) -> Result<HashMap<ChannelId, MessageGist>, sqlx::Error> {
    if channels.is_empty() {
        return Ok(HashMap::new());
    }
    let ids: Vec<i64> = channels.iter().map(|c| c.get() as i64).collect();
    let sql = format!(
        "SELECT t.cid AS channel_id, m.id, m.author_id, m.content, \
                (SELECT count(*) FROM attachments a WHERE a.message_id = m.id) AS attachments \
         FROM unnest($1::bigint[]) AS t(cid) \
         CROSS JOIN LATERAL ( \
             SELECT m.id, m.author_id, m.content FROM messages m \
             WHERE m.channel_id = t.cid AND m.visibility = 'visible' AND {WRITTEN} \
               AND ($2::bigint IS NULL OR m.author_id <> $2) \
             ORDER BY m.id DESC LIMIT 1 \
         ) m"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(ids)
        .bind(not_by.map(|u| u.get() as i64))
        .fetch_all(pool)
        .await?;
    let mut out = HashMap::with_capacity(rows.len());
    for r in &rows {
        let channel: i64 = r.try_get("channel_id")?;
        let id: i64 = r.try_get("id")?;
        let author: i64 = r.try_get("author_id")?;
        let n: i64 = r.try_get("attachments")?;
        out.insert(
            ChannelId::new(channel as u64),
            MessageGist {
                id: MessageId::new(id as u64),
                author_id: UserId::new(author as u64),
                content: r.try_get("content")?,
                attachments: u32::try_from(n).unwrap_or(u32::MAX),
            },
        );
    }
    Ok(out)
}
