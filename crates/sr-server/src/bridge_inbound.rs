//! BRG-GW-001 (wave 35 integration): the inbound half of the Discord bridge wired into the server process.
//!
//! * [`ChatInbound`] is the production [`ChatSink`]: a Discord message becomes a SIDE ROOM message written straight
//!   through the message repository (the way the feature modules' messages are), stored under the bridge's own
//!   **integration author** (a bot account, `bridge_integration_authors`, never the owner's account), with **no
//!   mentions recorded**, so a `<@id>` coming from Discord pings nobody here. The Discord user's name, avatar and
//!   attachment links go to `message_bridge_authors` before the gateway event is built, so the `MESSAGE_CREATE`
//!   carries `sr_bridge` (which is also the outbound loop guard).
//! * [`spawn_inbound`] builds the shard manager (one `sr-sdk` shard per enabled bridge, at most `bridge.max_bridges`)
//!   and its supervisor, registers it as a teardown hook (switching the bridge off stops the shard at once), and
//!   keeps the two live figures (`abuse.bridge_messages_per_minute`, `bridge.max_bridges`) fresh.
//!
//! Environment: `SR_BRIDGE_DISCORD_GATEWAY` (default: Discord's own gateway) points the shards at the fake Discord
//! in the end-to-end suite; `SR_BRIDGE_SUPERVISE_SECS` (default 30) is how often the shards are matched to the
//! database. The presence intent is asked for and dropped down the intent ladder when Discord refuses it
//! (commander default: presence is used when Discord grants it, no extra toggle).

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use sqlx::PgPool;
use sr_api::routes::messages::{MessagesDeps, render::event_payload, store::EmojiKey};
use sr_bridge::inbound::{
    ChatSink, Inbound, InboundConfig, InboundManager, InboundPost, ServiceHost, convert::InLink,
};
use sr_bridge::{BridgeError, BridgeResult, BridgeService};
use sr_chat::message_rules::{DeletePlan, EditPlan, ResolvedMentions};
use sr_chat::rich_repo::{BridgeAuthor, BridgeLink};
use sr_chat::{Message, MessageType, Visibility};
use sr_core::{ChannelId, MessageId, UserId};
use sr_events::DomainEvent;
use tokio::task::JoinHandle;

/// Longest text a bridged message keeps (the inbound converter already cuts at this).
const CONTENT_MAX: usize = 4000;

fn cid(v: i64) -> ChannelId {
    ChannelId::new(u64::try_from(v).unwrap_or(0))
}

fn mid(v: i64) -> MessageId {
    MessageId::new(u64::try_from(v).unwrap_or(0))
}

fn uid(v: i64) -> UserId {
    UserId::new(u64::try_from(v).unwrap_or(0))
}

fn raw(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

fn storage(e: impl std::fmt::Display) -> BridgeError {
    BridgeError::Storage(e.to_string())
}

fn links_of(links: &[InLink]) -> Vec<BridgeLink> {
    links
        .iter()
        .map(|l| BridgeLink {
            filename: l.filename.clone(),
            url: l.url.clone(),
            expires_at_ms: (l.expires_at_ms > 0).then_some(l.expires_at_ms),
        })
        .collect()
}

/// The production chat side of the inbound bridge.
pub struct ChatInbound {
    pub pool: PgPool,
    pub messages: Arc<MessagesDeps>,
}

impl ChatInbound {
    async fn channel(&self, id: i64) -> BridgeResult<sr_chat::Channel> {
        self.messages
            .repo
            .get_channel(cid(id))
            .await
            .map_err(storage)?
            .ok_or(BridgeError::NotFound("channel"))
    }

    async fn visible(&self, channel: i64, message: i64) -> BridgeResult<Option<Message>> {
        Ok(self
            .messages
            .repo
            .get_message(cid(channel), mid(message))
            .await
            .map_err(storage)?
            .filter(Message::is_visible))
    }

    async fn announce_edit(&self, channel: i64, message: i64) {
        if let Ok(Some(fresh)) = self
            .messages
            .repo
            .get_message(cid(channel), mid(message))
            .await
            && let Ok(data) = event_payload(&self.messages, &fresh).await
        {
            self.messages
                .publish(DomainEvent::MessageEdited {
                    guild: fresh.guild_id,
                    channel: fresh.channel_id,
                    message_id: fresh.id.get(),
                    data,
                })
                .await;
        }
    }

    /// The integration author of the server a channel belongs to (reactions are added as it).
    async fn author_for_channel(&self, channel: i64) -> BridgeResult<Option<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT a.user_id FROM bridge_integration_authors a JOIN channels c ON c.guild_id = a.guild_id \
             WHERE c.id = $1",
        )
        .bind(channel)
        .fetch_optional(&self.pool)
        .await?)
    }
}

#[async_trait]
impl ChatSink for ChatInbound {
    async fn post(&self, post: InboundPost) -> BridgeResult<i64> {
        let ch = self.channel(post.channel_id).await?;
        if !ch.kind.accepts_messages() || ch.guild_id.map(|g| raw(g.get())) != Some(post.guild_id) {
            return Err(BridgeError::Conflict(
                "the mapped channel takes no messages".into(),
            ));
        }
        let reference = post
            .reply_to
            .map(|target| sr_chat::domain::MessageReference {
                message_id: mid(target),
                channel_id: ch.id,
                forward: None,
            });
        let m = Message {
            id: MessageId::from(self.messages.next_id()),
            channel_id: ch.id,
            guild_id: ch.guild_id,
            author_id: uid(post.as_user),
            kind: if reference.is_some() {
                MessageType::Reply
            } else {
                MessageType::Default
            },
            content: post.content.chars().take(CONTENT_MAX).collect(),
            flags: 0,
            edited_at_ms: None,
            reference,
            attachment_ids: Vec::new(),
            embeds: Vec::new(),
            components: Vec::new(),
            sticker_ids: Vec::new(),
            poll: None,
            // Nothing that came from Discord pings anyone here: `<@id>` stays text (a bridged member's chip).
            mention_everyone: false,
            mention_user_ids: Vec::new(),
            mention_role_ids: Vec::new(),
            mention_channel_ids: Vec::new(),
            pinned: false,
            visibility: Visibility::Visible,
            deleted_at_ms: None,
            purge_after_ms: None,
            hold_id: None,
        };
        self.messages
            .repo
            .insert_message(&m)
            .await
            .map_err(storage)?;
        if let Some(store) = &self.messages.rich.store {
            let author = BridgeAuthor {
                platform: post.author.platform.to_owned(),
                author_name: post.author.name.clone(),
                author_avatar: post.author.avatar.clone(),
                discord_user_id: Some(post.author.discord_user_id.to_string()),
                links: links_of(&post.author.links),
            };
            store
                .set_bridge_author(m.id, &author)
                .await
                .map_err(|e| storage(e.0))?;
        }
        match event_payload(&self.messages, &m).await {
            Ok(data) => {
                self.messages
                    .publish(DomainEvent::MessageSent {
                        guild: m.guild_id,
                        channel: m.channel_id,
                        message_id: m.id.get(),
                        author: m.author_id,
                        data,
                    })
                    .await;
            }
            Err(_) => tracing::warn!("rendering a bridged message failed"),
        }
        Ok(raw(m.id.get()))
    }

    async fn edit(
        &self,
        channel_id: i64,
        message_id: i64,
        content: Option<String>,
        links: Option<Vec<InLink>>,
    ) -> BridgeResult<()> {
        let Some(current) = self.visible(channel_id, message_id).await? else {
            return Ok(());
        };
        if let Some(text) = content {
            let text: String = text.chars().take(CONTENT_MAX).collect();
            let plan = EditPlan {
                message_id: current.id,
                content: text.clone(),
                flags: current.flags,
                edited_at_ms: self.messages.now_ms(),
                mentions: ResolvedMentions::default(),
                history_previous_content: (current.content != text)
                    .then(|| current.content.clone()),
            };
            self.messages
                .repo
                .apply_edit(&plan)
                .await
                .map_err(storage)?;
        }
        if let (Some(links), Some(store)) = (links, &self.messages.rich.store) {
            let known = store
                .bridge_authors(&[current.id])
                .await
                .map_err(|e| storage(e.0))?;
            if let Some(mut author) = known.get(&current.id).cloned() {
                author.links = links_of(&links);
                store
                    .set_bridge_author(current.id, &author)
                    .await
                    .map_err(|e| storage(e.0))?;
            }
        }
        self.announce_edit(channel_id, message_id).await;
        Ok(())
    }

    async fn delete(&self, channel_id: i64, message_id: i64) -> BridgeResult<()> {
        let Some(current) = self.visible(channel_id, message_id).await? else {
            return Ok(());
        };
        let now = self.messages.now_ms();
        let plan = DeletePlan {
            message_id: current.id,
            visibility: Visibility::UserDeleted,
            deleted_at_ms: now,
            purge_after_ms: current
                .hold_id
                .is_none()
                .then(|| now + self.messages.limits.purge_after_ms),
        };
        self.messages
            .repo
            .apply_delete(&plan)
            .await
            .map_err(storage)?;
        self.messages
            .publish(DomainEvent::MessageDeleted {
                guild: current.guild_id,
                channel: current.channel_id,
                message_id: current.id.get(),
            })
            .await;
        Ok(())
    }

    async fn typing(&self, channel_id: i64, discord_user_id: i64) -> BridgeResult<()> {
        let ch = self.channel(channel_id).await?;
        let timestamp = u64::try_from(self.messages.now_ms().div_euclid(1000)).unwrap_or(0);
        // The user id is the Discord member's id; clients resolve it among the server's bridged members first.
        self.messages
            .publish(DomainEvent::TypingStarted {
                guild: ch.guild_id,
                channel: ch.id,
                user: uid(discord_user_id),
                timestamp,
            })
            .await;
        Ok(())
    }

    async fn react(
        &self,
        channel_id: i64,
        message_id: i64,
        emoji: &str,
        add: bool,
    ) -> BridgeResult<()> {
        let Some(key) = EmojiKey::parse(emoji) else {
            return Ok(());
        };
        let Some(author) = self.author_for_channel(channel_id).await? else {
            return Ok(());
        };
        let Some(msg) = self.visible(channel_id, message_id).await? else {
            return Ok(());
        };
        let user = uid(author);
        let changed = if add {
            self.messages
                .store
                .add_reaction(msg.channel_id, msg.id, user, &key, self.messages.now_ms())
                .await
                .map_err(storage)?
        } else {
            self.messages
                .store
                .remove_reaction(msg.id, user, &key)
                .await
                .map_err(storage)?
        };
        if changed {
            let ev = if add {
                DomainEvent::ReactionAdded {
                    guild: msg.guild_id,
                    channel: msg.channel_id,
                    message_id: msg.id.get(),
                    user,
                    emoji: key.json(),
                }
            } else {
                DomainEvent::ReactionRemoved {
                    guild: msg.guild_id,
                    channel: msg.channel_id,
                    message_id: msg.id.get(),
                    user,
                    emoji: key.json(),
                }
            };
            self.messages.publish(ev).await;
        }
        Ok(())
    }
}

/// The running inbound half: its supervisor and the settings refresher. Abort both on shutdown.
pub struct InboundTasks {
    pub manager: Arc<InboundManager>,
    pub supervisor: JoinHandle<()>,
    pub refresher: JoinHandle<()>,
}

impl InboundTasks {
    pub fn abort(&self) {
        self.supervisor.abort();
        self.refresher.abort();
        self.manager.stop_all();
    }
}

/// How often the shards are matched to the database (`SR_BRIDGE_SUPERVISE_SECS`, default 30).
fn supervise_every() -> Duration {
    let secs = std::env::var("SR_BRIDGE_SUPERVISE_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| (1..=3600).contains(v))
        .unwrap_or(30);
    Duration::from_secs(secs)
}

/// The gateway the shards connect to: `[bridge] discord_gateway`, else `SR_BRIDGE_DISCORD_GATEWAY`, else Discord's own.
pub fn discord_gateway_url(configured: Option<&str>) -> Option<String> {
    configured
        .map(str::to_owned)
        .or_else(|| std::env::var("SR_BRIDGE_DISCORD_GATEWAY").ok())
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

/// Start the inbound half: one shard per enabled bridge, the supervisor, and the live figures.
pub fn spawn_inbound(
    pool: PgPool,
    messages: Arc<MessagesDeps>,
    bridge: Arc<BridgeService>,
    admin: sr_admin::AdminService,
    gateway: Option<String>,
    structure: Arc<dyn sr_bridge::inbound::structure::StructureSink>,
) -> InboundTasks {
    use sr_admin::settings as k;
    let per_minute = Arc::new(AtomicU32::new(60));
    let max_bridges = Arc::new(AtomicU32::new(10));
    let cfg = InboundConfig {
        messages_per_minute: {
            let v = per_minute.clone();
            Arc::new(move || v.load(Ordering::Relaxed))
        },
        max_bridges: {
            let v = max_bridges.clone();
            Arc::new(move || usize::try_from(v.load(Ordering::Relaxed)).unwrap_or(10))
        },
        gateway_url: gateway.clone(),
        // Asked for; a server whose bot lacks the privileged intent steps down the ladder and keeps working.
        presence: true,
    };
    let sink = Arc::new(ChatInbound {
        pool: pool.clone(),
        messages,
    });
    let inbound = Arc::new(Inbound::new(pool.clone(), sink, cfg).with_structure(structure));
    let manager = InboundManager::new(
        pool,
        Arc::new(ServiceHost((*bridge).clone())),
        inbound,
        Arc::new(sr_bridge::inbound::WsConnector),
    );
    bridge.add_teardown_hook(manager.clone());
    let refresher = tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(30));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            for (key, cell, default) in [
                (k::ABUSE_BRIDGE_MESSAGES, &per_minute, 60_u32),
                (k::BRIDGE_MAX_BRIDGES, &max_bridges, 10_u32),
            ] {
                let v = admin
                    .setting_value_i64(key)
                    .await
                    .ok()
                    .and_then(|n| u32::try_from(n).ok())
                    .unwrap_or(default);
                cell.store(v, Ordering::Relaxed);
            }
        }
    });
    let supervisor = manager.spawn_supervisor(supervise_every());
    tracing::info!(
        gateway = %gateway.as_deref().unwrap_or("discord"),
        "discord bridge inbound supervisor started"
    );
    InboundTasks {
        manager,
        supervisor,
        refresher,
    }
}
