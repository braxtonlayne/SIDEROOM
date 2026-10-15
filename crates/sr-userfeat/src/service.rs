//! The handle every module hangs off.

use std::sync::Arc;

use sqlx::PgPool;
use sr_features::{FeatureService, ScopeRef};

use crate::config::Settings;
use crate::error::{UfError, UfResult};

/// Source of snowflake ids.
pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;

/// The feature modules over one database. Cheap to clone.
#[derive(Clone)]
pub struct UserFeatures {
    pub pool: PgPool,
    pub features: FeatureService,
    pub ids: IdSource,
}

fn scope_of(guild: Option<i64>) -> ScopeRef {
    match guild.and_then(|g| u64::try_from(g).ok()) {
        Some(g) => ScopeRef::Guild {
            guild: sr_core::GuildId::new(g),
        },
        None => ScopeRef::Platform,
    }
}

impl UserFeatures {
    pub fn new(pool: PgPool, features: FeatureService, ids: IdSource) -> Self {
        Self {
            pool,
            features,
            ids,
        }
    }

    pub(crate) fn next_id(&self) -> i64 {
        (self.ids)()
    }

    /// Whether `feature` is on for a server (or, with `None`, for the platform: DMs and account-level uses).
    pub async fn enabled(&self, feature: &str, guild: Option<i64>) -> UfResult<bool> {
        Ok(self.features.is_enabled(feature, &scope_of(guild)).await?)
    }

    /// [`Self::enabled`] as an error.
    pub async fn require(&self, feature: &'static str, guild: Option<i64>) -> UfResult<()> {
        if self.enabled(feature, guild).await? {
            Ok(())
        } else {
            Err(UfError::Disabled(feature))
        }
    }

    /// The effective settings of a module at a server (or the platform), as a typed reader.
    pub async fn settings(&self, feature: &str, guild: Option<i64>) -> UfResult<Settings> {
        Ok(Settings(
            self.features
                .effective_settings(feature, &scope_of(guild))
                .await?,
        ))
    }

    /// Servers where `feature` has been switched on explicitly and is not killed platform-wide.
    pub async fn guilds_with(&self, feature: &str) -> UfResult<Vec<i64>> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT scope_id FROM feature_enablements \
             WHERE feature = $1 AND scope = 'guild'::enablement_scope AND enabled IS TRUE \
             ORDER BY scope_id",
        )
        .bind(feature)
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::new();
        for g in ids {
            if self.enabled(feature, Some(g)).await.unwrap_or(false) {
                out.push(g);
            }
        }
        Ok(out)
    }
}
