//! `platform.public_profiles` (PRF-PRO-001, D-382, D-384): the operator's switch that makes a public profile
//! page possible at all. **Off by default.** Off, the `public` audience a member may choose is stored but never
//! effective (it is clamped to `members`), `GET /public/profiles/{handle}` and the public guestbook answer 404 for
//! every handle, and the instance stays the invite-only walled garden. A public page is never indexed
//! (`X-Robots-Tag: noindex, nofollow`, D-384); there is no per-member option for that.

use crate::feature::{Feature, FeatureMeta, Scope};
use crate::schema::SettingsSchema;

/// Stable feature id.
pub const PUBLIC_PROFILES_ID: &str = "platform.public_profiles";

#[derive(Default)]
pub struct PublicProfilesFeature;

impl Feature for PublicProfilesFeature {
    fn id(&self) -> &'static str {
        PUBLIC_PROFILES_ID
    }
    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Public profiles".into(),
            description: "Lets members choose to show parts of their profile to people without an account, at /~handle. Off (the default), every profile stays inside the instance and the public address answers 404 for everyone. Public pages are never indexed by search engines.".into(),
            section: "Privacy".into(),
            phase: 34,
            default_enabled: false,
        }
    }
    fn settings_schema(&self) -> SettingsSchema {
        SettingsSchema::default()
    }
    fn scopes(&self) -> &[Scope] {
        &[Scope::Platform]
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{FeatureRegistry, FeatureService, InMemorySettingsStore, ScopeRef};

    #[tokio::test]
    async fn public_profiles_are_off_until_the_operator_turns_them_on() {
        let mut registry = FeatureRegistry::new();
        registry.register(PublicProfilesFeature).unwrap();
        let svc = FeatureService::new(
            Arc::new(registry),
            Arc::new(InMemorySettingsStore::new()),
            None,
        );
        assert!(!PublicProfilesFeature.meta().default_enabled);
        assert!(!PublicProfilesFeature.client_visible());
        assert!(
            !svc.is_enabled(PUBLIC_PROFILES_ID, &ScopeRef::Platform)
                .await
                .unwrap()
        );
    }
}
