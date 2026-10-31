//! sr-features: feature registry, settings schemas, scoped settings and
//! feature flags. See docs/11-extensibility-and-developer-platform.md section 1.
//!
//! Built-in modules (docs/inventory/09-feature-modules.md) live in [`modules`]; the
//! [`runtime`] feeds them platform events and ticks, and they act on the platform
//! only through the [`actions::FeatureActions`] port.

pub mod actions;
pub mod byos;
pub mod compliance;
pub mod directory;
pub mod feature;
pub mod game_servers;
pub mod moderation;
pub mod modules;
pub mod platform_defaults;
pub mod public_profiles;
pub mod runtime;
pub mod schema;
pub mod service;
pub mod state;
pub mod store;
pub mod template;
pub mod testing;

pub use byos::{BYOS_ID, ByosFeature, ByosSettings};
pub use compliance::{COMPLIANCE_ID, ComplianceFeature, ComplianceSettings, INVOICE_YEARS_RANGE};
pub use directory::{DirectoryHiddenCategoriesFeature, HIDDEN_CATEGORIES_ID, hidden_from};
pub use feature::{
    Feature, FeatureCtx, FeatureError, FeatureMeta, FeatureRegistry, Scope, ScopeRef,
};
pub use game_servers::{GAME_SERVERS_ID, GameServersFeature};
pub use moderation::{ModerationReportsFeature, REPORTS_FEATURE_ID};
pub use modules::bridge::{BRIDGE_MODULE_ID, BridgeModule, PLATFORM_BRIDGE_ID, PlatformBridge};
pub use platform_defaults::{
    APPEARANCE_ID, AppearanceDefaultsFeature, INTEGRATIONS_ID, IntegrationsPolicy,
    IntegrationsPolicyFeature, default_themes_from,
};
pub use public_profiles::{PUBLIC_PROFILES_ID, PublicProfilesFeature};
pub use runtime::{FeatureRuntime, current_module, scope_module};
pub use schema::{Field, FieldKind, SettingsSchema, ValidationError};
pub use service::FeatureService;
pub use store::{InMemorySettingsStore, SettingsRecord, SettingsStore};
