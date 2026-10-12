//! Per-user, per-channel read state and mention counts (LNG-MSG-012).

use sr_core::{ChannelId, MessageId, UserId};

/// Mention badges stop counting here (the UI shows "99+" long before this).
pub const MAX_MENTION_COUNT: u32 = 9_999;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadState {
    pub user: UserId,
    pub channel: ChannelId,
    pub last_read: Option<MessageId>,
    pub mention_count: u32,
    pub last_pin_ms: Option<i64>,
}

impl ReadState {
    pub fn new(user: UserId, channel: ChannelId) -> Self {
        Self {
            user,
            channel,
            last_read: None,
            mention_count: 0,
            last_pin_ms: None,
        }
    }

    /// Whether the channel holds a message newer than the read marker.
    pub fn has_unread(&self, channel_last_message: Option<MessageId>) -> bool {
        match (channel_last_message, self.last_read) {
            (None, _) => false,
            (Some(_), None) => true,
            (Some(last), Some(read)) => last > read,
        }
    }
}

/// Acknowledge up to `up_to`. The marker only moves forward; mentions reset.
pub fn ack(
    prev: Option<ReadState>,
    user: UserId,
    channel: ChannelId,
    up_to: MessageId,
) -> ReadState {
    let mut s = prev.unwrap_or_else(|| ReadState::new(user, channel));
    if s.last_read.is_none_or(|r| up_to > r) {
        s.last_read = Some(up_to);
    }
    s.mention_count = 0;
    s
}

/// "Mark as unread": move the marker back to the message before the chosen one
/// (`None` = the channel start) and restore the mention count the client sends.
pub fn mark_unread(
    prev: Option<ReadState>,
    user: UserId,
    channel: ChannelId,
    previous_message: Option<MessageId>,
    mention_count: u32,
) -> ReadState {
    let mut s = prev.unwrap_or_else(|| ReadState::new(user, channel));
    s.last_read = previous_message;
    s.mention_count = mention_count.min(MAX_MENTION_COUNT);
    s
}

/// Who receives a mention badge for a new message (open question 117): the
/// author is excluded, duplicates collapse, and only users who can view the
/// channel count. `can_view` is the caller's VIEW_CHANNEL check (computed in the
/// send path, per user and for every `@everyone`/role fan-out member).
pub fn mention_recipients(
    author: UserId,
    mentioned: &[UserId],
    can_view: impl Fn(UserId) -> bool,
) -> Vec<UserId> {
    let mut users: Vec<UserId> = mentioned
        .iter()
        .copied()
        .filter(|u| *u != author && can_view(*u))
        .collect();
    users.sort();
    users.dedup();
    users
}

/// Add `n` mentions (saturating at [`MAX_MENTION_COUNT`]).
pub fn add_mentions(
    prev: Option<ReadState>,
    user: UserId,
    channel: ChannelId,
    n: u32,
) -> ReadState {
    let mut s = prev.unwrap_or_else(|| ReadState::new(user, channel));
    s.mention_count = s.mention_count.saturating_add(n).min(MAX_MENTION_COUNT);
    s
}

/// Record that the user has seen the channel's pins up to `now_ms`.
pub fn ack_pins(
    prev: Option<ReadState>,
    user: UserId,
    channel: ChannelId,
    now_ms: i64,
) -> ReadState {
    let mut s = prev.unwrap_or_else(|| ReadState::new(user, channel));
    s.last_pin_ms = Some(now_ms);
    s
}
