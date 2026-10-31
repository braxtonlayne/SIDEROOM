//! SIDE ROOM to Discord: turning messages into queue rows and draining the queue (BRG-COR-001).
//!
//! Every message event of a bridged channel becomes a row of `bridge_outbox` (create, edit, delete); a worker
//! drains the table one row per map at a time, in order, through the map's webhook:
//!
//! * the post carries `username` and `avatar_url` of the author (the avatar URL is a signed `/cdn` URL with a
//!   24 hour life, the one place a signed CDN URL leaves the instance), the content rendered to Discord markdown
//!   ([`crate::render`]), attachments as links to the signed `/cdn` URL (never uploaded), no embeds and
//!   `allowed_mentions: {parse: []}`;
//! * an edit is a webhook message edit, a delete a webhook message delete, found through `bridge_message_map`;
//! * the queue honours Discord's `X-RateLimit-*` headers (the webhook is *blocked* for the time Discord named,
//!   see [`crate::discord`]); a 429 reschedules the row without counting an attempt; other failures retry with
//!   exponential backoff and after `max_attempts` the row is a dead letter, counted on Admin > System;
//! * loops are prevented here by three rules: a message carrying `sr_bridge` (it came from Discord) is never
//!   mirrored back, a message already in the message map is never posted twice, and the unique index on creates
//!   makes a repeated event harmless.
//!

use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use sr_events::{DomainEvent, Topic};
use sr_features::modules::bridge::{BRIDGE_MODULE_ID, PLATFORM_BRIDGE_ID};

use crate::discord::{CallError, WebhookPost};
use crate::error::{BridgeError, BridgeResult};
use crate::model::{Direction, MessageMapDirection, get_bridge, map_for_channel, message_map_for};
use crate::render::{
    AttachmentLink, Names, SIGNED_LINK_TTL_SECS, attachment_links, channel_mention_ids, compose,
    content_for_discord, username_for_discord,
};
use crate::service::{BridgeService, SYSTEM_USER_ID};
use crate::teardown::TeardownReason;

/// What a `create` row carries until it is sent.
#[derive(Debug, Deserialize)]
struct CreatePayload {
    #[serde(default)]
    author_id: i64,
    #[serde(default)]
    username: String,
    #[serde(default)]
    avatar_hash: Option<String>,
    #[serde(default)]
    content: String,
    #[serde(default)]
    attachments: Vec<AttachmentLink>,
}

/// A row picked from the queue.
struct QueueRow {
    id: i64,
    map_id: i64,
    guild_id: i64,
    kind: String,
    sr_message_id: Option<i64>,
    payload: Value,
    attempts: i32,
}

fn parse_id(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::String(s) => s.parse().ok(),
        Value::Number(n) => n.as_i64(),
        _ => None,
    }
}

impl BridgeService {
    /// Feed one platform event to the bridge. Failures are logged, never raised: the bridge must not break the
    /// chat path it listens to.
    pub async fn handle_event(&self, ev: &DomainEvent) {
        let result = match ev {
            DomainEvent::MessageSent {
                guild: Some(g),
                channel,
                message_id,
                author,
                data,
            } => {
                self.mirror_new(
                    to_i64(g.get()),
                    to_i64(channel.get()),
                    to_i64(*message_id),
                    to_i64(author.get()),
                    data,
                )
                .await
            }
            DomainEvent::MessageEdited {
                guild: Some(g),
                channel,
                message_id,
                data,
            } => {
                self.mirror_edit(
                    to_i64(g.get()),
                    to_i64(channel.get()),
                    to_i64(*message_id),
                    data,
                )
                .await
            }
            DomainEvent::MessageDeleted {
                guild: Some(_),
                channel,
                message_id,
            } => {
                self.mirror_delete(to_i64(channel.get()), to_i64(*message_id))
                    .await
            }
            DomainEvent::MessagesBulkDeleted {
                guild: Some(_),
                channel,
                message_ids,
            } => {
                let mut r = Ok(());
                for m in message_ids {
                    if let Err(e) = self.mirror_delete(to_i64(channel.get()), to_i64(*m)).await {
                        r = Err(e);
                    }
                }
                r
            }
            DomainEvent::ChannelDeleted { channel, .. } => {
                self.channel_deleted(to_i64(channel.get())).await
            }
            DomainEvent::FeatureSettingsChanged { feature_id, scope } => {
                self.feature_changed(feature_id, scope).await
            }
            _ => Ok(()),
        };
        if let Err(e) = result {
            tracing::warn!(error = %e, "discord bridge could not handle an event");
        }
    }

    /// Backstop for every way a switch can change (Admin > Features, the modules API, a test): the module off in a
    /// server tears its bridge down, and the platform switch off tears down every server's.
    async fn feature_changed(&self, feature: &str, scope: &Topic) -> BridgeResult<()> {
        match (feature, scope) {
            (PLATFORM_BRIDGE_ID, Topic::Platform) => {
                if !self.platform_on().await? {
                    self.teardown_all().await?;
                }
            }
            (BRIDGE_MODULE_ID, Topic::Guild(g)) => {
                let g = to_i64(g.get());
                if !self.module_on(g).await? {
                    self.teardown(g, SYSTEM_USER_ID, TeardownReason::Owner)
                        .await?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn channel_deleted(&self, channel: i64) -> BridgeResult<()> {
        if let Some(map) = map_for_channel(&self.pool, channel).await? {
            self.remove_map(&map, false).await?;
        }
        Ok(())
    }

    /// Mirror only while the platform switch and the server's module are on.
    async fn live(&self, guild: i64) -> BridgeResult<bool> {
        Ok(self.platform_on().await? && self.module_on(guild).await?)
    }

    async fn mirror_new(
        &self,
        guild: i64,
        channel: i64,
        message: i64,
        author: i64,
        data: &Value,
    ) -> BridgeResult<()> {
        let Some(map) = map_for_channel(&self.pool, channel).await? else {
            return Ok(());
        };
        if !map.direction.sends() || map.discord_webhook_id.is_none() {
            return Ok(());
        }
        // Loop guard: a message that came from Discord is never sent back.
        if data.get("sr_bridge").is_some_and(|v| !v.is_null()) {
            return Ok(());
        }
        // Plain messages and replies only; joins, pins and the like are not conversation.
        if !matches!(
            data.get("type").and_then(Value::as_i64).unwrap_or(0),
            0 | 19
        ) {
            return Ok(());
        }
        if author == SYSTEM_USER_ID || !self.live(guild).await? {
            return Ok(());
        }
        if message_map_for(&self.pool, message).await?.is_some() {
            return Ok(());
        }
        let payload = self.build_payload(guild, author, data).await?;
        sqlx::query(
            "INSERT INTO bridge_outbox (id, guild_id, map_id, kind, sr_message_id, payload) \
             VALUES ($1, $2, $3, 'create', $4, $5) ON CONFLICT DO NOTHING",
        )
        .bind(self.next_id())
        .bind(guild)
        .bind(map.id)
        .bind(message)
        .bind(payload)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn mirror_edit(
        &self,
        guild: i64,
        channel: i64,
        message: i64,
        data: &Value,
    ) -> BridgeResult<()> {
        let Some(map) = map_for_channel(&self.pool, channel).await? else {
            return Ok(());
        };
        if !map.direction.sends()
            || data.get("sr_bridge").is_some_and(|v| !v.is_null())
            || !self.live(guild).await?
        {
            return Ok(());
        }
        let author = parse_id(data.get("author").and_then(|a| a.get("id"))).unwrap_or(0);
        let payload = self.build_payload(guild, author, data).await?;
        // Still waiting to be posted: the edit simply becomes the content of the post.
        let pending = sqlx::query(
            "UPDATE bridge_outbox SET payload = $2 WHERE kind = 'create' AND sr_message_id = $1",
        )
        .bind(message)
        .bind(&payload)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if pending > 0 {
            return Ok(());
        }
        let Some(mm) = message_map_for(&self.pool, message).await? else {
            return Ok(());
        };
        if mm.direction != MessageMapDirection::Out || mm.map_id != map.id {
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO bridge_outbox (id, guild_id, map_id, kind, sr_message_id, payload) \
             VALUES ($1, $2, $3, 'edit', $4, $5)",
        )
        .bind(self.next_id())
        .bind(guild)
        .bind(map.id)
        .bind(message)
        .bind(payload)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn mirror_delete(&self, channel: i64, message: i64) -> BridgeResult<()> {
        let Some(map) = map_for_channel(&self.pool, channel).await? else {
            return Ok(());
        };
        // Never posted yet: it simply never will be.
        let dropped =
            sqlx::query("DELETE FROM bridge_outbox WHERE kind = 'create' AND sr_message_id = $1")
                .bind(message)
                .execute(&self.pool)
                .await?
                .rows_affected();
        if dropped > 0 {
            return Ok(());
        }
        let Some(mm) = message_map_for(&self.pool, message).await? else {
            return Ok(());
        };
        if mm.direction != MessageMapDirection::Out || mm.map_id != map.id {
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO bridge_outbox (id, guild_id, map_id, kind, sr_message_id) VALUES ($1, $2, $3, 'delete', $4)",
        )
        .bind(self.next_id())
        .bind(map.guild_id)
        .bind(map.id)
        .bind(message)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The queue payload of a message: who wrote it, and what to say, rendered for Discord. Names are resolved
    /// now; links are signed at send time, so a retried post carries a fresh signature.
    async fn build_payload(&self, guild: i64, author: i64, data: &Value) -> BridgeResult<Value> {
        let content = data.get("content").and_then(Value::as_str).unwrap_or("");
        let row = sqlx::query(
            "SELECT COALESCE(gm.nick, a.display_name, a.username::text) AS name, a.avatar_key AS avatar \
             FROM accounts a LEFT JOIN guild_members gm ON gm.user_id = a.id AND gm.guild_id = $2 \
             WHERE a.id = $1",
        )
        .bind(author)
        .bind(guild)
        .fetch_optional(&self.pool)
        .await?;
        let (name, avatar): (String, Option<String>) = match row {
            Some(r) => (r.try_get("name")?, r.try_get("avatar")?),
            None => (
                data.get("author")
                    .and_then(|a| a.get("username"))
                    .and_then(Value::as_str)
                    .unwrap_or("member")
                    .to_owned(),
                None,
            ),
        };
        let mut names = Names::default();
        if let Some(list) = data.get("mentions").and_then(Value::as_array) {
            for u in list {
                if let Some(id) = u.get("id").and_then(Value::as_str) {
                    let n = u
                        .get("global_name")
                        .and_then(Value::as_str)
                        .or_else(|| u.get("username").and_then(Value::as_str));
                    if let Some(n) = n {
                        names.users.insert(id.to_owned(), n.to_owned());
                    }
                }
            }
        }
        let role_ids: Vec<i64> = data
            .get("mention_roles")
            .and_then(Value::as_array)
            .map(|l| l.iter().filter_map(|v| parse_id(Some(v))).collect())
            .unwrap_or_default();
        if !role_ids.is_empty() {
            let rows = sqlx::query(
                "SELECT id::text AS id, name FROM roles WHERE guild_id = $1 AND id = ANY($2)",
            )
            .bind(guild)
            .bind(&role_ids)
            .fetch_all(&self.pool)
            .await?;
            for r in rows {
                names.roles.insert(r.try_get("id")?, r.try_get("name")?);
            }
        }
        let channel_ids: Vec<i64> = channel_mention_ids(content)
            .iter()
            .filter_map(|s| s.parse().ok())
            .collect();
        if !channel_ids.is_empty() {
            let rows = sqlx::query(
                "SELECT id::text AS id, name FROM channels WHERE guild_id = $1 AND id = ANY($2) AND name IS NOT NULL",
            )
            .bind(guild)
            .bind(&channel_ids)
            .fetch_all(&self.pool)
            .await?;
            for r in rows {
                names.channels.insert(r.try_get("id")?, r.try_get("name")?);
            }
        }
        Ok(json!({
            "author_id": author,
            "username": name,
            "avatar_hash": avatar,
            "content": content_for_discord(content, &names),
            "attachments": attachment_links(data),
        }))
    }

    // ---- the worker ----

    fn post_from(&self, p: &CreatePayload) -> WebhookPost {
        let links: Vec<(String, String)> = p
            .attachments
            .iter()
            .map(|a| {
                (
                    a.filename.clone(),
                    self.media.signed_url(&a.path, SIGNED_LINK_TTL_SECS),
                )
            })
            .collect();
        WebhookPost {
            content: compose(&p.content, &links),
            username: username_for_discord(&p.username),
            avatar_url: p.avatar_hash.as_deref().filter(|h| !h.is_empty()).map(|h| {
                self.media.signed_url(
                    &format!("/cdn/avatars/{}/{h}.png", p.author_id),
                    SIGNED_LINK_TTL_SECS,
                )
            }),
        }
    }

    /// One pass over the queue: the oldest due row of each map is sent. Returns how many rows left the queue
    /// (sent, dropped or dead).
    pub async fn run_outbox_once(&self) -> BridgeResult<usize> {
        let rows = sqlx::query(
            "SELECT DISTINCT ON (map_id) id, map_id, guild_id, kind, sr_message_id, payload, attempts, \
                    (due_at <= now()) AS due \
             FROM bridge_outbox WHERE state = 'pending' ORDER BY map_id, id LIMIT $1",
        )
        .bind(i64::try_from(self.tuning.batch).unwrap_or(50))
        .fetch_all(&self.pool)
        .await?;
        let mut left = 0;
        for r in rows {
            if !r.try_get::<bool, _>("due")? {
                continue;
            }
            let row = QueueRow {
                id: r.try_get("id")?,
                map_id: r.try_get("map_id")?,
                guild_id: r.try_get("guild_id")?,
                kind: r.try_get("kind")?,
                sr_message_id: r.try_get("sr_message_id")?,
                payload: r.try_get("payload")?,
                attempts: r.try_get("attempts")?,
            };
            if self.send_row(&row).await? {
                left += 1;
            }
        }
        Ok(left)
    }

    /// Drain the queue until nothing is due (bounded), for the worker and for tests.
    pub async fn drain_outbox(&self) -> BridgeResult<usize> {
        let mut total = 0;
        for _ in 0..200 {
            let n = self.run_outbox_once().await?;
            if n == 0 {
                break;
            }
            total += n;
        }
        Ok(total)
    }

    /// Send one row. `Ok(true)` when it left the queue.
    async fn send_row(&self, row: &QueueRow) -> BridgeResult<bool> {
        let Some((webhook, token)) = self.webhook_of(row.map_id).await.unwrap_or(None) else {
            // The map lost its webhook (direction changed, map removed): nothing can send this.
            return self.drop_row(row.id).await.map(|()| true);
        };
        if self
            .discord
            .blocked_for(&crate::discord::DiscordApi::webhook_key(webhook))
            .is_some()
        {
            return Ok(false);
        }
        let outcome = self.attempt(row, webhook, &token).await;
        match outcome {
            Ok(()) => {
                self.drop_row(row.id).await?;
                Ok(true)
            }
            Err(CallError::RateLimited(wait)) => {
                self.reschedule(row.id, wait.as_secs_f64(), None, false)
                    .await?;
                Ok(false)
            }
            Err(CallError::Gone(_)) if matches!(row.kind.as_str(), "edit" | "delete") => {
                // The Discord copy is already gone: nothing to edit or delete.
                self.forget_mapping(row).await?;
                self.drop_row(row.id).await?;
                Ok(true)
            }
            Err(CallError::Gone(_)) => {
                self.kill_row(row.id, "the webhook or channel was deleted on Discord")
                    .await?;
                Ok(true)
            }
            Err(e) => {
                let attempts = row.attempts + 1;
                if attempts >= self.tuning.max_attempts {
                    tracing::warn!(
                        guild = row.guild_id,
                        map = row.map_id,
                        "bridge post is a dead letter"
                    );
                    self.kill_row(row.id, &e.to_string()).await?;
                    Ok(true)
                } else {
                    let exp = u32::try_from(row.attempts).unwrap_or(0).min(16);
                    let wait = self
                        .tuning
                        .retry_base
                        .saturating_mul(2u32.saturating_pow(exp))
                        .min(self.tuning.retry_cap);
                    self.reschedule(row.id, wait.as_secs_f64(), Some(&e.to_string()), true)
                        .await?;
                    Ok(false)
                }
            }
        }
    }

    async fn attempt(&self, row: &QueueRow, webhook: i64, token: &str) -> Result<(), CallError> {
        let db = |e: sqlx::Error| CallError::Transport(format!("database: {e}"));
        match row.kind.as_str() {
            "create" | "hello" => {
                let p: CreatePayload = serde_json::from_value(row.payload.clone())
                    .map_err(|_| CallError::Transport("unreadable queue row".into()))?;
                let post = self.post_from(&p);
                let discord_message = self.discord.execute_webhook(webhook, token, &post).await?;
                if let Some(sr) = row.sr_message_id {
                    sqlx::query(
                        "INSERT INTO bridge_message_map (sr_message_id, discord_message_id, map_id, webhook_id, author_id, direction) \
                         VALUES ($1, $2, $3, $4, $5, 'out') ON CONFLICT DO NOTHING",
                    )
                    .bind(sr)
                    .bind(discord_message)
                    .bind(row.map_id)
                    .bind(webhook)
                    .bind(p.author_id)
                    .execute(&self.pool)
                    .await
                    .map_err(db)?;
                }
                Ok(())
            }
            "edit" => {
                let Some(sr) = row.sr_message_id else {
                    return Ok(());
                };
                let Some(mm) = message_map_for(&self.pool, sr)
                    .await
                    .map_err(|e| CallError::Transport(e.to_string()))?
                else {
                    return Ok(());
                };
                let p: CreatePayload = serde_json::from_value(row.payload.clone())
                    .map_err(|_| CallError::Transport("unreadable queue row".into()))?;
                let post = self.post_from(&p);
                self.discord
                    .edit_webhook_message(webhook, token, mm.discord_message_id, &post.content)
                    .await
            }
            "delete" => {
                // An erasure row carries the Discord message id itself (its map row is already gone).
                let target = match parse_id(row.payload.get("discord_message_id")) {
                    Some(id) => id,
                    None => {
                        let Some(sr) = row.sr_message_id else {
                            return Ok(());
                        };
                        let Some(mm) = message_map_for(&self.pool, sr)
                            .await
                            .map_err(|e| CallError::Transport(e.to_string()))?
                        else {
                            return Ok(());
                        };
                        mm.discord_message_id
                    }
                };
                self.discord
                    .delete_webhook_message(webhook, token, target)
                    .await?;
                self.forget_mapping(row)
                    .await
                    .map_err(|e| CallError::Transport(e.to_string()))?;
                Ok(())
            }
            other => Err(CallError::Transport(format!(
                "unknown queue row kind {other}"
            ))),
        }
    }

    async fn forget_mapping(&self, row: &QueueRow) -> BridgeResult<()> {
        if let (Some(sr), "delete") = (row.sr_message_id, row.kind.as_str()) {
            sqlx::query("DELETE FROM bridge_message_map WHERE sr_message_id = $1")
                .bind(sr)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    async fn drop_row(&self, id: i64) -> BridgeResult<()> {
        sqlx::query("DELETE FROM bridge_outbox WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn kill_row(&self, id: i64, why: &str) -> BridgeResult<()> {
        let why: String = why.chars().take(500).collect();
        sqlx::query("UPDATE bridge_outbox SET state = 'dead', last_error = $2, attempts = attempts + 1 WHERE id = $1")
            .bind(id)
            .bind(why)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn reschedule(
        &self,
        id: i64,
        secs: f64,
        error: Option<&str>,
        count: bool,
    ) -> BridgeResult<()> {
        let error: Option<String> = error.map(|e| e.chars().take(500).collect());
        sqlx::query(
            "UPDATE bridge_outbox SET due_at = now() + make_interval(secs => $2), \
             attempts = attempts + $3, last_error = COALESCE($4, last_error) WHERE id = $1",
        )
        .bind(id)
        .bind(secs.max(0.0))
        .bind(i32::from(count))
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// "Post a hello" (the owner's test): sent at once, so the owner sees Discord's answer, not a queued promise.
    pub async fn test_post(&self, guild: i64, map_id: i64) -> BridgeResult<()> {
        let map = self.get_map(guild, map_id).await?;
        if !map.direction.sends() {
            return Err(BridgeError::Conflict(
                "This channel only receives from Discord, so there is nothing to post to.".into(),
            ));
        }
        let (webhook, token) = self
            .webhook_of(map.id)
            .await?
            .ok_or_else(|| BridgeError::Conflict("This channel has no webhook.".into()))?;
        let bridge = get_bridge(&self.pool, guild)
            .await?
            .ok_or(BridgeError::NotFound("bridge"))?;
        let post = WebhookPost {
            content: format!(
                "Hello from SIDE ROOM! The bridge to {} is working.",
                bridge.discord_guild_name
            ),
            username: "SIDE ROOM bridge".into(),
            avatar_url: None,
        };
        self.discord.execute_webhook(webhook, &token, &post).await?;
        Ok(())
    }

    /// Whether `direction` of a map sends (for callers that only hold the enum).
    pub fn sends(direction: Direction) -> bool {
        direction.sends()
    }
}

fn to_i64(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}
