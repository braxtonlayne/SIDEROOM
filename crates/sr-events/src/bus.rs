use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;
use thiserror::Error;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

use crate::event::{DomainEvent, Topic};

#[derive(Debug, Error)]
pub enum EventError {
    #[error("event bus backend error: {0}")]
    Backend(String),
}

pub type Result<T> = std::result::Result<T, EventError>;

#[async_trait]
pub trait EventBus: Send + Sync {
    /// Publish to `topic`. Succeeds even if nobody is subscribed. Every
    /// implementation also delivers the event to [`Topic::Firehose`] subscribers.
    async fn publish(&self, topic: Topic, event: DomainEvent) -> Result<()>;
    /// Subscribe to `topic`. Only events published after this call are seen.
    /// Slow subscribers may miss events (lagged items are skipped).
    /// [`Topic::Firehose`] sees everything published to any topic.
    fn subscribe(&self, topic: Topic) -> BoxStream<'static, DomainEvent>;
    /// Like [`subscribe`](Self::subscribe), but a subscriber that fell behind is told: `Err(missed)` carries the
    /// number of events dropped before the next item (SEC-AUD-124, D-620). A consumer whose state depends on every
    /// event (the gateway's membership pump) reconciles on `Err`. Default: no gap is ever reported.
    fn subscribe_lagging(
        &self,
        topic: Topic,
    ) -> BoxStream<'static, std::result::Result<DomainEvent, u64>> {
        self.subscribe(topic).map(Ok).boxed()
    }
    /// Whether every publisher and subscriber of this bus lives in this process. Only then can
    /// in-memory state (voice occupancy, say) answer for the whole deployment. Default: no.
    fn is_single_node(&self) -> bool {
        false
    }
}

type TopicMap = HashMap<Topic, broadcast::Sender<DomainEvent>>;

/// Single-node bus: one tokio broadcast channel per topic.
#[derive(Debug)]
pub struct InProcessBus {
    capacity: usize,
    topics: Mutex<TopicMap>,
}

impl InProcessBus {
    pub const DEFAULT_CAPACITY: usize = 256;

    pub fn new() -> Self {
        Self::with_capacity(Self::DEFAULT_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            topics: Mutex::new(HashMap::new()),
        }
    }

    /// Buffer of the firehose channel: it carries every event, so it gets more
    /// headroom than a per-topic channel before slow consumers start to lag.
    pub const FIREHOSE_CAPACITY: usize = 4096;

    fn lock(&self) -> std::sync::MutexGuard<'_, TopicMap> {
        self.topics.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn capacity_of(&self, topic: Topic) -> usize {
        if topic == Topic::Firehose {
            self.capacity.max(Self::FIREHOSE_CAPACITY)
        } else {
            self.capacity
        }
    }
}

/// Send `event` to `topic`'s subscribers, pruning the topic when nobody listens.
fn deliver(topics: &mut TopicMap, topic: Topic, event: DomainEvent) {
    if let Some(tx) = topics.get(&topic) {
        if tx.receiver_count() == 0 {
            topics.remove(&topic); // prune dead topics
        } else {
            // Err only means no receivers, which is fine.
            let _ = tx.send(event);
        }
    }
}

impl Default for InProcessBus {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventBus for InProcessBus {
    async fn publish(&self, topic: Topic, event: DomainEvent) -> Result<()> {
        let mut topics = self.lock();
        if topic != Topic::Firehose && topics.contains_key(&Topic::Firehose) {
            deliver(&mut topics, Topic::Firehose, event.clone());
        }
        deliver(&mut topics, topic, event);
        Ok(())
    }

    fn subscribe(&self, topic: Topic) -> BoxStream<'static, DomainEvent> {
        self.subscribe_lagging(topic)
            .filter_map(|r| async move { r.ok() }) // skip Lagged
            .boxed()
    }

    fn subscribe_lagging(
        &self,
        topic: Topic,
    ) -> BoxStream<'static, std::result::Result<DomainEvent, u64>> {
        let capacity = self.capacity_of(topic);
        let rx = self
            .lock()
            .entry(topic)
            .or_insert_with(|| broadcast::channel(capacity).0)
            .subscribe();
        BroadcastStream::new(rx)
            .map(|r| r.map_err(|BroadcastStreamRecvError::Lagged(n)| n))
            .boxed()
    }

    fn is_single_node(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GuildId;

    #[tokio::test]
    async fn contract() {
        crate::contract::run_all(InProcessBus::new).await;
    }

    #[tokio::test]
    async fn in_process_subscribe_lagging_reports_the_gap() {
        let bus = InProcessBus::with_capacity(2);
        let mut s = bus.subscribe_lagging(Topic::Platform);
        for n in 1..=5 {
            bus.publish(
                Topic::Platform,
                DomainEvent::GuildDeleted {
                    guild: GuildId::new(n),
                },
            )
            .await
            .unwrap();
        }
        assert_eq!(s.next().await, Some(Err(3)));
        let next = s.next().await.unwrap().unwrap();
        assert_eq!(
            next,
            DomainEvent::GuildDeleted {
                guild: GuildId::new(4)
            }
        );
        // The plain subscription still skips the gap silently.
        let mut plain = bus.subscribe(Topic::Platform);
        for n in 6..=10 {
            bus.publish(
                Topic::Platform,
                DomainEvent::GuildDeleted {
                    guild: GuildId::new(n),
                },
            )
            .await
            .unwrap();
        }
        assert_eq!(
            plain.next().await,
            Some(DomainEvent::GuildDeleted {
                guild: GuildId::new(9)
            })
        );
    }

    #[tokio::test]
    async fn prunes_topics_without_subscribers() {
        let bus = InProcessBus::new();
        drop(bus.subscribe(Topic::Platform));
        bus.publish(
            Topic::Platform,
            DomainEvent::GuildDeleted {
                guild: GuildId::new(1),
            },
        )
        .await
        .unwrap();
        assert!(bus.lock().is_empty());
    }
}
