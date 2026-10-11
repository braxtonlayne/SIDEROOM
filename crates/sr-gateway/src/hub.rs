//! The gateway hub: session registry, per-guild subscriber sets, EventBus
//! driven fan-out with per-recipient permission filtering, presence, typing.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

use futures::StreamExt;
use serde_json::{Value, json};
use sr_core::{ChannelId, GuildId, UserId};
use sr_events::{DomainEvent, EventBus, Topic};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::protocol::PresenceUpdate;
use crate::session::{Out, Session};
use crate::traits::{AuthedUser, Authenticator, GuildDirectory};

#[derive(Debug, Clone)]
pub struct GatewayConfig {
    /// Sent in HELLO. Docs/05 6.4: 41250 ms.
    pub heartbeat_interval: Duration,
    /// Extra slack before a silent client is closed with 4009.
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
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: Duration::from_millis(41_250),
            heartbeat_grace: Duration::from_secs(10),
            resume_window: Duration::from_secs(120),
            resume_buffer: 5_000,
            resume_gateway_url: None,
            typing_ttl: Duration::from_secs(10),
            sweep_interval: Duration::from_secs(15),
        }
    }
}

#[derive(Default)]
struct State {
    sessions: HashMap<String, Arc<Session>>,
    by_guild: HashMap<GuildId, HashSet<String>>,
    guild_tasks: HashMap<GuildId, JoinHandle<()>>,
    presence: HashMap<UserId, PresenceUpdate>,
    typing: HashMap<(ChannelId, UserId), Instant>,
    background: Vec<JoinHandle<()>>,
}

pub(crate) struct Inner {
    pub cfg: GatewayConfig,
    pub auth: Arc<dyn Authenticator>,
    pub dir: Arc<dyn GuildDirectory>,
    bus: Arc<dyn EventBus>,
    state: Mutex<State>,
    next_conn: AtomicU64,
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
}

/// A dispatch whose `d` is the event's ready-made Discord payload object.
fn passthrough(name: &'static str, payload: &Value, channel: Option<ChannelId>) -> Dispatch {
    Dispatch {
        name,
        data: payload.clone(),
        channel,
    }
}

fn to_dispatch(ev: &DomainEvent) -> Option<Dispatch> {
    Some(match ev {
        DomainEvent::MessageCreated {
            guild,
            channel,
            message_id,
            author,
        } => Dispatch {
            name: "MESSAGE_CREATE",
            data: json!({
                "id": message_id.to_string(), "channel_id": channel, "guild_id": guild,
                "author": { "id": author },
            }),
            channel: Some(*channel),
        },
        DomainEvent::ChannelCreated { guild, channel } => Dispatch {
            name: "CHANNEL_CREATE",
            data: json!({ "id": channel, "guild_id": guild }),
            channel: Some(*channel),
        },
        DomainEvent::MemberJoined { guild, user } => Dispatch {
            name: "GUILD_MEMBER_ADD",
            data: json!({ "guild_id": guild, "user": { "id": user } }),
            channel: None,
        },
        DomainEvent::MemberLeft { guild, user } => Dispatch {
            name: "GUILD_MEMBER_REMOVE",
            data: json!({ "guild_id": guild, "user": { "id": user } }),
            channel: None,
        },
        DomainEvent::ChannelCreatedFull {
            channel, payload, ..
        } => passthrough("CHANNEL_CREATE", payload, Some(*channel)),
        DomainEvent::ChannelUpdated {
            channel, payload, ..
        } => passthrough("CHANNEL_UPDATE", payload, Some(*channel)),
        // The channel no longer exists, so it cannot be permission-filtered;
        // delivered to the whole guild.
        DomainEvent::ChannelDeleted { payload, .. } => passthrough("CHANNEL_DELETE", payload, None),
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
        // Handled as subscription lifecycle by the platform pump.
        DomainEvent::GuildCreated { .. }
        | DomainEvent::GuildDeleted { .. }
        | DomainEvent::FeatureSettingsChanged { .. } => return None,
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
            }),
        }
    }

    pub fn config(&self) -> &GatewayConfig {
        &self.inner.cfg
    }

    fn st(&self) -> MutexGuard<'_, State> {
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
    }

    pub(crate) fn next_conn_id(&self) -> u64 {
        self.inner.next_conn.fetch_add(1, Ordering::Relaxed)
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
    ) -> Arc<Session> {
        let cfg = &self.inner.cfg;
        let guilds = self.inner.dir.guilds_for_user(user.user_id).await;
        let mut payloads = Vec::with_capacity(guilds.len());
        for g in &guilds {
            payloads.push(self.inner.dir.guild_payload(user.user_id, *g).await);
        }
        let extras = self.inner.dir.ready_extras(user.user_id).await;
        let session = Arc::new(Session::new(
            Self::new_session_id(),
            user.user_id,
            intents,
            cfg.resume_window,
            cfg.resume_buffer,
            conn_id,
            tx,
        ));
        let mut ready = json!({
            "v": 1,
            "user": user.user,
            "guilds": guilds.iter().map(|g| json!({"id": g, "unavailable": true})).collect::<Vec<_>>(),
            "session_id": session.id,
            "resume_gateway_url": cfg.resume_gateway_url,
        });
        if let (Some(r), Some(e)) = (ready.as_object_mut(), extras.as_object()) {
            for (k, v) in e {
                r.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        session.deliver("READY", ready);
        for p in payloads {
            session.deliver("GUILD_CREATE", p);
        }
        for g in &guilds {
            session.add_guild(*g);
        }
        let first = {
            let mut st = self.st();
            st.sessions.insert(session.id.clone(), session.clone());
            for g in &guilds {
                st.by_guild
                    .entry(*g)
                    .or_default()
                    .insert(session.id.clone());
                self.ensure_guild_task(&mut st, *g);
            }
            !st.presence.contains_key(&user.user_id)
        };
        if first {
            self.apply_presence(user.user_id, PresenceUpdate::default())
                .await;
        }
        session
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
                    if let Some(d) = to_dispatch(&ev) {
                        Gateway { inner }
                            .dispatch_guild(g, d.name, d.data, d.channel)
                            .await;
                    }
                }
            }),
        );
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
        let last = {
            let mut st = self.st();
            st.sessions.remove(&s.id);
            for g in s.guilds() {
                if let Some(set) = st.by_guild.get_mut(&g) {
                    set.remove(&s.id);
                }
                Self::drop_guild_if_empty(&mut st, g);
            }
            let last = !st.sessions.values().any(|o| o.user == s.user);
            if last {
                st.presence.remove(&s.user);
            }
            last
        };
        if last {
            let upd = PresenceUpdate::offline();
            for g in s.guilds() {
                self.dispatch_guild(g, "PRESENCE_UPDATE", presence_body(s.user, g, &upd), None)
                    .await;
            }
        }
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
    ) {
        let targets: Vec<Arc<Session>> = {
            let st = self.st();
            st.by_guild
                .get(&guild)
                .into_iter()
                .flatten()
                .filter_map(|id| st.sessions.get(id).cloned())
                .filter(|s| s.wants(name))
                .collect()
        };
        let allowed: HashMap<UserId, bool> = match channel {
            None => targets.iter().map(|s| (s.user, true)).collect(),
            Some(ch) => {
                let users: HashSet<UserId> = targets.iter().map(|s| s.user).collect();
                let checks = users.into_iter().map(|u| {
                    let dir = self.inner.dir.clone();
                    async move { (u, dir.can_view_channel(u, guild, ch).await) }
                });
                futures::future::join_all(checks)
                    .await
                    .into_iter()
                    .collect()
            }
        };
        for s in targets {
            if allowed.get(&s.user).copied().unwrap_or(false) {
                s.deliver(name, data.clone());
            }
        }
    }

    /// Guild lifecycle events from the platform topic.
    async fn handle_platform(&self, ev: DomainEvent) {
        match ev {
            DomainEvent::GuildCreated { guild, owner: user }
            | DomainEvent::MemberJoined { guild, user } => self.join_user(user, guild).await,
            DomainEvent::MemberLeft { guild, user } => self.leave_user(user, guild),
            DomainEvent::GuildDeleted { guild } => self.drop_guild(guild),
            _ => {}
        }
    }

    async fn join_user(&self, user: UserId, guild: GuildId) {
        let sessions = self.user_sessions(user);
        if sessions.is_empty() {
            return;
        }
        let payload = self.inner.dir.guild_payload(user, guild).await;
        let mut st = self.st();
        for s in sessions {
            if s.add_guild(guild) {
                s.deliver("GUILD_CREATE", payload.clone());
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

    /// Current presence, if the user has any live or resumable session.
    pub fn presence(&self, user: UserId) -> Option<PresenceUpdate> {
        self.st().presence.get(&user).cloned()
    }

    /// Op 3 / API entry point: set presence and broadcast to the user's guilds.
    pub async fn set_presence(&self, user: UserId, update: PresenceUpdate) {
        self.apply_presence(user, update).await;
    }

    async fn apply_presence(&self, user: UserId, update: PresenceUpdate) {
        self.st().presence.insert(user, update.clone());
        let guilds: HashSet<GuildId> = self
            .user_sessions(user)
            .iter()
            .flat_map(|s| s.guilds())
            .collect();
        for g in guilds {
            self.dispatch_guild(g, "PRESENCE_UPDATE", presence_body(user, g, &update), None)
                .await;
        }
    }

    /// Record a typing indicator and broadcast `TYPING_START` to guild members
    /// who can view the channel. (REST `POST /channels/{id}/typing` calls this.)
    pub async fn typing_start(&self, user: UserId, guild: GuildId, channel: ChannelId) {
        self.st().typing.insert((channel, user), Instant::now());
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let data = json!({
            "channel_id": channel, "guild_id": guild, "user_id": user, "timestamp": ts,
        });
        self.dispatch_guild(guild, "TYPING_START", data, Some(channel))
            .await;
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

fn presence_body(user: UserId, guild: GuildId, p: &PresenceUpdate) -> Value {
    json!({
        "user": { "id": user }, "guild_id": guild,
        "status": p.visible_status(), "activities": p.activities,
    })
}
