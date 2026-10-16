//! Participants follow the call (J-16, D-248): when a member leaves the voice channel an Activity runs in,
//! they leave the Activity, and the last one out ends it. The gateway announces `VoiceLeft` when a voice
//! state is removed, which already covers the session grace period of a dropped connection; this adds a
//! short debounce so a move or a quick rejoin of the same channel is not a leave.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::StreamExt;
use sr_events::{DomainEvent, EventBus, Topic};

use crate::ActivityService;

/// How long a departure waits for the same member to come back to the same channel.
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_secs(5);

/// Turns voice events into Activity participant changes. Cheap to clone.
#[derive(Clone)]
pub struct VoiceFollower {
    service: ActivityService,
    debounce: Duration,
    /// Departures not yet applied: `(channel, user)` to a generation; a join bumps or removes it.
    waiting: Arc<Mutex<HashMap<(i64, i64), u64>>>,
}

fn raw(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(0)
}

impl VoiceFollower {
    pub fn new(service: ActivityService, debounce: Duration) -> Self {
        Self {
            service,
            debounce,
            waiting: Arc::default(),
        }
    }

    /// Feed one event. A `VoiceLeft` schedules the removal; a `VoiceJoined` of the same member and channel
    /// within the debounce cancels it. Other events are ignored.
    pub fn on_event(&self, ev: &DomainEvent) {
        match ev {
            DomainEvent::VoiceJoined { channel, user, .. } => {
                if let Ok(mut w) = self.waiting.lock() {
                    w.remove(&(raw(channel.get()), raw(user.get())));
                }
            }
            DomainEvent::VoiceLeft { channel, user, .. } => {
                let key = (raw(channel.get()), raw(user.get()));
                let generation = {
                    let Ok(mut w) = self.waiting.lock() else {
                        return;
                    };
                    let g = w.get(&key).copied().unwrap_or(0) + 1;
                    w.insert(key, g);
                    g
                };
                let me = self.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(me.debounce).await;
                    let still = me
                        .waiting
                        .lock()
                        .ok()
                        .and_then(|mut w| {
                            (w.get(&key) == Some(&generation)).then(|| w.remove(&key))
                        })
                        .is_some();
                    if still && let Err(e) = me.service.voice_left(key.1, key.0).await {
                        tracing::warn!(error = %e, "removing a voice leaver from an activity failed");
                    }
                });
            }
            _ => {}
        }
    }

    /// Follow the bus until it closes. Spawn once at startup.
    pub fn spawn(self, bus: Arc<dyn EventBus>) -> tokio::task::JoinHandle<()> {
        let mut events = bus.subscribe(Topic::Firehose);
        tokio::spawn(async move {
            while let Some(ev) = events.next().await {
                self.on_event(&ev);
            }
        })
    }
}
