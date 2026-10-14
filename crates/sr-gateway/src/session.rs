//! One gateway session: sequence counter, replay buffer, live connection slot.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde_json::{Value, json};
use sr_core::{GuildId, UserId};
use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::protocol::{Payload, op};

/// Messages from a session to its connection task.
#[derive(Debug, Clone)]
pub(crate) enum Out {
    Frame(Arc<str>),
    Reconnect,
}

struct Buffered {
    seq: u64,
    at: Instant,
    text: Arc<str>,
}

struct Link {
    conn_id: u64,
    tx: mpsc::Sender<Out>,
}

struct Inner {
    seq: u64,
    buf: VecDeque<Buffered>,
    link: Option<Link>,
    detached_at: Option<Instant>,
    guilds: HashSet<GuildId>,
}

pub(crate) struct Session {
    pub id: String,
    pub user: UserId,
    pub intents: u64,
    window: Duration,
    capacity: usize,
    inner: Mutex<Inner>,
}

impl Session {
    pub fn new(
        id: String,
        user: UserId,
        intents: u64,
        window: Duration,
        capacity: usize,
        conn_id: u64,
        tx: mpsc::Sender<Out>,
    ) -> Self {
        Self {
            id,
            user,
            intents,
            window,
            capacity,
            inner: Mutex::new(Inner {
                seq: 0,
                buf: VecDeque::new(),
                link: Some(Link { conn_id, tx }),
                detached_at: None,
                guilds: HashSet::new(),
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn trim(&self, g: &mut Inner, now: Instant) {
        while g.buf.len() > self.capacity
            || g.buf
                .front()
                .is_some_and(|b| now.duration_since(b.at) > self.window)
        {
            g.buf.pop_front();
        }
    }

    /// Assign the next sequence number, buffer for resume, push to the live
    /// connection. A full or closed queue detaches the connection (the client
    /// then resumes from the buffer).
    pub fn deliver(&self, name: &str, data: Value) {
        let now = Instant::now();
        let mut g = self.lock();
        g.seq += 1;
        let seq = g.seq;
        let mut p = Payload::new(op::DISPATCH, data);
        p.s = Some(seq);
        p.t = Some(name.to_owned());
        let text: Arc<str> = p.to_text().into();
        g.buf.push_back(Buffered {
            seq,
            at: now,
            text: text.clone(),
        });
        self.trim(&mut g, now);
        if let Some(l) = &g.link
            && l.tx.try_send(Out::Frame(text)).is_err()
        {
            g.link = None;
            g.detached_at = Some(now);
        }
    }

    pub fn deliver_marker(&self, name: &str) {
        self.deliver(name, json!({}));
    }

    /// Attach a new connection and replay everything after `after`.
    /// `false` when the buffer no longer covers the gap or `after` is ahead.
    pub fn resume(&self, conn_id: u64, tx: mpsc::Sender<Out>, after: u64) -> bool {
        let now = Instant::now();
        let mut g = self.lock();
        self.trim(&mut g, now);
        if after > g.seq {
            return false;
        }
        if after < g.seq && g.buf.front().is_none_or(|b| b.seq > after + 1) {
            return false;
        }
        for b in g.buf.iter().filter(|b| b.seq > after) {
            if tx.try_send(Out::Frame(b.text.clone())).is_err() {
                return false;
            }
        }
        g.link = Some(Link { conn_id, tx });
        g.detached_at = None;
        true
    }

    pub fn detach(&self, conn_id: u64) {
        let mut g = self.lock();
        if g.link.as_ref().is_some_and(|l| l.conn_id == conn_id) {
            g.link = None;
            g.detached_at = Some(Instant::now());
        }
    }

    pub fn send_reconnect(&self) {
        if let Some(l) = &self.lock().link {
            let _ = l.tx.try_send(Out::Reconnect);
        }
    }

    pub fn is_expired(&self) -> bool {
        self.lock()
            .detached_at
            .is_some_and(|t| Instant::now().duration_since(t) > self.window)
    }

    /// How long the session has had no socket attached; `None` while one is.
    pub fn detached_for(&self, now: Instant) -> Option<Duration> {
        self.lock()
            .detached_at
            .map(|t| now.saturating_duration_since(t))
    }

    /// Whether a live socket is currently attached (not just resumable).
    pub fn is_attached(&self) -> bool {
        self.lock().link.is_some()
    }

    pub fn guilds(&self) -> Vec<GuildId> {
        self.lock().guilds.iter().copied().collect()
    }

    pub fn add_guild(&self, g: GuildId) -> bool {
        self.lock().guilds.insert(g)
    }

    pub fn remove_guild(&self, g: GuildId) -> bool {
        self.lock().guilds.remove(&g)
    }

    /// Whether this session should receive `event` given its intents.
    pub fn wants(&self, event: &str) -> bool {
        crate::protocol::intents::required_for(event).is_none_or(|bit| self.intents & bit != 0)
    }
}

/// What an [`crate::traits::OpHandler`] gets to see of an authenticated session.
#[derive(Clone)]
pub struct SessionHandle {
    inner: Arc<Session>,
}

impl SessionHandle {
    pub(crate) fn new(inner: Arc<Session>) -> Self {
        Self { inner }
    }

    pub fn id(&self) -> &str {
        &self.inner.id
    }

    pub fn user(&self) -> UserId {
        self.inner.user
    }

    /// Send a dispatch to this session only.
    pub fn deliver(&self, name: &str, data: Value) {
        self.inner.deliver(name, data);
    }
}
