//! Delivers platform events and ticks to the registered feature modules.
//!
//! One task reads the event firehose and hands each guild event to every module
//! that is enabled in that guild, one module at a time, so a module never sees two
//! events concurrently (sticky counters and starboard links are read-modify-write).
//! A second task calls [`crate::Feature::on_tick`]. Failures are logged and
//! never propagate: a broken module must not take the others down.

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use sr_events::{DomainEvent, EventBus, FirehoseDedup, GuildId};
use tokio::task::JoinHandle;

use crate::feature::{FeatureCtx, ScopeRef};

/// Default interval between ticks.
pub const TICK_INTERVAL: Duration = Duration::from_secs(30);

/// The guild an event belongs to, if it is a guild event.
pub fn guild_of(ev: &DomainEvent) -> Option<GuildId> {
    use DomainEvent as E;
    match ev {
        E::MemberJoined { guild, .. }
        | E::MemberLeft { guild, .. }
        | E::ChannelCreated { guild, .. }
        | E::ChannelUpdated { guild, .. }
        | E::ChannelDeleted { guild, .. }
        | E::RoleCreated { guild, .. }
        | E::RoleUpdated { guild, .. }
        | E::RoleDeleted { guild, .. }
        | E::MemberUpdated { guild, .. }
        | E::MemberBanned { guild, .. }
        | E::MemberUnbanned { guild, .. }
        | E::ThreadCreated { guild, .. } => Some(*guild),
        E::MessageSent { guild, .. }
        | E::MessageEdited { guild, .. }
        | E::MessageDeleted { guild, .. }
        | E::MessagesBulkDeleted { guild, .. }
        | E::ReactionAdded { guild, .. }
        | E::ReactionRemoved { guild, .. }
        | E::ReactionsCleared { guild, .. }
        | E::ReactionEmojiCleared { guild, .. } => *guild,
        _ => None,
    }
}

/// Dispatches events and ticks. Cheap to clone.
#[derive(Clone)]
pub struct FeatureRuntime {
    ctx: FeatureCtx,
}

impl FeatureRuntime {
    pub fn new(ctx: FeatureCtx) -> Self {
        Self { ctx }
    }

    /// Give one event to every module enabled in its guild.
    pub async fn dispatch(&self, ev: &DomainEvent) {
        let Some(guild) = guild_of(ev) else { return };
        let scope = ScopeRef::Guild { guild };
        for feature in self.ctx.settings.registry().iter() {
            match self.ctx.settings.is_enabled(feature.id(), &scope).await {
                Ok(true) => {}
                Ok(false) => continue,
                Err(e) => {
                    tracing::warn!(feature = feature.id(), error = %e, "feature flag lookup failed");
                    continue;
                }
            }
            if let Err(e) = feature.on_event(ev, &self.ctx).await {
                tracing::warn!(feature = feature.id(), error = %e, "feature event handler failed");
            }
        }
    }

    /// Run every module's tick once.
    pub async fn tick(&self) {
        for feature in self.ctx.settings.registry().iter() {
            if let Err(e) = feature.on_tick(&self.ctx).await {
                tracing::warn!(feature = feature.id(), error = %e, "feature tick failed");
            }
        }
    }

    /// Start the event and tick tasks. Returns both handles.
    pub fn spawn(&self, bus: Arc<dyn EventBus>, tick: Duration) -> Vec<JoinHandle<()>> {
        let events = {
            let rt = self.clone();
            let mut stream = FirehoseDedup::subscribe(bus.as_ref());
            tokio::spawn(async move {
                while let Some(ev) = stream.next().await {
                    rt.dispatch(&ev).await;
                }
            })
        };
        let ticks = {
            let rt = self.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(tick);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                // The first tick fires immediately; skip it so startup is quiet.
                interval.tick().await;
                loop {
                    interval.tick().await;
                    rt.tick().await;
                }
            })
        };
        vec![events, ticks]
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use futures::future::BoxFuture;
    use sr_events::{ChannelId, UserId};

    use super::*;
    use crate::FeatureRegistry;
    use crate::feature::{Feature, FeatureError, FeatureMeta, Scope};
    use crate::schema::SettingsSchema;
    use crate::testing::Harness;

    static EVENTS: AtomicUsize = AtomicUsize::new(0);
    static TICKS: AtomicUsize = AtomicUsize::new(0);

    #[derive(Default)]
    struct Counter;

    impl Feature for Counter {
        fn id(&self) -> &'static str {
            "test.counter"
        }
        fn meta(&self) -> FeatureMeta {
            FeatureMeta {
                name: "c".into(),
                description: "c".into(),
                section: "Lounge".into(),
                phase: 11,
                default_enabled: false,
            }
        }
        fn settings_schema(&self) -> SettingsSchema {
            SettingsSchema::default()
        }
        fn scopes(&self) -> &[Scope] {
            &[Scope::Platform, Scope::Guild]
        }
        fn on_event<'a>(
            &'a self,
            _ev: &'a DomainEvent,
            _ctx: &'a FeatureCtx,
        ) -> BoxFuture<'a, Result<(), FeatureError>> {
            Box::pin(async {
                EVENTS.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }
        fn on_tick<'a>(&'a self, _ctx: &'a FeatureCtx) -> BoxFuture<'a, Result<(), FeatureError>> {
            Box::pin(async {
                TICKS.fetch_add(1, Ordering::SeqCst);
                Err(FeatureError::UnknownFeature(
                    "a failing tick is only logged".into(),
                ))
            })
        }
    }

    fn said(guild: Option<u64>) -> DomainEvent {
        DomainEvent::MessageSent {
            guild: guild.map(GuildId::new),
            channel: ChannelId::new(5),
            message_id: 1,
            author: UserId::new(2),
            data: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn events_reach_only_modules_enabled_in_that_guild() {
        let mut reg = FeatureRegistry::new();
        reg.register(Counter).unwrap();
        // The harness enables features in guild 1 only.
        let h = Harness::new(reg).await;
        let rt = FeatureRuntime::new(h.ctx.clone());

        rt.dispatch(&said(Some(1))).await;
        assert_eq!(EVENTS.load(Ordering::SeqCst), 1);
        rt.dispatch(&said(Some(2))).await;
        rt.dispatch(&said(None)).await;
        assert_eq!(
            EVENTS.load(Ordering::SeqCst),
            1,
            "other guilds and DMs are skipped"
        );

        h.service
            .set_enabled("test.counter", &crate::ScopeRef::Platform, Some(false))
            .await
            .unwrap();
        rt.dispatch(&said(Some(1))).await;
        assert_eq!(EVENTS.load(Ordering::SeqCst), 1, "platform kill switch");

        rt.tick().await;
        assert_eq!(TICKS.load(Ordering::SeqCst), 1);
    }
}
