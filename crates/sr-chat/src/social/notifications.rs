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
/// guild setting, then the guild default. See [`effective_level_with_user_default`]
/// for the user's own global default.
pub fn effective_level(
    guild_default: NotificationLevel,
    scope: Option<&ScopeSettings>,
    category: Option<&ChannelOverride>,
    channel: Option<&ChannelOverride>,
) -> NotificationLevel {
    effective_level_with_user_default(guild_default, None, scope, category, channel)
}

/// Like [`effective_level`], with the user's global default (`user_default`) between
/// the per-guild setting and the guild default: an explicit per-guild or per-channel
/// choice always wins, then the user's own default, then what the server suggests.
pub fn effective_level_with_user_default(
    guild_default: NotificationLevel,
    user_default: Option<NotificationLevel>,
    scope: Option<&ScopeSettings>,
    category: Option<&ChannelOverride>,
    channel: Option<&ChannelOverride>,
) -> NotificationLevel {
    channel
        .and_then(|c| c.level)
        .or_else(|| category.and_then(|c| c.level))
        .or_else(|| scope.and_then(|s| s.level))
        .or(user_default)
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

/// Why a message notifies, strongest reason first (LNG-NTF-002). The client uses it to
/// choose the push title and the inbox tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NotifyReason {
    /// The recipient is mentioned by name.
    Mention,
    /// A reply to the recipient's message.
    Reply,
    /// `@everyone` / `@here`.
    Everyone,
    /// One of the recipient's roles is mentioned.
    Role,
    /// A message in a thread the recipient follows.
    ThreadFollow,
    /// The message contains one of the recipient's highlight keywords.
    Keyword,
    /// A message in a DM or group DM that notifies for every message.
    DirectMessage,
    /// The level is "all messages" and nothing more specific applied.
    AllMessages,
}

/// Facts about one message for one recipient, beyond [`MentionKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NotifyInput {
    pub kind: MentionKind,
    /// The channel is a DM or group DM.
    pub is_dm: bool,
    /// The recipient follows the thread the message was posted in.
    pub followed_thread: bool,
    /// The content matches one of the recipient's keywords.
    pub keyword_hit: bool,
}

/// The engine's verdict for one recipient.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotifyDecision {
    /// Show an in-app/desktop notification.
    pub notify: bool,
    /// Also send it to the recipient's push subscriptions (mobile push on).
    pub push: bool,
    pub reason: Option<NotifyReason>,
}

impl NotifyDecision {
    const NONE: Self = Self {
        notify: false,
        push: false,
        reason: None,
    };
}

fn words_of(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\u{27}')
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Case-insensitive whole-word (or whole-phrase) match of any of `keywords` in `content`.
pub fn matches_keyword(content: &str, keywords: &[String]) -> bool {
    if keywords.is_empty() {
        return false;
    }
    let words = words_of(content);
    keywords.iter().any(|k| {
        let parts = words_of(k);
        !parts.is_empty() && words.windows(parts.len()).any(|w| w == parts.as_slice())
    })
}

/// The notification decision engine: level, mute, suppression toggles, mentions, replies,
/// DMs, thread follows and keyword highlights combine into one verdict.
///
/// * A muted scope or a level of "nothing" never notifies.
/// * "All messages" notifies for everything; the reason is the strongest applicable one.
/// * "Only @mentions" notifies for direct mentions and replies, `@everyone`/role mentions
///   unless suppressed, followed threads, and keyword hits unless highlights are suppressed.
/// * `push` additionally requires the scope's `mobile_push` (on by default).
pub fn decide(
    level: NotificationLevel,
    muted: bool,
    scope: Option<&ScopeSettings>,
    input: NotifyInput,
) -> NotifyDecision {
    if muted || level == NotificationLevel::Nothing {
        return NotifyDecision::NONE;
    }
    let (suppress_everyone, suppress_roles, suppress_highlights, mobile_push) =
        scope.map_or((false, false, false, true), |s| {
            (
                s.suppress_everyone,
                s.suppress_roles,
                s.suppress_highlights,
                s.mobile_push,
            )
        });
    let k = input.kind;
    let mut reasons: Vec<NotifyReason> = Vec::new();
    if k.direct {
        reasons.push(NotifyReason::Mention);
    }
    if k.reply {
        reasons.push(NotifyReason::Reply);
    }
    if k.everyone && !suppress_everyone {
        reasons.push(NotifyReason::Everyone);
    }
    if k.role && !suppress_roles {
        reasons.push(NotifyReason::Role);
    }
    if input.followed_thread {
        reasons.push(NotifyReason::ThreadFollow);
    }
    if input.keyword_hit && !suppress_highlights {
        reasons.push(NotifyReason::Keyword);
    }
    if level == NotificationLevel::All {
        reasons.push(if input.is_dm {
            NotifyReason::DirectMessage
        } else {
            NotifyReason::AllMessages
        });
    }
    let reason = reasons.iter().min().copied();
    NotifyDecision {
        notify: reason.is_some(),
        push: reason.is_some() && mobile_push,
        reason,
    }
}

#[cfg(test)]
mod engine_tests {
    use super::*;

    fn mention() -> NotifyInput {
        NotifyInput {
            kind: MentionKind {
                direct: true,
                ..MentionKind::default()
            },
            ..NotifyInput::default()
        }
    }

    #[test]
    fn mute_and_nothing_silence_everything() {
        assert!(!decide(NotificationLevel::All, true, None, mention()).notify);
        assert!(!decide(NotificationLevel::Nothing, false, None, mention()).notify);
    }

    #[test]
    fn all_messages_notifies_with_the_strongest_reason() {
        let plain = decide(NotificationLevel::All, false, None, NotifyInput::default());
        assert_eq!(plain.reason, Some(NotifyReason::AllMessages));
        assert!(plain.push);
        let dm = decide(
            NotificationLevel::All,
            false,
            None,
            NotifyInput {
                is_dm: true,
                ..NotifyInput::default()
            },
        );
        assert_eq!(dm.reason, Some(NotifyReason::DirectMessage));
        let both = decide(NotificationLevel::All, false, None, mention());
        assert_eq!(both.reason, Some(NotifyReason::Mention));
    }

    #[test]
    fn only_mentions_needs_a_reason() {
        let l = NotificationLevel::OnlyMentions;
        assert!(!decide(l, false, None, NotifyInput::default()).notify);
        assert!(
            !decide(
                l,
                false,
                None,
                NotifyInput {
                    is_dm: true,
                    ..NotifyInput::default()
                }
            )
            .notify
        );
        assert!(decide(l, false, None, mention()).notify);
        let reply = NotifyInput {
            kind: MentionKind {
                reply: true,
                ..MentionKind::default()
            },
            ..NotifyInput::default()
        };
        assert_eq!(
            decide(l, false, None, reply).reason,
            Some(NotifyReason::Reply)
        );
        let thread = NotifyInput {
            followed_thread: true,
            ..NotifyInput::default()
        };
        assert_eq!(
            decide(l, false, None, thread).reason,
            Some(NotifyReason::ThreadFollow)
        );
    }

    #[test]
    fn suppress_toggles_and_mobile_push_apply() {
        let l = NotificationLevel::OnlyMentions;
        let mut scope = ScopeSettings::new(UserId::new(1), None);
        let everyone = NotifyInput {
            kind: MentionKind {
                everyone: true,
                role: true,
                ..MentionKind::default()
            },
            keyword_hit: true,
            ..NotifyInput::default()
        };
        assert!(decide(l, false, Some(&scope), everyone).notify);
        scope.suppress_everyone = true;
        scope.suppress_roles = true;
        scope.suppress_highlights = true;
        assert!(!decide(l, false, Some(&scope), everyone).notify);
        scope.mobile_push = false;
        let d = decide(l, false, Some(&scope), mention());
        assert!(d.notify && !d.push, "in-app yes, push no");
    }

    #[test]
    fn keywords_match_whole_words_case_insensitively() {
        let k = vec!["Deploy".to_owned(), "red alert".to_owned(), String::new()];
        assert!(matches_keyword("we should DEPLOY now", &k));
        assert!(matches_keyword("It is a Red  Alert!", &k));
        assert!(!matches_keyword("redeployment", &k));
        assert!(!matches_keyword("red then alert", &k));
        assert!(!matches_keyword("anything", &[]));
    }
}
