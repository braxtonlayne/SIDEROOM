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

/// The two server limits for the in-memory repositories (D-659, D-660): the same rules as
/// `sr-db::guild_caps`, with limits a test can set. `None` means the default from `sr-core`.
/// (The in-memory repositories know no bot flag, so every account counts.)
#[derive(Debug, Clone, Copy, Default)]
pub struct MemoryCaps {
    pub per_user: Option<i64>,
    pub members: Option<i64>,
}

impl MemoryCaps {
    pub fn per_user_limit(&self) -> i64 {
        self.per_user
            .unwrap_or(sr_core::guild_caps::DEFAULT_GUILDS_PER_USER)
    }

    pub fn members_limit(&self) -> i64 {
        self.members
            .unwrap_or(sr_core::guild_caps::DEFAULT_GUILD_MEMBERS)
    }

    /// May `user` be in one more server? Counts memberships in servers that exist and are not the Staff Server.
    pub fn check_user(
        &self,
        members: &[crate::domain::Member],
        guilds: &std::collections::HashMap<sr_core::GuildId, crate::domain::Guild>,
        user: sr_core::UserId,
    ) -> Result<(), sr_core::guild_caps::CapHit> {
        let count = members
            .iter()
            .filter(|m| m.user_id == user)
            .filter(|m| guilds.get(&m.guild_id).is_some_and(|g| !g.is_staff_guild))
            .count();
        let limit = self.per_user_limit();
        if i64::try_from(count).unwrap_or(i64::MAX) >= limit {
            return Err(sr_core::guild_caps::CapHit::UserServers(limit));
        }
        Ok(())
    }

    /// May `guild` take one more member? The Staff Server always may.
    pub fn check_guild(
        &self,
        members: &[crate::domain::Member],
        guilds: &std::collections::HashMap<sr_core::GuildId, crate::domain::Guild>,
        guild: sr_core::GuildId,
    ) -> Result<(), sr_core::guild_caps::CapHit> {
        if guilds.get(&guild).is_some_and(|g| g.is_staff_guild) {
            return Ok(());
        }
        let count = members.iter().filter(|m| m.guild_id == guild).count();
        let limit = self.members_limit();
        if i64::try_from(count).unwrap_or(i64::MAX) >= limit {
            return Err(sr_core::guild_caps::CapHit::GuildMembers(limit));
        }
        Ok(())
    }
}
