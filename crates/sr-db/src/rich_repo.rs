//! Postgres implementation of [`sr_chat::rich_repo::RichStore`]: link-preview
//! embeds, poll votes and their finalisation, sticker rows. Tests live in
//! `tests/rich_store.rs` and need `SR_TEST_DATABASE_URL`.

use async_trait::async_trait;
use sqlx::{PgPool, Row};
use sr_chat::message::{Embed, Poll};
use sr_chat::poll_rules;
use sr_chat::repo::{StoreError, StoreResult};
use sr_chat::rich_repo::{
    PackSticker, PollTally, RetractResult, RichStore, StickerPack, StickerRecord, VoteResult,
};
use sr_core::{ChannelId, GuildId, MessageId, UserId};
use std::collections::HashMap;

#[derive(Clone)]
pub struct PgRichStore {
    pool: PgPool,
}

impl PgRichStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn sdb(e: sqlx::Error) -> StoreError {
    StoreError(e.to_string())
}

// Ids are positive Snowflakes (< 2^63), so these casts are lossless.
fn i(id: u64) -> i64 {
    id as i64
}
fn u(v: i64) -> u64 {
    v as u64
}

fn seconds(ms: i64) -> f64 {
    ms as f64 / 1000.0
}

/// The poll of a message, locked `FOR SHARE` so a concurrent finalisation (which
/// updates the row) waits for in-flight votes and later votes see it closed.
async fn lock_poll(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    message: MessageId,
) -> StoreResult<Option<Poll>> {
    let row: Option<Option<serde_json::Value>> =
        sqlx::query_scalar("SELECT poll FROM messages WHERE id = $1 FOR SHARE")
            .bind(i(message.get()))
            .fetch_optional(&mut **tx)
            .await
            .map_err(sdb)?;
    Ok(row
        .flatten()
        .and_then(|v| serde_json::from_value::<Poll>(v).ok()))
}

#[async_trait]
impl RichStore for PgRichStore {
    async fn bridged_members(
        &self,
        guild: sr_core::GuildId,
    ) -> StoreResult<Vec<sr_chat::rich_repo::BridgedMember>> {
        let rows = sqlx::query(
            "SELECT m.discord_user_id, m.display_name, m.avatar_url, m.role_name, m.role_color,                     m.is_bot, m.presence              FROM bridge_members m WHERE m.guild_id = $1              ORDER BY lower(m.display_name), m.discord_user_id LIMIT 5000",
        )
        .bind(i(guild.get()))
        .fetch_all(&self.pool)
        .await
        .map_err(sdb)?;
        rows.iter()
            .map(|r| {
                Ok(sr_chat::rich_repo::BridgedMember {
                    discord_user_id: r
                        .try_get::<i64, _>("discord_user_id")
                        .map_err(sdb)?
                        .to_string(),
                    display_name: r.try_get("display_name").map_err(sdb)?,
                    avatar_url: r.try_get("avatar_url").map_err(sdb)?,
                    role_name: r.try_get("role_name").map_err(sdb)?,
                    role_color: r.try_get("role_color").map_err(sdb)?,
                    bot: r.try_get("is_bot").map_err(sdb)?,
                    presence: r.try_get("presence").map_err(sdb)?,
                })
            })
            .collect()
    }

    async fn set_bridge_author(
        &self,
        message: MessageId,
        author: &sr_chat::rich_repo::BridgeAuthor,
    ) -> StoreResult<()> {
        let links = serde_json::to_value(&author.links).unwrap_or_else(|_| serde_json::json!([]));
        sqlx::query(
            "INSERT INTO message_bridge_authors \
               (message_id, platform, author_name, author_avatar, discord_user_id, links) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (message_id) DO UPDATE SET \
               platform = EXCLUDED.platform, author_name = EXCLUDED.author_name, \
               author_avatar = EXCLUDED.author_avatar, \
               discord_user_id = EXCLUDED.discord_user_id, links = EXCLUDED.links",
        )
        .bind(i(message.get()))
        .bind(&author.platform)
        .bind(&author.author_name)
        .bind(&author.author_avatar)
        .bind(&author.discord_user_id)
        .bind(links)
        .execute(&self.pool)
        .await
        .map_err(sdb)?;
        Ok(())
    }

    async fn bridge_authors(
        &self,
        messages: &[MessageId],
    ) -> StoreResult<HashMap<MessageId, sr_chat::rich_repo::BridgeAuthor>> {
        if messages.is_empty() {
            return Ok(HashMap::new());
        }
        let ids: Vec<i64> = messages.iter().map(|m| i(m.get())).collect();
        let rows = sqlx::query(
            "SELECT message_id, platform, author_name, author_avatar, discord_user_id, links \
             FROM message_bridge_authors WHERE message_id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .map_err(sdb)?;
        let mut out = HashMap::new();
        for r in rows {
            let links: serde_json::Value = r.try_get("links").map_err(sdb)?;
            out.insert(
                MessageId::new(u(r.try_get("message_id").map_err(sdb)?)),
                sr_chat::rich_repo::BridgeAuthor {
                    platform: r.try_get("platform").map_err(sdb)?,
                    author_name: r.try_get("author_name").map_err(sdb)?,
                    author_avatar: r.try_get("author_avatar").map_err(sdb)?,
                    discord_user_id: r.try_get("discord_user_id").map_err(sdb)?,
                    links: serde_json::from_value(links).unwrap_or_default(),
                },
            );
        }
        Ok(out)
    }

    async fn set_webhook_author(
        &self,
        message: MessageId,
        author: &sr_chat::rich_repo::WebhookAuthor,
    ) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO message_webhook_authors (message_id, webhook_id, username, avatar_url) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (message_id) DO UPDATE SET \
               webhook_id = EXCLUDED.webhook_id, username = EXCLUDED.username, \
               avatar_url = EXCLUDED.avatar_url",
        )
        .bind(i(message.get()))
        .bind(i(author.webhook_id))
        .bind(&author.username)
        .bind(&author.avatar_url)
        .execute(&self.pool)
        .await
        .map_err(sdb)?;
        Ok(())
    }

    async fn webhook_authors(
        &self,
        messages: &[MessageId],
    ) -> StoreResult<HashMap<MessageId, sr_chat::rich_repo::WebhookAuthor>> {
        if messages.is_empty() {
            return Ok(HashMap::new());
        }
        let ids: Vec<i64> = messages.iter().map(|m| i(m.get())).collect();
        let rows = sqlx::query(
            "SELECT message_id, webhook_id, username, avatar_url \
             FROM message_webhook_authors WHERE message_id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .map_err(sdb)?;
        let mut out = HashMap::new();
        for r in rows {
            out.insert(
                MessageId::new(u(r.try_get("message_id").map_err(sdb)?)),
                sr_chat::rich_repo::WebhookAuthor {
                    webhook_id: u(r.try_get("webhook_id").map_err(sdb)?),
                    username: r.try_get("username").map_err(sdb)?,
                    avatar_url: r.try_get("avatar_url").map_err(sdb)?,
                },
            );
        }
        Ok(out)
    }

    async fn app_emoji_owners(&self, emoji: &[u64]) -> StoreResult<HashMap<u64, i64>> {
        if emoji.is_empty() {
            return Ok(HashMap::new());
        }
        let ids: Vec<i64> = emoji.iter().map(|e| i(*e)).collect();
        let rows: Vec<(i64, i64)> =
            sqlx::query_as("SELECT id, application_id FROM application_emojis WHERE id = ANY($1)")
                .bind(ids)
                .fetch_all(&self.pool)
                .await
                .map_err(sdb)?;
        Ok(rows.into_iter().map(|(e, a)| (u(e), a)).collect())
    }

    async fn guild_emojis(
        &self,
        emoji: &[u64],
    ) -> StoreResult<HashMap<u64, sr_chat::rich_repo::GuildEmojiRef>> {
        if emoji.is_empty() {
            return Ok(HashMap::new());
        }
        let ids: Vec<i64> = emoji.iter().map(|e| i(*e)).collect();
        // One round trip: the emoji and the roles it is restricted to.
        let rows: Vec<(i64, i64, bool, Vec<i64>)> = sqlx::query_as(
            "SELECT e.id, e.guild_id, e.available, \n                    COALESCE(array_agg(r.role_id) FILTER (WHERE r.role_id IS NOT NULL), '{}') \n             FROM emojis e LEFT JOIN emoji_roles r ON r.emoji_id = e.id \n             WHERE e.id = ANY($1) GROUP BY e.id",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .map_err(sdb)?;
        Ok(rows
            .into_iter()
            .map(|(id, guild, available, roles)| {
                (
                    u(id),
                    sr_chat::rich_repo::GuildEmojiRef {
                        id: u(id),
                        guild_id: GuildId::new(u(guild)),
                        available,
                        role_ids: roles
                            .into_iter()
                            .map(|r| sr_core::RoleId::new(u(r)))
                            .collect(),
                    },
                )
            })
            .collect())
    }

    async fn bot_applications(&self, users: &[UserId]) -> StoreResult<HashMap<UserId, i64>> {
        if users.is_empty() {
            return Ok(HashMap::new());
        }
        let ids: Vec<i64> = users.iter().map(|x| i(x.get())).collect();
        let rows: Vec<(i64, i64)> =
            sqlx::query_as("SELECT bot_user_id, id FROM applications WHERE bot_user_id = ANY($1)")
                .bind(ids)
                .fetch_all(&self.pool)
                .await
                .map_err(sdb)?;
        Ok(rows
            .into_iter()
            .map(|(b, a)| (UserId::new(u(b)), a))
            .collect())
    }

    async fn set_embeds(&self, message: MessageId, embeds: &[Embed]) -> StoreResult<()> {
        let json = serde_json::to_value(embeds).map_err(|e| StoreError(e.to_string()))?;
        sqlx::query("UPDATE messages SET embeds = $2 WHERE id = $1")
            .bind(i(message.get()))
            .bind(json)
            .execute(&self.pool)
            .await
            .map_err(sdb)?;
        Ok(())
    }

    async fn poll_tallies(
        &self,
        messages: &[MessageId],
        viewer: Option<UserId>,
    ) -> StoreResult<HashMap<MessageId, PollTally>> {
        if messages.is_empty() {
            return Ok(HashMap::new());
        }
        let ids: Vec<i64> = messages.iter().map(|m| i(m.get())).collect();
        let rows = sqlx::query(
            "SELECT message_id, answer_id, count(*) AS n, \
                    coalesce(bool_or(user_id = $2), false) AS mine \
             FROM poll_votes WHERE message_id = ANY($1) GROUP BY message_id, answer_id",
        )
        .bind(ids)
        .bind(viewer.map_or(0_i64, |v| i(v.get())))
        .fetch_all(&self.pool)
        .await
        .map_err(sdb)?;
        let mut out: HashMap<MessageId, PollTally> = HashMap::new();
        for r in rows {
            let m = MessageId::new(u(r.try_get("message_id").map_err(sdb)?));
            let answer = u32::try_from(r.try_get::<i32, _>("answer_id").map_err(sdb)?).unwrap_or(0);
            let n = u32::try_from(r.try_get::<i64, _>("n").map_err(sdb)?).unwrap_or(u32::MAX);
            let t = out.entry(m).or_default();
            t.counts.insert(answer, n);
            if r.try_get::<bool, _>("mine").map_err(sdb)? {
                t.mine.insert(answer);
            }
        }
        // Votes removed after the poll closed stay in the result without a voter (D-332).
        let anon = sqlx::query(
            "SELECT id, poll->'anonymous_counts' AS anon FROM messages 
             WHERE id = ANY($1) AND poll ? 'anonymous_counts'",
        )
        .bind(messages.iter().map(|m| i(m.get())).collect::<Vec<_>>())
        .fetch_all(&self.pool)
        .await
        .map_err(sdb)?;
        for r in anon {
            let m = MessageId::new(u(r.try_get("id").map_err(sdb)?));
            let v: Option<serde_json::Value> = r.try_get("anon").map_err(sdb)?;
            if let Some(obj) = v.as_ref().and_then(serde_json::Value::as_object) {
                for (k, n) in obj {
                    if let (Ok(a), Some(n)) = (k.parse::<u32>(), n.as_u64()) {
                        let n = u32::try_from(n).unwrap_or(u32::MAX);
                        *out.entry(m).or_default().counts.entry(a).or_default() += n;
                    }
                }
            }
        }
        Ok(out)
    }

    async fn cast_vote(
        &self,
        message: MessageId,
        answer: u32,
        user: UserId,
        now_ms: i64,
    ) -> StoreResult<VoteResult> {
        let mut tx = self.pool.begin().await.map_err(sdb)?;
        let Some(poll) = lock_poll(&mut tx, message).await? else {
            return Ok(VoteResult::NoPoll);
        };
        if !poll_rules::is_open(&poll, now_ms) {
            return Ok(VoteResult::Closed);
        }
        if !poll_rules::has_answer(&poll, answer) {
            return Ok(VoteResult::UnknownAnswer);
        }
        let answer_i = i32::try_from(answer).unwrap_or(i32::MAX);
        let inserted = sqlx::query(
            "INSERT INTO poll_votes (message_id, answer_id, user_id) VALUES ($1, $2, $3) \
             ON CONFLICT DO NOTHING",
        )
        .bind(i(message.get()))
        .bind(answer_i)
        .bind(i(user.get()))
        .execute(&mut *tx)
        .await
        .map_err(sdb)?
        .rows_affected();
        if inserted == 0 {
            return Ok(VoteResult::AlreadyVoted);
        }
        let mut replaced = Vec::new();
        if !poll.allow_multiselect {
            let dropped: Vec<i32> = sqlx::query_scalar(
                "DELETE FROM poll_votes WHERE message_id = $1 AND user_id = $2 AND answer_id <> $3 \
                 RETURNING answer_id",
            )
            .bind(i(message.get()))
            .bind(i(user.get()))
            .bind(answer_i)
            .fetch_all(&mut *tx)
            .await
            .map_err(sdb)?;
            replaced = dropped
                .into_iter()
                .filter_map(|a| u32::try_from(a).ok())
                .collect();
            replaced.sort_unstable();
        }
        tx.commit().await.map_err(sdb)?;
        Ok(VoteResult::Added { replaced })
    }

    async fn retract_vote(
        &self,
        message: MessageId,
        answer: u32,
        user: UserId,
        now_ms: i64,
    ) -> StoreResult<RetractResult> {
        let mut tx = self.pool.begin().await.map_err(sdb)?;
        let Some(poll) = lock_poll(&mut tx, message).await? else {
            return Ok(RetractResult::NoPoll);
        };
        if !poll_rules::is_open(&poll, now_ms) {
            return Ok(RetractResult::Closed);
        }
        let removed = sqlx::query(
            "DELETE FROM poll_votes WHERE message_id = $1 AND answer_id = $2 AND user_id = $3",
        )
        .bind(i(message.get()))
        .bind(i32::try_from(answer).unwrap_or(i32::MAX))
        .bind(i(user.get()))
        .execute(&mut *tx)
        .await
        .map_err(sdb)?
        .rows_affected();
        tx.commit().await.map_err(sdb)?;
        Ok(if removed > 0 {
            RetractResult::Removed
        } else {
            RetractResult::NotVoted
        })
    }

    async fn poll_voters(
        &self,
        message: MessageId,
        answer: u32,
        after: Option<UserId>,
        limit: usize,
    ) -> StoreResult<Vec<UserId>> {
        let rows: Vec<i64> = sqlx::query_scalar(
            "SELECT user_id FROM poll_votes \
             WHERE message_id = $1 AND answer_id = $2 AND user_id > $3 \
             ORDER BY user_id LIMIT $4",
        )
        .bind(i(message.get()))
        .bind(i32::try_from(answer).unwrap_or(i32::MAX))
        .bind(after.map_or(0_i64, |a| i(a.get())))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(sdb)?;
        Ok(rows.into_iter().map(|v| UserId::new(u(v))).collect())
    }

    async fn expire_poll_now(&self, message: MessageId, now_ms: i64) -> StoreResult<bool> {
        let r = sqlx::query(
            "UPDATE messages \
             SET poll = jsonb_set(poll, '{expiry_ms}', to_jsonb($2::bigint)), \
                 poll_expires_at = to_timestamp($3::double precision) \
             WHERE id = $1 AND poll IS NOT NULL \
               AND NOT coalesce((poll->>'finalized')::boolean, false) \
               AND coalesce((poll->>'expiry_ms')::bigint, 9223372036854775807) > $2",
        )
        .bind(i(message.get()))
        .bind(now_ms)
        .bind(seconds(now_ms))
        .execute(&self.pool)
        .await
        .map_err(sdb)?;
        Ok(r.rows_affected() > 0)
    }

    async fn finalize_poll(&self, message: MessageId, now_ms: i64) -> StoreResult<bool> {
        let r = sqlx::query(
            "UPDATE messages \
             SET poll = jsonb_set(poll, '{finalized}', 'true'::jsonb), poll_expires_at = NULL \
             WHERE id = $1 AND poll IS NOT NULL \
               AND NOT coalesce((poll->>'finalized')::boolean, false) \
               AND (poll->>'expiry_ms')::bigint <= $2",
        )
        .bind(i(message.get()))
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(sdb)?;
        Ok(r.rows_affected() > 0)
    }

    async fn due_polls(
        &self,
        now_ms: i64,
        limit: usize,
    ) -> StoreResult<Vec<(ChannelId, MessageId)>> {
        let rows = sqlx::query(
            "SELECT channel_id, id FROM messages \
             WHERE poll_expires_at IS NOT NULL AND poll_expires_at <= to_timestamp($1::double precision) \
             ORDER BY poll_expires_at LIMIT $2",
        )
        .bind(seconds(now_ms))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(sdb)?;
        rows.iter()
            .map(|r| {
                Ok((
                    ChannelId::new(u(r.try_get("channel_id").map_err(sdb)?)),
                    MessageId::new(u(r.try_get("id").map_err(sdb)?)),
                ))
            })
            .collect()
    }

    async fn stickers(&self, ids: &[u64]) -> StoreResult<Vec<StickerRecord>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<i64> = ids.iter().map(|v| i(*v)).collect();
        let rows = sqlx::query(
            "SELECT id, guild_id, name, format_type, available FROM stickers WHERE id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .map_err(sdb)?;
        rows.iter()
            .map(|r| {
                Ok(StickerRecord {
                    id: u(r.try_get("id").map_err(sdb)?),
                    guild_id: r
                        .try_get::<Option<i64>, _>("guild_id")
                        .map_err(sdb)?
                        .map(|g| GuildId::new(u(g))),
                    name: r.try_get("name").map_err(sdb)?,
                    format_type: r.try_get("format_type").map_err(sdb)?,
                    available: r.try_get("available").map_err(sdb)?,
                })
            })
            .collect()
    }

    async fn sticker_packs(&self) -> StoreResult<Vec<StickerPack>> {
        let packs = sqlx::query(
            "SELECT id, name, description, cover_sticker_id FROM sticker_packs WHERE published ORDER BY sort_order, id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sdb)?;
        let rows = sqlx::query(
            "SELECT s.id, s.pack_id, s.name, s.description, s.tags, s.format_type, s.available, s.media_key FROM stickers s JOIN sticker_packs p ON p.id = s.pack_id WHERE p.published ORDER BY s.created_at, s.id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sdb)?;
        let mut out = Vec::with_capacity(packs.len());
        for p in &packs {
            let id: i64 = p.try_get("id").map_err(sdb)?;
            let mut stickers = Vec::new();
            for r in &rows {
                if r.try_get::<i64, _>("pack_id").map_err(sdb)? != id {
                    continue;
                }
                stickers.push(PackSticker {
                    id: u(r.try_get("id").map_err(sdb)?),
                    name: r.try_get("name").map_err(sdb)?,
                    description: r.try_get("description").map_err(sdb)?,
                    tags: r.try_get("tags").map_err(sdb)?,
                    format_type: r.try_get("format_type").map_err(sdb)?,
                    available: r.try_get("available").map_err(sdb)?,
                    media_key: r.try_get("media_key").map_err(sdb)?,
                });
            }
            let cover = p
                .try_get::<Option<i64>, _>("cover_sticker_id")
                .map_err(sdb)?
                .map(u)
                .or_else(|| stickers.first().map(|s| s.id));
            out.push(StickerPack {
                id: u(id),
                name: p.try_get("name").map_err(sdb)?,
                description: p.try_get("description").map_err(sdb)?,
                cover_sticker_id: cover,
                stickers,
            });
        }
        Ok(out)
    }

    async fn pack_sticker_media(&self, id: u64) -> StoreResult<Option<String>> {
        sqlx::query_scalar(
            "SELECT s.media_key FROM stickers s JOIN sticker_packs p ON p.id = s.pack_id WHERE s.id = $1 AND p.published",
        )
        .bind(i(id))
        .fetch_optional(&self.pool)
        .await
        .map_err(sdb)
    }
}
