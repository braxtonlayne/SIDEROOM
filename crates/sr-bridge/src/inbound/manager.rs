//! One gateway shard per bridge, at most `bridge.max_bridges` per instance, kept in step with the database.
//!
//! [`InboundManager::reconcile`] makes the set of running shards equal the set of bridges that should have one:
//! the module is on, the platform switch is on, Discord has not refused the token, and the instance is under its
//! limit. A supervisor task runs it on an interval, so a bridge switched on gets its shard within one interval,
//! and a teardown (maps and token gone, module off) loses it the same way; the teardown may also call
//! [`InboundManager::stop`] for an immediate stop. The shard itself is `sr-sdk`'s: reconnect, resume and the
//! heartbeat are its, and its fatal close codes end the task and set the bridge's status.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use sqlx::PgPool;
use sr_sdk::gateway::{Dispatch, GatewayConnector, ShardConfig, run_shard};
use sr_sdk::intents;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::handler::Inbound;
use super::store;
use crate::error::BridgeResult;
use crate::model::{Bridge, BridgeStatus};

/// `GUILDS | GUILD_MESSAGES | MESSAGE_CONTENT | GUILD_MESSAGE_REACTIONS`. `MESSAGE_CONTENT` is privileged: the
/// owner turns it on for their bot in the Discord developer portal, and without it Discord closes with 4014.
pub const INBOUND_INTENTS: u64 = intents::GUILDS
    | intents::GUILD_MESSAGES
    | intents::MESSAGE_CONTENT
    | intents::GUILD_MESSAGE_REACTIONS;

/// How long a bridge whose shard ended on a fatal close (not a bad token) is left alone before it is tried again.
const RETRY_AFTER_FATAL: Duration = Duration::from_secs(300);

/// What the manager needs from the rest of the bridge: the sealed token, opened, and whether the bridge is
/// switched on. The server wiring implements it over `BridgeService` (`open_bot_token`, `module_on`,
/// `platform_on`).
#[async_trait]
pub trait InboundHost: Send + Sync {
    /// The bot token of a server. Never log or return it.
    async fn bot_token(&self, guild: i64) -> BridgeResult<String>;
    /// The platform switch and the server's module are both on.
    async fn active(&self, guild: i64) -> BridgeResult<bool>;
}

/// What one pass of [`InboundManager::reconcile`] did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Reconciled {
    pub started: Vec<i64>,
    pub stopped: Vec<i64>,
    /// Bridges that should run but are over `bridge.max_bridges`.
    pub over_limit: Vec<i64>,
}

struct Running {
    task: JoinHandle<()>,
}

pub struct InboundManager {
    pool: PgPool,
    host: Arc<dyn InboundHost>,
    inbound: Arc<Inbound>,
    connector: Arc<dyn GatewayConnector>,
    running: Mutex<HashMap<i64, Running>>,
    /// Bridges whose shard ended on a fatal close, and when they may be tried again.
    cooling: Arc<Mutex<HashMap<i64, Instant>>>,
}

impl InboundManager {
    pub fn new(
        pool: PgPool,
        host: Arc<dyn InboundHost>,
        inbound: Arc<Inbound>,
        connector: Arc<dyn GatewayConnector>,
    ) -> Arc<Self> {
        Arc::new(Self {
            pool,
            host,
            inbound,
            connector,
            running: Mutex::new(HashMap::new()),
            cooling: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Servers with a shard running now.
    pub fn running(&self) -> Vec<i64> {
        let mut v: Vec<i64> = self.lock_running().keys().copied().collect();
        v.sort_unstable();
        v
    }

    fn lock_running(&self) -> std::sync::MutexGuard<'_, HashMap<i64, Running>> {
        self.running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Stop one server's shard now (a teardown, or the token was revoked). Idempotent.
    pub fn stop(&self, guild: i64) {
        if let Some(r) = self.lock_running().remove(&guild) {
            r.task.abort();
        }
        self.inbound.limiter.forget(guild);
    }

    /// Stop every shard (shutdown).
    pub fn stop_all(&self) {
        let all: Vec<i64> = self.lock_running().keys().copied().collect();
        for g in all {
            self.stop(g);
        }
    }

    /// Make the running shards match the bridges that should run. See the module comment.
    pub async fn reconcile(self: &Arc<Self>) -> BridgeResult<Reconciled> {
        let mut report = Reconciled::default();
        // Forget shards whose task ended by itself (a fatal close).
        self.lock_running().retain(|_, r| !r.task.is_finished());
        let limit = (self.inbound.cfg.max_bridges)();
        let candidates = store::startable(&self.pool).await?;
        let mut want: Vec<Bridge> = Vec::new();
        for b in candidates {
            if self.host.active(b.guild_id).await? {
                want.push(b);
            }
        }
        // Stop what is no longer wanted.
        let wanted: Vec<i64> = want.iter().map(|b| b.guild_id).collect();
        let stale: Vec<i64> = self
            .lock_running()
            .keys()
            .filter(|g| !wanted.contains(g))
            .copied()
            .collect();
        for g in stale {
            self.stop(g);
            report.stopped.push(g);
        }
        // Start what is missing, oldest bridge first, up to the limit.
        let now = Instant::now();
        for b in want {
            if self.lock_running().contains_key(&b.guild_id) {
                continue;
            }
            if self.lock_running().len() >= limit {
                tracing::warn!(
                    guild = b.guild_id,
                    limit,
                    "bridge not started: bridge.max_bridges reached"
                );
                report.over_limit.push(b.guild_id);
                continue;
            }
            let cooling = self
                .cooling
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&b.guild_id)
                .is_some_and(|until| *until > now);
            if cooling {
                continue;
            }
            match self.start(b.clone()).await {
                Ok(()) => report.started.push(b.guild_id),
                Err(e) => tracing::warn!(guild = b.guild_id, error = %e, "bridge shard did not start"),
            }
        }
        Ok(report)
    }

    async fn start(self: &Arc<Self>, bridge: Bridge) -> BridgeResult<()> {
        let token = self.host.bot_token(bridge.guild_id).await?;
        let cfg = ShardConfig::discord(
            token,
            INBOUND_INTENTS,
            self.inbound.cfg.gateway_url.as_deref(),
        );
        let guild = bridge.guild_id;
        let me = Arc::clone(self);
        let task = tokio::spawn(async move {
            me.run_bridge(bridge, cfg).await;
        });
        self.lock_running().insert(guild, Running { task });
        Ok(())
    }

    /// The life of one bridge's shard: dispatches in, effects out, status kept.
    async fn run_bridge(self: Arc<Self>, bridge: Bridge, cfg: ShardConfig) {
        let guild = bridge.guild_id;
        let (tx, mut rx) = mpsc::channel::<Dispatch>(256);
        let connector = Arc::clone(&self.connector);
        let shard = tokio::spawn(async move { run_shard(cfg, connector.as_ref(), tx).await });
        while let Some(d) = rx.recv().await {
            if d.name == "READY" {
                let _ = store::set_status(&self.pool, guild, BridgeStatus::Connected).await;
                let _ = store::set_intents_granted(&self.pool, guild, INBOUND_INTENTS).await;
                continue;
            }
            if let Err(e) = self.inbound.handle(&bridge, &d).await {
                // Never the token; the error text of a Discord or storage failure carries none.
                tracing::warn!(guild, event = %d.name, error = %e, "bridge inbound dispatch failed");
            }
        }
        // The dispatch channel closed: the shard ended.
        let status = match shard.await {
            Ok(Err(sr_sdk::Error::GatewayFatal { code, reason })) => {
                tracing::warn!(guild, code, reason, "bridge gateway closed for good");
                if code == 4004 {
                    BridgeStatus::TokenInvalid
                } else {
                    self.cooling
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .insert(guild, Instant::now() + RETRY_AFTER_FATAL);
                    BridgeStatus::Disconnected
                }
            }
            _ => BridgeStatus::Disconnected,
        };
        let _ = store::set_status(&self.pool, guild, status).await;
    }

    /// Run [`Self::reconcile`] now and then, until the returned handle is aborted.
    pub fn spawn_supervisor(self: &Arc<Self>, every: Duration) -> JoinHandle<()> {
        let me = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                if let Err(e) = me.reconcile().await {
                    tracing::warn!(error = %e, "bridge reconcile failed");
                }
                tokio::time::sleep(every).await;
            }
        })
    }
}
