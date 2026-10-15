//! The `moderation.reports` flag (XC-MOD-002; D-126, docs/09 section 5,
//! docs/specs/moderation-compliance.md).
//!
//! Reporting is a Phase 7 feature. Only the flag exists now: it is **off** by
//! default, and the Report entries in the web client render only when it is on.
//! There is no submit endpoint yet, so with the flag on the client shows a
//! dialog saying reporting is not available. The flag is platform-wide; the
//! operator turns it on once the Phase 7 backend ships.

use crate::feature::{Feature, FeatureMeta, Scope};
use crate::schema::SettingsSchema;

/// Stable feature id; the web client uses the same string.
pub const REPORTS_FEATURE_ID: &str = "moderation.reports";

#[derive(Default)]
pub struct ModerationReportsFeature;

impl Feature for ModerationReportsFeature {
    fn id(&self) -> &'static str {
        REPORTS_FEATURE_ID
    }
    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Reports".into(),
            description: "Show Report in the message, user, channel, library and cinema menus. \
                          Reports cannot be submitted until the Phase 7 moderation backend ships."
                .into(),
            section: "Moderation".into(),
            phase: 7,
            default_enabled: false,
        }
    }
    fn settings_schema(&self) -> SettingsSchema {
        SettingsSchema::default()
    }
    fn scopes(&self) -> &[Scope] {
        &[Scope::Platform]
    }
    fn client_visible(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{FeatureRegistry, FeatureService, InMemorySettingsStore, ScopeRef};

    fn service() -> FeatureService {
        let mut registry = FeatureRegistry::new();
        registry.register(ModerationReportsFeature).unwrap();
        FeatureService::new(
            Arc::new(registry),
            Arc::new(InMemorySettingsStore::new()),
            None,
        )
    }

    #[tokio::test]
    async fn reports_are_off_by_default_and_can_be_enabled_platform_wide() {
        let svc = service();
        assert!(
            !svc.is_enabled(REPORTS_FEATURE_ID, &ScopeRef::Platform)
                .await
                .unwrap()
        );
        svc.set_enabled(REPORTS_FEATURE_ID, &ScopeRef::Platform, Some(true))
            .await
            .unwrap();
        assert!(
            svc.is_enabled(REPORTS_FEATURE_ID, &ScopeRef::Platform)
                .await
                .unwrap()
        );
    }

    #[test]
    fn meta_is_phase_seven_and_default_off() {
        let m = ModerationReportsFeature.meta();
        assert!(!m.default_enabled);
        assert_eq!(m.phase, 7);
    }
}
