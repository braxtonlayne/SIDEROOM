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
use sr_sdk::gateway::{Dispatch, GatewayConnector, ShardConfig, run_shard_with};
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
    | intents::GUILD_MESSAGE_REACTIONS
    | intents::GUILD_MESSAGE_TYPING;

/// The intents to try, richest first: members (privileged, for the bridged member list), then presences
/// (privileged, optional), each only when the owner enabled it in the developer portal. Discord answers a bot
/// asking for one it was not granted with close code 4014; the manager then steps down one rung and the bridge
/// keeps working with fewer extras (no member list, no presence), never failing for want of them.
pub fn intent_ladder(presence: bool) -> Vec<u64> {
    let members = INBOUND_INTENTS | intents::GUILD_MEMBERS;
    let mut ladder = Vec::new();
    if presence {
        ladder.push(members | intents::GUILD_PRESENCES);
    }
    ladder.push(members);
    ladder.push(INBOUND_INTENTS);
    ladder
}

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

/// Aborts the shard's connection task when dropped: aborting a bridge's task (a teardown, a stop) must also close
/// its gateway connection, which runs on a task of its own.
struct ShardGuard(Option<JoinHandle<sr_sdk::Result<()>>>);

impl Drop for ShardGuard {
    fn drop(&mut self) {
        if let Some(h) = self.0.take() {
            h.abort();
        }
    }
}

pub struct InboundManager {
    pool: PgPool,
    host: Arc<dyn InboundHost>,
    inbound: Arc<Inbound>,
    connector: Arc<dyn GatewayConnector>,
    running: Mutex<HashMap<i64, Running>>,
    /// Bridges whose shard ended on a fatal close, and when they may be tried again.
    cooling: Arc<Mutex<HashMap<i64, Instant>>>,
    /// The rung of [`intent_ladder`] each bridge is on.
    levels: Arc<Mutex<HashMap<i64, usize>>>,
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
            levels: Arc::new(Mutex::new(HashMap::new())),
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
            tracing::info!(guild, "bridge shard stopped");
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
                Err(e) => {
                    tracing::warn!(guild = b.guild_id, error = %e, "bridge shard did not start")
                }
            }
        }
        Ok(report)
    }

    async fn start(self: &Arc<Self>, bridge: Bridge) -> BridgeResult<()> {
        let token = self.host.bot_token(bridge.guild_id).await?;
        let ladder = intent_ladder(self.inbound.cfg.presence);
        let level = self
            .levels
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&bridge.guild_id)
            .copied()
            .unwrap_or(0)
            .min(ladder.len() - 1);
        let wanted = ladder[level];
        let cfg = ShardConfig::discord(token, wanted, self.inbound.cfg.gateway_url.as_deref());
        let guild = bridge.guild_id;
        let me = Arc::clone(self);
        let task = tokio::spawn(async move {
            me.run_bridge(bridge, cfg, level, ladder.len()).await;
        });
        self.lock_running().insert(guild, Running { task });
        tracing::info!(guild, "bridge shard started");
        Ok(())
    }

    /// The life of one bridge's shard: dispatches in, effects out, status kept.
    async fn run_bridge(
        self: Arc<Self>,
        bridge: Bridge,
        cfg: ShardConfig,
        level: usize,
        rungs: usize,
    ) {
        let guild = bridge.guild_id;
        let used = cfg.intents;
        let (tx, mut rx) = mpsc::channel::<Dispatch>(256);
        let (out_tx, out_rx) = mpsc::channel::<String>(8);
        let connector = Arc::clone(&self.connector);
        // The connection runs on its own task; the guard aborts it when this task is aborted (a teardown or a
        // stop), so the bot does not stay connected to Discord after the bridge is gone.
        let mut shard = ShardGuard(Some(tokio::spawn(async move {
            run_shard_with(cfg, connector.as_ref(), tx, Some(out_rx)).await
        })));
        while let Some(d) = rx.recv().await {
            if d.name == "READY" {
                let _ = store::set_status(&self.pool, guild, BridgeStatus::Connected).await;
                let _ = store::set_intents_granted(&self.pool, guild, used).await;
                // The initial member sync: everyone (and their presence when that intent is on).
                if used & intents::GUILD_MEMBERS != 0 {
                    let frame = serde_json::json!({"op": 8, "d": {
                        "guild_id": bridge.discord_guild_id.to_string(),
                        "query": "", "limit": 0,
                        "presences": used & intents::GUILD_PRESENCES != 0}});
                    let _ = out_tx.send(frame.to_string()).await;
                }
                continue;
            }
            if let Err(e) = self.inbound.handle(&bridge, &d).await {
                // Never the token; the error text of a Discord or storage failure carries none.
                tracing::warn!(guild, event = %d.name, error = %e, "bridge inbound dispatch failed");
            }
        }
        // The dispatch channel closed: the shard ended.
        let Some(handle) = shard.0.take() else {
            return;
        };
        let status = match handle.await {
            Ok(Err(sr_sdk::Error::GatewayFatal { code, reason })) => {
                tracing::warn!(guild, code, reason, "bridge gateway closed for good");
                if code == 4004 {
                    BridgeStatus::TokenInvalid
                } else if code == 4014 && level + 1 < rungs {
                    // An optional privileged intent was not granted: try again with fewer, at the next pass.
                    self.levels
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .insert(guild, level + 1);
                    BridgeStatus::Disconnected
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

/// A teardown disconnects the shard at once (the supervisor would notice within one interval anyway).
#[async_trait]
impl crate::service::TeardownHook for InboundManager {
    async fn disconnect(&self, guild: i64) {
        self.stop(guild);
    }
}

/// [`InboundHost`] over the bridge service: the sealed token opened by it, the module and platform switches read
/// from the feature service.
pub struct ServiceHost(pub crate::service::BridgeService);

#[async_trait]
impl InboundHost for ServiceHost {
    async fn bot_token(&self, guild: i64) -> BridgeResult<String> {
        self.0.open_bot_token(guild).await
    }
    async fn active(&self, guild: i64) -> BridgeResult<bool> {
        Ok(self.0.platform_on().await? && self.0.module_on(guild).await?)
    }
}
