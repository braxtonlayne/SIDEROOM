//! Resync after event-bus lag (SEC-AUD-169, D-646).
//!
//! When a gateway pump misses events the sessions behind it can be left with a stale picture (a deleted channel
//! still shown, a permission change not seen). The pump first does what D-628 says (credential re-check, caches,
//! member list, memberships); then it queues a *resync round* for its key: a server (`Key::Guild`), a user
//! (`Key::User`) or the platform. One worker takes rounds oldest first and re-sends state to the sessions that are
//! already connected, on the same socket and under the same session id, through `Session::deliver` so every
//! frame is sequenced and resume-safe:
//!
//! * server key: per member a fresh `GUILD_CREATE` (the same permission filter as at connect), a
//!   `THREAD_LIST_SYNC` when there is one, then the marker `SR_RESYNC {"scope":"guild","guild_id":".."}`;
//! * user key: a fresh `READY` with the same `session_id`, then each guild's `GUILD_CREATE`;
//! * platform key: the marker `SR_RESYNC {"scope":"platform"}` alone (clients refetch feature flags and
//!   announcements).
//!
//! Storm control: a floor between two rounds for one key (a lag inside the floor sets one flag and yields one
//! more round when the floor ends), at most `GatewayConfig::resync_queue_max` keys waiting (the oldest is
//! dropped with a warning), a node-wide cap of sessions told per second and at most
//! `GUILD_CREATE_CONCURRENCY` payload builds in flight. Bot sessions are not resynced.
//!
//! The snapshot race: a payload built at T0 and delivered at T1 could overwrite a live event the pump delivered in
//! between. The guild pump bumps a per-guild epoch for every event that changes what a `GUILD_CREATE` carries
//! (`crate::lists::changes_snapshot`); the worker compares the epoch before the build with the epoch at the
//! moment it delivers (in one step, no await between check and delivery) and rebuilds when it moved, at most
//! `MAX_REBUILDS` times. After that it delivers anyway and queues the key once more.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

use futures::StreamExt;
use serde_json::{Value, json};
use sr_core::{GuildId, UserId};
use tokio::sync::Notify;
use tokio::time::Instant;

use crate::hub::{GUILD_CREATE_CONCURRENCY, Gateway, GatewayConfig, Inner, State, session_entry};
use crate::session::Session;

/// Times a snapshot is rebuilt because a structural event overtook it, before it is delivered anyway.
const MAX_REBUILDS: usize = 2;

/// What a resync round repairs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Key {
    Guild(GuildId),
    User(UserId),
    Platform,
}

struct Waiting {
    /// Arrival order: the oldest key runs first.
    seq: u64,
    /// Not before this (the end of the previous round of the key plus the floor).
    ready_at: Instant,
}

#[derive(Default)]
struct Book {
    waiting: HashMap<Key, Waiting>,
    /// When the last round of a key ended, for the floor.
    last_end: HashMap<Key, Instant>,
    next_seq: u64,
}

struct Bucket {
    tokens: f64,
    at: Option<Instant>,
}

/// What the worker does next.
pub(crate) enum Take {
    Run(Key),
    /// Nothing is due; wake at the instant (or on a new request).
    Wait(Option<Instant>),
}

/// The queue of resync rounds, the floors and the node-wide rate cap.
pub(crate) struct ResyncQueue {
    floor: Duration,
    platform_floor: Duration,
    rate: f64,
    burst: f64,
    max: usize,
    book: Mutex<Book>,
    bucket: Mutex<Bucket>,
    wake: Notify,
}

impl ResyncQueue {
    pub(crate) fn new(cfg: &GatewayConfig) -> Self {
        let rate = f64::from(cfg.resync_sessions_per_second.max(1));
        Self {
            floor: cfg.resync_floor,
            platform_floor: cfg.resync_platform_floor,
            rate,
            burst: rate,
            max: cfg.resync_queue_max.max(1),
            book: Mutex::new(Book::default()),
            bucket: Mutex::new(Bucket {
                tokens: rate,
                at: None,
            }),
            wake: Notify::new(),
        }
    }

    fn book(&self) -> MutexGuard<'_, Book> {
        self.book.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn floor_of(&self, key: Key) -> Duration {
        match key {
            Key::Platform => self.platform_floor,
            Key::Guild(_) | Key::User(_) => self.floor,
        }
    }

    /// Ask for a round for `key`. A key already waiting needs nothing more (the round that is queued will
    /// carry the state of this lag too). Inside the floor the round waits until the floor ends. Past the bound the
    /// oldest waiting key is dropped.
    pub(crate) fn request(&self, key: Key) {
        let now = Instant::now();
        {
            let mut b = self.book();
            if b.waiting.contains_key(&key) {
                return;
            }
            let floor = self.floor_of(key);
            let ready_at = b
                .last_end
                .get(&key)
                .map(|t| *t + floor)
                .filter(|t| *t > now)
                .unwrap_or(now);
            if b.waiting.len() >= self.max {
                let oldest = b.waiting.iter().min_by_key(|(_, w)| w.seq).map(|(k, _)| *k);
                if let Some(k) = oldest {
                    b.waiting.remove(&k);
                    tracing::warn!(
                        dropped = ?k,
                        max = self.max,
                        "resync queue is full; dropping the oldest key (its clients resync on their next reconnect)"
                    );
                }
            }
            let seq = b.next_seq;
            b.next_seq += 1;
            b.waiting.insert(key, Waiting { seq, ready_at });
        }
        self.wake.notify_one();
    }

    /// The keys waiting for a round, oldest first.
    #[cfg(test)]
    pub(crate) fn waiting(&self) -> Vec<Key> {
        let b = self.book();
        let mut v: Vec<(u64, Key)> = b.waiting.iter().map(|(k, w)| (w.seq, *k)).collect();
        v.sort_by_key(|(s, _)| *s);
        v.into_iter().map(|(_, k)| k).collect()
    }

    /// The oldest key whose floor has ended, or when to look again.
    pub(crate) fn take(&self, now: Instant) -> Take {
        let mut b = self.book();
        let keep = self.floor.max(self.platform_floor);
        b.last_end
            .retain(|_, t| now.saturating_duration_since(*t) < keep);
        let due = b
            .waiting
            .iter()
            .filter(|(_, w)| w.ready_at <= now)
            .min_by_key(|(_, w)| w.seq)
            .map(|(k, _)| *k);
        if let Some(k) = due {
            b.waiting.remove(&k);
            return Take::Run(k);
        }
        Take::Wait(b.waiting.values().map(|w| w.ready_at).min())
    }

    /// A round of `key` ended: the floor counts from here, also for a round that was requested while it ran.
    pub(crate) fn finish(&self, key: Key, now: Instant) {
        let floor = self.floor_of(key);
        let mut b = self.book();
        b.last_end.insert(key, now);
        if let Some(w) = b.waiting.get_mut(&key) {
            w.ready_at = w.ready_at.max(now + floor);
        }
    }

    /// Reserve one session's worth of the node-wide rate (a token bucket whose burst is one second's worth) and
    /// wait until the reservation is due. Reservations are taken in call order, so callers do not starve each other.
    pub(crate) async fn acquire(&self) {
        let wait = {
            let mut b = self.bucket.lock().unwrap_or_else(|p| p.into_inner());
            let now = Instant::now();
            let at = b.at.unwrap_or(now);
            let refill = now.saturating_duration_since(at).as_secs_f64() * self.rate;
            b.tokens = (b.tokens + refill).min(self.burst) - 1.0;
            b.at = Some(now);
            if b.tokens < 0.0 {
                Duration::from_secs_f64(-b.tokens / self.rate)
            } else {
                Duration::ZERO
            }
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }
}

/// The worker: takes due rounds oldest first, one at a time.
pub(crate) async fn run(queue: Arc<ResyncQueue>, weak: Weak<Inner>) {
    loop {
        match queue.take(Instant::now()) {
            Take::Run(key) => {
                let Some(inner) = weak.upgrade() else { return };
                Gateway { inner }.run_resync(key).await;
                queue.finish(key, Instant::now());
            }
            Take::Wait(Some(at)) => {
                tokio::select! {
                    () = tokio::time::sleep_until(at) => {}
                    () = queue.wake.notified() => {}
                }
            }
            Take::Wait(None) => queue.wake.notified().await,
        }
    }
}

/// A session that may be sent a resync now: a user session with a socket attached that is still registered.
fn live(st: &State, s: &Session) -> bool {
    !s.bot && s.is_attached() && st.sessions.contains_key(&s.id)
}

impl Gateway {
    async fn run_resync(&self, key: Key) {
        match key {
            Key::Guild(g) => self.resync_guild(g).await,
            Key::User(u) => self.resync_user(u).await,
            Key::Platform => self.resync_platform().await,
        }
    }

    fn snapshot_epoch(&self, guild: GuildId) -> u64 {
        self.st().snapshot_epoch.get(&guild).copied().unwrap_or(0)
    }

    /// Server round: every live session of the guild gets its own member's `GUILD_CREATE`.
    async fn resync_guild(&self, guild: GuildId) {
        let sessions: Vec<Arc<Session>> = {
            let st = self.st();
            st.by_guild
                .get(&guild)
                .into_iter()
                .flatten()
                .filter_map(|id| st.sessions.get(id))
                .filter(|s| live(&st, s) && s.owns_guild(guild) && s.guilds().contains(&guild))
                .cloned()
                .collect()
        };
        // One build per member, however many sessions they hold.
        let mut order: Vec<UserId> = Vec::new();
        let mut by_user: HashMap<UserId, Vec<Arc<Session>>> = HashMap::new();
        for s in sessions {
            if !by_user.contains_key(&s.user) {
                order.push(s.user);
            }
            by_user.entry(s.user).or_default().push(s);
        }
        let groups: Vec<(UserId, Vec<Arc<Session>>)> = order
            .into_iter()
            .filter_map(|u| by_user.remove(&u).map(|v| (u, v)))
            .collect();
        let again: Vec<bool> = futures::stream::iter(groups)
            .map(|(user, sessions)| async move {
                for _ in &sessions {
                    self.inner.resync.acquire().await;
                }
                self.resync_guild_member(guild, user, &sessions).await
            })
            .buffered(GUILD_CREATE_CONCURRENCY)
            .collect()
            .await;
        if again.into_iter().any(|moved| moved) {
            self.inner.resync.request(Key::Guild(guild));
        }
    }

    /// Build and deliver one member's snapshot of `guild`. `true` when a structural event overtook it even after
    /// the rebuilds (the caller queues the key once more).
    async fn resync_guild_member(
        &self,
        guild: GuildId,
        user: UserId,
        sessions: &[Arc<Session>],
    ) -> bool {
        let mut rebuilt = 0;
        loop {
            let before = self.snapshot_epoch(guild);
            let body = self.guild_create_body(user, guild).await;
            let sync = self.inner.dir.thread_list_sync(user, guild).await;
            let st = self.st();
            let moved = st.snapshot_epoch.get(&guild).copied().unwrap_or(0) != before;
            if moved && rebuilt < MAX_REBUILDS {
                drop(st);
                rebuilt += 1;
                continue;
            }
            for s in sessions {
                // A member who left meanwhile (`leave_user`) or a socket that closed gets nothing.
                if !live(&st, s) || !s.guilds().contains(&guild) {
                    continue;
                }
                s.deliver("GUILD_CREATE", body.clone());
                if let Some(sy) = &sync
                    && s.wants("THREAD_LIST_SYNC")
                {
                    s.deliver("THREAD_LIST_SYNC", sy.clone());
                }
                s.deliver("SR_RESYNC", json!({ "scope": "guild", "guild_id": guild }));
            }
            return moved;
        }
    }

    /// User round: every live session of the user gets a fresh READY (same session id) and its guilds.
    async fn resync_user(&self, user: UserId) {
        let sessions: Vec<Arc<Session>> = {
            let candidates = self.user_sessions(user);
            let st = self.st();
            candidates.into_iter().filter(|s| live(&st, s)).collect()
        };
        let mut moved = false;
        for s in sessions {
            self.inner.resync.acquire().await;
            moved |= self.resync_user_session(&s).await;
        }
        if moved {
            self.inner.resync.request(Key::User(user));
        }
    }

    /// `true` when a structural event overtook the snapshot even after the rebuilds.
    async fn resync_user_session(&self, s: &Arc<Session>) -> bool {
        let user = s.user;
        let mut rebuilt = 0;
        loop {
            let mut guilds: Vec<GuildId> = s
                .guilds()
                .into_iter()
                .filter(|g| s.owns_guild(*g))
                .collect();
            guilds.sort_by_key(|g| g.get());
            let before: Vec<u64> = guilds.iter().map(|g| self.snapshot_epoch(*g)).collect();
            // Bodies first, then READY and the guilds back to back, as `create_session` does: the client batches
            // them into one store update.
            let built: Vec<(GuildId, Value, Option<Value>)> =
                futures::stream::iter(guilds.iter().copied())
                    .map(|g| async move {
                        let body = self.guild_create_body(user, g).await;
                        let sync = self.inner.dir.thread_list_sync(user, g).await;
                        (g, body, sync)
                    })
                    .buffered(GUILD_CREATE_CONCURRENCY)
                    .collect()
                    .await;
            let extras = self.inner.dir.ready_extras(user).await;
            let entries: Vec<Value> = self
                .user_sessions(user)
                .iter()
                .map(|x| session_entry(x))
                .collect();
            let ready = self.ready_body(&s.user_json(), &guilds, s, entries, &extras);
            let st = self.st();
            let moved = guilds
                .iter()
                .zip(&before)
                .any(|(g, e)| st.snapshot_epoch.get(g).copied().unwrap_or(0) != *e);
            if moved && rebuilt < MAX_REBUILDS {
                drop(st);
                rebuilt += 1;
                continue;
            }
            if !live(&st, s) {
                return false;
            }
            s.deliver("READY", ready);
            let held = s.guilds();
            for (g, body, sync) in built {
                if !held.contains(&g) {
                    continue;
                }
                s.deliver("GUILD_CREATE", body);
                if let Some(sy) = sync
                    && s.wants("THREAD_LIST_SYNC")
                {
                    s.deliver("THREAD_LIST_SYNC", sy);
                }
            }
            return moved;
        }
    }

    /// Platform round: the marker alone, to every live session (no payload to build).
    async fn resync_platform(&self) {
        let sessions: Vec<Arc<Session>> = {
            let st = self.st();
            st.sessions
                .values()
                .filter(|s| live(&st, s))
                .cloned()
                .collect()
        };
        for s in sessions {
            self.inner.resync.acquire().await;
            let alive = {
                let st = self.st();
                live(&st, &s)
            };
            if alive {
                s.deliver("SR_RESYNC", json!({ "scope": "platform" }));
            }
        }
    }
}
