//! Keeps the SFU's data-channel gate and the activity service equal to the platform feature `platform.activities_datachannel`
//! (OPS-FLG-001). The SFU reads an atomic flag on every channel event; this task copies the feature into it every
//! couple of seconds (the feature service caches, and its cache is invalidated when the setting changes), so a
//! kill switch applies within seconds with no restart. A switch that cannot be read leaves the channels on:
//! the gate only refuses what the operator plainly switched off, and it never touches the call's encryption.

use std::time::Duration;

use sr_features::{ACTIVITIES_DATACHANNEL_ID, FeatureService, ScopeRef};
use sr_activities::ActivityService;
use sr_voice::datachannel::Gate;
use tokio::task::JoinHandle;

/// How often the flag is refreshed.
pub const REFRESH: Duration = Duration::from_secs(2);

/// Copy the feature into the gate once. Returns the value set.
pub async fn sync_once(
    features: &FeatureService,
    gate: Option<&Gate>,
    activities: Option<&ActivityService>,
) -> bool {
    let on = features
        .is_enabled(ACTIVITIES_DATACHANNEL_ID, &ScopeRef::Platform)
        .await
        .unwrap_or(true);
    if let Some(g) = gate {
        g.set(on);
    }
    if let Some(a) = activities {
        a.set_datachannel_allowed(on);
    }
    on
}

/// Start the refresh task. The first copy happens before this returns to the caller's next await point.
pub fn spawn(
    features: FeatureService,
    gate: Option<Gate>,
    activities: Option<ActivityService>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            sync_once(&features, gate.as_ref(), activities.as_ref()).await;
            tokio::time::sleep(REFRESH).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sr_features::{FeatureRegistry, InMemorySettingsStore};

    use super::*;

    #[tokio::test]
    async fn the_gate_follows_the_platform_feature() {
        let mut reg = FeatureRegistry::new();
        sr_features::wave_modules::register_all(&mut reg).unwrap();
        let svc = FeatureService::new(Arc::new(reg), Arc::new(InMemorySettingsStore::new()), None);
        let gate = Gate::default();
        assert!(sync_once(&svc, Some(&gate), None).await);
        assert!(gate.is_on());
        svc.set_enabled(ACTIVITIES_DATACHANNEL_ID, &ScopeRef::Platform, Some(false))
            .await
            .unwrap();
        assert!(!sync_once(&svc, Some(&gate), None).await);
        assert!(!gate.is_on());
        svc.set_enabled(ACTIVITIES_DATACHANNEL_ID, &ScopeRef::Platform, Some(true))
            .await
            .unwrap();
        assert!(sync_once(&svc, Some(&gate), None).await);
    }

    #[tokio::test]
    async fn an_unregistered_feature_leaves_the_channels_on() {
        let svc = FeatureService::new(
            Arc::new(FeatureRegistry::new()),
            Arc::new(InMemorySettingsStore::new()),
            None,
        );
        let gate = Gate::default();
        gate.set(false);
        assert!(sync_once(&svc, Some(&gate), None).await);
    }
}
