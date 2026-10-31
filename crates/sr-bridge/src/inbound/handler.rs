//! One dispatch from a bridge's shard to its effect here. Everything that decides is in this file: which channel,
//! which messages the bridge itself wrote (the loop guard), the abuse limit, replies, edits, deletes, reactions.

use std::sync::Arc;

use serde_json::Value;
use sqlx::PgPool;
use sr_sdk::gateway::Dispatch;

use super::convert::{self, InMessage};
use super::limit::BridgeLimiter;
use super::sink::{ChatSink, InboundAuthor, InboundPost};
use super::store;
use crate::error::BridgeResult;
use crate::model::{Bridge, MessageMapDirection};

/// Live figures the host supplies (the platform settings, read on every use so a change applies at once).
#[derive(Clone)]
pub struct InboundConfig {
    /// `abuse.bridge_messages_per_minute`.
    pub messages_per_minute: Arc<dyn Fn() -> u32 + Send + Sync>,
    /// `bridge.max_bridges`.
    pub max_bridges: Arc<dyn Fn() -> usize + Send + Sync>,
    /// The gateway to connect to; `None` is Discord's own. A fake Discord in tests and end-to-end runs.
    pub gateway_url: Option<String>,
}

impl InboundConfig {
    /// The catalog defaults (60 a minute, 10 bridges) and Discord's own gateway.
    pub fn defaults() -> Self {
        Self {
            messages_per_minute: Arc::new(|| 60),
            max_bridges: Arc::new(|| 10),
            gateway_url: None,
        }
    }
}

/// What happened to one dispatch (the tests read it; the shard loop only logs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Posted(i64),
    Edited(i64),
    Deleted(usize),
    Reacted { added: bool },
    Ignored(Why),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// Not a payload the bridge understands, or not a kind of message it mirrors.
    Unparseable,
    /// The Discord channel is not mapped, or its map does not bring messages in.
    Unmapped,
    /// Written by the bridge's own bot.
    OwnBot,
    /// Posted through one of the bridge's own webhooks: it is our message coming back.
    OwnWebhook,
    /// Already shown (a repeated dispatch, or a message the bridge posted there).
    Known,
    /// Over `abuse.bridge_messages_per_minute`.
    RateLimited,
    /// An edit, delete or reaction for a message the bridge never brought in.
    NotBridged,
    /// A partial update (no text, no attachments).
    NothingToChange,
    /// A custom emoji reaction: it does not exist here.
    CustomEmoji,
    /// Another dispatch the bridge has no use for.
    Other,
}

/// The inbound handler: shared by every bridge's shard.
pub struct Inbound {
    pub(crate) pool: PgPool,
    pub(crate) sink: Arc<dyn ChatSink>,
    pub(crate) limiter: BridgeLimiter,
    pub(crate) cfg: InboundConfig,
}

impl Inbound {
    pub fn new(pool: PgPool, sink: Arc<dyn ChatSink>, cfg: InboundConfig) -> Self {
        Self {
            pool,
            sink,
            limiter: BridgeLimiter::new(),
            cfg,
        }
    }

    /// Messages dropped for the abuse limit since the process started (for the status).
    pub fn dropped(&self, guild: i64) -> u64 {
        self.limiter.dropped(guild)
    }

    pub async fn handle(&self, bridge: &Bridge, d: &Dispatch) -> BridgeResult<Outcome> {
        match d.name.as_str() {
            "MESSAGE_CREATE" => self.create(bridge, &d.data).await,
            "MESSAGE_UPDATE" => self.update(bridge, &d.data).await,
            "MESSAGE_DELETE" | "MESSAGE_DELETE_BULK" => self.delete(bridge, &d.name, &d.data).await,
            "MESSAGE_REACTION_ADD" | "MESSAGE_REACTION_REMOVE" => {
                self.reaction(bridge, &d.name, &d.data).await
            }
            _ => Ok(Outcome::Ignored(Why::Other)),
        }
    }

    async fn create(&self, bridge: &Bridge, data: &Value) -> BridgeResult<Outcome> {
        let Some(m) = convert::parse_message(data) else {
            return Ok(Outcome::Ignored(Why::Unparseable));
        };
        let Some(map) =
            store::map_for_discord_channel(&self.pool, bridge.guild_id, m.channel_id).await?
        else {
            return Ok(Outcome::Ignored(Why::Unmapped));
        };
        if !map.direction.receives() {
            return Ok(Outcome::Ignored(Why::Unmapped));
        }
        // The loop guard: what the bridge itself wrote must never come back as a new message.
        if m.author_id == bridge.bot_user_id {
            return Ok(Outcome::Ignored(Why::OwnBot));
        }
        if let Some(w) = m.webhook_id
            && store::is_own_webhook(&self.pool, bridge.guild_id, w).await?
        {
            return Ok(Outcome::Ignored(Why::OwnWebhook));
        }
        if store::sr_message_for_discord(&self.pool, map.id, m.id)
            .await?
            .is_some()
        {
            return Ok(Outcome::Ignored(Why::Known));
        }
        if !self
            .limiter
            .allow(bridge.guild_id, (self.cfg.messages_per_minute)())
        {
            if self.limiter.dropped(bridge.guild_id).is_power_of_two() {
                tracing::warn!(
                    guild = bridge.guild_id,
                    dropped = self.limiter.dropped(bridge.guild_id),
                    "bridge inbound over abuse.bridge_messages_per_minute: dropping"
                );
            }
            return Ok(Outcome::Ignored(Why::RateLimited));
        }
        let reply_to = match m.reply_to {
            Some(target) => store::sr_message_for_discord(&self.pool, map.id, target)
                .await?
                .map(|(sr, _)| sr),
            None => None,
        };
        if m.content.trim().is_empty() && m.links.is_empty() {
            // Stickers, embeds only, and the like: nothing to show.
            return Ok(Outcome::Ignored(Why::Unparseable));
        }
        let post = post_of(bridge, map.channel_id, reply_to, m);
        let discord_id = post_discord_id(data);
        let sr = self.sink.post(post).await?;
        if let Some(did) = discord_id {
            store::insert_inbound_map(&self.pool, sr, did, map.id).await?;
        }
        Ok(Outcome::Posted(sr))
    }

    async fn update(&self, bridge: &Bridge, data: &Value) -> BridgeResult<Outcome> {
        let Some(e) = convert::parse_edit(data) else {
            return Ok(Outcome::Ignored(Why::Unparseable));
        };
        if e.content.is_none() && e.links.is_none() {
            return Ok(Outcome::Ignored(Why::NothingToChange));
        }
        let Some(map) =
            store::map_for_discord_channel(&self.pool, bridge.guild_id, e.channel_id).await?
        else {
            return Ok(Outcome::Ignored(Why::Unmapped));
        };
        match store::sr_message_for_discord(&self.pool, map.id, e.id).await? {
            Some((sr, MessageMapDirection::In)) => {
                self.sink
                    .edit(map.channel_id, sr, e.content, e.links)
                    .await?;
                Ok(Outcome::Edited(sr))
            }
            _ => Ok(Outcome::Ignored(Why::NotBridged)),
        }
    }

    async fn delete(&self, bridge: &Bridge, name: &str, data: &Value) -> BridgeResult<Outcome> {
        let Some(del) = convert::parse_delete(name, data) else {
            return Ok(Outcome::Ignored(Why::Unparseable));
        };
        let Some(map) =
            store::map_for_discord_channel(&self.pool, bridge.guild_id, del.channel_id).await?
        else {
            return Ok(Outcome::Ignored(Why::Unmapped));
        };
        let mut gone = 0;
        for id in del.ids {
            // Only a message that came in is removed here. One of ours that someone deleted on Discord stays.
            if let Some((sr, MessageMapDirection::In)) =
                store::sr_message_for_discord(&self.pool, map.id, id).await?
            {
                self.sink.delete(map.channel_id, sr).await?;
                store::delete_inbound_map(&self.pool, sr).await?;
                gone += 1;
            }
        }
        if gone == 0 {
            Ok(Outcome::Ignored(Why::NotBridged))
        } else {
            Ok(Outcome::Deleted(gone))
        }
    }

    async fn reaction(&self, bridge: &Bridge, name: &str, data: &Value) -> BridgeResult<Outcome> {
        let Some(r) = convert::parse_reaction(name, data) else {
            return Ok(Outcome::Ignored(Why::Unparseable));
        };
        let Some(emoji) = r.unicode.as_deref() else {
            return Ok(Outcome::Ignored(Why::CustomEmoji));
        };
        if r.user_id == bridge.bot_user_id {
            return Ok(Outcome::Ignored(Why::OwnBot));
        }
        let Some(map) =
            store::map_for_discord_channel(&self.pool, bridge.guild_id, r.channel_id).await?
        else {
            return Ok(Outcome::Ignored(Why::Unmapped));
        };
        if !map.direction.receives() {
            return Ok(Outcome::Ignored(Why::Unmapped));
        }
        // Reactions land on any mirrored message, ours or theirs.
        let Some((sr, _)) =
            store::sr_message_for_discord(&self.pool, map.id, r.message_id).await?
        else {
            return Ok(Outcome::Ignored(Why::NotBridged));
        };
        let change = if r.add {
            store::add_reactor(&self.pool, map.id, sr, emoji, r.user_id).await?
        } else {
            store::remove_reactor(&self.pool, sr, emoji, r.user_id).await?
        };
        if change {
            self.sink.react(map.channel_id, sr, emoji, r.add).await?;
        }
        Ok(Outcome::Reacted { added: r.add })
    }
}

fn post_discord_id(data: &Value) -> Option<i64> {
    data.get("id").and_then(convert::id_of)
}

fn post_of(bridge: &Bridge, channel: i64, reply_to: Option<i64>, m: InMessage) -> InboundPost {
    InboundPost {
        guild_id: bridge.guild_id,
        channel_id: channel,
        as_user: bridge.created_by,
        reply_to,
        content: m.content,
        author: InboundAuthor {
            platform: "discord",
            name: m.author_name,
            avatar: m.author_avatar,
            discord_user_id: m.author_id,
            links: m.links,
        },
    }
}
