//! Postgres [`SettingsStore`] over `feature_enablements` (migrations 0003 and
//! 0050). `enabled` is nullable: `NULL` means "inherit from the parent scope"
//! (`enabled: None`), and every scope including `channel` is stored natively.

use async_trait::async_trait;
use serde_json::{Map, Value, json};
use sqlx::{PgPool, Row};
use sr_core::{SnowflakeGenerator, SystemClock};
use sr_features::{
    ScopeRef, SettingsRecord, SettingsStore,
    state::{FeatureState, StateEntry, StateError},
    store::StoreError,
};
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

/// `(enablement_scope value, scope_id)` for a scope.
fn key(scope: &ScopeRef) -> (&'static str, i64) {
    match scope {
        ScopeRef::Platform => ("platform", 0),
        ScopeRef::Guild { guild } => ("guild", guild.get() as i64),
        ScopeRef::Channel { channel, .. } => ("channel", channel.get() as i64),
        ScopeRef::User { user } => ("user", user.get() as i64),
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
        let (kind, id) = key(scope);
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
        // NULL is the inherit state (migration 0050).
        let enabled: Option<bool> = r.try_get("enabled").map_err(be)?;
        let config: Value = r.try_get("config").map_err(be)?;
        let values: Map<String, Value> = config
            .get("values")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        Ok(Some(SettingsRecord { enabled, values }))
    }

    async fn put(
        &self,
        feature_id: &str,
        scope: &ScopeRef,
        record: SettingsRecord,
    ) -> Result<(), StoreError> {
        let (kind, id) = key(scope);
        let config = json!({ "values": record.values });
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
        let (kind, id) = key(scope);
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

    async fn list_guilds(
        &self,
        feature_id: &str,
    ) -> Result<Vec<(sr_core::GuildId, SettingsRecord)>, StoreError> {
        let rows = sqlx::query(
            "SELECT scope_id, enabled, config FROM feature_enablements \
             WHERE feature = $1 AND scope = 'guild'::enablement_scope",
        )
        .bind(feature_id)
        .fetch_all(&self.pool)
        .await
        .map_err(be)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let id: i64 = r.try_get("scope_id").map_err(be)?;
            let Ok(id) = u64::try_from(id) else { continue };
            let enabled: Option<bool> = r.try_get("enabled").map_err(be)?;
            let config: Value = r.try_get("config").map_err(be)?;
            let values = config
                .get("values")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            out.push((
                sr_core::GuildId::new(id),
                SettingsRecord { enabled, values },
            ));
        }
        Ok(out)
    }
}

/// Postgres [`FeatureState`] over `feature_state` (migration 0680).
pub struct PgFeatureState {
    pool: PgPool,
}

impl PgFeatureState {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn se(e: sqlx::Error) -> StateError {
    StateError(e.to_string())
}

fn entry_of(r: &sqlx::postgres::PgRow) -> Result<StateEntry, StateError> {
    let feature: String = r.try_get("feature").map_err(se)?;
    let guild: i64 = r.try_get("guild_id").map_err(se)?;
    let key: String = r.try_get("key").map_err(se)?;
    let value: Value = r.try_get("value").map_err(se)?;
    let due: Option<i64> = r.try_get("due_ms").map_err(se)?;
    Ok(StateEntry {
        feature,
        guild: sr_core::GuildId::new(u64::try_from(guild).unwrap_or_default()),
        key,
        value,
        due_ms: due,
    })
}

/// `SELECT` of state rows, with the due time as unix ms; `$tail` is the rest of the query.
macro_rules! state_select {
    ($tail:literal) => {
        concat!(
            "SELECT feature, guild_id, key, value, ",
            "(EXTRACT(EPOCH FROM due_at) * 1000)::BIGINT AS due_ms FROM feature_state ",
            $tail
        )
    };
}

#[async_trait]
impl FeatureState for PgFeatureState {
    async fn get(
        &self,
        feature: &str,
        guild: sr_core::GuildId,
        key: &str,
    ) -> Result<Option<StateEntry>, StateError> {
        let row = sqlx::query(state_select!(
            "WHERE feature = $1 AND guild_id = $2 AND key = $3"
        ))
        .bind(feature)
        .bind(guild.get() as i64)
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(se)?;
        row.as_ref().map(entry_of).transpose()
    }

    async fn put(&self, entry: StateEntry) -> Result<(), StateError> {
        sqlx::query(
            "INSERT INTO feature_state (feature, guild_id, key, value, due_at) \
             VALUES ($1, $2, $3, $4, CASE WHEN $5::BIGINT IS NULL THEN NULL \
                     ELSE to_timestamp($5::DOUBLE PRECISION / 1000) END) \
             ON CONFLICT (feature, guild_id, key) DO UPDATE \
             SET value = EXCLUDED.value, due_at = EXCLUDED.due_at, updated_at = now()",
        )
        .bind(&entry.feature)
        .bind(entry.guild.get() as i64)
        .bind(&entry.key)
        .bind(&entry.value)
        .bind(entry.due_ms)
        .execute(&self.pool)
        .await
        .map_err(se)?;
        Ok(())
    }

    async fn delete(
        &self,
        feature: &str,
        guild: sr_core::GuildId,
        key: &str,
    ) -> Result<(), StateError> {
        sqlx::query("DELETE FROM feature_state WHERE feature = $1 AND guild_id = $2 AND key = $3")
            .bind(feature)
            .bind(guild.get() as i64)
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(se)?;
        Ok(())
    }

    async fn due(
        &self,
        feature: &str,
        now_ms: i64,
        limit: usize,
    ) -> Result<Vec<StateEntry>, StateError> {
        let rows = sqlx::query(state_select!(
            "WHERE feature = $1 AND due_at IS NOT NULL \
             AND due_at <= to_timestamp($2::DOUBLE PRECISION / 1000) \
             ORDER BY due_at LIMIT $3"
        ))
        .bind(feature)
        .bind(now_ms)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(se)?;
        rows.iter().map(entry_of).collect()
    }

    async fn list(
        &self,
        feature: &str,
        guild: sr_core::GuildId,
        prefix: &str,
    ) -> Result<Vec<StateEntry>, StateError> {
        // `starts_with` avoids LIKE wildcards in the prefix.
        let rows = sqlx::query(state_select!(
            "WHERE feature = $1 AND guild_id = $2 AND starts_with(key, $3) ORDER BY key"
        ))
        .bind(feature)
        .bind(guild.get() as i64)
        .bind(prefix)
        .fetch_all(&self.pool)
        .await
        .map_err(se)?;
        rows.iter().map(entry_of).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_core::{ChannelId, GuildId};

    #[test]
    fn channel_scope_is_stored_natively() {
        let s = ScopeRef::Channel {
            guild: GuildId::new(1),
            channel: ChannelId::new(2),
        };
        assert_eq!(key(&s), ("channel", 2));
        assert_eq!(key(&ScopeRef::Platform), ("platform", 0));
    }
}
