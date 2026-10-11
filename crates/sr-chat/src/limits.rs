//! Operator-configurable limits (docs/specs/lounge-chat.md section 14).

/// Discord-parity defaults; the operator may override via platform settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatLimits {
    pub max_message_chars: usize,
    pub max_mentions: usize,
    pub max_attachments: usize,
    pub max_embeds: usize,
    pub max_channels_per_guild: usize,
    pub max_roles_per_guild: usize,
    pub max_pins_per_channel: usize,
    pub max_distinct_reactions: usize,
    pub bulk_delete_min: usize,
    pub bulk_delete_max: usize,
    /// Bulk delete refuses messages older than this (Discord: 14 days).
    pub bulk_delete_max_age_ms: i64,
    /// D-121: user-deleted messages are purged after this long.
    pub purge_after_ms: i64,
}

pub const DAY_MS: i64 = 86_400_000;

impl Default for ChatLimits {
    fn default() -> Self {
        Self {
            max_message_chars: sr_markdown::DEFAULT_MAX_CHARS,
            max_mentions: sr_markdown::DEFAULT_MAX_MENTIONS,
            max_attachments: 10,
            max_embeds: 10,
            max_channels_per_guild: 500,
            max_roles_per_guild: 250,
            max_pins_per_channel: 250,
            max_distinct_reactions: 20,
            bulk_delete_min: 2,
            bulk_delete_max: 100,
            bulk_delete_max_age_ms: 14 * DAY_MS,
            purge_after_ms: 30 * DAY_MS,
        }
    }
}

impl ChatLimits {
    pub fn markdown(&self) -> sr_markdown::Limits {
        sr_markdown::Limits {
            max_chars: self.max_message_chars,
            max_mentions: self.max_mentions,
        }
    }
}
