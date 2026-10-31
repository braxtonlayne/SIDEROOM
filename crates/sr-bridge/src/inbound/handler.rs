//! One dispatch from a bridge's shard to its effect here. Everything that decides is in this file: which channel,
//! which messages the bridge itself wrote (the loop guard), the abuse limit, replies, edits, deletes, reactions.

use std::sync::Arc;

use serde_json::Value;
use sqlx::PgPool;
use sr_sdk::gateway::Dispatch;

use super::convert::{self, InMessage};
use super::limit::BridgeLimiter;
use super::members::{self, RoleCache};
use super::sink::{ChatSink, InboundAuthor, InboundPost};
use super::store;
use super::structure::{self, StructureSink};
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
    /// Also ask for `GUILD_PRESENCES` (privileged): online/idle/dnd on the bridged members. Off by default.
    pub presence: bool,
}

impl InboundConfig {
    /// The catalog defaults (60 a minute, 10 bridges) and Discord's own gateway.
    pub fn defaults() -> Self {
        Self {
            messages_per_minute: Arc::new(|| 60),
            max_bridges: Arc::new(|| 10),
            gateway_url: None,
            presence: false,
        }
    }
}

/// What happened to one dispatch (the tests read it; the shard loop only logs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Posted(i64),
    Edited(i64),
    Deleted(usize),
    Reacted {
        added: bool,
    },
    /// A bridged member changed (added, updated, removed, presence, a members chunk).
    Member,
    Typing,
    /// BRG-SYN-003: this many structural events were handed to the reconciler.
    Structure(usize),
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
    /// An event of another Discord server the bot is in.
    OtherServer,
    /// BRG-SYN-003: the bridge's own structural write coming back (audit reason `sr-bridge:`).
    Echo,
    /// BRG-SYN-003: structural event while no reconciler is attached.
    NoReconciler,
    /// The bridge has no integration author to store the message under (a row not made by connect).
    NoAuthor,
    /// Another dispatch the bridge has no use for.
    Other,
}

/// The inbound handler: shared by every bridge's shard.
pub struct Inbound {
    pub(crate) pool: PgPool,
    pub(crate) sink: Arc<dyn ChatSink>,
    pub(crate) limiter: BridgeLimiter,
    pub(crate) cfg: InboundConfig,
    pub(crate) roles: RoleCache,
    /// BRG-SYN-003: where structural dispatches go (the sync engine); `None` until it is attached.
    pub(crate) structure: Option<Arc<dyn StructureSink>>,
}

impl Inbound {
    pub fn new(pool: PgPool, sink: Arc<dyn ChatSink>, cfg: InboundConfig) -> Self {
        Self {
            pool,
            sink,
            limiter: BridgeLimiter::new(),
            cfg,
            roles: RoleCache::default(),
            structure: None,
        }
    }

    /// Attach the reconciler that gets structural dispatches (BRG-SYN-003).
    #[must_use]
    pub fn with_structure(mut self, sink: Arc<dyn StructureSink>) -> Self {
        self.structure = Some(sink);
        self
    }

    /// Messages dropped for the abuse limit since the process started (for the status).
    pub fn dropped(&self, guild: i64) -> u64 {
        self.limiter.dropped(guild)
    }

    pub async fn handle(&self, bridge: &Bridge, d: &Dispatch) -> BridgeResult<Outcome> {
        // Structure first: role events feed both the member cache below and the reconciler.
        let structural = if structure::is_structural(&d.name) {
            Some(self.structural(bridge, d).await?)
        } else {
            None
        };
        match d.name.as_str() {
            "MESSAGE_CREATE" => self.create(bridge, &d.data).await,
            "MESSAGE_UPDATE" => self.update(bridge, &d.data).await,
            "MESSAGE_DELETE" | "MESSAGE_DELETE_BULK" => self.delete(bridge, &d.name, &d.data).await,
            "MESSAGE_REACTION_ADD" | "MESSAGE_REACTION_REMOVE" => {
                self.reaction(bridge, &d.name, &d.data).await
            }
            "TYPING_START" => self.typing(bridge, &d.data).await,
            "GUILD_CREATE"
            | "GUILD_MEMBER_ADD"
            | "GUILD_MEMBER_UPDATE"
            | "GUILD_MEMBER_REMOVE"
            | "GUILD_MEMBERS_CHUNK"
            | "GUILD_ROLE_CREATE"
            | "GUILD_ROLE_UPDATE"
            | "GUILD_ROLE_DELETE"
            | "PRESENCE_UPDATE" => {
                let member = self.member_event(bridge, &d.name, &d.data).await?;
                // A role event is also structure; report the reconciler's outcome when it took it.
                Ok(match structural {
                    Some(o @ Outcome::Structure(_)) => o,
                    _ => member,
                })
            }
            _ => Ok(structural.unwrap_or(Outcome::Ignored(Why::Other))),
        }
    }

    /// Structural dispatches to the reconciler (BRG-SYN-003). Events of other servers and the bridge's own
    /// audit-log echoes are dropped.
    async fn structural(&self, bridge: &Bridge, d: &Dispatch) -> BridgeResult<Outcome> {
        let event_guild = d.data.get("guild_id").and_then(convert::id_of);
        if event_guild.is_some_and(|g| g != bridge.discord_guild_id) {
            return Ok(Outcome::Ignored(Why::OtherServer));
        }
        let Some(sink) = &self.structure else {
            return Ok(Outcome::Ignored(Why::NoReconciler));
        };
        if d.name == "GUILD_AUDIT_LOG_ENTRY_CREATE" {
            if structure::is_echo(&d.data) {
                return Ok(Outcome::Ignored(Why::Echo));
            }
            let target = d.data.get("target_id").and_then(convert::id_of);
            let user = d.data.get("user_id").and_then(convert::id_of);
            let action = d
                .data
                .get("action_type")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            if let (Some(t), Some(u)) = (target, user)
                && u != bridge.bot_user_id
            {
                sink.attribute(bridge, t, u, action).await;
            }
            return Ok(Outcome::Ignored(Why::Other));
        }
        let events = structure::events_for(&d.name, &d.data);
        if events.is_empty() {
            return Ok(Outcome::Ignored(Why::Unparseable));
        }
        let n = events.len();
        sink.look_at(bridge, events).await;
        Ok(Outcome::Structure(n))
    }

    async fn typing(&self, bridge: &Bridge, data: &Value) -> BridgeResult<Outcome> {
        let (Some(ch), Some(user)) = (
            data.get("channel_id").and_then(convert::id_of),
            data.get("user_id").and_then(convert::id_of),
        ) else {
            return Ok(Outcome::Ignored(Why::Unparseable));
        };
        if user == bridge.bot_user_id {
            return Ok(Outcome::Ignored(Why::OwnBot));
        }
        match store::map_for_discord_channel(&self.pool, bridge.guild_id, ch).await? {
            Some(map) if map.direction.receives() => {
                self.sink.typing(map.channel_id, user).await?;
                Ok(Outcome::Typing)
            }
            _ => Ok(Outcome::Ignored(Why::Unmapped)),
        }
    }

    /// Members, roles and presence of the bridge's Discord server. Events of other servers the bot is in are
    /// ignored. Nothing here makes an account: the rows only feed the member list.
    async fn member_event(&self, bridge: &Bridge, name: &str, d: &Value) -> BridgeResult<Outcome> {
        let dg = bridge.discord_guild_id;
        let g = bridge.guild_id;
        let event_guild = if name == "GUILD_CREATE" {
            d.get("id")
        } else {
            d.get("guild_id")
        }
        .and_then(convert::id_of);
        if event_guild != Some(dg) {
            return Ok(Outcome::Ignored(Why::OtherServer));
        }
        let put = |m: members::InMember| async move {
            let top = self.roles.top(dg, &m.role_ids);
            members::upsert(&self.pool, g, &m, top).await
        };
        match name {
            "GUILD_CREATE" => {
                self.roles.replace(
                    dg,
                    members::parse_roles(d.get("roles").unwrap_or(&Value::Null)),
                );
                for m in d
                    .get("members")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(members::parse_member)
                {
                    put(m).await?;
                }
            }
            "GUILD_MEMBER_ADD" | "GUILD_MEMBER_UPDATE" => {
                if let Some(m) = members::parse_member(d) {
                    put(m).await?;
                }
            }
            "GUILD_MEMBER_REMOVE" => {
                if let Some(u) = d.pointer("/user/id").and_then(convert::id_of) {
                    members::remove(&self.pool, g, u).await?;
                }
            }
            "GUILD_MEMBERS_CHUNK" => {
                let index = d.get("chunk_index").and_then(Value::as_i64).unwrap_or(0);
                let count = d.get("chunk_count").and_then(Value::as_i64).unwrap_or(1);
                if index == 0 {
                    members::mark_all_stale(&self.pool, g).await?;
                }
                for m in d
                    .get("members")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(members::parse_member)
                {
                    put(m).await?;
                }
                for p in d
                    .get("presences")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if let (Some(u), Some(s)) = (
                        p.pointer("/user/id").and_then(convert::id_of),
                        p.get("status").and_then(Value::as_str),
                    ) {
                        members::set_presence(&self.pool, g, u, s).await?;
                    }
                }
                if index + 1 >= count {
                    members::drop_stale(&self.pool, g).await?;
                }
            }
            "GUILD_ROLE_CREATE" | "GUILD_ROLE_UPDATE" => {
                if let Some((id, role)) = d.get("role").and_then(members::parse_role) {
                    members::update_role(&self.pool, g, id, &role.name, role.color).await?;
                    self.roles.set(dg, id, role);
                }
            }
            "GUILD_ROLE_DELETE" => {
                if let Some(id) = d.get("role_id").and_then(convert::id_of) {
                    self.roles.remove(dg, id);
                }
            }
            "PRESENCE_UPDATE" => {
                if let (Some(u), Some(s)) = (
                    d.pointer("/user/id").and_then(convert::id_of),
                    d.get("status").and_then(Value::as_str),
                ) {
                    members::set_presence(&self.pool, g, u, s).await?;
                }
            }
            _ => {}
        }
        Ok(Outcome::Member)
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
        // Stored under the bridge's own integration author (1721), never the owner's account.
        let Some(as_user) = bridge.integration_user_id else {
            tracing::warn!(
                guild = bridge.guild_id,
                "bridge without an integration author: message not shown"
            );
            return Ok(Outcome::Ignored(Why::NoAuthor));
        };
        let post = post_of(bridge, as_user, map.channel_id, reply_to, m);
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
        let Some((sr, _)) = store::sr_message_for_discord(&self.pool, map.id, r.message_id).await?
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

fn post_of(
    bridge: &Bridge,
    as_user: i64,
    channel: i64,
    reply_to: Option<i64>,
    m: InMessage,
) -> InboundPost {
    InboundPost {
        guild_id: bridge.guild_id,
        channel_id: channel,
        as_user,
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
