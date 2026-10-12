//! Scoped settings persistence, abstracted so the DB-backed implementation
//! (TODO(FND-DB): `sr-db` feature_settings table) can be added later.

use std::collections::HashMap;
use std::sync::RwLock;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

use crate::feature::ScopeRef;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("settings store error: {0}")]
    Backend(String),
}

/// What is stored for one (feature, scope instance).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SettingsRecord {
    /// `None` inherits from the parent scope / feature default.
    pub enabled: Option<bool>,
    /// Explicit overrides only; defaults are resolved at read time.
    pub values: Map<String, Value>,
}

#[async_trait]
pub trait SettingsStore: Send + Sync {
    async fn get(
        &self,
        feature_id: &str,
        scope: &ScopeRef,
    ) -> Result<Option<SettingsRecord>, StoreError>;
    async fn put(
        &self,
        feature_id: &str,
        scope: &ScopeRef,
        record: SettingsRecord,
    ) -> Result<(), StoreError>;
    async fn delete(&self, feature_id: &str, scope: &ScopeRef) -> Result<(), StoreError>;
}

#[derive(Debug, Default)]
pub struct InMemorySettingsStore {
    map: RwLock<HashMap<(String, ScopeRef), SettingsRecord>>,
}

impl InMemorySettingsStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl SettingsStore for InMemorySettingsStore {
    async fn get(
        &self,
        feature_id: &str,
        scope: &ScopeRef,
    ) -> Result<Option<SettingsRecord>, StoreError> {
        let map = self.map.read().unwrap_or_else(|p| p.into_inner());
        Ok(map.get(&(feature_id.to_owned(), *scope)).cloned())
    }

    async fn put(
        &self,
        feature_id: &str,
        scope: &ScopeRef,
        record: SettingsRecord,
    ) -> Result<(), StoreError> {
        let mut map = self.map.write().unwrap_or_else(|p| p.into_inner());
        map.insert((feature_id.to_owned(), *scope), record);
        Ok(())
    }

    async fn delete(&self, feature_id: &str, scope: &ScopeRef) -> Result<(), StoreError> {
        let mut map = self.map.write().unwrap_or_else(|p| p.into_inner());
        map.remove(&(feature_id.to_owned(), *scope));
        Ok(())
    }
}
