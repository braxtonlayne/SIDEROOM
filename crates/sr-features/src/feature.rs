use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Router;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use sr_core::Clock;
use sr_events::{ChannelId, DomainEvent, EventBus, GuildId, Topic, UserId};
use thiserror::Error;

use crate::actions::FeatureActions;
use crate::schema::{SettingsSchema, ValidationError};
use crate::service::FeatureService;
use crate::state::FeatureState;
use crate::store::StoreError;

#[derive(Debug, Error)]
pub enum FeatureError {
    #[error("unknown feature `{0}`")]
    UnknownFeature(String),
    #[error("feature `{feature}` does not support {scope:?} scope")]
    UnsupportedScope { feature: String, scope: Scope },
    #[error("duplicate feature id `{0}`")]
    DuplicateFeature(String),
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Action(#[from] crate::actions::ActionError),
    #[error(transparent)]
    State(#[from] crate::state::StateError),
}

/// The level a setting or flag applies at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Platform,
    Guild,
    Channel,
    User,
}

/// A concrete scope instance. Channel carries its guild so resolution can
/// inherit from the guild.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum ScopeRef {
    Platform,
    Guild { guild: GuildId },
    Channel { guild: GuildId, channel: ChannelId },
    User { user: UserId },
}

impl ScopeRef {
    pub fn kind(&self) -> Scope {
        match self {
            Self::Platform => Scope::Platform,
            Self::Guild { .. } => Scope::Guild,
            Self::Channel { .. } => Scope::Channel,
            Self::User { .. } => Scope::User,
        }
    }

    /// Inheritance chain, most specific first, always ending at `Platform`.
    pub fn chain(&self) -> Vec<ScopeRef> {
        match *self {
            Self::Platform => vec![Self::Platform],
            Self::Guild { .. } => vec![*self, Self::Platform],
            Self::Channel { guild, .. } => vec![*self, Self::Guild { guild }, Self::Platform],
            Self::User { .. } => vec![*self, Self::Platform],
        }
    }

    pub fn topic(&self) -> Topic {
        match *self {
            Self::Platform => Topic::Platform,
            Self::Guild { guild } => Topic::Guild(guild),
            Self::Channel { channel, .. } => Topic::Channel(channel),
            Self::User { user } => Topic::User(user),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureMeta {
    pub name: String,
    pub description: String,
    /// Settings-menu section, e.g. "Lounge".
    pub section: String,
    /// Delivery phase from the roadmap.
    pub phase: u8,
    pub default_enabled: bool,
}

/// Handles a feature gets from the composition root (doc 11 section 1.1).
/// Feature tables live in `crates/sr-db/migrations` (one forward-only series),
/// so there is no per-feature migration hook.
#[derive(Clone)]
pub struct FeatureCtx {
    pub bus: Arc<dyn EventBus>,
    pub settings: FeatureService,
    pub clock: Arc<dyn Clock>,
    /// What the module may do to the platform (post, role changes, threads).
    pub actions: Arc<dyn FeatureActions>,
    /// The module's own runtime rows.
    pub state: Arc<dyn FeatureState>,
}

impl FeatureCtx {
    /// The effective settings of `feature` for `guild`, parsed into the module's
    /// config type. Missing keys fall back to the type's serde defaults.
    pub async fn guild_config<T: serde::de::DeserializeOwned>(
        &self,
        feature: &str,
        guild: GuildId,
    ) -> Result<T, FeatureError> {
        let values = self
            .settings
            .effective_settings(feature, &ScopeRef::Guild { guild })
            .await?;
        serde_json::from_value(serde_json::Value::Object(values)).map_err(|e| {
            FeatureError::Validation(ValidationError {
                path: feature.to_owned(),
                reason: e.to_string(),
            })
        })
    }

    pub fn now_ms(&self) -> i64 {
        self.clock.now_unix_ms()
    }
}

/// A unit of functionality with auto-rendered settings (doc 11 section 1.1).
pub trait Feature: Send + Sync + 'static {
    /// Stable id, e.g. "lounge.slowmode_plus".
    fn id(&self) -> &'static str;
    fn meta(&self) -> FeatureMeta;
    /// Whether the platform-scope flag is exposed to signed-in clients through
    /// `GET /sr/features/flags` (D-216). Off unless the module says so.
    fn client_visible(&self) -> bool {
        false
    }
    fn settings_schema(&self) -> SettingsSchema;
    fn scopes(&self) -> &[Scope];
    fn on_event<'a>(
        &'a self,
        _ev: &'a DomainEvent,
        _ctx: &'a FeatureCtx,
    ) -> BoxFuture<'a, Result<(), FeatureError>> {
        Box::pin(async { Ok(()) })
    }
    /// Called on a fixed interval by the runtime (default 30 s), for modules that
    /// act on time: expiries, schedules, periodic counters.
    fn on_tick<'a>(&'a self, _ctx: &'a FeatureCtx) -> BoxFuture<'a, Result<(), FeatureError>> {
        Box::pin(async { Ok(()) })
    }
    /// Routes served under `/api/v1/sr/features/{id}/`, or `None`. Features
    /// own their state: the router has no `AppState`.
    fn routes(&self) -> Option<Router> {
        None
    }
}

/// Holds every registered feature, keyed by id.
#[derive(Default)]
pub struct FeatureRegistry {
    features: BTreeMap<&'static str, Arc<dyn Feature>>,
}

impl FeatureRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, feature: impl Feature) -> Result<(), FeatureError> {
        let id = feature.id();
        if self.features.contains_key(id) {
            return Err(FeatureError::DuplicateFeature(id.to_owned()));
        }
        self.features.insert(id, Arc::new(feature));
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Arc<dyn Feature>> {
        self.features.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn Feature>> {
        self.features.values()
    }

    pub fn len(&self) -> usize {
        self.features.len()
    }

    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }
}

/// Startup registration: `register_features!(registry, SlowmodePlus, Starboard)?;`
/// Each type must implement `Feature` and `Default`.
#[macro_export]
macro_rules! register_features {
    ($registry:expr, $($feature:ty),+ $(,)?) => {
        (|| -> ::std::result::Result<(), $crate::FeatureError> {
            $( $registry.register(<$feature as ::std::default::Default>::default())?; )+
            Ok(())
        })()
    };
}
