//! Scheduled events at runtime (G-91, G-92, G-93; D-222):
//!
//! * `scheduled_events.tick` is the event clock, a leased singleton job (`SKIP LOCKED`
//!   claim, re-enqueued 30 s ahead by its own handler, seeded at startup when absent), so
//!   one node advances events however many run. `advance` is idempotent, so an overlap after
//!   a crash costs nothing.
//! * [`spawn_voice_listener`] starts a voice or stage event when a member joins its channel
//!   at or after the start time.
//! * [`GatewayEventWorld`] is the voice side the clock reads (occupancy, stage instances) and
//!   the stage instances it opens and closes.
//! * [`EventStartPush`] tells interested members when an event starts: a user-scoped gateway
//!   dispatch (`SR_SCHEDULED_EVENT_START`) and Web Push (suppressed while a session is active,
//!   D-191; honours the "mute new events" setting of the server).

use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;
use serde_json::json;
use sr_api::routes::settings::scheduled_events as event_routes;
use sr_api::routes::settings::{EventStartNotifier, EventWorld, SettingsDeps};
use sr_api::routes::social::SocialDeps;
use sr_chat::repo::ChatRepository;
use sr_chat::settings::scheduled_events::{ScheduledEvent, VoiceView};
use sr_core::{ChannelId, GuildId, UserId};
use sr_db::jobs::{self, Job};
use sr_db::runner::{HandlerError, JobRunner};
use sr_db::{DbResult, PgPool};
use sr_events::{DomainEvent, EventBus, Topic};
use sr_push::{Notification, PushSender, PushStore, SendOutcome, Urgency};
use tokio::task::JoinHandle;

use crate::lag_bus::TapItem;
use crate::push_notify::SessionActivity;

/// Seconds a push service may hold an undelivered start notification: an event that began an
/// hour ago is no news.
const PUSH_TTL_SECS: u32 = 15 * 60;

async fn tick_handler(pool: PgPool, deps: Arc<SettingsDeps>, job: Job) -> Result<(), HandlerError> {
    // Re-arm before working: a node that dies mid-turn leaves a queued job, not a stuck one.
    jobs::reschedule_running(&pool, job.id, jobs::SCHEDULED_EVENTS_TICK_INTERVAL).await?;
    event_routes::advance(&deps).await;
    Ok(())
}

/// Register the event clock on `runner`. Pair with [`schedule`].
pub fn register(runner: JobRunner, pool: &PgPool, deps: Arc<SettingsDeps>) -> JobRunner {
    let pool = pool.clone();
    runner.register(jobs::SCHEDULED_EVENTS_TICK_KIND, 1, move |job| {
        Box::pin(tick_handler(pool.clone(), deps.clone(), job))
    })
}

/// Seed the clock when no node has yet. Call only when [`register`] is on the runner.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::seed_scheduled_events_tick(pool).await?;
    Ok(())
}

/// Start voice and stage events when someone joins their channel (`VoiceJoined` on the bus).
/// Subscribe to the tap before calling, so nothing published afterwards is missed.
pub fn spawn_voice_listener(
    mut events: BoxStream<'static, TapItem>,
    deps: Arc<SettingsDeps>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(item) = events.next().await {
            let Ok(DomainEvent::VoiceJoined { guild, channel, .. }) = item else {
                continue;
            };
            let deps = deps.clone();
            tokio::spawn(async move {
                event_routes::on_voice_joined(&deps, guild, channel).await;
            });
        }
    })
}

/// The gateway behind the event clock.
pub struct GatewayEventWorld(pub sr_gateway::Gateway);

#[async_trait]
impl EventWorld for GatewayEventWorld {
    async fn view(&self, channels: &[ChannelId]) -> VoiceView {
        let mut view = VoiceView::default();
        for ch in channels {
            let n = self.0.voice_occupants(*ch).len();
            if n > 0 {
                view.occupants.insert(*ch, n);
            }
            if self.0.stage_instance(*ch).is_some() {
                view.live_stages.insert(*ch);
            }
        }
        view
    }

    async fn start_stage(
        &self,
        _guild: GuildId,
        channel: ChannelId,
        topic: &str,
        by: UserId,
        event_id: u64,
    ) -> Result<(), String> {
        use sr_gateway::StageError as E;
        self.0
            .stage_open_for_event(channel, topic, by, event_id)
            .await
            .map(|_| ())
            .map_err(|e| {
                match e {
                    E::AlreadyExists => "A stage is already running in this channel.",
                    E::NotStageChannel | E::UnknownChannel => "That is not a stage channel.",
                    E::Unavailable => "Voice is switched off on this server.",
                    _ => "The stage could not be started.",
                }
                .to_owned()
            })
    }

    async fn end_stage(&self, channel: ChannelId) {
        self.0.stage_end_for_event(channel).await;
    }
}

/// Gateway dispatch and Web Push for a started event.
pub struct EventStartPush {
    pub bus: Arc<dyn EventBus>,
    pub social: Arc<SocialDeps>,
    pub chat: Arc<dyn ChatRepository>,
    pub store: Arc<dyn PushStore>,
    pub sender: Arc<dyn PushSender>,
    /// Gateway view of who is active; `None` never suppresses.
    pub activity: Option<Arc<dyn SessionActivity>>,
}

/// Where the notification opens: the event's channel (or the server) with the event named,
/// which the web opens in the Events dialog.
pub fn event_url(ev: &ScheduledEvent) -> String {
    match ev.channel_id {
        Some(ch) => format!(
            "/channels/{}/{}?event={}",
            ev.guild_id.get(),
            ch.get(),
            ev.id
        ),
        None => format!("/channels/{}?event={}", ev.guild_id.get(), ev.id),
    }
}

#[async_trait]
impl EventStartNotifier for EventStartPush {
    async fn event_started(&self, ev: &ScheduledEvent, interested: &[UserId]) {
        // Members who muted new events of this server hear nothing.
        let mut users = Vec::with_capacity(interested.len());
        for u in interested {
            match self
                .social
                .service
                .scope_settings(*u, Some(ev.guild_id))
                .await
            {
                Ok(s) if s.mute_scheduled_events => {}
                _ => users.push(*u),
            }
        }
        if users.is_empty() {
            return;
        }
        let url = event_url(ev);
        let guild_name = self
            .chat
            .get_guild(ev.guild_id)
            .await
            .ok()
            .flatten()
            .map(|g| g.name);
        let payload = json!({
            "guild_id": ev.guild_id.to_string(),
            "guild_scheduled_event_id": ev.id.to_string(),
            "channel_id": ev.channel_id.map(|c| c.to_string()),
            "name": ev.name,
            "url": url,
        });
        for u in &users {
            let sent = self
                .bus
                .publish(
                    Topic::User(*u),
                    DomainEvent::UserScoped {
                        users: vec![*u],
                        name: "SR_SCHEDULED_EVENT_START".to_owned(),
                        payload: payload.clone(),
                    },
                )
                .await;
            if let Err(e) = sent {
                tracing::warn!(error = %e, "event start dispatch failed");
            }
        }
        // Push only to people who are not using the app right now (D-191).
        let away: Vec<u64> = users
            .iter()
            .filter(|u| {
                self.activity
                    .as_deref()
                    .is_none_or(|a| !a.has_active_session(**u))
            })
            .map(|u| u.get())
            .collect();
        if away.is_empty() {
            return;
        }
        let subs = match self.store.for_users(&away).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "could not load push subscriptions");
                return;
            }
        };
        let body = match &guild_name {
            Some(g) => format!("{g} \u{b7} starting now"),
            None => "Starting now".to_owned(),
        };
        let note = json!({
            "title": ev.name,
            "body": body,
            "url": url,
            "tag": format!("event-{}", ev.id),
        });
        for s in subs {
            let n = Notification {
                payload: note.to_string().into_bytes(),
                collapse_key: Some(format!("event-{}", ev.id)),
                urgency: Urgency::Normal,
                ttl_secs: PUSH_TTL_SECS,
            };
            match self.sender.send(&s.sub, &n).await {
                Ok(SendOutcome::Gone) => {
                    let _ = self.store.delete_endpoint(&s.sub.endpoint).await;
                }
                Ok(SendOutcome::Failed(status)) => {
                    tracing::debug!(status, "push service refused an event notification");
                }
                Ok(SendOutcome::Delivered) => {}
                Err(e) => tracing::debug!(error = %e, "event push delivery failed"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_chat::settings::scheduled_events::{EntityType, EventStatus};

    fn ev(channel: Option<u64>) -> ScheduledEvent {
        ScheduledEvent {
            id: 9,
            guild_id: GuildId::new(1),
            channel_id: channel.map(ChannelId::new),
            creator_id: UserId::new(2),
            name: "Movie night".into(),
            description: None,
            start_ms: 0,
            end_ms: None,
            status: EventStatus::Active,
            entity_type: EntityType::Voice,
            location: None,
            created_at_ms: 0,
            entity_id: None,
            image_key: None,
            empty_since_ms: None,
        }
    }

    #[test]
    fn the_notification_opens_the_event() {
        assert_eq!(event_url(&ev(Some(5))), "/channels/1/5?event=9");
        assert_eq!(event_url(&ev(None)), "/channels/1?event=9");
    }
}
