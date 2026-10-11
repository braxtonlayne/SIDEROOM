//! Postgres [`SettingsStore`] over `feature_enablements` (migration 0003).
//!
//! Since migration 0050 `enabled` is nullable (NULL = inherit) and the scope
//! enum includes `channel`.

use async_trait::async_trait;
use serde_json::{Map, Value, json};
use sqlx::{PgPool, Row};
use sr_core::{SnowflakeGenerator, SystemClock};
use sr_features::{ScopeRef, SettingsRecord, SettingsStore, store::StoreError};
use std::sync::Arc;

pub struct PgSettingsStore {
    pool: PgPool,
    ids: Arc<SnowflakeGenerator<SystemClock>>,
}

impl PgSettingsStore {
    pub fn new(pool: PgPool, ids: Arc<SnowflakeGenerator<SystemClock>>) -> Self {
        Self { pool, ids }
    }
}

fn key(scope: &ScopeRef) -> Result<(&'static str, i64), StoreError> {
    match scope {
        ScopeRef::Platform => Ok(("platform", 0)),
        ScopeRef::Guild { guild } => Ok(("guild", guild.get() as i64)),
        ScopeRef::User { user } => Ok(("user", user.get() as i64)),
        ScopeRef::Channel { channel, .. } => Ok(("channel", channel.get() as i64)),
    }
}

fn be(e: sqlx::Error) -> StoreError {
    StoreError::Backend(e.to_string())
}

#[async_trait]
impl SettingsStore for PgSettingsStore {
    async fn get(
        &self,
        feature_id: &str,
        scope: &ScopeRef,
    ) -> Result<Option<SettingsRecord>, StoreError> {
        let (kind, id) = key(scope)?;
        let row = sqlx::query(
            "SELECT enabled, config FROM feature_enablements \
             WHERE feature = $1 AND scope = $2::enablement_scope AND scope_id = $3",
        )
        .bind(feature_id)
        .bind(kind)
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(be)?;
        let Some(r) = row else { return Ok(None) };
        // NULL means "inherit" since migration 0050.
        let enabled: Option<bool> = r.try_get("enabled").map_err(be)?;
        let config: Value = r.try_get("config").map_err(be)?;
        let inherit = config.get("inherit").and_then(Value::as_bool) == Some(true);
        let values: Map<String, Value> = config
            .get("values")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        Ok(Some(SettingsRecord {
            enabled: if inherit { None } else { enabled },
            values,
        }))
    }

    async fn put(
        &self,
        feature_id: &str,
        scope: &ScopeRef,
        record: SettingsRecord,
    ) -> Result<(), StoreError> {
        let (kind, id) = key(scope)?;
        let config = json!({ "inherit": record.enabled.is_none(), "values": record.values });
        sqlx::query(
            "INSERT INTO feature_enablements (id, feature, scope, scope_id, enabled, config) \
             VALUES ($1, $2, $3::enablement_scope, $4, $5, $6) \
             ON CONFLICT (feature, scope, scope_id) DO UPDATE \
             SET enabled = EXCLUDED.enabled, config = EXCLUDED.config, updated_at = now()",
        )
        .bind(self.ids.generate().get() as i64)
        .bind(feature_id)
        .bind(kind)
        .bind(id)
        .bind(record.enabled)
        .bind(config)
        .execute(&self.pool)
        .await
        .map_err(be)?;
        Ok(())
    }

    async fn delete(&self, feature_id: &str, scope: &ScopeRef) -> Result<(), StoreError> {
        let (kind, id) = key(scope)?;
        sqlx::query(
            "DELETE FROM feature_enablements \
             WHERE feature = $1 AND scope = $2::enablement_scope AND scope_id = $3",
        )
        .bind(feature_id)
        .bind(kind)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(be)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_core::{ChannelId, GuildId};

    #[test]
    fn channel_scope_uses_the_channel_id() {
        let s = ScopeRef::Channel {
            guild: GuildId::new(1),
            channel: ChannelId::new(2),
        };
        assert_eq!(key(&s).unwrap(), ("channel", 2));
        assert_eq!(key(&ScopeRef::Platform).unwrap(), ("platform", 0));
    }
}
