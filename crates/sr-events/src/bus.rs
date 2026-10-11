use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;
use thiserror::Error;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::event::{DomainEvent, Topic};

#[derive(Debug, Error)]
pub enum EventError {
    #[error("event bus backend error: {0}")]
    Backend(String),
}

pub type Result<T> = std::result::Result<T, EventError>;

#[async_trait]
pub trait EventBus: Send + Sync {
    /// Publish to `topic`. Succeeds even if nobody is subscribed.
    async fn publish(&self, topic: Topic, event: DomainEvent) -> Result<()>;
    /// Subscribe to `topic`. Only events published after this call are seen.
    /// Slow subscribers may miss events (lagged items are skipped).
    fn subscribe(&self, topic: Topic) -> BoxStream<'static, DomainEvent>;
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

    fn lock(&self) -> std::sync::MutexGuard<'_, TopicMap> {
        self.topics.lock().unwrap_or_else(|p| p.into_inner())
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
        if let Some(tx) = topics.get(&topic) {
            if tx.receiver_count() == 0 {
                topics.remove(&topic); // prune dead topics
            } else {
                // Err only means no receivers, which is fine.
                let _ = tx.send(event);
            }
        }
        Ok(())
    }

    fn subscribe(&self, topic: Topic) -> BoxStream<'static, DomainEvent> {
        let rx = self
            .lock()
            .entry(topic)
            .or_insert_with(|| broadcast::channel(self.capacity).0)
            .subscribe();
        BroadcastStream::new(rx)
            .filter_map(|r| async move { r.ok() }) // skip Lagged
            .boxed()
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
