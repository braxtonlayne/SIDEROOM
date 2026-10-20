//! The `moderation.reports` flag (XC-MOD-002; D-126, docs/09 section 5,
//! docs/specs/moderation-compliance.md).
//!
//! Reporting is a Phase 7 feature (step 1 is built, `sr-moderation`). The flag is **off** by default; the
//! Report entries in the web client render only when it is on, and `POST /moderation/reports` answers 403
//! while it is off. Reports are routed per D-209. The flag is platform-wide, with no per-server switch (D-329).

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
 description: "Members can report messages, people, channels, servers, library items and cinema titles. Reports go to the server's moderators or to platform staff (D-209). The setting is platform-wide, not per server (D-329)."
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
