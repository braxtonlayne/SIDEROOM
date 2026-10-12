//! sr-features: feature registry, settings schemas, scoped settings and
//! feature flags. See docs/11-extensibility-and-developer-platform.md section 1.

pub mod feature;
pub mod schema;
pub mod service;
pub mod store;

pub use feature::{
    Feature, FeatureCtx, FeatureError, FeatureMeta, FeatureRegistry, Scope, ScopeRef,
};
pub use schema::{Field, FieldKind, SettingsSchema, ValidationError};
pub use service::FeatureService;
pub use store::{InMemorySettingsStore, SettingsRecord, SettingsStore};
