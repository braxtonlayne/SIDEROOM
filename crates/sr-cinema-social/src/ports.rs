//! Outbound ports: gateway events and the moderation hook.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use sr_core::UserId;

/// Gateway event of the account's own state (progress, lists, ratings), delivered to its other sessions.
pub const EVENT_CINEMA_UPDATE: &str = "SR_CINEMA_UPDATE";
/// Gateway event announcing "watching" presence to the audience of friends.
pub const EVENT_ACTIVITY_UPDATE: &str = "SR_ACTIVITY_UPDATE";

/// Delivers user-scoped gateway events. `name` is one of the `EVENT_*` constants.
#[async_trait]
pub trait CinemaEvents: Send + Sync {
    async fn emit(&self, users: Vec<i64>, name: &'static str, payload: Value);
}

/// Drops every event (tests, tools).
pub struct NoEvents;

#[async_trait]
impl CinemaEvents for NoEvents {
    async fn emit(&self, _users: Vec<i64>, _name: &'static str, _payload: Value) {}
}

/// Publishes as `DomainEvent::UserScoped` on each addressed user's topic.
pub struct BusEvents {
    bus: Arc<dyn sr_events::EventBus>,
}

impl BusEvents {
    pub fn new(bus: Arc<dyn sr_events::EventBus>) -> Self {
        Self { bus }
    }
}

#[async_trait]
impl CinemaEvents for BusEvents {
    async fn emit(&self, users: Vec<i64>, name: &'static str, payload: Value) {
        let users: Vec<UserId> = users
            .into_iter()
            .filter_map(|u| u64::try_from(u).ok().map(UserId::new))
            .collect();
        let ev = sr_events::DomainEvent::UserScoped {
            users: users.clone(),
            name: name.to_owned(),
            payload,
        };
        for u in users {
            if let Err(e) = self
                .bus
                .publish(sr_events::Topic::User(u), ev.clone())
                .await
            {
                tracing::warn!(error = %e, event = name, "publishing a cinema event failed");
            }
        }
    }
}

/// What kind of content a report or screening is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    Review,
    Comment,
}

impl ContentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Review => "review",
            Self::Comment => "comment",
        }
    }
}

/// The verdict of screening new text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screening {
    Allow,
    /// Refuse the text; the reason is shown to the author.
    Reject(String),
}

/// A report that was just stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewReport {
    pub id: i64,
    pub reporter: i64,
    pub kind: ContentKind,
    pub target: i64,
    pub reason: String,
}

/// Moderation extension points (automod filters, staff queues). The default allows everything.
#[async_trait]
pub trait ModerationHook: Send + Sync {
    /// Runs before a review or comment is stored.
    async fn screen(&self, _author: i64, _kind: ContentKind, _text: &str) -> Screening {
        Screening::Allow
    }
    /// Runs after a report was stored (a staff alert, an auto-hide rule).
    async fn on_report(&self, _report: &NewReport) {}
}

/// Allows everything and ignores reports.
pub struct NoModeration;

#[async_trait]
impl ModerationHook for NoModeration {}
