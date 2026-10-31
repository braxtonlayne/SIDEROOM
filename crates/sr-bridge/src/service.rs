//! [`BridgeService`]: the handle the routes and the server wiring use.
//!
//! The service is split by concern: this file holds the handle and its ports; `tokens` connects and replaces the
//! bot token; `maps` maps and unmaps channels; `outbound` turns messages into queue rows and drains the queue;
//! `teardown` is the clean stop; `status` reads what the owner's screen shows.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use sqlx::PgPool;
use sr_features::modules::bridge::{BRIDGE_MODULE_ID, PLATFORM_BRIDGE_ID};
use sr_features::{FeatureService, ScopeRef};

use crate::discord::DiscordApi;
use crate::error::{BridgeError, BridgeResult};

/// Source of snowflake ids.
pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;

/// The platform's own user, the actor of an audit row nobody triggered by hand (a platform kill).
pub const SYSTEM_USER_ID: i64 = 1;
/// Discord audit-log event number the bridge writes: `IntegrationCreate` when it is connected,
/// `IntegrationDelete` when it is torn down.
pub const AUDIT_INTEGRATION_CREATE: i16 = 80;
pub const AUDIT_INTEGRATION_DELETE: i16 = 82;

/// Seals and opens the bridge's secrets (the bot token, the webhook tokens) with `server.secret_key`, the same
/// layout as TOTP secrets (`sr_db::sealed::SecretBox`, D-185). `sr-server` implements it over a `SecretBox`.
/// The associated data ties a ciphertext to its row (the guild id for a bot token, the map id for a webhook).
pub trait TokenSealer: Send + Sync {
    fn seal(&self, aad: &[u8], plain: &[u8]) -> BridgeResult<Vec<u8>>;
    fn open(&self, aad: &[u8], blob: &[u8]) -> BridgeResult<Vec<u8>>;
}

/// Signed links that leave the instance: the author's avatar and attachment links. `path` is a relative
/// `/cdn/...` path; the result is an absolute URL carrying the signature and an expiry `ttl_secs` from now.
pub trait MediaUrls: Send + Sync {
    fn signed_url(&self, path: &str, ttl_secs: u64) -> String;
}

/// The development default: the path itself (tests; the server wires the real signer).
pub struct RelativeUrls;

impl MediaUrls for RelativeUrls {
    fn signed_url(&self, path: &str, _ttl_secs: u64) -> String {
        path.to_owned()
    }
}

/// The system user speaking in a SIDE ROOM channel (a map was created or removed, the bridge was switched off).
#[async_trait]
pub trait BridgeNotices: Send + Sync {
    async fn system_message(&self, channel: i64, text: &str);
}

/// Notices go nowhere (tests that do not look at them).
pub struct NoNotices;

#[async_trait]
impl BridgeNotices for NoNotices {
    async fn system_message(&self, _channel: i64, _text: &str) {}
}

/// Something that must stop when a bridge is torn down: the gateway half disconnects the shard.
#[async_trait]
pub trait TeardownHook: Send + Sync {
    async fn disconnect(&self, guild: i64);
}

/// Waits and limits, overridable so tests do not sleep.
#[derive(Debug, Clone)]
pub struct Tuning {
    /// Waits between the attempts to delete a webhook on Discord (so `len() + 1` attempts: three by default).
    pub delete_backoff: Vec<Duration>,
    /// Attempts at one queue row before it becomes a dead letter.
    pub max_attempts: i32,
    /// First wait after a failed send; doubles per attempt up to `retry_cap`.
    pub retry_base: Duration,
    pub retry_cap: Duration,
    /// Rows the worker takes per pass.
    pub batch: usize,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            delete_backoff: vec![Duration::from_millis(500), Duration::from_secs(2)],
            max_attempts: 8,
            retry_base: Duration::from_secs(2),
            retry_cap: Duration::from_secs(300),
            batch: 50,
        }
    }
}

/// The bridge over one database. Cheap to clone.
#[derive(Clone)]
pub struct BridgeService {
    pub pool: PgPool,
    pub features: FeatureService,
    pub discord: DiscordApi,
    pub(crate) ids: IdSource,
    pub(crate) sealer: Arc<dyn TokenSealer>,
    pub(crate) media: Arc<dyn MediaUrls>,
    pub(crate) notices: Arc<dyn BridgeNotices>,
    pub(crate) hooks: Arc<RwLock<Vec<Arc<dyn TeardownHook>>>>,
    pub(crate) tuning: Tuning,
    pub(crate) teardown_lock: Arc<tokio::sync::Mutex<()>>,
}

impl BridgeService {
    pub fn new(
        pool: PgPool,
        features: FeatureService,
        ids: IdSource,
        sealer: Arc<dyn TokenSealer>,
        discord: DiscordApi,
    ) -> Self {
        Self {
            pool,
            features,
            discord,
            ids,
            sealer,
            media: Arc::new(RelativeUrls),
            notices: Arc::new(NoNotices),
            hooks: Arc::new(RwLock::new(Vec::new())),
            tuning: Tuning::default(),
            teardown_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub fn with_media(mut self, media: Arc<dyn MediaUrls>) -> Self {
        self.media = media;
        self
    }

    pub fn with_notices(mut self, notices: Arc<dyn BridgeNotices>) -> Self {
        self.notices = notices;
        self
    }

    pub fn with_tuning(mut self, tuning: Tuning) -> Self {
        self.tuning = tuning;
        self
    }

    /// Register something that must stop when a server's bridge is torn down (the gateway shard).
    pub fn add_teardown_hook(&self, hook: Arc<dyn TeardownHook>) {
        if let Ok(mut hooks) = self.hooks.write() {
            hooks.push(hook);
        }
    }

    pub(crate) fn next_id(&self) -> i64 {
        (self.ids)()
    }

    pub(crate) fn guild_scope(guild: i64) -> ScopeRef {
        ScopeRef::Guild {
            guild: sr_core::GuildId::new(u64::try_from(guild).unwrap_or_default()),
        }
    }

    /// The operator's switch is on (the default).
    pub async fn platform_on(&self) -> BridgeResult<bool> {
        Ok(!self.features.platform_killed(PLATFORM_BRIDGE_ID).await?)
    }

    /// The server's module is on (and the platform switch is on: a killed parent turns the module off).
    pub async fn module_on(&self, guild: i64) -> BridgeResult<bool> {
        Ok(self
            .features
            .is_enabled(BRIDGE_MODULE_ID, &Self::guild_scope(guild))
            .await?)
    }

    pub(crate) fn bot_aad(guild: i64) -> Vec<u8> {
        format!("bridge:bot:{guild}").into_bytes()
    }

    pub(crate) fn webhook_aad(map: i64) -> Vec<u8> {
        format!("bridge:webhook:{map}").into_bytes()
    }

    /// The bot token of a server, opened. For the Discord client and the gateway half; never log or return it.
    pub async fn open_bot_token(&self, guild: i64) -> BridgeResult<String> {
        let blob: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT bot_token_enc FROM bridges WHERE guild_id = $1")
                .bind(guild)
                .fetch_optional(&self.pool)
                .await?;
        let blob = blob.ok_or(BridgeError::NotFound("bridge"))?;
        let plain = self.sealer.open(&Self::bot_aad(guild), &blob)?;
        String::from_utf8(plain).map_err(|_| BridgeError::Seal)
    }

    /// Write one audit-log row (Discord-shaped, append-only table) on `conn`.
    pub(crate) async fn audit(
        &self,
        conn: &mut sqlx::PgConnection,
        row: AuditRow,
    ) -> BridgeResult<()> {
        sqlx::query(
            "INSERT INTO audit_log_entries (id, guild_id, actor_id, target_id, action, changes, options) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(self.next_id())
        .bind(row.guild)
        .bind(row.actor)
        .bind(row.target)
        .bind(row.action)
        .bind(row.changes)
        .bind(row.options)
        .execute(conn)
        .await?;
        Ok(())
    }
}

/// One audit-log row to write.
pub(crate) struct AuditRow {
    pub guild: i64,
    pub actor: i64,
    pub action: i16,
    pub target: Option<i64>,
    pub changes: serde_json::Value,
    pub options: serde_json::Value,
}
