use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use futures::StreamExt;
use moka::future::Cache;
use serde_json::{Map, Value};
use sr_events::{DomainEvent, EventBus};
use tokio::task::JoinHandle;

use crate::feature::{Feature, FeatureError, FeatureRegistry, Scope, ScopeRef};
use crate::schema::merge;
use crate::store::{SettingsRecord, SettingsStore};

const CACHE_CAPACITY: u64 = 50_000;

struct Inner {
    registry: Arc<FeatureRegistry>,
    store: Arc<dyn SettingsStore>,
    bus: Option<Arc<dyn EventBus>>,
    cache: Cache<(String, ScopeRef), bool>,
    /// Bumped on every invalidation so a load that raced with a write is not cached.
    generation: AtomicU64,
}

impl Inner {
    async fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        // A change at one scope can affect inheriting scopes, so drop everything.
        self.cache.invalidate_all();
        self.cache.run_pending_tasks().await;
    }
}

/// Feature flags and scoped settings on top of a registry and a store.
#[derive(Clone)]
pub struct FeatureService {
    inner: Arc<Inner>,
}

impl FeatureService {
    pub fn new(
        registry: Arc<FeatureRegistry>,
        store: Arc<dyn SettingsStore>,
        bus: Option<Arc<dyn EventBus>>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                registry,
                store,
                bus,
                cache: Cache::new(CACHE_CAPACITY),
                generation: AtomicU64::new(0),
            }),
        }
    }

    fn feature(&self, id: &str) -> Result<&Arc<dyn Feature>, FeatureError> {
        self.inner
            .registry
            .get(id)
            .ok_or_else(|| FeatureError::UnknownFeature(id.to_owned()))
    }

    /// Whether `id` is enabled at `scope`. The platform scope acts as a kill
    /// switch: an explicit `false` there disables the feature everywhere.
    /// Otherwise the most specific explicit flag wins, falling back to the
    /// feature's `default_enabled`.
    pub async fn is_enabled(&self, id: &str, scope: &ScopeRef) -> Result<bool, FeatureError> {
        let key = (id.to_owned(), *scope);
        if let Some(v) = self.inner.cache.get(&key).await {
            return Ok(v);
        }
        let gen_before = self.inner.generation.load(Ordering::SeqCst);
        let feature = self.feature(id)?;
        let mut explicit = None;
        let mut killed = false;
        let mut platform_on = false;
        for s in scope.chain() {
            let rec = self.inner.store.get(id, &s).await?;
            if let Some(enabled) = rec.and_then(|r| r.enabled) {
                if s == ScopeRef::Platform {
                    platform_on = enabled;
                }
                if s == ScopeRef::Platform && !enabled {
                    killed = true;
                }
                // For an operator-gated module the platform's `true` only offers the module; it does not
                // switch it on in servers that never chose to.
                let offer_only = s == ScopeRef::Platform
                    && *scope != ScopeRef::Platform
                    && feature.operator_gated();
                if !offer_only {
                    explicit = explicit.or(Some(enabled));
                }
            }
        }
        // An operator-gated module (PRF-SCR-001) is off everywhere until the platform opted in.
        let opted_in = !feature.operator_gated() || platform_on;
        let mut value = !killed && opted_in && explicit.unwrap_or(feature.meta().default_enabled);
        // A module under a platform feature (`bridge.enabled` under `platform.bridge`) dies with it.
        if value
            && let Some(parent) = feature.gated_by()
            && self.parent_killed(parent).await?
        {
            value = false;
        }
        if self.inner.generation.load(Ordering::SeqCst) == gen_before {
            self.inner.cache.insert(key, value).await;
        }
        Ok(value)
    }

    /// Whether `id` may be offered to servers at all: always, unless the module is
    /// [operator-gated](Feature::operator_gated), in which case only once the platform scope holds an
    /// explicit `enabled: true`. Guild Settings > Modules lists and accepts only offered modules.
    pub async fn offered(&self, id: &str) -> Result<bool, FeatureError> {
        let feature = self.feature(id)?;
        if let Some(parent) = feature.gated_by()
            && self.parent_killed(parent).await?
        {
            return Ok(false);
        }
        if !feature.operator_gated() {
            return Ok(true);
        }
        Ok(self
            .inner
            .store
            .get(id, &ScopeRef::Platform)
            .await?
            .and_then(|r| r.enabled)
            == Some(true))
    }

    /// Whether the platform has switched `id` off for every server (the kill switch).
    /// Whether the platform feature a module is gated by is switched off. A parent that is not registered (a
    /// registry built without it, as in tests that register only the guild modules) gates nothing.
    async fn parent_killed(&self, parent: &str) -> Result<bool, FeatureError> {
        match self.platform_killed(parent).await {
            Err(FeatureError::UnknownFeature(_)) => Ok(false),
            other => other,
        }
    }

    pub async fn platform_killed(&self, id: &str) -> Result<bool, FeatureError> {
        self.feature(id)?;
        Ok(self
            .inner
            .store
            .get(id, &ScopeRef::Platform)
            .await?
            .and_then(|r| r.enabled)
            == Some(false))
    }

    /// Guilds in which `id` is enabled, for ticking modules. Only guilds with a
    /// stored record are considered, then each is checked with [`Self::is_enabled`]
    /// (so the platform kill switch and defaults apply).
    pub async fn enabled_guilds(&self, id: &str) -> Result<Vec<sr_events::GuildId>, FeatureError> {
        self.feature(id)?;
        let mut out = Vec::new();
        for (guild, _) in self.inner.store.list_guilds(id).await? {
            if self.is_enabled(id, &ScopeRef::Guild { guild }).await? {
                out.push(guild);
            }
        }
        Ok(out)
    }

    /// Guilds whose own record for `id` holds an explicit `enabled` flag equal to `enabled`, whatever the
    /// platform kill switch says (it is not applied). For the teardown that follows a platform kill, which must
    /// find the servers that had chosen a module on while the kill hid it.
    pub async fn guilds_with_flag(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<Vec<sr_events::GuildId>, FeatureError> {
        self.feature(id)?;
        Ok(self
            .inner
            .store
            .list_guilds(id)
            .await?
            .into_iter()
            .filter(|(_, rec)| rec.enabled == Some(enabled))
            .map(|(g, _)| g)
            .collect())
    }

    /// Set (or with `None`, clear) the enabled flag at `scope`.
    pub async fn set_enabled(
        &self,
        id: &str,
        scope: &ScopeRef,
        enabled: Option<bool>,
    ) -> Result<(), FeatureError> {
        let feature = self.feature(id)?;
        // Platform and server flags have always been settable for any module; the narrower
        // levels exist only where a module declares them.
        if matches!(scope.kind(), Scope::Channel | Scope::User)
            && !feature.scopes().contains(&scope.kind())
        {
            return Err(FeatureError::UnsupportedScope {
                feature: id.to_owned(),
                scope: scope.kind(),
            });
        }
        let mut rec = self.inner.store.get(id, scope).await?.unwrap_or_default();
        rec.enabled = enabled;
        self.write(id, scope, rec).await
    }

    /// What `scope` itself stores for `id`, without inheritance: the explicit flag
    /// (`None` inherits) and the override values.
    pub async fn overrides(
        &self,
        id: &str,
        scope: &ScopeRef,
    ) -> Result<SettingsRecord, FeatureError> {
        self.feature(id)?;
        Ok(self.inner.store.get(id, scope).await?.unwrap_or_default())
    }

    /// Remove everything `scope` stores for `id`, so it follows its parent again.
    pub async fn clear(&self, id: &str, scope: &ScopeRef) -> Result<(), FeatureError> {
        self.feature(id)?;
        self.write(id, scope, SettingsRecord::default()).await
    }

    /// Forget the channel-scope rows of a deleted channel, for every module that has them.
    pub async fn forget_channel(
        &self,
        guild: sr_events::GuildId,
        channel: sr_events::ChannelId,
    ) -> Result<(), FeatureError> {
        let scope = ScopeRef::Channel { guild, channel };
        let ids: Vec<&'static str> = self
            .inner
            .registry
            .iter()
            .filter(|f| f.scopes().contains(&Scope::Channel))
            .map(|f| f.id())
            .collect();
        for id in ids {
            if self.inner.store.get(id, &scope).await?.is_some() {
                self.write(id, &scope, SettingsRecord::default()).await?;
            }
        }
        Ok(())
    }

    /// Replace the explicit setting overrides at `scope` after validating them
    /// against the feature's schema.
    pub async fn set_settings(
        &self,
        id: &str,
        scope: &ScopeRef,
        values: Map<String, Value>,
    ) -> Result<(), FeatureError> {
        let feature = self.feature(id)?;
        if !feature.scopes().contains(&scope.kind()) {
            return Err(FeatureError::UnsupportedScope {
                feature: id.to_owned(),
                scope: scope.kind(),
            });
        }
        feature
            .settings_schema()
            .validate_at(scope.kind(), &values)?;
        let mut rec = self.inner.store.get(id, scope).await?.unwrap_or_default();
        rec.values = values;
        self.write(id, scope, rec).await
    }

    /// Defaults for the scope kind overlaid by overrides from platform down to `scope`.
    pub async fn effective_settings(
        &self,
        id: &str,
        scope: &ScopeRef,
    ) -> Result<Map<String, Value>, FeatureError> {
        let feature = self.feature(id)?;
        let schema = feature.settings_schema();
        let mut out = schema.defaults(scope.kind());
        for s in scope.chain().iter().rev() {
            if let Some(rec) = self.inner.store.get(id, s).await? {
                merge(&mut out, &rec.values);
            }
        }
        // A key that belongs to another level (a server-only field seen from a channel)
        // is not part of this level's settings.
        schema.retain_scope(scope.kind(), &mut out);
        Ok(out)
    }

    async fn write(
        &self,
        id: &str,
        scope: &ScopeRef,
        rec: SettingsRecord,
    ) -> Result<(), FeatureError> {
        if rec == SettingsRecord::default() {
            self.inner.store.delete(id, scope).await?;
        } else {
            self.inner.store.put(id, scope, rec).await?;
        }
        self.inner.invalidate().await;
        if let Some(bus) = &self.inner.bus {
            let ev = DomainEvent::FeatureSettingsChanged {
                feature_id: id.to_owned(),
                scope: scope.topic(),
            };
            for topic in ev.topics() {
                // Cache is already invalidated locally; bus failure only delays other nodes.
                let _ = bus.publish(topic, ev.clone()).await;
            }
            // G-81: a client-visible platform flag changed, so every session gets the full map.
            if matches!(scope, ScopeRef::Platform)
                && self.feature(id).is_ok_and(|f| f.client_visible())
                && let Ok(flags) = self.client_flags().await
            {
                let ev = DomainEvent::PlatformBroadcast {
                    name: "SR_FEATURE_UPDATE".into(),
                    payload: serde_json::json!({ "scope": "platform", "flags": flags }),
                };
                let _ = bus.publish(sr_events::Topic::Platform, ev).await;
            }
        }
        Ok(())
    }

    /// Platform flags clients may read: feature id to on/off for platform-scope modules that
    /// declare themselves client-visible (D-216). The same map `GET /sr/features/flags` returns
    /// and `SR_FEATURE_UPDATE` carries.
    pub async fn client_flags(
        &self,
    ) -> Result<std::collections::BTreeMap<String, bool>, FeatureError> {
        let mut flags = std::collections::BTreeMap::new();
        for f in self.inner.registry.iter() {
            if !f.client_visible() || !f.scopes().contains(&Scope::Platform) {
                continue;
            }
            flags.insert(
                f.id().to_owned(),
                self.is_enabled(f.id(), &ScopeRef::Platform).await?,
            );
        }
        Ok(flags)
    }

    /// Spawn a task that invalidates the cache when a `FeatureSettingsChanged`
    /// event arrives (e.g. published by another node). Requires a bus and a tokio runtime.
    pub fn spawn_invalidation_listener(&self) -> Option<JoinHandle<()>> {
        let bus = self.inner.bus.clone()?;
        let mut stream = bus.subscribe(sr_events::Topic::Platform);
        let inner = self.inner.clone();
        Some(tokio::spawn(async move {
            while let Some(ev) = stream.next().await {
                if matches!(ev, DomainEvent::FeatureSettingsChanged { .. }) {
                    inner.invalidate().await;
                }
            }
        }))
    }

    pub fn registry(&self) -> &Arc<FeatureRegistry> {
        &self.inner.registry
    }

    /// Scopes a feature declares, for callers building UI. Platform is always settable for flags.
    pub fn declared_scopes(&self, id: &str) -> Result<Vec<Scope>, FeatureError> {
        Ok(self.feature(id)?.scopes().to_vec())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    use async_trait::async_trait;
    use serde_json::json;
    use sr_events::{ChannelId, GuildId, InProcessBus, Topic};

    use super::*;
    use crate::feature::FeatureMeta;
    use crate::schema::{Field, FieldKind, SettingsSchema};
    use crate::store::{InMemorySettingsStore, StoreError};

    #[derive(Default)]
    struct Demo;

    impl Feature for Demo {
        fn id(&self) -> &'static str {
            "demo.thing"
        }
        fn meta(&self) -> FeatureMeta {
            FeatureMeta {
                name: "Demo".into(),
                description: "d".into(),
                section: "Lounge".into(),
                phase: 1,
                default_enabled: false,
            }
        }
        fn settings_schema(&self) -> SettingsSchema {
            SettingsSchema::new(vec![
                Field::new(
                    "limit",
                    "Limit",
                    FieldKind::Int {
                        min: Some(1),
                        max: Some(100),
                    },
                )
                .at(&[Scope::Platform, Scope::Guild, Scope::Channel])
                .default_value(json!(10)),
            ])
        }
        fn scopes(&self) -> &[Scope] {
            &[Scope::Platform, Scope::Guild, Scope::Channel]
        }
    }

    const G: GuildId = GuildId::new(1);
    const C: ChannelId = ChannelId::new(2);

    fn guild() -> ScopeRef {
        ScopeRef::Guild { guild: G }
    }
    fn channel() -> ScopeRef {
        ScopeRef::Channel {
            guild: G,
            channel: C,
        }
    }

    fn service_with(
        store: Arc<dyn SettingsStore>,
        bus: Option<Arc<dyn EventBus>>,
    ) -> FeatureService {
        let mut reg = FeatureRegistry::new();
        crate::register_features!(reg, Demo).unwrap();
        FeatureService::new(Arc::new(reg), store, bus)
    }

    fn service() -> FeatureService {
        service_with(Arc::new(InMemorySettingsStore::new()), None)
    }

    #[test]
    fn duplicate_registration_fails() {
        let mut reg = FeatureRegistry::new();
        let r = crate::register_features!(reg, Demo, Demo);
        assert!(matches!(r, Err(FeatureError::DuplicateFeature(_))));
        assert_eq!(reg.len(), 1);
    }

    #[tokio::test]
    async fn default_then_inheritance() {
        let s = service();
        assert!(!s.is_enabled("demo.thing", &channel()).await.unwrap());
        s.set_enabled("demo.thing", &guild(), Some(true))
            .await
            .unwrap();
        assert!(s.is_enabled("demo.thing", &channel()).await.unwrap());
        s.set_enabled("demo.thing", &channel(), Some(false))
            .await
            .unwrap();
        assert!(!s.is_enabled("demo.thing", &channel()).await.unwrap());
        assert!(s.is_enabled("demo.thing", &guild()).await.unwrap());
        s.set_enabled("demo.thing", &channel(), None).await.unwrap();
        assert!(s.is_enabled("demo.thing", &channel()).await.unwrap());
    }

    #[tokio::test]
    async fn platform_kill_switch() {
        let s = service();
        s.set_enabled("demo.thing", &guild(), Some(true))
            .await
            .unwrap();
        s.set_enabled("demo.thing", &ScopeRef::Platform, Some(false))
            .await
            .unwrap();
        assert!(!s.is_enabled("demo.thing", &guild()).await.unwrap());
    }

    #[tokio::test]
    async fn clear_overrides_and_forgetting_a_channel() {
        let s = service();
        let c = json!({"limit": 30}).as_object().cloned().unwrap();
        s.set_settings("demo.thing", &channel(), c).await.unwrap();
        s.set_enabled("demo.thing", &channel(), Some(false))
            .await
            .unwrap();
        let own = s.overrides("demo.thing", &channel()).await.unwrap();
        assert_eq!(own.enabled, Some(false));
        assert_eq!(own.values["limit"], json!(30));
        s.clear("demo.thing", &channel()).await.unwrap();
        assert_eq!(
            s.overrides("demo.thing", &channel()).await.unwrap(),
            SettingsRecord::default()
        );
        s.set_enabled("demo.thing", &channel(), Some(true))
            .await
            .unwrap();
        s.forget_channel(G, C).await.unwrap();
        assert_eq!(
            s.overrides("demo.thing", &channel()).await.unwrap(),
            SettingsRecord::default()
        );
    }

    #[tokio::test]
    async fn narrow_levels_exist_only_where_declared() {
        let s = service();
        let user = ScopeRef::User {
            user: sr_events::UserId::new(1),
        };
        assert!(matches!(
            s.set_enabled("demo.thing", &user, Some(true)).await,
            Err(FeatureError::UnsupportedScope { .. })
        ));
        // Server and platform flags stay settable for every module.
        s.set_enabled("demo.thing", &guild(), Some(true))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn unknown_feature_and_scope_errors() {
        let s = service();
        assert!(matches!(
            s.is_enabled("nope", &guild()).await,
            Err(FeatureError::UnknownFeature(_))
        ));
        let user = ScopeRef::User {
            user: sr_events::UserId::new(1),
        };
        assert!(matches!(
            s.set_settings("demo.thing", &user, Map::new()).await,
            Err(FeatureError::UnsupportedScope { .. })
        ));
    }

    #[tokio::test]
    async fn settings_validate_and_resolve() {
        let s = service();
        let bad = json!({"limit": 1000}).as_object().cloned().unwrap();
        assert!(matches!(
            s.set_settings("demo.thing", &guild(), bad).await,
            Err(FeatureError::Validation(_))
        ));
        assert_eq!(
            s.effective_settings("demo.thing", &channel())
                .await
                .unwrap()["limit"],
            json!(10)
        );
        let g = json!({"limit": 20}).as_object().cloned().unwrap();
        s.set_settings("demo.thing", &guild(), g).await.unwrap();
        assert_eq!(
            s.effective_settings("demo.thing", &channel())
                .await
                .unwrap()["limit"],
            json!(20)
        );
        let c = json!({"limit": 30}).as_object().cloned().unwrap();
        s.set_settings("demo.thing", &channel(), c).await.unwrap();
        assert_eq!(
            s.effective_settings("demo.thing", &channel())
                .await
                .unwrap()["limit"],
            json!(30)
        );
        assert_eq!(
            s.effective_settings("demo.thing", &guild()).await.unwrap()["limit"],
            json!(20)
        );
    }

    /// Store wrapper that counts reads to prove caching.
    struct Counting {
        inner: InMemorySettingsStore,
        gets: AtomicUsize,
    }

    #[async_trait]
    impl SettingsStore for Counting {
        async fn get(&self, f: &str, s: &ScopeRef) -> Result<Option<SettingsRecord>, StoreError> {
            self.gets.fetch_add(1, Ordering::SeqCst);
            self.inner.get(f, s).await
        }
        async fn put(&self, f: &str, s: &ScopeRef, r: SettingsRecord) -> Result<(), StoreError> {
            self.inner.put(f, s, r).await
        }
        async fn delete(&self, f: &str, s: &ScopeRef) -> Result<(), StoreError> {
            self.inner.delete(f, s).await
        }
    }

    #[tokio::test]
    async fn caches_and_invalidates_on_write() {
        let store = Arc::new(Counting {
            inner: InMemorySettingsStore::new(),
            gets: AtomicUsize::new(0),
        });
        let s = service_with(store.clone(), None);
        s.is_enabled("demo.thing", &channel()).await.unwrap();
        let after_first = store.gets.load(Ordering::SeqCst);
        s.is_enabled("demo.thing", &channel()).await.unwrap();
        assert_eq!(store.gets.load(Ordering::SeqCst), after_first, "cache miss");
        s.set_enabled("demo.thing", &guild(), Some(true))
            .await
            .unwrap();
        assert!(s.is_enabled("demo.thing", &channel()).await.unwrap());
    }

    #[tokio::test]
    async fn invalidates_on_bus_event_from_elsewhere() {
        let store = Arc::new(InMemorySettingsStore::new());
        let bus: Arc<dyn EventBus> = Arc::new(InProcessBus::new());
        let s = service_with(store.clone(), Some(bus.clone()));
        let _h = s.spawn_invalidation_listener().unwrap();
        tokio::task::yield_now().await;

        assert!(!s.is_enabled("demo.thing", &guild()).await.unwrap()); // cached false
        // Simulate another node: write straight to the store, then announce.
        store
            .put(
                "demo.thing",
                &guild(),
                SettingsRecord {
                    enabled: Some(true),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(!s.is_enabled("demo.thing", &guild()).await.unwrap()); // still stale
        bus.publish(
            Topic::Platform,
            DomainEvent::FeatureSettingsChanged {
                feature_id: "demo.thing".into(),
                scope: Topic::Guild(G),
            },
        )
        .await
        .unwrap();
        for _ in 0..50 {
            if s.is_enabled("demo.thing", &guild()).await.unwrap() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("cache was not invalidated by bus event");
    }
}

#[cfg(test)]
mod kill_tests {
    use std::sync::Arc;

    use crate::moderation::{ModerationReportsFeature, REPORTS_FEATURE_ID};
    use crate::{FeatureRegistry, FeatureService, InMemorySettingsStore, ScopeRef};

    #[tokio::test]
    async fn platform_killed_reflects_only_an_explicit_platform_off() {
        let mut reg = FeatureRegistry::new();
        reg.register(ModerationReportsFeature).unwrap();
        let svc = FeatureService::new(Arc::new(reg), Arc::new(InMemorySettingsStore::new()), None);
        assert!(!svc.platform_killed(REPORTS_FEATURE_ID).await.unwrap());
        svc.set_enabled(REPORTS_FEATURE_ID, &ScopeRef::Platform, Some(true))
            .await
            .unwrap();
        assert!(!svc.platform_killed(REPORTS_FEATURE_ID).await.unwrap());
        svc.set_enabled(REPORTS_FEATURE_ID, &ScopeRef::Platform, Some(false))
            .await
            .unwrap();
        assert!(svc.platform_killed(REPORTS_FEATURE_ID).await.unwrap());
        assert!(svc.platform_killed("nope").await.is_err());
    }
}

#[cfg(test)]
mod broadcast_tests {
    use std::sync::Arc;
    use std::time::Duration;

    use futures::StreamExt;
    use sr_events::{DomainEvent, EventBus, InProcessBus, Topic};

    use crate::moderation::{ModerationReportsFeature, REPORTS_FEATURE_ID};
    use crate::{FeatureRegistry, FeatureService, InMemorySettingsStore, ScopeRef};

    #[tokio::test]
    async fn flipping_a_visible_platform_flag_broadcasts_the_whole_map() {
        let mut reg = FeatureRegistry::new();
        reg.register(ModerationReportsFeature).unwrap();
        let bus = Arc::new(InProcessBus::new());
        let svc = FeatureService::new(
            Arc::new(reg),
            Arc::new(InMemorySettingsStore::new()),
            Some(bus.clone() as Arc<dyn EventBus>),
        );
        let mut events = bus.subscribe(Topic::Platform);
        svc.set_enabled(REPORTS_FEATURE_ID, &ScopeRef::Platform, Some(false))
            .await
            .unwrap();
        let mut seen = None;
        while let Ok(Some(ev)) =
            tokio::time::timeout(Duration::from_millis(200), events.next()).await
        {
            if let DomainEvent::PlatformBroadcast { name, payload } = ev {
                seen = Some((name, payload));
                break;
            }
        }
        let (name, payload) = seen.expect("SR_FEATURE_UPDATE was broadcast");
        assert_eq!(name, "SR_FEATURE_UPDATE");
        assert_eq!(payload["scope"], "platform");
        assert_eq!(payload["flags"][REPORTS_FEATURE_ID], false);
    }
}
