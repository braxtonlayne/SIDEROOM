//! Notification settings: per-guild defaults, channel/category overrides and
//! timed mutes (LNG-NTF-001, docs/specs/lounge-chat.md section 10).

use sr_core::{ChannelId, GuildId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationLevel {
    All,
    OnlyMentions,
    Nothing,
}

impl NotificationLevel {
    /// Discord `message_notifications` codes.
    pub const fn code(self) -> i16 {
        match self {
            Self::All => 0,
            Self::OnlyMentions => 1,
            Self::Nothing => 2,
        }
    }

    pub const fn from_code(code: i16) -> Option<Self> {
        Some(match code {
            0 => Self::All,
            1 => Self::OnlyMentions,
            2 => Self::Nothing,
            _ => return None,
        })
    }
}

/// Mute presets offered by the UI ("15 min ... until I turn it back on").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MuteDuration {
    Minutes15,
    Hour1,
    Hours3,
    Hours8,
    Hours24,
    UntilTurnedOff,
}

impl MuteDuration {
    pub const fn millis(self) -> Option<i64> {
        Some(match self {
            Self::Minutes15 => 15 * 60_000,
            Self::Hour1 => 3_600_000,
            Self::Hours3 => 3 * 3_600_000,
            Self::Hours8 => 8 * 3_600_000,
            Self::Hours24 => 24 * 3_600_000,
            Self::UntilTurnedOff => return None,
        })
    }
}

/// A mute with an optional expiry (`end_ms == None` means until turned off).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mute {
    pub muted: bool,
    pub end_ms: Option<i64>,
}

impl Mute {
    pub fn new(duration: MuteDuration, now_ms: i64) -> Self {
        Self {
            muted: true,
            end_ms: duration.millis().map(|d| now_ms.saturating_add(d)),
        }
    }

    pub fn is_active(&self, now_ms: i64) -> bool {
        self.muted && self.end_ms.is_none_or(|end| now_ms < end)
    }

    /// Drop an expired mute so stored state converges.
    pub fn normalized(self, now_ms: i64) -> Self {
        if self.is_active(now_ms) {
            self
        } else {
            Self::default()
        }
    }
}

/// Settings for one guild, or for DMs and group DMs when `guild` is `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeSettings {
    pub user: UserId,
    pub guild: Option<GuildId>,
    /// `None` inherits the guild's `default_message_notifications`.
    pub level: Option<NotificationLevel>,
    pub suppress_everyone: bool,
    pub suppress_roles: bool,
    pub suppress_highlights: bool,
    pub mute_scheduled_events: bool,
    pub mobile_push: bool,
    pub mute: Mute,
}

impl ScopeSettings {
    pub fn new(user: UserId, guild: Option<GuildId>) -> Self {
        Self {
            user,
            guild,
            level: None,
            suppress_everyone: false,
            suppress_roles: false,
            suppress_highlights: false,
            mute_scheduled_events: false,
            mobile_push: true,
            mute: Mute::default(),
        }
    }
}

/// Override for one channel or category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelOverride {
    pub user: UserId,
    pub channel: ChannelId,
    pub level: Option<NotificationLevel>,
    pub mute: Mute,
}

impl ChannelOverride {
    pub fn new(user: UserId, channel: ChannelId) -> Self {
        Self {
            user,
            channel,
            level: None,
            mute: Mute::default(),
        }
    }
}

/// Resolve the level: channel override, then category override, then the user's
/// guild setting, then the guild default.
pub fn effective_level(
    guild_default: NotificationLevel,
    scope: Option<&ScopeSettings>,
    category: Option<&ChannelOverride>,
    channel: Option<&ChannelOverride>,
) -> NotificationLevel {
    channel
        .and_then(|c| c.level)
        .or_else(|| category.and_then(|c| c.level))
        .or_else(|| scope.and_then(|s| s.level))
        .unwrap_or(guild_default)
}

/// Muted when the channel, its category or the whole scope is muted.
pub fn is_muted(
    now_ms: i64,
    scope: Option<&ScopeSettings>,
    category: Option<&ChannelOverride>,
    channel: Option<&ChannelOverride>,
) -> bool {
    channel.is_some_and(|c| c.mute.is_active(now_ms))
        || category.is_some_and(|c| c.mute.is_active(now_ms))
        || scope.is_some_and(|s| s.mute.is_active(now_ms))
}

/// How a message relates to the recipient.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MentionKind {
    pub direct: bool,
    pub reply: bool,
    pub everyone: bool,
    pub role: bool,
}

/// Whether the message counts as a mention for the recipient (badge + count).
pub fn counts_as_mention(kind: MentionKind, scope: Option<&ScopeSettings>) -> bool {
    let (se, sr) = scope.map_or((false, false), |s| (s.suppress_everyone, s.suppress_roles));
    kind.direct || kind.reply || (kind.everyone && !se) || (kind.role && !sr)
}

/// Whether the message should raise a notification.
pub fn should_notify(
    level: NotificationLevel,
    muted: bool,
    kind: MentionKind,
    scope: Option<&ScopeSettings>,
) -> bool {
    if muted {
        return false;
    }
    match level {
        NotificationLevel::All => true,
        NotificationLevel::OnlyMentions => counts_as_mention(kind, scope),
        NotificationLevel::Nothing => false,
    }
}
