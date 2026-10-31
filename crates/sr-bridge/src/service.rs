//! [`BridgeService`]: the handle the routes and the server wiring use.

use std::sync::Arc;

use sqlx::PgPool;
use sr_features::FeatureService;

use crate::error::BridgeResult;

/// Source of snowflake ids.
pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;

/// Seals and opens the bridge's secrets (the bot token, the webhook tokens) with `server.secret_key`, the same
/// layout as TOTP secrets (`sr_db::sealed::SecretBox`, D-185). `sr-server` implements it over a `SecretBox`.
/// The associated data ties a ciphertext to its row (the guild id for a bot token, the map id for a webhook).
pub trait TokenSealer: Send + Sync {
    fn seal(&self, aad: &[u8], plain: &[u8]) -> BridgeResult<Vec<u8>>;
    fn open(&self, aad: &[u8], blob: &[u8]) -> BridgeResult<Vec<u8>>;
}

/// The bridge over one database. Cheap to clone.
#[derive(Clone)]
pub struct BridgeService {
    pub pool: PgPool,
    pub features: FeatureService,
    pub(crate) ids: IdSource,
    pub(crate) sealer: Arc<dyn TokenSealer>,
}

impl BridgeService {
    pub fn new(
        pool: PgPool,
        features: FeatureService,
        ids: IdSource,
        sealer: Arc<dyn TokenSealer>,
    ) -> Self {
        Self {
            pool,
            features,
            ids,
            sealer,
        }
    }

    pub(crate) fn next_id(&self) -> i64 {
        (self.ids)()
    }
}
