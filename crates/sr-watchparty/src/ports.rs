//! Collaborators of the Watch Party service, all behind small traits so the crate stays free of the chat
//! and catalog crates: channel permissions, the catalog play check, the age gate, guild limits, gateway
//! events and the channel announcement hook.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use sr_core::UserId;

/// Party state changed (created, ended, joined, left, host changed, queue, media, settings).
pub const EVENT_PARTY_UPDATE: &str = "WATCH_PARTY_UPDATE";
/// Playback state changed (play, pause, seek, rate, auto-pause).
pub const EVENT_PARTY_PLAYBACK: &str = "WATCH_PARTY_PLAYBACK";
/// An emoji reaction floating over the screen.
pub const EVENT_PARTY_REACTION: &str = "WATCH_PARTY_REACTION";
/// An ephemeral message in a private party's own chat.
pub const EVENT_PARTY_MESSAGE: &str = "WATCH_PARTY_MESSAGE";

/// What kind of conversation a channel is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    Text,
    Voice,
    /// A DM or group DM.
    Dm,
}

/// What an account may do in a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelPerms {
    pub guild_id: Option<i64>,
    pub kind: ChannelKind,
    /// `VIEW_CHANNEL` (a DM recipient always has it).
    pub view: bool,
    /// `START_WATCH_PARTY` (DM recipients may always start).
    pub start: bool,
    /// `CONTROL_WATCH_PARTY`.
    pub control: bool,
    /// `MANAGE_CHANNELS` or administrator.
    pub manage: bool,
    /// Voice channel user limit, when set.
    pub user_limit: Option<u32>,
}

/// Channel permissions, computed from guild roles and overwrites by the chat system.
#[async_trait]
pub trait ChannelAccess: Send + Sync {
    /// `None` when the channel does not exist or the account has no access to it.
    async fn perms(&self, user: i64, channel: i64) -> Result<Option<ChannelPerms>, String>;
    /// Everybody who can see `channel`, to tell them about a party card. Capped by the implementation.
    async fn viewers(&self, channel: i64) -> Result<Vec<i64>, String>;
}

/// No channels at all: every bound party is refused (tests, private-only installs).
pub struct NoChannels;

#[async_trait]
impl ChannelAccess for NoChannels {
    async fn perms(&self, _user: i64, _channel: i64) -> Result<Option<ChannelPerms>, String> {
        Ok(None)
    }
    async fn viewers(&self, _channel: i64) -> Result<Vec<i64>, String> {
        Ok(Vec::new())
    }
}

/// A title resolved by the catalog's play check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub title_id: i64,
    /// `0` for a movie.
    pub episode_id: i64,
    pub variant_id: Option<i64>,
    pub asset_id: i64,
    pub title: String,
    pub episode_label: Option<String>,
    pub duration_ms: Option<i64>,
    pub age_restricted: bool,
    pub live: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateError {
    /// Unknown, unpublished, merged, or without a playable asset.
    NotFound,
    /// Restricted title and the account has not confirmed 18+.
    AgeConfirmationRequired,
}

/// The catalog's play check (`CinemaService::resolve_play`): the title must be published and playable,
/// and a restricted one needs the account's age confirmation (D-184).
#[async_trait]
pub trait PlayGate: Send + Sync {
    async fn resolve(
        &self,
        adult: bool,
        title_id: i64,
        episode_id: Option<i64>,
        variant_id: Option<i64>,
    ) -> Result<Resolved, GateError>;

    /// A Live TV channel (`tv:<channel>`, G-16): it must exist, be enabled and not be off air, and what is on
    /// now needs the account's age confirmation when it is restricted. The party's media then names the
    /// channel: `title_id` and `asset_id` carry the TV channel id, `live` is set, there is no duration and
    /// nobody seeks (everybody follows the live edge). The default knows no channels.
    async fn resolve_live(&self, _adult: bool, _channel_id: i64) -> Result<Resolved, GateError> {
        Err(GateError::NotFound)
    }
}

/// No catalog: nothing is playable.
pub struct NoGate;

#[async_trait]
impl PlayGate for NoGate {
    async fn resolve(
        &self,
        _adult: bool,
        _title_id: i64,
        _episode_id: Option<i64>,
        _variant_id: Option<i64>,
    ) -> Result<Resolved, GateError> {
        Err(GateError::NotFound)
    }
}

/// Whether accounts confirmed they are 18 or older (`accounts.age_confirmed_at`).
#[async_trait]
pub trait AgeLookup: Send + Sync {
    async fn confirmed(&self, user: i64) -> bool;
}

/// Reads `accounts.age_confirmed_at`. A failure counts as "not confirmed".
pub struct PgAgeLookup(pub sqlx::PgPool);

#[async_trait]
impl AgeLookup for PgAgeLookup {
    async fn confirmed(&self, user: i64) -> bool {
        sqlx::query_scalar::<_, bool>(
            "SELECT age_confirmed_at IS NOT NULL FROM accounts WHERE id = $1",
        )
        .bind(user)
        .fetch_optional(&self.0)
        .await
        .map(|v| v.unwrap_or(false))
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "age lookup failed; treating the account as unconfirmed");
            false
        })
    }
}

/// Concurrent-party limit of a server by its level (docs/08 section 2.2).
#[async_trait]
pub trait GuildLimits: Send + Sync {
    async fn max_parties(&self, guild: i64) -> usize;
}

/// The same limit for every server. The default when no entitlements are wired in (tests, tools); the
/// server uses `sr_server::watchparty::EntitlementLimits`, which follows the server level.
pub struct FixedLimit(pub usize);

#[async_trait]
impl GuildLimits for FixedLimit {
    async fn max_parties(&self, _guild: i64) -> usize {
        self.0
    }
}

/// Delivers user-scoped gateway events. `name` is one of the `EVENT_*` constants.
#[async_trait]
pub trait PartyEvents: Send + Sync {
    async fn emit(&self, users: Vec<i64>, name: &'static str, payload: Value);
}

/// Drops every event (tests, tools).
pub struct NoEvents;

#[async_trait]
impl PartyEvents for NoEvents {
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
impl PartyEvents for BusEvents {
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
                tracing::warn!(error = %e, event = name, "publishing a watch party event failed");
            }
        }
    }
}

/// What the channel should be told about a party (WP-BE-008).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announcement {
    pub party_id: i64,
    pub channel_id: i64,
    pub host_id: i64,
    pub title: String,
    pub episode_label: Option<String>,
    pub viewers: usize,
    pub kind: AnnouncementKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnouncementKind {
    Started,
    Ended,
}

/// Posts the party card into the bound channel's chat. Failures never fail the party operation.
#[async_trait]
pub trait ChatHook: Send + Sync {
    async fn announce(&self, a: Announcement);
}

/// No announcements.
pub struct NoChat;

#[async_trait]
impl ChatHook for NoChat {
    async fn announce(&self, _a: Announcement) {}
}
