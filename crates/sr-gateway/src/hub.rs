//! The gateway hub: session registry, per-guild subscriber sets, EventBus
//! driven fan-out with per-recipient permission filtering, presence, typing.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

use futures::StreamExt;
use serde_json::{Value, json};
use sr_core::{ChannelId, GuildId, Permissions, UserId};
use sr_events::{DomainEvent, EventBus, Topic};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::presence::{Report, merge as presence_merge, sanitize_activities};
use crate::protocol::{PresenceUpdate, intents};
use crate::session::{Out, Session};
use crate::traits::{AuthedUser, Authenticator, GuildDirectory, OpHandler};
use crate::voice::{VoiceConfig, VoiceState};

/// Discord's default `large_threshold`: guilds at or above it get no initial presences.
pub const LARGE_THRESHOLD_DEFAULT: u64 = 50;
/// Discord's maximum `large_threshold`.
pub const LARGE_THRESHOLD_MAX: u64 = 250;

/// GUILD_CREATE bodies built at once while a session identifies (each is a dozen or more directory reads).
const GUILD_CREATE_CONCURRENCY: usize = 4;

#[derive(Clone)]
pub struct GatewayConfig {
    /// Sent in HELLO. Docs/05 6.4: 41250 ms.
    pub heartbeat_interval: Duration,
    /// Extra slack before a silent client is closed with 4009: 30 s (P24-GW-1), so a stalled page or a
    /// backgrounded tab whose timers Chrome aligns to one wake-up a minute is not timed out.
    pub heartbeat_grace: Duration,
    /// How long a disconnected session stays resumable (2 minutes).
    pub resume_window: Duration,
    /// Max buffered events per session (5,000).
    pub resume_buffer: usize,
    pub resume_gateway_url: Option<String>,
    /// How long a TYPING_START keeps a user "typing".
    pub typing_ttl: Duration,
    /// Period of the background expiry sweep.
    pub sweep_interval: Duration,
    /// GUILD_CREATE carries `presences` only for guilds with fewer members than this
    /// (clamped to [`LARGE_THRESHOLD_MAX`]).
    pub large_threshold: u64,
    /// `Origin` values allowed to authenticate with the `sr_session` cookie.
    /// Empty disables cookie authentication.
    pub allowed_origins: Vec<String>,
    /// Handlers for client opcodes the gateway does not implement itself.
    pub handlers: Vec<Arc<dyn OpHandler>>,
    /// Voice (op 4, `VOICE_SERVER_UPDATE`). `None` disables joining voice channels.
    pub voice: Option<VoiceConfig>,
}

impl fmt::Debug for GatewayConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GatewayConfig")
            .field("heartbeat_interval", &self.heartbeat_interval)
            .field("heartbeat_grace", &self.heartbeat_grace)
            .field("resume_window", &self.resume_window)
            .field("resume_buffer", &self.resume_buffer)
            .field("resume_gateway_url", &self.resume_gateway_url)
            .field("typing_ttl", &self.typing_ttl)
            .field("sweep_interval", &self.sweep_interval)
            .field("large_threshold", &self.large_threshold)
            .field("allowed_origins", &self.allowed_origins)
            .field("handlers", &self.handlers.len())
            .field("voice", &self.voice.is_some())
            .finish()
    }
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: Duration::from_millis(41_250),
            heartbeat_grace: Duration::from_secs(30),
            resume_window: Duration::from_secs(120),
            resume_buffer: 5_000,
            resume_gateway_url: None,
            typing_ttl: Duration::from_secs(10),
            sweep_interval: Duration::from_secs(15),
            large_threshold: LARGE_THRESHOLD_DEFAULT,
            allowed_origins: Vec::new(),
            handlers: Vec::new(),
            voice: None,
        }
    }
}

#[derive(Default)]
pub(crate) struct State {
    pub(crate) sessions: HashMap<String, Arc<Session>>,
    by_guild: HashMap<GuildId, HashSet<String>>,
    guild_tasks: HashMap<GuildId, JoinHandle<()>>,
    /// One `Topic::User` subscription per connected user (DM and self events).
    user_tasks: HashMap<UserId, JoinHandle<()>>,
    /// The user's chosen status and custom status (from `PATCH /users/@me/presence`).
    /// Visible only while online.
    presence: HashMap<UserId, PresenceUpdate>,
    /// What each session last reported over op 3 / IDENTIFY, by session id (LNG-PRF-015).
    session_presence: HashMap<String, SessionPresence>,
    /// Orders session reports: larger is newer.
    presence_seq: u64,
    /// The last presence broadcast per user, so a closing session only announces a change.
    last_presence: HashMap<UserId, PresenceUpdate>,
    /// Users with at least one attached socket.
    online: HashSet<UserId>,
    typing: HashMap<(ChannelId, UserId), Instant>,
    /// Voice states by user (a user is in one voice channel at a time). In memory, per node.
    pub(crate) voice: HashMap<UserId, VoiceState>,
    /// Lazy member lists by (guild, channel) while somebody is subscribed (LNG-GLD-010).
    pub(crate) lists: HashMap<(GuildId, ChannelId), crate::lists::ListState>,
    /// Guilds whose lists wait for a debounced refresh; the flag asks for a member refetch.
    pub(crate) list_dirty: HashMap<GuildId, bool>,
    pub(crate) vx: crate::voice::VoiceExtra,
    background: Vec<JoinHandle<()>>,
}

/// One session's last presence report and its client platform.
pub(crate) struct SessionPresence {
    pub platform: &'static str,
    pub update: Option<PresenceUpdate>,
    pub order: u64,
}

pub(crate) struct Inner {
    pub cfg: GatewayConfig,
    pub auth: Arc<dyn Authenticator>,
    pub dir: Arc<dyn GuildDirectory>,
    pub(crate) bus: Arc<dyn EventBus>,
    state: Mutex<State>,
    next_conn: AtomicU64,
    /// Users seen since the last [`Gateway::drain_seen`] (D-166).
    seen: Mutex<HashSet<UserId>>,
}

/// Cheap-to-clone handle; mount [`crate::ws::routes`] with it as router state.
#[derive(Clone)]
pub struct Gateway {
    pub(crate) inner: Arc<Inner>,
}

/// A dispatch derived from a domain event.
struct Dispatch {
    name: &'static str,
    data: Value,
    channel: Option<ChannelId>,
    /// When set, only these users receive the dispatch (producer-computed audience).
    audience: Option<HashSet<UserId>>,
}

/// Unix milliseconds as an ISO 8601 UTC timestamp (`2026-01-02T03:04:05.678000+00:00`).
pub(crate) fn iso_utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let millis = ms.rem_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant's civil-from-days algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{millis:03}000+00:00",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// A dispatch whose `d` is the event's ready-made Discord payload object.
fn passthrough(name: &'static str, payload: &Value, channel: Option<ChannelId>) -> Dispatch {
    Dispatch {
        name,
        data: payload.clone(),
        channel,
        audience: None,
    }
}

/// Guild-level permission a recipient must hold for `name`, on top of the intent
/// check. These dispatches describe moderation and configuration state that
/// ordinary members must not see.
pub fn required_guild_permission(name: &str) -> Option<Permissions> {
    match name {
        "INVITE_CREATE" | "INVITE_DELETE" => Some(Permissions::MANAGE_CHANNELS),
        "AUTO_MODERATION_RULE_CREATE"
        | "AUTO_MODERATION_RULE_UPDATE"
        | "AUTO_MODERATION_RULE_DELETE"
        | "AUTO_MODERATION_ACTION_EXECUTION" => Some(Permissions::MANAGE_GUILD),
        "GUILD_AUDIT_LOG_ENTRY_CREATE" => Some(Permissions::VIEW_AUDIT_LOG),
        _ => None,
    }
}

/// Dispatches of an age-restricted channel that sessions without the 18+ answer do not
/// receive (D-199).
fn age_gated(name: &str) -> bool {
    name.starts_with("MESSAGE_")
        || name.starts_with("THREAD_")
        || matches!(name, "TYPING_START" | "CHANNEL_PINS_UPDATE")
}

/// Per-session view of a dispatch. Without `MESSAGE_CONTENT` a bot sees message
/// content, embeds, attachments, components and polls only for DMs, messages that
/// mention it, and its own messages (Discord rule, docs/05 section 6.2).
fn redact_for(session: &Session, name: &str, data: &Value) -> Option<Value> {
    if !matches!(name, "MESSAGE_CREATE" | "MESSAGE_UPDATE")
        || session.intents & intents::MESSAGE_CONTENT != 0
    {
        return None;
    }
    let me = session.user.to_string();
    let is_dm = data.get("guild_id").is_none_or(Value::is_null);
    let own = data.pointer("/author/id").and_then(Value::as_str) == Some(me.as_str());
    let mentioned = data
        .get("mentions")
        .and_then(Value::as_array)
        .is_some_and(|m| {
            m.iter()
                .any(|u| u.get("id").and_then(Value::as_str) == Some(me.as_str()))
        });
    if is_dm || own || mentioned {
        return None;
    }
    let mut v = data.clone();
    if let Some(o) = v.as_object_mut() {
        o.insert("content".into(), json!(""));
        o.remove("message_snapshots");
        o.remove("sticker_items");
        for k in ["embeds", "attachments", "components"] {
            o.insert(k.into(), json!([]));
        }
        o.remove("poll");
    }
    Some(v)
}

/// Deliver `data` to one session, redacted for its intents.
fn deliver_to(session: &Session, name: &str, data: &Value) {
    match redact_for(session, name, data) {
        Some(v) => session.deliver(name, v),
        None => session.deliver(name, data.clone()),
    }
}

fn to_dispatch(ev: &DomainEvent) -> Option<Dispatch> {
    Some(match ev {
        // The message family is full-payload (Discord shaped), built by the producer.
        DomainEvent::MessageSent { channel, data, .. } => Dispatch {
            name: "MESSAGE_CREATE",
            data: data.clone(),
            channel: Some(*channel),
            audience: None,
        },
        // A moderator restored a deleted message (N-21): clients get the full object again, marked so they do
        // not treat it as new (no mention bump, no notification; D-338). Only the dispatch carries the flag; the
        // producer's payload (search, audit) stays the plain object.
        DomainEvent::MessageRestored { channel, data, .. } => {
            let mut data = data.clone();
            if let Some(o) = data.as_object_mut() {
                o.insert("restored".into(), Value::Bool(true));
            }
            Dispatch {
                name: "MESSAGE_CREATE",
                data,
                channel: Some(*channel),
                audience: None,
            }
        }
        DomainEvent::MessageEdited { channel, data, .. } => Dispatch {
            name: "MESSAGE_UPDATE",
            data: data.clone(),
            channel: Some(*channel),
            audience: None,
        },
        DomainEvent::MessageDeleted {
            guild,
            channel,
            message_id,
        } => Dispatch {
            name: "MESSAGE_DELETE",
            data: json!({
                "id": message_id.to_string(), "channel_id": channel, "guild_id": guild,
            }),
            channel: Some(*channel),
            audience: None,
        },
        DomainEvent::MessagesBulkDeleted {
            guild,
            channel,
            message_ids,
        } => Dispatch {
            name: "MESSAGE_DELETE_BULK",
            data: json!({
                "ids": message_ids.iter().map(u64::to_string).collect::<Vec<_>>(),
                "channel_id": channel, "guild_id": guild,
            }),
            channel: Some(*channel),
            audience: None,
        },
        DomainEvent::ReactionAdded {
            guild,
            channel,
            message_id,
            user,
            emoji,
        } => Dispatch {
            name: "MESSAGE_REACTION_ADD",
            data: json!({
                "user_id": user, "channel_id": channel, "message_id": message_id.to_string(),
                "guild_id": guild, "emoji": emoji, "burst": false,
            }),
            channel: Some(*channel),
            audience: None,
        },
        DomainEvent::ReactionRemoved {
            guild,
            channel,
            message_id,
            user,
            emoji,
        } => Dispatch {
            name: "MESSAGE_REACTION_REMOVE",
            data: json!({
                "user_id": user, "channel_id": channel, "message_id": message_id.to_string(),
                "guild_id": guild, "emoji": emoji, "burst": false,
            }),
            channel: Some(*channel),
            audience: None,
        },
        DomainEvent::ReactionsCleared {
            guild,
            channel,
            message_id,
        } => Dispatch {
            name: "MESSAGE_REACTION_REMOVE_ALL",
            data: json!({
                "channel_id": channel, "message_id": message_id.to_string(), "guild_id": guild,
            }),
            channel: Some(*channel),
            audience: None,
        },
        DomainEvent::ReactionEmojiCleared {
            guild,
            channel,
            message_id,
            emoji,
        } => Dispatch {
            name: "MESSAGE_REACTION_REMOVE_EMOJI",
            data: json!({
                "channel_id": channel, "message_id": message_id.to_string(),
                "guild_id": guild, "emoji": emoji,
            }),
            channel: Some(*channel),
            audience: None,
        },
        DomainEvent::PollVoteAdded {
            guild,
            channel,
            message_id,
            user,
            answer_id,
        } => Dispatch {
            name: "MESSAGE_POLL_VOTE_ADD",
            data: json!({
                "user_id": user, "channel_id": channel, "message_id": message_id.to_string(),
                "guild_id": guild, "answer_id": answer_id,
            }),
            channel: Some(*channel),
            audience: None,
        },
        DomainEvent::PollVoteRemoved {
            guild,
            channel,
            message_id,
            user,
            answer_id,
        } => Dispatch {
            name: "MESSAGE_POLL_VOTE_REMOVE",
            data: json!({
                "user_id": user, "channel_id": channel, "message_id": message_id.to_string(),
                "guild_id": guild, "answer_id": answer_id,
            }),
            channel: Some(*channel),
            audience: None,
        },
        DomainEvent::ToolRoomUpdated {
            channel, payload, ..
        } => passthrough("TOOL_ROOM_UPDATE", payload, Some(*channel)),
        DomainEvent::ChannelPinsUpdated {
            guild,
            channel,
            last_pin_ms,
        } => Dispatch {
            name: "CHANNEL_PINS_UPDATE",
            data: json!({
                "channel_id": channel, "guild_id": guild,
                "last_pin_timestamp": last_pin_ms.map(iso_utc),
            }),
            channel: Some(*channel),
            audience: None,
        },
        DomainEvent::TypingStarted {
            guild,
            channel,
            user,
            timestamp,
        } => Dispatch {
            name: "TYPING_START",
            data: json!({
                "channel_id": channel, "guild_id": guild, "user_id": user, "timestamp": timestamp,
            }),
            channel: Some(*channel),
            audience: None,
        },
        DomainEvent::ThreadCreated {
            thread, payload, ..
        } => passthrough("THREAD_CREATE", payload, Some(*thread)),
        DomainEvent::ThreadUpdated {
            thread, payload, ..
        } => passthrough("THREAD_UPDATE", payload, Some(*thread)),
        // The thread is gone, so the producer carries who could see it.
        DomainEvent::ThreadDeleted {
            payload, viewers, ..
        } => Dispatch {
            audience: Some(viewers.iter().copied().collect()),
            ..passthrough("THREAD_DELETE", payload, None)
        },
        DomainEvent::ThreadMembersUpdated {
            thread, payload, ..
        } => passthrough("THREAD_MEMBERS_UPDATE", payload, Some(*thread)),
        DomainEvent::ThreadMemberUpdated { payload, .. } => {
            passthrough("THREAD_MEMBER_UPDATE", payload, None)
        }
        DomainEvent::MemberJoined { payload, .. } => passthrough("GUILD_MEMBER_ADD", payload, None),
        DomainEvent::MemberLeft { guild, user } => Dispatch {
            name: "GUILD_MEMBER_REMOVE",
            data: json!({ "guild_id": guild, "user": { "id": user } }),
            channel: None,
            audience: None,
        },
        DomainEvent::ChannelCreated {
            channel, payload, ..
        } => passthrough("CHANNEL_CREATE", payload, Some(*channel)),
        DomainEvent::ChannelUpdated {
            channel, payload, ..
        } => passthrough("CHANNEL_UPDATE", payload, Some(*channel)),
        // The channel no longer exists, so it cannot be permission-filtered here; the
        // producer carries the set of members who could see it (open question 159).
        DomainEvent::ChannelDeleted {
            payload, viewers, ..
        } => Dispatch {
            audience: Some(viewers.iter().copied().collect()),
            ..passthrough("CHANNEL_DELETE", payload, None)
        },
        DomainEvent::GuildUpdated { payload, .. } => passthrough("GUILD_UPDATE", payload, None),
        DomainEvent::RoleCreated { payload, .. } => passthrough("GUILD_ROLE_CREATE", payload, None),
        DomainEvent::RoleUpdated { payload, .. } => passthrough("GUILD_ROLE_UPDATE", payload, None),
        DomainEvent::RoleDeleted { payload, .. } => passthrough("GUILD_ROLE_DELETE", payload, None),
        DomainEvent::MemberUpdated { payload, .. } => {
            passthrough("GUILD_MEMBER_UPDATE", payload, None)
        }
        DomainEvent::MemberBanned { payload, .. } => passthrough("GUILD_BAN_ADD", payload, None),
        DomainEvent::MemberUnbanned { payload, .. } => {
            passthrough("GUILD_BAN_REMOVE", payload, None)
        }
        DomainEvent::InviteCreated {
            channel, payload, ..
        } => passthrough("INVITE_CREATE", payload, Some(*channel)),
        DomainEvent::InviteDeleted {
            channel, payload, ..
        } => passthrough("INVITE_DELETE", payload, Some(*channel)),
        DomainEvent::EmojisUpdated { payload, .. } => {
            passthrough("GUILD_EMOJIS_UPDATE", payload, None)
        }
        DomainEvent::ListeningRoomUpdated {
            channel, payload, ..
        } => passthrough("LISTENING_ROOM_UPDATE", payload, Some(*channel)),
        DomainEvent::StorageMountUpdated { payload, .. } => {
            passthrough("STORAGE_MOUNT_UPDATE", payload, None)
        }
        DomainEvent::GuildAppearanceUpdated { payload, .. } => {
            passthrough("SR_GUILD_APPEARANCE_UPDATE", payload, None)
        }
        DomainEvent::GuildEntitlementUpdated { payload, .. } => {
            passthrough("SR_ENTITLEMENT_UPDATE", payload, None)
        }
        DomainEvent::StickersUpdated { payload, .. } => {
            passthrough("GUILD_STICKERS_UPDATE", payload, None)
        }
        DomainEvent::SoundboardUpdated { payload, .. } => {
            passthrough("GUILD_SOUNDBOARD_SOUNDS_UPDATE", payload, None)
        }
        DomainEvent::ScheduledEvent { kind, payload, .. } => {
            passthrough(kind.dispatch_name(), payload, None)
        }
        DomainEvent::WebhooksUpdated { guild, channel } => Dispatch {
            name: "WEBHOOKS_UPDATE",
            data: json!({ "guild_id": guild, "channel_id": channel }),
            channel: Some(*channel),
            audience: None,
        },
        DomainEvent::AutoModRuleCreated { payload, .. } => {
            passthrough("AUTO_MODERATION_RULE_CREATE", payload, None)
        }
        DomainEvent::AutoModRuleUpdated { payload, .. } => {
            passthrough("AUTO_MODERATION_RULE_UPDATE", payload, None)
        }
        DomainEvent::AutoModRuleDeleted { payload, .. } => {
            passthrough("AUTO_MODERATION_RULE_DELETE", payload, None)
        }
        DomainEvent::AutoModActionExecuted { payload, .. } => {
            passthrough("AUTO_MODERATION_ACTION_EXECUTION", payload, None)
        }
        DomainEvent::AuditLogEntryCreated { payload, .. } => {
            passthrough("GUILD_AUDIT_LOG_ENTRY_CREATE", payload, None)
        }
        DomainEvent::UserUpdated { payload, .. } => passthrough("USER_UPDATE", payload, None),
        // Handled as subscription lifecycle by the platform pump.
        DomainEvent::GuildCreated { .. }
        | DomainEvent::GuildDeleted { .. }
        | DomainEvent::FeatureSettingsChanged { .. }
        // Internal voice presence for feature modules; clients get VOICE_STATE_UPDATE.
        | DomainEvent::VoiceJoined { .. }
        | DomainEvent::VoiceLeft { .. }
        | DomainEvent::StaffAlert { .. }
        | DomainEvent::StaffFeed { .. }
        | DomainEvent::PlatformBroadcast { .. }
        | DomainEvent::ReviewCard { .. }
        | DomainEvent::PackageCard { .. }
        // Merged with the live sessions by `Gateway::on_user_event`.
        | DomainEvent::PresenceChanged { .. }
        | DomainEvent::UserScoped { .. } => return None,
    })
}

impl Gateway {
    pub fn new(
        cfg: GatewayConfig,
        auth: Arc<dyn Authenticator>,
        dir: Arc<dyn GuildDirectory>,
        bus: Arc<dyn EventBus>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                cfg,
                auth,
                dir,
                bus,
                state: Mutex::new(State::default()),
                next_conn: AtomicU64::new(1),
                seen: Mutex::new(HashSet::new()),
            }),
        }
    }

    pub fn config(&self) -> &GatewayConfig {
        &self.inner.cfg
    }

    pub(crate) fn st(&self) -> MutexGuard<'_, State> {
        self.inner.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Spawn the platform-topic pump (guild lifecycle) and the expiry sweeper.
    /// Call once from within a tokio runtime.
    pub fn start(&self) {
        let weak = Arc::downgrade(&self.inner);
        let mut platform = self.inner.bus.subscribe(Topic::Platform);
        let pump = tokio::spawn(async move {
            while let Some(ev) = platform.next().await {
                let Some(inner) = weak.upgrade() else { break };
                Gateway { inner }.handle_platform(ev).await;
            }
        });
        let weak = Arc::downgrade(&self.inner);
        let period = self.inner.cfg.sweep_interval;
        let sweeper = tokio::spawn(async move {
            loop {
                tokio::time::sleep(period).await;
                let Some(inner) = weak.upgrade() else { break };
                Gateway { inner }.sweep().await;
            }
        });
        self.st().background.extend([pump, sweeper]);
    }

    /// Abort every background task.
    pub fn shutdown(&self) {
        let mut st = self.st();
        for h in st.background.drain(..) {
            h.abort();
        }
        for (_, h) in st.guild_tasks.drain() {
            h.abort();
        }
        for (_, h) in st.user_tasks.drain() {
            h.abort();
        }
    }

    pub(crate) fn next_conn_id(&self) -> u64 {
        self.inner.next_conn.fetch_add(1, Ordering::Relaxed)
    }

    /// Note that `user` was seen now (IDENTIFY, RESUME, or the 10-minute refresh of a
    /// live session). The embedder writes the batch with [`Gateway::drain_seen`].
    pub(crate) fn mark_seen(&self, user: UserId) {
        self.inner
            .seen
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(user);
    }

    /// Take the users seen since the last call, for one batched `last_seen_at` write.
    pub fn drain_seen(&self) -> Vec<UserId> {
        let mut seen = self.inner.seen.lock().unwrap_or_else(|p| p.into_inner());
        seen.drain().collect()
    }

    pub fn session_count(&self) -> usize {
        self.st().sessions.len()
    }

    // ---- session registry -------------------------------------------------

    fn new_session_id() -> String {
        let mut b = [0u8; 16];
        // A failure here is an OS RNG failure; fall back to a time-derived id
        // (resume also requires a valid token for the same user).
        if getrandom::fill(&mut b).is_err() {
            let n = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            b.copy_from_slice(&n.to_le_bytes());
        }
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    /// Create a session wired to `tx`, deliver READY and one GUILD_CREATE per
    /// guild (in that order), then register for fan-out.
    pub(crate) async fn create_session(
        &self,
        user: &AuthedUser,
        intents: u64,
        conn_id: u64,
        tx: mpsc::Sender<Out>,
        shard: Option<(u32, u32)>,
    ) -> Arc<Session> {
        let cfg = &self.inner.cfg;
        let mut guilds = self.inner.dir.guilds_for_user(user.user_id).await;
        // A shard is only told about the servers it owns (DEV-BOT-002).
        if let Some((id, n)) = shard {
            guilds.retain(|g| (g.get() >> 22) % u64::from(n.max(1)) == u64::from(id));
        }
        // A few guilds at a time, in order (RDY-PERF-002: one after the other, READY of a member of 60
        // servers waited for every GUILD_CREATE to be read in turn).
        let built: Vec<(Value, Option<Value>)> = futures::stream::iter(guilds.iter().copied())
            .map(|g| async move {
                let body = self.guild_create_body(user.user_id, g).await;
                let sync = self.inner.dir.thread_list_sync(user.user_id, g).await;
                (body, sync)
            })
            .buffered(GUILD_CREATE_CONCURRENCY)
            .collect()
            .await;
        let (payloads, syncs): (Vec<Value>, Vec<Option<Value>>) = built.into_iter().unzip();
        let extras = self.inner.dir.ready_extras(user.user_id).await;
        let mut fresh = Session::new(
            Self::new_session_id(),
            user.user_id,
            intents,
            cfg.resume_window,
            cfg.resume_buffer,
            conn_id,
            tx,
        );
        fresh.shard = shard;
        fresh.bot = user.bot;
        let session = Arc::new(fresh);
        let mut sessions: Vec<Value> = self
            .user_sessions(user.user_id)
            .iter()
            .map(|s| session_entry(s))
            .collect();
        sessions.push(session_entry(&session));
        let mut ready = json!({
            "v": 1,
            "user": user.user,
            "guilds": guilds.iter().map(|g| json!({"id": g, "unavailable": true})).collect::<Vec<_>>(),
            "session_id": session.id,
            "shard": shard.map(|(i, n)| json!([i, n])),
            "sessions": sessions,
            "resume_gateway_url": cfg.resume_gateway_url,
        });
        if let (Some(r), Some(e)) = (ready.as_object_mut(), extras.as_object()) {
            for (k, v) in e {
                r.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        session.deliver("READY", ready);
        for (p, sync) in payloads.into_iter().zip(syncs) {
            session.deliver("GUILD_CREATE", p);
            if let Some(s) = sync
                && session.wants("THREAD_LIST_SYNC")
            {
                session.deliver("THREAD_LIST_SYNC", s);
            }
        }
        for g in &guilds {
            session.add_guild(*g);
        }
        {
            let mut st = self.st();
            st.sessions.insert(session.id.clone(), session.clone());
            for g in &guilds {
                st.by_guild
                    .entry(*g)
                    .or_default()
                    .insert(session.id.clone());
                self.ensure_guild_task(&mut st, *g);
            }
            self.ensure_user_task(&mut st, user.user_id);
        }
        self.refresh_presence(user.user_id, false).await;
        session
    }

    /// GUILD_CREATE body: the directory's payload plus initial `presences` when the
    /// guild is below `large_threshold` (Q53).
    async fn guild_create_body(&self, user: UserId, guild: GuildId) -> Value {
        let mut body = self.inner.dir.guild_payload(user, guild).await;
        let voice_states = self.voice_states_for(user, guild).await;
        let stages: Vec<Value> = self
            .stage_instances_for(user, guild)
            .await
            .iter()
            .map(crate::voice_stage::StageInstance::to_json)
            .collect();
        let sounds = self.inner.dir.soundboard_sounds(guild).await;
        // Voice channel statuses ride on the channel objects, as in Discord.
        let statuses = self.voice_statuses_for(user, guild).await;
        if !statuses.is_empty()
            && let Some(channels) = body.get_mut("channels").and_then(Value::as_array_mut)
        {
            for ch in channels {
                let id = ch.get("id").and_then(Value::as_str).map(str::to_owned);
                let found = statuses
                    .iter()
                    .find(|s| id.as_deref() == Some(s.channel_id.to_string().as_str()));
                if let (Some(s), Some(o)) = (found, ch.as_object_mut()) {
                    o.insert("status".into(), Value::String(s.text.clone()));
                }
            }
        }
        if let Some(obj) = body.as_object_mut() {
            obj.insert("voice_states".into(), Value::Array(voice_states));
            obj.insert("stage_instances".into(), Value::Array(stages));
            obj.insert("soundboard_sounds".into(), Value::Array(sounds));
        }
        let threshold = self.inner.cfg.large_threshold.min(LARGE_THRESHOLD_MAX);
        let members: Vec<UserId> = match body.get("members").and_then(Value::as_array) {
            Some(m) if (m.len() as u64) < threshold => m
                .iter()
                .filter_map(|m| m.pointer("/user/id")?.as_str()?.parse::<u64>().ok())
                .map(UserId::new)
                .collect(),
            _ => return body,
        };
        let presences: Vec<Value> = {
            let st = self.st();
            members
                .into_iter()
                .filter_map(|u| {
                    let p = Self::effective(&st, u)?;
                    (p.visible_status() != "offline").then(|| presence_body(u, guild, &p))
                })
                .collect()
        };
        if let Some(obj) = body.as_object_mut() {
            obj.insert("presences".into(), Value::Array(presences));
        }
        body
    }

    /// Look up a resumable session for `user`.
    pub(crate) fn find_resumable(&self, id: &str, user: UserId) -> Option<Arc<Session>> {
        let s = self.st().sessions.get(id).cloned()?;
        (s.user == user && !s.is_expired()).then_some(s)
    }

    fn ensure_guild_task(&self, st: &mut State, g: GuildId) {
        if st.guild_tasks.contains_key(&g) {
            return;
        }
        let mut stream = self.inner.bus.subscribe(Topic::Guild(g));
        let weak: Weak<Inner> = Arc::downgrade(&self.inner);
        st.guild_tasks.insert(
            g,
            tokio::spawn(async move {
                while let Some(ev) = stream.next().await {
                    let Some(inner) = weak.upgrade() else { break };
                    Gateway { inner }.on_guild_event(g, ev).await;
                }
            }),
        );
    }

    /// One `Topic::User` subscription per connected user: DM events and self events.
    fn ensure_user_task(&self, st: &mut State, user: UserId) {
        if st.user_tasks.contains_key(&user) {
            return;
        }
        let mut stream = self.inner.bus.subscribe(Topic::User(user));
        let weak: Weak<Inner> = Arc::downgrade(&self.inner);
        st.user_tasks.insert(
            user,
            tokio::spawn(async move {
                while let Some(ev) = stream.next().await {
                    let Some(inner) = weak.upgrade() else { break };
                    Gateway { inner }.on_user_event(user, ev).await;
                }
            }),
        );
    }

    async fn on_guild_event(&self, guild: GuildId, ev: DomainEvent) {
        self.note_typing(&ev);
        if crate::lists::changes_members(&ev) {
            self.schedule_list_refresh(guild, true);
        }
        self.voice_on_guild_event(guild, &ev).await;
        if let Some(mut d) = to_dispatch(&ev) {
            // The Discord bridge banner rides on the channel object (BRG-COR-001).
            if matches!(d.name, "CHANNEL_CREATE" | "CHANNEL_UPDATE")
                && let Some(ch) = d.channel
                && d.data.get("sr_bridge").is_none()
                && let Some(state) = self.inner.dir.channel_bridge_state(ch).await
                && let Some(o) = d.data.as_object_mut()
            {
                o.insert("sr_bridge".into(), state);
            }
            // The changed user's own sessions hear about it as a self event.
            if let DomainEvent::UserUpdated { user, .. } = &ev {
                d.audience = Some(self.guild_users(guild, Some(*user)));
            }
            if let DomainEvent::ThreadMembersUpdated { also, .. } = &ev {
                for u in also {
                    self.dispatch_user(*u, d.name, d.data.clone()).await;
                }
            }
            self.dispatch_guild(guild, d.name, d.data, d.channel, d.audience.as_ref())
                .await;
        }
    }

    /// Every session subscribed to `guild`.
    pub(crate) fn guild_sessions(&self, guild: GuildId) -> Vec<Arc<Session>> {
        let st = self.st();
        st.by_guild
            .get(&guild)
            .into_iter()
            .flatten()
            .filter_map(|id| st.sessions.get(id).cloned())
            .collect()
    }

    /// Users with a session in `guild`, optionally without `except`.
    fn guild_users(&self, guild: GuildId, except: Option<UserId>) -> HashSet<UserId> {
        let st = self.st();
        st.by_guild
            .get(&guild)
            .into_iter()
            .flatten()
            .filter_map(|id| st.sessions.get(id))
            .map(|s| s.user)
            .filter(|u| Some(*u) != except)
            .collect()
    }

    async fn on_user_event(&self, user: UserId, ev: DomainEvent) {
        self.note_typing(&ev);
        if let DomainEvent::PresenceChanged {
            user: changed,
            status,
            custom_status,
        } = &ev
        {
            let update = PresenceUpdate {
                status: status.clone(),
                activities: custom_status.iter().cloned().collect(),
                ..PresenceUpdate::default()
            };
            self.set_presence(*changed, update).await;
            return;
        }
        if let DomainEvent::UserScoped { name, payload, .. } = &ev {
            if name == "SR_USER_UPDATE" {
                for s in self.user_sessions(user) {
                    s.clear_age_cache();
                    s.clear_safety_cache();
                }
            } else if name.starts_with("RELATIONSHIP_") {
                for s in self.user_sessions(user) {
                    s.clear_safety_cache();
                }
            }
            self.dispatch_user(user, name, payload.clone()).await;
            return;
        }
        if let Some(d) = to_dispatch(&ev) {
            self.dispatch_user(user, d.name, d.data).await;
        }
    }

    /// Typing is recorded from the bus event, so REST and gateway paths converge.
    fn note_typing(&self, ev: &DomainEvent) {
        if let DomainEvent::TypingStarted { channel, user, .. } = ev {
            self.st().typing.insert((*channel, *user), Instant::now());
        }
    }

    fn drop_guild_if_empty(st: &mut State, g: GuildId) {
        if st.by_guild.get(&g).is_some_and(HashSet::is_empty) {
            st.by_guild.remove(&g);
            if let Some(h) = st.guild_tasks.remove(&g) {
                h.abort();
            }
        }
    }

    fn user_sessions(&self, user: UserId) -> Vec<Arc<Session>> {
        self.st()
            .sessions
            .values()
            .filter(|s| s.user == user)
            .cloned()
            .collect()
    }

    /// Remove a session and update presence; used by expiry.
    async fn remove_session(&self, s: &Arc<Session>) {
        {
            let mut st = self.st();
            st.sessions.remove(&s.id);
            Self::drop_list_subs(&mut st, &s.id, None);
            for g in s.guilds() {
                if let Some(set) = st.by_guild.get_mut(&g) {
                    set.remove(&s.id);
                }
                Self::drop_guild_if_empty(&mut st, g);
            }
            st.session_presence.remove(&s.id);
            if !st.sessions.values().any(|o| o.user == s.user) {
                st.presence.remove(&s.user);
                if let Some(h) = st.user_tasks.remove(&s.user) {
                    h.abort();
                }
            }
        }
        // Expiry only follows a detach, so the user was already marked offline.
        self.refresh_presence(s.user, false).await;
    }

    /// Drop sessions whose resume window elapsed and prune typing state.
    pub async fn sweep(&self) {
        let expired: Vec<_> = self
            .st()
            .sessions
            .values()
            .filter(|s| s.is_expired())
            .cloned()
            .collect();
        for s in expired {
            self.remove_session(&s).await;
        }
        self.voice_sweep().await;
        let ttl = self.inner.cfg.typing_ttl;
        let now = Instant::now();
        self.st()
            .typing
            .retain(|_, at| now.duration_since(*at) <= ttl);
    }

    /// Ask every live client to reconnect (op 7). Sessions stay resumable.
    pub fn request_reconnect(&self) {
        let all: Vec<_> = self.st().sessions.values().cloned().collect();
        for s in all {
            s.send_reconnect();
        }
    }

    // ---- fan-out ----------------------------------------------------------

    /// Deliver to every session subscribed to `guild` that passes the intent
    /// check and, when `channel` is set, may view that channel.
    pub(crate) async fn dispatch_guild(
        &self,
        guild: GuildId,
        name: &str,
        data: Value,
        channel: Option<ChannelId>,
        audience: Option<&HashSet<UserId>>,
    ) {
        let targets: Vec<Arc<Session>> = {
            let st = self.st();
            st.by_guild
                .get(&guild)
                .into_iter()
                .flatten()
                .filter_map(|id| st.sessions.get(id).cloned())
                .filter(|s| s.wants(name))
                .filter(|s| audience.is_none_or(|a| a.contains(&s.user)))
                .collect()
        };
        let needs = required_guild_permission(name);
        let users: HashSet<UserId> = targets.iter().map(|s| s.user).collect();
        // One directory call for the whole audience, not one per recipient (RDY-PERF-002).
        let allowed: HashSet<UserId> = if channel.is_none() && needs.is_none() {
            users
        } else {
            let users: Vec<UserId> = users.into_iter().collect();
            self.inner
                .dir
                .dispatch_audience(guild, channel, needs, &users)
                .await
        };
        let gate = match channel {
            Some(ch) if age_gated(name) => self.inner.dir.channel_age_restricted(ch).await,
            _ => false,
        };
        for s in targets {
            if !allowed.contains(&s.user) {
                continue;
            }
            if gate && !self.session_age_confirmed(&s).await {
                continue;
            }
            self.deliver_filtered(&s, name, &data, gate).await;
        }
    }

    /// [`deliver_to`] after the viewer's sensitive-media settings (D-230): the same
    /// `sr_chat::sensitive` function the REST decorators call. `restricted`: the channel is
    /// age-restricted.
    async fn deliver_filtered(&self, s: &Session, name: &str, data: &Value, restricted: bool) {
        // PAR-FU-002: the likely-spam flag is the recipient's view; the author's sessions get the payload without it.
        let masked;
        let data = if matches!(name, "MESSAGE_CREATE" | "MESSAGE_UPDATE")
            && data
                .get("flags")
                .and_then(Value::as_u64)
                .is_some_and(|f| f & u64::from(sr_chat::message_flags::LIKELY_SPAM) != 0)
            && data
                .pointer("/author/id")
                .and_then(Value::as_str)
                .and_then(|a| a.parse::<u64>().ok())
                == Some(s.user.get())
        {
            let mut v = data.clone();
            if let Some(f) = v.get("flags").and_then(Value::as_u64) {
                v["flags"] = Value::from(f & !u64::from(sr_chat::message_flags::LIKELY_SPAM));
            }
            masked = v;
            &masked
        } else {
            data
        };
        let has_files = matches!(name, "MESSAGE_CREATE" | "MESSAGE_UPDATE")
            && data
                .get("attachments")
                .and_then(Value::as_array)
                .is_some_and(|a| !a.is_empty());
        if !has_files {
            return deliver_to(s, name, data);
        }
        let view = match s.safety_cached() {
            Some(v) => v,
            None => {
                let looked = self.inner.dir.sensitive_view(s.user).await;
                s.set_safety_cached(looked)
            }
        };
        let Some((prefs, friends)) = view.as_ref() else {
            return deliver_to(s, name, data);
        };
        let author = data
            .pointer("/author/id")
            .and_then(Value::as_str)
            .and_then(|a| a.parse::<u64>().ok())
            .map(UserId::new);
        let bot = data
            .pointer("/author/bot")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let relation = match author {
            Some(a) if a == s.user => sr_chat::sensitive::Relation::Exempt,
            _ if bot => sr_chat::sensitive::Relation::Exempt,
            Some(a) if friends.contains(&a) => sr_chat::sensitive::Relation::Friend,
            _ => sr_chat::sensitive::Relation::Other,
        };
        let ctx = sr_chat::sensitive::ChannelCtx {
            dm: data.get("guild_id").is_none_or(Value::is_null),
            age_restricted: restricted,
        };
        let mut v = data.clone();
        if sr_chat::sensitive::apply_to_message(prefs, relation, &ctx, &mut v) {
            deliver_to(s, name, &v);
        } else {
            deliver_to(s, name, data);
        }
    }

    /// The session age-gate answer, looked up once and cached until `SR_USER_UPDATE`.
    async fn session_age_confirmed(&self, s: &Session) -> bool {
        if let Some(v) = s.age_cached() {
            return v;
        }
        let v = self.inner.dir.age_confirmed(s.user).await;
        s.set_age_cached(v);
        v
    }

    /// Deliver to every session of `user` (DM and self events; no guild check).
    pub(crate) async fn dispatch_user(&self, user: UserId, name: &str, data: Value) {
        for s in self.user_sessions(user) {
            if s.wants(name) && s.owns_user_event(&data) {
                self.deliver_filtered(&s, name, &data, false).await;
            }
        }
    }

    /// Deliver to every session (`SR_FEATURE_UPDATE` and the like).
    pub(crate) fn dispatch_all(&self, name: &str, data: &Value) {
        let sessions: Vec<Arc<Session>> = self.st().sessions.values().cloned().collect();
        for s in sessions {
            if s.wants(name) {
                deliver_to(&s, name, data);
            }
        }
    }

    /// Guild lifecycle events from the platform topic.
    async fn handle_platform(&self, ev: DomainEvent) {
        match ev {
            DomainEvent::GuildCreated { guild, owner: user }
            | DomainEvent::MemberJoined { guild, user, .. } => self.join_user(user, guild).await,
            DomainEvent::MemberLeft { guild, user } => {
                self.leave_user(user, guild);
                self.voice_member_left(guild, user).await;
            }
            DomainEvent::GuildDeleted { guild } => {
                self.drop_guild(guild);
                self.voice_guild_deleted(guild).await;
            }
            DomainEvent::PlatformBroadcast { name, payload } => {
                self.dispatch_all(&name, &payload);
            }
            DomainEvent::StaffAlert { kind, detail } => {
                let body = json!({ "kind": kind.as_str(), "detail": detail });
                for user in self.inner.dir.staff_users().await {
                    self.dispatch_user(user, "SR_STAFF_ALERT", body.clone())
                        .await;
                }
            }
            _ => {}
        }
    }

    async fn join_user(&self, user: UserId, guild: GuildId) {
        let sessions = self.user_sessions(user);
        if sessions.is_empty() {
            return;
        }
        let payload = self.guild_create_body(user, guild).await;
        let sync = self.inner.dir.thread_list_sync(user, guild).await;
        let mut st = self.st();
        for s in sessions {
            if s.owns_guild(guild) && s.add_guild(guild) {
                s.deliver("GUILD_CREATE", payload.clone());
                if let Some(sy) = &sync
                    && s.wants("THREAD_LIST_SYNC")
                {
                    s.deliver("THREAD_LIST_SYNC", sy.clone());
                }
                st.by_guild.entry(guild).or_default().insert(s.id.clone());
            }
        }
        if st.by_guild.contains_key(&guild) {
            self.ensure_guild_task(&mut st, guild);
        }
    }

    fn leave_user(&self, user: UserId, guild: GuildId) {
        let sessions = self.user_sessions(user);
        let mut st = self.st();
        for s in sessions {
            if s.remove_guild(guild) {
                s.deliver("GUILD_DELETE", json!({ "id": guild }));
                Self::drop_list_subs(&mut st, &s.id, Some(guild));
                if let Some(set) = st.by_guild.get_mut(&guild) {
                    set.remove(&s.id);
                }
            }
        }
        Self::drop_guild_if_empty(&mut st, guild);
    }

    fn drop_guild(&self, guild: GuildId) {
        let mut st = self.st();
        let ids = st.by_guild.remove(&guild).unwrap_or_default();
        st.lists.retain(|k, _| k.0 != guild);
        for id in ids {
            if let Some(s) = st.sessions.get(&id) {
                s.remove_guild(guild);
                s.deliver("GUILD_DELETE", json!({ "id": guild }));
            }
        }
        if let Some(h) = st.guild_tasks.remove(&guild) {
            h.abort();
        }
    }

    // ---- presence & typing (memory only) -----------------------------------

    /// What other users see for `user`: `None` without any live or resumable session,
    /// `offline` without an attached socket, otherwise the chosen status.
    /// Whether `user` has an attached session that is not idle (D-191). Web Push is held
    /// back while this is true. A session that never reported counts as active; one idle
    /// session does not make the user idle while another is active.
    pub fn has_active_session(&self, user: UserId) -> bool {
        let st = self.st();
        st.sessions
            .values()
            .filter(|s| s.user == user && s.is_attached())
            .any(|s| {
                st.session_presence
                    .get(&s.id)
                    .and_then(|sp| sp.update.as_ref())
                    .is_none_or(|u| !(u.afk || u.status == "idle"))
            })
    }

    pub fn presence(&self, user: UserId) -> Option<PresenceUpdate> {
        Self::effective(&self.st(), user)
    }

    /// API entry point (`PATCH /users/@me/presence`): store the user's chosen status and
    /// custom status and broadcast the merged presence while online.
    pub async fn set_presence(&self, user: UserId, mut update: PresenceUpdate) {
        update.activities = sanitize_activities(&update.activities);
        self.st().presence.insert(user, update);
        self.refresh_presence(user, true).await;
    }

    /// Op 3 / IDENTIFY: record what one session reports (status, `afk`, activities) and
    /// broadcast the presence merged over all of the user's sessions.
    pub(crate) async fn set_session_presence(&self, session: &Session, mut update: PresenceUpdate) {
        update.activities = sanitize_activities(&update.activities);
        {
            let mut st = self.st();
            st.presence_seq += 1;
            let order = st.presence_seq;
            let entry = st
                .session_presence
                .entry(session.id.clone())
                .or_insert(SessionPresence {
                    platform: "web",
                    update: None,
                    order,
                });
            entry.update = Some(update);
            entry.order = order;
        }
        self.refresh_presence(session.user, true).await;
    }

    /// Note the client platform of a new session (from IDENTIFY `properties`).
    pub(crate) fn set_session_platform(&self, session: &Session, platform: &'static str) {
        let mut st = self.st();
        st.session_presence
            .entry(session.id.clone())
            .or_insert(SessionPresence {
                platform,
                update: None,
                order: 0,
            })
            .platform = platform;
    }

    pub(crate) fn effective(st: &State, user: UserId) -> Option<PresenceUpdate> {
        if !st.sessions.values().any(|s| s.user == user) {
            return None;
        }
        Some(if st.online.contains(&user) {
            // The merge rules (open question 119, LNG-PRF-015) are in `presence::merge`.
            let reports: Vec<Report<'_>> = st
                .sessions
                .values()
                .filter(|s| s.user == user && s.is_attached())
                .map(|s| match st.session_presence.get(&s.id) {
                    Some(sp) => Report {
                        platform: sp.platform,
                        update: sp.update.as_ref(),
                        order: sp.order,
                    },
                    None => Report {
                        platform: "web",
                        update: None,
                        order: 0,
                    },
                })
                .collect();
            if reports.is_empty() {
                PresenceUpdate::offline()
            } else {
                presence_merge(st.presence.get(&user), &reports)
            }
        } else {
            PresenceUpdate::offline()
        })
    }

    /// Recompute whether `user` has an attached socket and broadcast the effective
    /// presence on a transition (or always when `force`). Offline is immediate when
    /// the last socket closes; the resume buffer does not keep the user online.
    pub(crate) async fn refresh_presence(&self, user: UserId, force: bool) {
        let (changed, update, guilds, online) = {
            let mut st = self.st();
            // A forced refresh is also an announcement: remember what was sent.
            let announce = |st: &mut State, u: Option<&PresenceUpdate>| match u {
                Some(u) => {
                    st.last_presence.insert(user, u.clone());
                }
                None => {
                    st.last_presence.remove(&user);
                }
            };
            let online = st
                .sessions
                .values()
                .any(|s| s.user == user && s.is_attached());
            let was = st.online.contains(&user);
            if online {
                st.online.insert(user);
            } else {
                st.online.remove(&user);
            }
            let guilds: HashSet<GuildId> = st
                .sessions
                .values()
                .filter(|s| s.user == user)
                .flat_map(|s| s.guilds())
                .collect();
            let update = Self::effective(&st, user);
            // Another session closing or changing can alter the merged presence without
            // changing whether the user is online: announce when it differs.
            let differs = st.last_presence.get(&user) != update.as_ref();
            if online && !force && differs {
                announce(&mut st, update.as_ref());
                (true, update, guilds, online)
            } else {
                if online && (force || online != was) {
                    announce(&mut st, update.as_ref());
                } else if !online {
                    announce(&mut st, None);
                }
                (online != was, update, guilds, online)
            }
        };
        let went_offline = changed && !online;
        let Some(update) = update else {
            if went_offline {
                self.inner.dir.user_offline(user).await;
            }
            return;
        };
        if changed || force {
            self.notify_friends(user, &update, &guilds).await;
            for g in guilds {
                self.schedule_list_refresh(g, false);
                self.dispatch_guild(
                    g,
                    "PRESENCE_UPDATE",
                    presence_body(user, g, &update),
                    None,
                    None,
                )
                .await;
            }
        }
        if went_offline {
            self.inner.dir.user_offline(user).await;
        }
    }

    /// `PRESENCE_UPDATE` to the user's friends. Friends who already hear about the
    /// user through one of `guilds` are skipped so nobody gets it twice.
    async fn notify_friends(
        &self,
        user: UserId,
        update: &PresenceUpdate,
        guilds: &HashSet<GuildId>,
    ) {
        let friends = self.inner.dir.friends_of(user).await;
        if friends.is_empty() {
            return;
        }
        let via_guild: HashSet<UserId> = guilds
            .iter()
            .flat_map(|g| self.guild_users(*g, Some(user)))
            .collect();
        let users: Vec<UserId> = friends
            .into_iter()
            .filter(|f| !via_guild.contains(f))
            .collect();
        if users.is_empty() {
            return;
        }
        let ev = DomainEvent::UserScoped {
            users: users.clone(),
            name: "PRESENCE_UPDATE".into(),
            payload: friend_presence_body(user, update),
        };
        for f in users {
            // Best effort: a failed publish only delays a presence dot.
            let _ = self.inner.bus.publish(Topic::User(f), ev.clone()).await;
        }
    }

    /// The socket of `session` closed (or was taken over). Marks the user offline
    /// when it was their last attached socket.
    pub(crate) async fn session_detached(&self, session: &Session) {
        self.refresh_presence(session.user, false).await;
    }

    /// Users currently typing in `channel` (TTL not yet elapsed).
    pub fn typing_users(&self, channel: ChannelId) -> Vec<UserId> {
        let ttl = self.inner.cfg.typing_ttl;
        let now = Instant::now();
        self.st()
            .typing
            .iter()
            .filter(|((c, _), at)| *c == channel && now.duration_since(**at) <= ttl)
            .map(|((_, u), _)| *u)
            .collect()
    }
}

/// One entry of READY's `sessions` array.
fn session_entry(s: &Session) -> Value {
    let active = s.is_attached();
    json!({
        "session_id": s.id,
        "status": if active { "online" } else { "offline" },
        "active": active,
    })
}

/// `PRESENCE_UPDATE` body for friends, who see it outside any guild.
pub(crate) fn friend_presence_body(user: UserId, p: &PresenceUpdate) -> Value {
    json!({
        "user": { "id": user },
        "status": p.visible_status(), "activities": p.activities,
        "client_status": client_status_of(p),
    })
}

/// `client_status` as other users see it: invisible users show no platform at all.
fn client_status_of(p: &PresenceUpdate) -> Value {
    if p.visible_status() == "offline" || p.client_status.is_null() {
        json!({})
    } else {
        p.client_status.clone()
    }
}

fn presence_body(user: UserId, guild: GuildId, p: &PresenceUpdate) -> Value {
    json!({
        "user": { "id": user }, "guild_id": guild,
        "status": p.visible_status(), "activities": p.activities,
        "client_status": client_status_of(p),
    })
}
