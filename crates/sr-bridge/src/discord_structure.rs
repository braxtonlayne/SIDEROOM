//! BRG-SYN-003 (wave 35b): the Discord REST calls of the whole-guild mirror. Roles, channels, categories,
//! overwrites, positions, threads and forum tags, plus the facts the preflight needs (guild, bot access).
//!
//! This module owns every structural Discord HTTP call. `sync-core` (plan, seed, reconcile) and `sync-perms`
//! (permission payloads) call through [`DiscordApi`]; nothing else in the bridge builds a Discord URL.
//!
//! Rules every call here keeps:
//!
//! * **Audit reason.** Every write takes a `reason` and sends it as `X-Audit-Log-Reason`
//!   (design 11.3). The bridge passes [`reason_for_op`] (`sr-bridge:<op_id>`), so Discord's audit log shows who
//!   did it and `GUILD_AUDIT_LOG_ENTRY_CREATE` carrying that reason is recognised as an echo.
//! * **Ids and bit words.** Snowflakes are `i64`, permission words are `u64` (Discord sends and wants decimal
//!   strings). Nothing here knows which bits are SIDE ROOM-only: callers hand over the word that is to be sent
//!   (`sync-perms` masks it with `BRIDGE_SHARED` first) and this layer never adds or removes a bit.
//! * **Pacing.** Structural writes go through the [`Pacer`] (`bridge.struct_writes_per_sec`, default 2); reads
//!   do not. Discord's own headers still win: a 429 or an empty bucket blocks the route key
//!   ([`DiscordApi::blocked_for`] with [`channel_key`] / [`guild_key`]).
//! * **Breaker input.** 401/403/429 answers are counted ([`DiscordApi::invalid_requests_within`]) so the
//!   per-bridge breaker (200 in 10 minutes) and the instance breaker (2,000) can decide.
//! * **No secrets.** Errors go through [`redact`](crate::discord::redact); a token never appears in one.

use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use sr_sdk::http::Method;
use tokio::sync::Mutex as AsyncMutex;

use crate::discord::{CallError, DiscordApi};

// ---------------------------------------------------------------------------------------------------------
// Channel types and intents
// ---------------------------------------------------------------------------------------------------------

/// Discord channel type numbers.
pub mod channel_type {
    pub const TEXT: i64 = 0;
    pub const DM: i64 = 1;
    pub const VOICE: i64 = 2;
    pub const GROUP_DM: i64 = 3;
    pub const CATEGORY: i64 = 4;
    pub const ANNOUNCEMENT: i64 = 5;
    pub const ANNOUNCEMENT_THREAD: i64 = 10;
    pub const PUBLIC_THREAD: i64 = 11;
    pub const PRIVATE_THREAD: i64 = 12;
    pub const STAGE: i64 = 13;
    pub const DIRECTORY: i64 = 14;
    pub const FORUM: i64 = 15;
    pub const MEDIA: i64 = 16;

    pub fn is_thread(kind: i64) -> bool {
        matches!(kind, ANNOUNCEMENT_THREAD | PUBLIC_THREAD | PRIVATE_THREAD)
    }
}

/// Gateway intent bits per bridge mode (design section 10; Discord positions).
pub mod intents {
    pub const GUILDS: u64 = 1 << 0;
    pub const GUILD_MEMBERS: u64 = 1 << 1;
    pub const GUILD_MODERATION: u64 = 1 << 2;
    pub const GUILD_PRESENCES: u64 = 1 << 8;
    pub const GUILD_MESSAGES: u64 = 1 << 9;
    pub const GUILD_MESSAGE_REACTIONS: u64 = 1 << 10;
    pub const MESSAGE_CONTENT: u64 = 1 << 15;

    /// Selective mode (wave 35 plus the member list of 1732).
    pub const SELECTIVE: u64 =
        GUILDS | GUILD_MEMBERS | GUILD_MESSAGES | GUILD_MESSAGE_REACTIONS | MESSAGE_CONTENT;
    /// Whole-guild mode: the above plus the audit-log entries that name the Discord actor.
    pub const WHOLE_GUILD: u64 = SELECTIVE | GUILD_MODERATION;

    /// The intents for a bridge mode (`"selective"` or `"whole_guild"`); an unknown mode gets the narrow set.
    pub fn for_mode(mode: &str, presences: bool) -> u64 {
        let base = if mode == "whole_guild" {
            WHOLE_GUILD
        } else {
            SELECTIVE
        };
        if presences {
            base | GUILD_PRESENCES
        } else {
            base
        }
    }
}

/// Discord permission bits the bridge itself needs to know about (everything else is `sync-perms`' business).
pub mod perm {
    pub const ADMINISTRATOR: u64 = 1 << 3;
    pub const MANAGE_CHANNELS: u64 = 1 << 4;
    pub const MANAGE_ROLES: u64 = 1 << 28;
    pub const MANAGE_WEBHOOKS: u64 = 1 << 29;
    pub const MANAGE_THREADS: u64 = 1 << 34;
}

// ---------------------------------------------------------------------------------------------------------
// Audit reason, route keys, pacing
// ---------------------------------------------------------------------------------------------------------

/// The `X-Audit-Log-Reason` the bridge sends for op `op_id`.
pub fn reason_for_op(op_id: i64) -> String {
    format!("sr-bridge:{op_id}")
}

/// The leading text of every reason the bridge sends; an audit-log entry whose reason starts with it is an echo.
pub const REASON_PREFIX: &str = "sr-bridge:";

/// Whether an audit-log `reason` (as Discord echoes it) was written by the bridge.
pub fn is_bridge_reason(reason: Option<&str>) -> bool {
    reason.is_some_and(|r| r.starts_with(REASON_PREFIX))
}

/// Discord wants the reason percent-encoded in the header and at most 512 characters.
pub(crate) fn encode_reason(reason: &str) -> String {
    let mut out = String::new();
    for ch in reason.chars().take(512) {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~' | ':' | ' ' | ',') {
            out.push(ch);
        } else {
            let mut buf = [0u8; 4];
            for b in ch.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    out
}

/// The rate-limit key of everything under a channel (messages are keyed by webhook in `discord.rs`).
pub fn channel_key(id: i64) -> String {
    format!("ch:{id}")
}

/// The rate-limit key of guild-level structure (roles, channel list, positions).
pub fn guild_key(id: i64) -> String {
    format!("g:{id}")
}

/// Spaces structural writes at `bridge.struct_writes_per_sec`. One per [`DiscordApi`] (shared by clones).
#[derive(Debug)]
pub struct Pacer {
    state: AsyncMutex<PacerState>,
}

#[derive(Debug)]
struct PacerState {
    gap: Duration,
    next: Instant,
}

fn gap_for(per_sec: f64) -> Duration {
    if per_sec.is_finite() && per_sec > 0.0 {
        Duration::from_secs_f64(1.0 / per_sec.min(1000.0))
    } else {
        Duration::from_millis(500)
    }
}

impl Pacer {
    pub fn new(per_sec: f64) -> Self {
        Self {
            state: AsyncMutex::new(PacerState {
                gap: gap_for(per_sec),
                next: Instant::now(),
            }),
        }
    }

    pub async fn set_rate(&self, per_sec: f64) {
        self.state.lock().await.gap = gap_for(per_sec);
    }

    /// Wait for this write's turn.
    pub async fn wait(&self) {
        let sleep = {
            let mut s = self.state.lock().await;
            let now = Instant::now();
            let at = s.next.max(now);
            s.next = at + s.gap;
            at.saturating_duration_since(now)
        };
        if !sleep.is_zero() {
            tokio::time::sleep(sleep).await;
        }
    }
}

// ---------------------------------------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------------------------------------

/// A role overwrite or member overwrite on a channel. `kind`: 0 role, 1 member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Overwrite {
    pub id: i64,
    pub kind: i64,
    pub allow: u64,
    pub deny: u64,
}

impl Overwrite {
    pub const ROLE: i64 = 0;
    pub const MEMBER: i64 = 1;

    pub fn role(id: i64, allow: u64, deny: u64) -> Self {
        Self {
            id,
            kind: Self::ROLE,
            allow,
            deny,
        }
    }

    fn to_json(self) -> Value {
        json!({
            "id": self.id.to_string(),
            "type": self.kind,
            "allow": self.allow.to_string(),
            "deny": self.deny.to_string(),
        })
    }

    fn from_json(v: &Value) -> Option<Self> {
        Some(Self {
            id: id_of(v.get("id")?)?,
            kind: v.get("type").and_then(Value::as_i64).unwrap_or(0),
            allow: bits_of(v.get("allow")),
            deny: bits_of(v.get("deny")),
        })
    }
}

/// A Discord role as the mirror reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuildRole {
    pub id: i64,
    pub name: String,
    pub color: i64,
    pub hoist: bool,
    pub position: i64,
    /// The full word as Discord holds it, Discord-only bits included.
    pub permissions: u64,
    /// Bot, booster, integration and linked roles: never mapped (design 6.5).
    pub managed: bool,
    pub mentionable: bool,
    pub unicode_emoji: Option<String>,
    /// The icon hash, when the role has one.
    pub icon: Option<String>,
    /// The bot user a bot role belongs to.
    pub bot_id: Option<i64>,
}

impl GuildRole {
    fn from_json(v: &Value) -> Option<Self> {
        Some(Self {
            id: id_of(v.get("id")?)?,
            name: str_of(v, "name"),
            color: v.get("color").and_then(Value::as_i64).unwrap_or(0),
            hoist: v.get("hoist").and_then(Value::as_bool).unwrap_or(false),
            position: v.get("position").and_then(Value::as_i64).unwrap_or(0),
            permissions: bits_of(v.get("permissions")),
            managed: v.get("managed").and_then(Value::as_bool).unwrap_or(false),
            mentionable: v
                .get("mentionable")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            unicode_emoji: v
                .get("unicode_emoji")
                .and_then(Value::as_str)
                .map(str::to_owned),
            icon: v.get("icon").and_then(Value::as_str).map(str::to_owned),
            bot_id: v.get("tags").and_then(|t| t.get("bot_id")).and_then(id_of),
        })
    }
}

/// Fields of a role create or edit. `None` leaves the field out (an edit keeps it; a create uses Discord's
/// default). A nullable field is `Option<Option<_>>`: `Some(None)` clears it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RoleParams {
    pub name: Option<String>,
    /// The word to send (callers have already applied the bridge masks).
    pub permissions: Option<u64>,
    pub color: Option<i64>,
    pub hoist: Option<bool>,
    pub mentionable: Option<bool>,
    pub unicode_emoji: Option<Option<String>>,
}

impl RoleParams {
    fn body(&self) -> Value {
        let mut m = Map::new();
        put(&mut m, "name", self.name.as_ref().map(|s| json!(s)));
        put(
            &mut m,
            "permissions",
            self.permissions.map(|p| json!(p.to_string())),
        );
        put(&mut m, "color", self.color.map(|c| json!(c)));
        put(&mut m, "hoist", self.hoist.map(|b| json!(b)));
        put(&mut m, "mentionable", self.mentionable.map(|b| json!(b)));
        put(
            &mut m,
            "unicode_emoji",
            self.unicode_emoji.as_ref().map(|e| json!(e)),
        );
        Value::Object(m)
    }
}

/// A forum tag. `id` is `None` for a tag to be created (Discord assigns it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForumTag {
    pub id: Option<i64>,
    pub name: String,
    pub moderated: bool,
    pub emoji_id: Option<i64>,
    pub emoji_name: Option<String>,
}

impl ForumTag {
    fn to_json(&self) -> Value {
        let mut m = Map::new();
        if let Some(id) = self.id {
            m.insert("id".into(), json!(id.to_string()));
        }
        m.insert("name".into(), json!(self.name));
        m.insert("moderated".into(), json!(self.moderated));
        m.insert(
            "emoji_id".into(),
            self.emoji_id.map_or(Value::Null, |i| json!(i.to_string())),
        );
        m.insert("emoji_name".into(), json!(self.emoji_name));
        Value::Object(m)
    }

    fn from_json(v: &Value) -> Option<Self> {
        Some(Self {
            id: v.get("id").and_then(id_of),
            name: str_of(v, "name"),
            moderated: v.get("moderated").and_then(Value::as_bool).unwrap_or(false),
            emoji_id: v.get("emoji_id").and_then(id_of),
            emoji_name: v
                .get("emoji_name")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
    }
}

/// Thread state (`thread_metadata`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ThreadState {
    pub archived: bool,
    pub locked: bool,
    pub auto_archive_duration: i64,
    pub invitable: bool,
}

/// A channel, category, thread or forum as the mirror reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct GuildChannel {
    pub id: i64,
    pub kind: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub position: i64,
    pub topic: Option<String>,
    pub nsfw: bool,
    pub rate_limit_per_user: i64,
    pub bitrate: Option<i64>,
    pub user_limit: Option<i64>,
    pub video_quality_mode: Option<i64>,
    pub overwrites: Vec<Overwrite>,
    pub default_auto_archive_duration: Option<i64>,
    pub default_thread_rate_limit_per_user: Option<i64>,
    pub available_tags: Vec<ForumTag>,
    pub applied_tags: Vec<i64>,
    pub default_reaction_emoji_name: Option<String>,
    pub default_sort_order: Option<i64>,
    pub default_forum_layout: Option<i64>,
    pub flags: i64,
    /// Present on threads.
    pub thread: Option<ThreadState>,
    /// The thread's creator (a forum post's author), when sent.
    pub owner_id: Option<i64>,
}

impl GuildChannel {
    pub fn from_json(v: &Value) -> Option<Self> {
        let thread = v.get("thread_metadata").map(|t| ThreadState {
            archived: t.get("archived").and_then(Value::as_bool).unwrap_or(false),
            locked: t.get("locked").and_then(Value::as_bool).unwrap_or(false),
            auto_archive_duration: t
                .get("auto_archive_duration")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            invitable: t.get("invitable").and_then(Value::as_bool).unwrap_or(true),
        });
        Some(Self {
            id: id_of(v.get("id")?)?,
            kind: v.get("type").and_then(Value::as_i64).unwrap_or(0),
            name: str_of(v, "name"),
            parent_id: v.get("parent_id").and_then(id_of),
            position: v.get("position").and_then(Value::as_i64).unwrap_or(0),
            topic: v.get("topic").and_then(Value::as_str).map(str::to_owned),
            nsfw: v.get("nsfw").and_then(Value::as_bool).unwrap_or(false),
            rate_limit_per_user: v
                .get("rate_limit_per_user")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            bitrate: v.get("bitrate").and_then(Value::as_i64),
            user_limit: v.get("user_limit").and_then(Value::as_i64),
            video_quality_mode: v.get("video_quality_mode").and_then(Value::as_i64),
            overwrites: v
                .get("permission_overwrites")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Overwrite::from_json).collect())
                .unwrap_or_default(),
            default_auto_archive_duration: v
                .get("default_auto_archive_duration")
                .and_then(Value::as_i64),
            default_thread_rate_limit_per_user: v
                .get("default_thread_rate_limit_per_user")
                .and_then(Value::as_i64),
            available_tags: v
                .get("available_tags")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(ForumTag::from_json).collect())
                .unwrap_or_default(),
            applied_tags: v
                .get("applied_tags")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(id_of).collect())
                .unwrap_or_default(),
            default_reaction_emoji_name: v
                .get("default_reaction_emoji")
                .and_then(|e| e.get("emoji_name"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            default_sort_order: v.get("default_sort_order").and_then(Value::as_i64),
            default_forum_layout: v.get("default_forum_layout").and_then(Value::as_i64),
            flags: v.get("flags").and_then(Value::as_i64).unwrap_or(0),
            thread,
            owner_id: v.get("owner_id").and_then(id_of),
        })
    }
}

/// Fields of a channel create or edit (a thread edit uses the same body). See [`RoleParams`] for the
/// `Option` conventions. `kind` is only sent on a create.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChannelParams {
    pub kind: Option<i64>,
    pub name: Option<String>,
    pub topic: Option<Option<String>>,
    pub parent_id: Option<Option<i64>>,
    pub position: Option<i64>,
    pub nsfw: Option<bool>,
    pub rate_limit_per_user: Option<i64>,
    pub bitrate: Option<i64>,
    pub user_limit: Option<i64>,
    pub video_quality_mode: Option<i64>,
    /// The complete overwrite list as it should be on Discord (it replaces what is there).
    pub overwrites: Option<Vec<Overwrite>>,
    pub default_auto_archive_duration: Option<i64>,
    pub default_thread_rate_limit_per_user: Option<i64>,
    /// The complete tag list; tags with an id are kept (renamed), tags without one are created, others removed.
    pub available_tags: Option<Vec<ForumTag>>,
    pub default_reaction_emoji_name: Option<Option<String>>,
    pub default_sort_order: Option<Option<i64>>,
    pub default_forum_layout: Option<i64>,
    pub flags: Option<i64>,
    // thread-only
    pub archived: Option<bool>,
    pub locked: Option<bool>,
    pub auto_archive_duration: Option<i64>,
    pub invitable: Option<bool>,
    pub applied_tags: Option<Vec<i64>>,
}

impl ChannelParams {
    pub fn body(&self) -> Value {
        let mut m = Map::new();
        put(&mut m, "type", self.kind.map(|k| json!(k)));
        put(&mut m, "name", self.name.as_ref().map(|s| json!(s)));
        put(&mut m, "topic", self.topic.as_ref().map(|t| json!(t)));
        put(
            &mut m,
            "parent_id",
            self.parent_id
                .map(|p| p.map_or(Value::Null, |i| json!(i.to_string()))),
        );
        put(&mut m, "position", self.position.map(|n| json!(n)));
        put(&mut m, "nsfw", self.nsfw.map(|b| json!(b)));
        put(
            &mut m,
            "rate_limit_per_user",
            self.rate_limit_per_user.map(|n| json!(n)),
        );
        put(&mut m, "bitrate", self.bitrate.map(|n| json!(n)));
        put(&mut m, "user_limit", self.user_limit.map(|n| json!(n)));
        put(
            &mut m,
            "video_quality_mode",
            self.video_quality_mode.map(|n| json!(n)),
        );
        put(
            &mut m,
            "permission_overwrites",
            self.overwrites
                .as_ref()
                .map(|o| Value::Array(o.iter().map(|x| x.to_json()).collect())),
        );
        put(
            &mut m,
            "default_auto_archive_duration",
            self.default_auto_archive_duration.map(|n| json!(n)),
        );
        put(
            &mut m,
            "default_thread_rate_limit_per_user",
            self.default_thread_rate_limit_per_user.map(|n| json!(n)),
        );
        put(
            &mut m,
            "available_tags",
            self.available_tags
                .as_ref()
                .map(|t| Value::Array(t.iter().map(ForumTag::to_json).collect())),
        );
        put(
            &mut m,
            "default_reaction_emoji",
            self.default_reaction_emoji_name.as_ref().map(|e| match e {
                Some(name) => json!({ "emoji_id": null, "emoji_name": name }),
                None => Value::Null,
            }),
        );
        put(
            &mut m,
            "default_sort_order",
            self.default_sort_order.map(|o| json!(o)),
        );
        put(
            &mut m,
            "default_forum_layout",
            self.default_forum_layout.map(|n| json!(n)),
        );
        put(&mut m, "flags", self.flags.map(|n| json!(n)));
        put(&mut m, "archived", self.archived.map(|b| json!(b)));
        put(&mut m, "locked", self.locked.map(|b| json!(b)));
        put(
            &mut m,
            "auto_archive_duration",
            self.auto_archive_duration.map(|n| json!(n)),
        );
        put(&mut m, "invitable", self.invitable.map(|b| json!(b)));
        put(
            &mut m,
            "applied_tags",
            self.applied_tags
                .as_ref()
                .map(|t| Value::Array(t.iter().map(|i| json!(i.to_string())).collect())),
        );
        Value::Object(m)
    }
}

/// One entry of a bulk position call. `parent_id`: `None` leaves the parent, `Some(None)` moves to the top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelPosition {
    pub id: i64,
    pub position: i64,
    pub parent_id: Option<Option<i64>>,
    /// Discord's "sync with category" (copies the parent's overwrites). The bridge never sets it: SIDE ROOM's
    /// overwrites are pushed explicitly.
    pub lock_permissions: bool,
}

/// What the mirror needs of the Discord server itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuildInfo {
    pub id: i64,
    pub name: String,
    pub owner_id: Option<i64>,
    /// 0 to 3: decides the voice bitrate ceiling and role icons.
    pub premium_tier: i64,
    pub features: Vec<String>,
    pub system_channel_id: Option<i64>,
    pub rules_channel_id: Option<i64>,
}

impl GuildInfo {
    /// Announcement and stage channels, forums and media need Community (design 8).
    pub fn is_community(&self) -> bool {
        self.features.iter().any(|f| f == "COMMUNITY")
    }

    /// Whether roles may carry an icon (boost level 2, or the feature flag).
    pub fn role_icons(&self) -> bool {
        self.premium_tier >= 2 || self.features.iter().any(|f| f == "ROLE_ICONS")
    }

    /// The voice bitrate ceiling for the boost tier, in bits per second.
    pub fn max_bitrate(&self) -> i64 {
        match self.premium_tier {
            0 => 96_000,
            1 => 128_000,
            2 => 256_000,
            _ => 384_000,
        }
    }
}

/// What the bot can do in the Discord server (the preflight of design 9.2 reads this).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotAccess {
    pub administrator: bool,
    /// The union of the bot's role permission words (and `@everyone`'s).
    pub permissions: u64,
    /// The bot's highest role position (0 when it has none but `@everyone`).
    pub top_position: i64,
    /// The highest position of any role in the guild.
    pub highest_position: i64,
    /// The bot's role ids.
    pub roles: Vec<i64>,
}

impl BotAccess {
    /// The bot's highest role is the highest in the guild (it can only edit roles below its own).
    pub fn is_top_role(&self) -> bool {
        self.top_position >= self.highest_position
    }

    pub fn has(&self, bits: u64) -> bool {
        self.administrator || self.permissions & bits == bits
    }
}

/// Where a webhook post goes inside a thread or forum.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ThreadTarget {
    /// Post into this existing thread.
    pub thread_id: Option<i64>,
    /// In a forum channel: start a post with this title.
    pub thread_name: Option<String>,
    /// Tag ids applied to a new forum post.
    pub applied_tags: Vec<i64>,
}

/// The message a webhook post created, with the channel it landed in (a new forum post's thread id).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostedMessage {
    pub message_id: i64,
    pub channel_id: i64,
}

// ---------------------------------------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------------------------------------

fn id_of(v: &Value) -> Option<i64> {
    match v {
        Value::String(s) => s.parse::<i64>().ok().filter(|n| *n > 0),
        Value::Number(n) => n.as_i64().filter(|n| *n > 0),
        _ => None,
    }
}

fn bits_of(v: Option<&Value>) -> u64 {
    match v {
        Some(Value::String(s)) => s.parse().unwrap_or(0),
        Some(Value::Number(n)) => n.as_u64().unwrap_or(0),
        _ => 0,
    }
}

fn str_of(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn put(m: &mut Map<String, Value>, key: &str, v: Option<Value>) {
    if let Some(v) = v {
        m.insert(key.to_owned(), v);
    }
}

fn unreadable(what: &str) -> CallError {
    CallError::Transport(format!("unreadable {what} from Discord"))
}

fn list_of<T>(v: Value, what: &str, f: impl Fn(&Value) -> Option<T>) -> Result<Vec<T>, CallError> {
    let arr = v.as_array().ok_or_else(|| unreadable(what))?;
    Ok(arr.iter().filter_map(f).collect())
}

// ---------------------------------------------------------------------------------------------------------
// The calls
// ---------------------------------------------------------------------------------------------------------

impl DiscordApi {
    /// Record an invalid response (401, 403, 429) for the breaker.
    pub(crate) fn note_invalid(&self) {
        if let Ok(mut q) = self.invalid.lock() {
            let now = Instant::now();
            q.push_back(now);
            while q
                .front()
                .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(3600))
            {
                q.pop_front();
            }
        }
    }

    /// How many invalid responses (401, 403, 429) this client has seen in the last `window`. The per-bridge
    /// breaker pauses sync at 200 in ten minutes (design 11.5).
    pub fn invalid_requests_within(&self, window: Duration) -> usize {
        let now = Instant::now();
        self.invalid.lock().map_or(0, |q| {
            q.iter()
                .filter(|t| now.duration_since(**t) <= window)
                .count()
        })
    }

    /// Set the structural write budget (`bridge.struct_writes_per_sec`, 1 to 10; tests raise it).
    pub async fn set_struct_writes_per_sec(&self, per_sec: f64) {
        self.pacer.set_rate(per_sec).await;
    }

    async fn write(
        &self,
        method: Method,
        path: &str,
        token: &str,
        body: Option<Value>,
        key: &str,
        reason: &str,
    ) -> Result<Value, CallError> {
        self.pacer.wait().await;
        self.call_with(method, path, Some(token), body, Some(key), Some(reason))
            .await
    }

    // ---- the guild and the bot's power ----

    /// `GET /guilds/{id}`.
    pub async fn guild_info(&self, token: &str, guild: i64) -> Result<GuildInfo, CallError> {
        let v = self
            .call_with(
                Method::Get,
                &format!("/guilds/{guild}"),
                Some(token),
                None,
                Some(&guild_key(guild)),
                None,
            )
            .await?;
        Ok(GuildInfo {
            id: v.get("id").and_then(id_of).unwrap_or(guild),
            name: str_of(&v, "name"),
            owner_id: v.get("owner_id").and_then(id_of),
            premium_tier: v.get("premium_tier").and_then(Value::as_i64).unwrap_or(0),
            features: v
                .get("features")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|f| f.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
            system_channel_id: v.get("system_channel_id").and_then(id_of),
            rules_channel_id: v.get("rules_channel_id").and_then(id_of),
        })
    }

    /// `GET /guilds/{id}/members/{bot}` and `GET /guilds/{id}/roles`: what the bot may do (design 9.2).
    pub async fn bot_access(
        &self,
        token: &str,
        guild: i64,
        bot_user: i64,
    ) -> Result<BotAccess, CallError> {
        let m = self
            .call_with(
                Method::Get,
                &format!("/guilds/{guild}/members/{bot_user}"),
                Some(token),
                None,
                Some(&guild_key(guild)),
                None,
            )
            .await?;
        let mine: Vec<i64> = m
            .get("roles")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(id_of).collect())
            .unwrap_or_default();
        let roles = self.guild_roles(token, guild).await?;
        let mut permissions = 0u64;
        let mut top = 0i64;
        for r in &roles {
            if r.id == guild || mine.contains(&r.id) {
                permissions |= r.permissions;
                if r.id != guild {
                    top = top.max(r.position);
                }
            }
        }
        Ok(BotAccess {
            administrator: permissions & perm::ADMINISTRATOR != 0,
            permissions,
            top_position: top,
            highest_position: roles.iter().map(|r| r.position).max().unwrap_or(0),
            roles: mine,
        })
    }

    // ---- roles ----

    /// `GET /guilds/{id}/roles`.
    pub async fn guild_roles(&self, token: &str, guild: i64) -> Result<Vec<GuildRole>, CallError> {
        let v = self
            .call_with(
                Method::Get,
                &format!("/guilds/{guild}/roles"),
                Some(token),
                None,
                Some(&guild_key(guild)),
                None,
            )
            .await?;
        list_of(v, "role list", GuildRole::from_json)
    }

    /// `POST /guilds/{id}/roles`.
    pub async fn create_role(
        &self,
        token: &str,
        guild: i64,
        params: &RoleParams,
        reason: &str,
    ) -> Result<GuildRole, CallError> {
        let v = self
            .write(
                Method::Post,
                &format!("/guilds/{guild}/roles"),
                token,
                Some(params.body()),
                &guild_key(guild),
                reason,
            )
            .await?;
        GuildRole::from_json(&v).ok_or_else(|| unreadable("role"))
    }

    /// `PATCH /guilds/{id}/roles/{role}`.
    pub async fn edit_role(
        &self,
        token: &str,
        guild: i64,
        role: i64,
        params: &RoleParams,
        reason: &str,
    ) -> Result<GuildRole, CallError> {
        let v = self
            .write(
                Method::Patch,
                &format!("/guilds/{guild}/roles/{role}"),
                token,
                Some(params.body()),
                &guild_key(guild),
                reason,
            )
            .await?;
        GuildRole::from_json(&v).ok_or_else(|| unreadable("role"))
    }

    /// `DELETE /guilds/{id}/roles/{role}`.
    pub async fn delete_role(
        &self,
        token: &str,
        guild: i64,
        role: i64,
        reason: &str,
    ) -> Result<(), CallError> {
        self.write(
            Method::Delete,
            &format!("/guilds/{guild}/roles/{role}"),
            token,
            None,
            &guild_key(guild),
            reason,
        )
        .await
        .map(|_| ())
    }

    /// `PATCH /guilds/{id}/roles`: the order of many roles in one call (`(role id, position)`; `@everyone`
    /// and the bot's own role are never included by the caller).
    pub async fn set_role_positions(
        &self,
        token: &str,
        guild: i64,
        order: &[(i64, i64)],
        reason: &str,
    ) -> Result<Vec<GuildRole>, CallError> {
        let body = Value::Array(
            order
                .iter()
                .map(|(id, pos)| json!({ "id": id.to_string(), "position": pos }))
                .collect(),
        );
        let v = self
            .write(
                Method::Patch,
                &format!("/guilds/{guild}/roles"),
                token,
                Some(body),
                &guild_key(guild),
                reason,
            )
            .await?;
        list_of(v, "role list", GuildRole::from_json)
    }

    // ---- channels, categories, overwrites ----

    /// `GET /guilds/{id}/channels`: every channel and category (not threads).
    pub async fn guild_channels_all(
        &self,
        token: &str,
        guild: i64,
    ) -> Result<Vec<GuildChannel>, CallError> {
        let v = self
            .call_with(
                Method::Get,
                &format!("/guilds/{guild}/channels"),
                Some(token),
                None,
                Some(&guild_key(guild)),
                None,
            )
            .await?;
        list_of(v, "channel list", GuildChannel::from_json)
    }

    /// `GET /channels/{id}` (a channel, a category or a thread).
    pub async fn get_channel(&self, token: &str, channel: i64) -> Result<GuildChannel, CallError> {
        let v = self
            .call_with(
                Method::Get,
                &format!("/channels/{channel}"),
                Some(token),
                None,
                Some(&channel_key(channel)),
                None,
            )
            .await?;
        GuildChannel::from_json(&v).ok_or_else(|| unreadable("channel"))
    }

    /// `POST /guilds/{id}/channels`. `params.kind` and `params.name` are required; overwrites ride in the
    /// create body (one call per channel, design 9.6).
    pub async fn create_channel(
        &self,
        token: &str,
        guild: i64,
        params: &ChannelParams,
        reason: &str,
    ) -> Result<GuildChannel, CallError> {
        let v = self
            .write(
                Method::Post,
                &format!("/guilds/{guild}/channels"),
                token,
                Some(params.body()),
                &guild_key(guild),
                reason,
            )
            .await?;
        GuildChannel::from_json(&v).ok_or_else(|| unreadable("channel"))
    }

    /// `PATCH /channels/{id}`: edits a channel, category or thread (name, topic, archive and lock state,
    /// applied tags, the whole overwrite list, available forum tags). `kind` is never sent on an edit.
    pub async fn edit_channel(
        &self,
        token: &str,
        channel: i64,
        params: &ChannelParams,
        reason: &str,
    ) -> Result<GuildChannel, CallError> {
        let mut body = params.body();
        if let Some(o) = body.as_object_mut() {
            o.remove("type");
        }
        let v = self
            .write(
                Method::Patch,
                &format!("/channels/{channel}"),
                token,
                Some(body),
                &channel_key(channel),
                reason,
            )
            .await?;
        GuildChannel::from_json(&v).ok_or_else(|| unreadable("channel"))
    }

    /// `PATCH /channels/{id}` with only `permission_overwrites`: replaces the whole list (design 9.6 step 6).
    pub async fn set_overwrites(
        &self,
        token: &str,
        channel: i64,
        overwrites: &[Overwrite],
        reason: &str,
    ) -> Result<GuildChannel, CallError> {
        let params = ChannelParams {
            overwrites: Some(overwrites.to_vec()),
            ..ChannelParams::default()
        };
        self.edit_channel(token, channel, &params, reason).await
    }

    /// `PUT /channels/{id}/permissions/{target}`: one overwrite.
    pub async fn put_overwrite(
        &self,
        token: &str,
        channel: i64,
        ow: Overwrite,
        reason: &str,
    ) -> Result<(), CallError> {
        self.write(
            Method::Put,
            &format!("/channels/{channel}/permissions/{}", ow.id),
            token,
            Some(json!({
                "allow": ow.allow.to_string(),
                "deny": ow.deny.to_string(),
                "type": ow.kind,
            })),
            &channel_key(channel),
            reason,
        )
        .await
        .map(|_| ())
    }

    /// `DELETE /channels/{id}/permissions/{target}`.
    pub async fn delete_overwrite(
        &self,
        token: &str,
        channel: i64,
        target: i64,
        reason: &str,
    ) -> Result<(), CallError> {
        self.write(
            Method::Delete,
            &format!("/channels/{channel}/permissions/{target}"),
            token,
            None,
            &channel_key(channel),
            reason,
        )
        .await
        .map(|_| ())
    }

    /// `DELETE /channels/{id}`: a channel, a category or a thread. The bridge calls this only for objects it
    /// created or that SIDE ROOM deleted (never for a Discord-native object on teardown, design 12).
    pub async fn delete_channel(
        &self,
        token: &str,
        channel: i64,
        reason: &str,
    ) -> Result<(), CallError> {
        self.write(
            Method::Delete,
            &format!("/channels/{channel}"),
            token,
            None,
            &channel_key(channel),
            reason,
        )
        .await
        .map(|_| ())
    }

    /// `PATCH /guilds/{id}/channels`: positions (and parents) of many channels in one call.
    pub async fn set_channel_positions(
        &self,
        token: &str,
        guild: i64,
        order: &[ChannelPosition],
        reason: &str,
    ) -> Result<(), CallError> {
        let body = Value::Array(
            order
                .iter()
                .map(|c| {
                    let mut m = Map::new();
                    m.insert("id".into(), json!(c.id.to_string()));
                    m.insert("position".into(), json!(c.position));
                    if let Some(p) = c.parent_id {
                        m.insert(
                            "parent_id".into(),
                            p.map_or(Value::Null, |i| json!(i.to_string())),
                        );
                    }
                    m.insert("lock_permissions".into(), json!(c.lock_permissions));
                    Value::Object(m)
                })
                .collect(),
        );
        self.write(
            Method::Patch,
            &format!("/guilds/{guild}/channels"),
            token,
            Some(body),
            &guild_key(guild),
            reason,
        )
        .await
        .map(|_| ())
    }

    // ---- threads and forum posts ----

    /// `GET /guilds/{id}/threads/active`: the active threads (archived ones are not imported, Q7).
    pub async fn active_threads(
        &self,
        token: &str,
        guild: i64,
    ) -> Result<Vec<GuildChannel>, CallError> {
        let v = self
            .call_with(
                Method::Get,
                &format!("/guilds/{guild}/threads/active"),
                Some(token),
                None,
                Some(&guild_key(guild)),
                None,
            )
            .await?;
        let arr = v.get("threads").ok_or_else(|| unreadable("thread list"))?;
        list_of(arr.clone(), "thread list", GuildChannel::from_json)
    }

    /// `POST /channels/{id}/threads` without a message: a public, private or announcement thread
    /// (`params.kind`, `name`, `auto_archive_duration`, `invitable`, `rate_limit_per_user`).
    pub async fn create_thread(
        &self,
        token: &str,
        channel: i64,
        params: &ChannelParams,
        reason: &str,
    ) -> Result<GuildChannel, CallError> {
        let v = self
            .write(
                Method::Post,
                &format!("/channels/{channel}/threads"),
                token,
                Some(params.body()),
                &channel_key(channel),
                reason,
            )
            .await?;
        GuildChannel::from_json(&v).ok_or_else(|| unreadable("thread"))
    }

    /// `POST /channels/{forum}/threads` with a first message: a forum or media post made by the bot (used when
    /// a webhook cannot be, for example before the forum has one). The webhook path of
    /// [`execute_webhook_in`](Self::execute_webhook_in) is the normal one (design 8).
    pub async fn create_forum_post(
        &self,
        token: &str,
        forum: i64,
        params: &ChannelParams,
        first_message: &str,
        reason: &str,
    ) -> Result<GuildChannel, CallError> {
        let mut body = params.body();
        if let Some(o) = body.as_object_mut() {
            o.remove("type");
            o.insert(
                "message".into(),
                json!({ "content": first_message, "allowed_mentions": { "parse": [] } }),
            );
        }
        let v = self
            .write(
                Method::Post,
                &format!("/channels/{forum}/threads"),
                token,
                Some(body),
                &channel_key(forum),
                reason,
            )
            .await?;
        GuildChannel::from_json(&v).ok_or_else(|| unreadable("thread"))
    }

    // ---- webhook posts into threads and forums ----

    /// `POST /webhooks/{id}/{token}?wait=true[&thread_id=..]`: like `execute_webhook`, into a thread, or as a
    /// new forum post (`thread_name`, `applied_tags`). Returns the message and the channel it landed in (for
    /// a new post, the new thread's id).
    pub async fn execute_webhook_in(
        &self,
        id: i64,
        wtoken: &str,
        post: &crate::discord::WebhookPost,
        target: &ThreadTarget,
    ) -> Result<PostedMessage, CallError> {
        let key = Self::webhook_key(id);
        let mut path = format!("/webhooks/{id}/{wtoken}?wait=true");
        if let Some(t) = target.thread_id {
            path.push_str(&format!("&thread_id={t}"));
        }
        let mut body = post.body();
        if let Some(name) = &target.thread_name {
            body["thread_name"] = json!(name);
            if !target.applied_tags.is_empty() {
                body["applied_tags"] = Value::Array(
                    target
                        .applied_tags
                        .iter()
                        .map(|i| json!(i.to_string()))
                        .collect(),
                );
            }
        }
        let v = self
            .call_with(Method::Post, &path, None, Some(body), Some(&key), None)
            .await?;
        let message_id = v
            .get("id")
            .and_then(id_of)
            .ok_or_else(|| CallError::Transport("Discord returned no message id".into()))?;
        let channel_id = v
            .get("channel_id")
            .and_then(id_of)
            .or(target.thread_id)
            .unwrap_or(0);
        Ok(PostedMessage {
            message_id,
            channel_id,
        })
    }

    /// `PATCH /webhooks/{id}/{token}/messages/{message}?thread_id=..`.
    pub async fn edit_webhook_message_in(
        &self,
        id: i64,
        wtoken: &str,
        message: i64,
        thread_id: i64,
        content: &str,
        ping_users: &[String],
    ) -> Result<(), CallError> {
        let key = Self::webhook_key(id);
        let allowed = if ping_users.is_empty() {
            json!({ "parse": [] })
        } else {
            json!({ "parse": [], "users": ping_users })
        };
        self.call_with(
            Method::Patch,
            &format!("/webhooks/{id}/{wtoken}/messages/{message}?thread_id={thread_id}"),
            None,
            Some(json!({ "content": content, "allowed_mentions": allowed })),
            Some(&key),
            None,
        )
        .await
        .map(|_| ())
    }

    /// `DELETE /webhooks/{id}/{token}/messages/{message}?thread_id=..`.
    pub async fn delete_webhook_message_in(
        &self,
        id: i64,
        wtoken: &str,
        message: i64,
        thread_id: i64,
    ) -> Result<(), CallError> {
        let key = Self::webhook_key(id);
        self.call_with(
            Method::Delete,
            &format!("/webhooks/{id}/{wtoken}/messages/{message}?thread_id={thread_id}"),
            None,
            None,
            Some(&key),
            None,
        )
        .await
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reason_is_encoded_and_recognised() {
        assert_eq!(reason_for_op(42), "sr-bridge:42");
        assert!(is_bridge_reason(Some("sr-bridge:42")));
        assert!(!is_bridge_reason(Some("someone else")));
        assert!(!is_bridge_reason(None));
        assert_eq!(encode_reason("sr-bridge:7"), "sr-bridge:7");
        assert_eq!(encode_reason("a/b é"), "a%2Fb %C3%A9");
        assert_eq!(encode_reason(&"x".repeat(900)).len(), 512);
    }

    #[test]
    fn intents_follow_the_mode() {
        assert_eq!(intents::for_mode("selective", false), intents::SELECTIVE);
        assert_eq!(
            intents::for_mode("whole_guild", false) & intents::GUILD_MODERATION,
            4
        );
        assert_eq!(
            intents::for_mode("selective", false) & intents::GUILD_MODERATION,
            0
        );
        assert_ne!(
            intents::for_mode("selective", true) & intents::GUILD_PRESENCES,
            0
        );
        assert_ne!(intents::WHOLE_GUILD & intents::MESSAGE_CONTENT, 0);
        assert_ne!(intents::WHOLE_GUILD & intents::GUILD_MEMBERS, 0);
    }

    #[test]
    fn a_role_body_sends_only_what_is_set() {
        let b = RoleParams {
            name: Some("Mods".into()),
            permissions: Some(1 << 3 | 1 << 40),
            unicode_emoji: Some(None),
            ..RoleParams::default()
        }
        .body();
        assert_eq!(b["name"], "Mods");
        assert_eq!(b["permissions"], (1u64 << 3 | 1 << 40).to_string());
        assert!(b["unicode_emoji"].is_null());
        assert!(b.get("color").is_none());
        assert!(b.get("hoist").is_none());
    }

    #[test]
    fn a_channel_body_carries_overwrites_as_strings_and_clears_with_null() {
        let b = ChannelParams {
            kind: Some(channel_type::TEXT),
            name: Some("general".into()),
            parent_id: Some(None),
            topic: Some(Some("hi".into())),
            overwrites: Some(vec![Overwrite::role(5, 1 << 10, 1 << 11)]),
            applied_tags: Some(vec![9]),
            ..ChannelParams::default()
        }
        .body();
        assert_eq!(b["type"], 0);
        assert!(b["parent_id"].is_null());
        assert_eq!(b["permission_overwrites"][0]["id"], "5");
        assert_eq!(b["permission_overwrites"][0]["allow"], "1024");
        assert_eq!(b["permission_overwrites"][0]["deny"], "2048");
        assert_eq!(b["permission_overwrites"][0]["type"], 0);
        assert_eq!(b["applied_tags"][0], "9");
        assert!(b.get("position").is_none());
    }

    #[test]
    fn a_channel_parses_with_threads_tags_and_discord_only_bits() {
        let c = GuildChannel::from_json(&json!({
            "id": "10", "type": 15, "name": "ideas", "parent_id": "3", "position": 2,
            "permission_overwrites": [{"id": "7", "type": 0, "allow": "0", "deny": "9007199254740993"}],
            "available_tags": [{"id": "88", "name": "bug", "moderated": true, "emoji_id": null, "emoji_name": "x"}],
            "thread_metadata": {"archived": true, "locked": false, "auto_archive_duration": 60}
        }))
        .unwrap();
        assert_eq!(c.kind, channel_type::FORUM);
        assert_eq!(c.parent_id, Some(3));
        // A word above 2^53 must not lose a bit.
        assert_eq!(c.overwrites[0].deny, 9_007_199_254_740_993);
        assert_eq!(c.available_tags[0].id, Some(88));
        assert!(c.thread.unwrap().archived);
    }

    #[test]
    fn guild_facts_follow_the_tier() {
        let g = GuildInfo {
            id: 1,
            name: "x".into(),
            owner_id: None,
            premium_tier: 1,
            features: vec!["COMMUNITY".into()],
            system_channel_id: None,
            rules_channel_id: None,
        };
        assert!(g.is_community());
        assert!(!g.role_icons());
        assert_eq!(g.max_bitrate(), 128_000);
    }

    #[tokio::test]
    async fn the_pacer_spaces_writes() {
        let p = Pacer::new(100.0);
        let t = Instant::now();
        for _ in 0..4 {
            p.wait().await;
        }
        // Four writes at 100/s: the first is free, three gaps of 10 ms.
        assert!(t.elapsed() >= Duration::from_millis(25));
    }
}
