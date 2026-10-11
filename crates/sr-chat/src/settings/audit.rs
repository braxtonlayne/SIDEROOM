//! Audit log (LNG-SSET-056/057): append-only, Discord-compatible entries.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sr_core::{GuildId, UserId};

use super::error::{SettingsError, SettingsResult};

pub const MAX_REASON_CHARS: usize = 512;
pub const DEFAULT_PAGE: usize = 50;
pub const MAX_PAGE: usize = 100;
pub const DAY_MS: i64 = 86_400_000;

macro_rules! audit_actions {
    ($($name:ident = $code:expr,)*) => {
        /// Discord audit-log event numbers (a subset; extend as modules land).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum AuditAction { $($name,)* }

        impl AuditAction {
            pub const fn code(self) -> i16 { match self { $(Self::$name => $code,)* } }
            pub const fn from_code(code: i16) -> Option<Self> {
                match code { $($code => Some(Self::$name),)* _ => None }
            }
            pub const ALL: &'static [AuditAction] = &[$(Self::$name,)*];
        }
    };
}

audit_actions! {
    GuildUpdate = 1,
    ChannelCreate = 10,
    ChannelUpdate = 11,
    ChannelDelete = 12,
    ChannelOverwriteCreate = 13,
    ChannelOverwriteUpdate = 14,
    ChannelOverwriteDelete = 15,
    MemberKick = 20,
    MemberPrune = 21,
    MemberBanAdd = 22,
    MemberBanRemove = 23,
    MemberUpdate = 24,
    MemberRoleUpdate = 25,
    MemberMove = 26,
    MemberDisconnect = 27,
    BotAdd = 28,
    RoleCreate = 30,
    RoleUpdate = 31,
    RoleDelete = 32,
    InviteCreate = 40,
    InviteUpdate = 41,
    InviteDelete = 42,
    WebhookCreate = 50,
    WebhookUpdate = 51,
    WebhookDelete = 52,
    EmojiCreate = 60,
    EmojiUpdate = 61,
    EmojiDelete = 62,
    MessageDelete = 72,
    MessageBulkDelete = 73,
    MessagePin = 74,
    MessageUnpin = 75,
    IntegrationCreate = 80,
    IntegrationUpdate = 81,
    IntegrationDelete = 82,
    StickerCreate = 90,
    StickerUpdate = 91,
    StickerDelete = 92,
    ScheduledEventCreate = 100,
    ScheduledEventUpdate = 101,
    ScheduledEventDelete = 102,
    ThreadCreate = 110,
    ThreadUpdate = 111,
    ThreadDelete = 112,
    SoundboardSoundCreate = 130,
    SoundboardSoundUpdate = 131,
    SoundboardSoundDelete = 132,
    AutoModerationRuleCreate = 140,
    AutoModerationRuleUpdate = 141,
    AutoModerationRuleDelete = 142,
    AutoModerationBlockMessage = 143,
    AutoModerationFlagToChannel = 144,
    AutoModerationUserCommunicationDisabled = 145,
    OnboardingPromptCreate = 163,
    OnboardingPromptUpdate = 164,
    OnboardingPromptDelete = 165,
    OnboardingCreate = 166,
    OnboardingUpdate = 167,
}

/// One changed key, Discord shape: `{key, old_value, new_value}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditChange {
    pub key: String,
    pub old_value: Option<Value>,
    pub new_value: Option<Value>,
}

impl AuditChange {
    pub fn new(key: &str, old: Option<Value>, new: Option<Value>) -> Self {
        Self {
            key: key.to_owned(),
            old_value: old,
            new_value: new,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: u64,
    pub guild_id: GuildId,
    /// The user (or automod/system user) who acted.
    pub actor_id: UserId,
    /// Id of the affected object (user, role, channel, ...).
    pub target_id: Option<u64>,
    pub action: AuditAction,
    pub changes: Vec<AuditChange>,
    /// Extra per-action data (`channel_id`, `count`, `delete_member_days`, ...).
    pub options: Option<Value>,
    /// From the `X-Audit-Log-Reason` header.
    pub reason: Option<String>,
    pub created_at_ms: i64,
}

impl AuditEntry {
    /// Build an entry, validating the reason (Discord: at most 512 characters).
    pub fn new(
        id: u64,
        guild_id: GuildId,
        actor_id: UserId,
        action: AuditAction,
        target_id: Option<u64>,
        reason: Option<&str>,
        now_ms: i64,
    ) -> SettingsResult<Self> {
        Ok(Self {
            id,
            guild_id,
            actor_id,
            target_id,
            action,
            changes: Vec::new(),
            options: None,
            reason: normalize_reason(reason)?,
            created_at_ms: now_ms,
        })
    }

    pub fn with_changes(mut self, changes: Vec<AuditChange>) -> Self {
        self.changes = changes;
        self
    }

    pub fn with_options(mut self, options: Value) -> Self {
        self.options = Some(options);
        self
    }
}

/// Trim; empty becomes `None`; over-long is rejected.
pub fn normalize_reason(reason: Option<&str>) -> SettingsResult<Option<String>> {
    let Some(r) = reason.map(str::trim).filter(|r| !r.is_empty()) else {
        return Ok(None);
    };
    if r.chars().count() > MAX_REASON_CHARS {
        return Err(SettingsError::invalid(
            "reason",
            "BASE_TYPE_MAX_LENGTH",
            format!("must be {MAX_REASON_CHARS} or fewer in length"),
        ));
    }
    Ok(Some(r.to_owned()))
}

/// Filters for listing; results are newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditQuery {
    pub guild_id: GuildId,
    /// Entries by this actor.
    pub actor_id: Option<UserId>,
    pub action: Option<AuditAction>,
    /// Only entries with an id below this (pagination cursor).
    pub before: Option<u64>,
    pub limit: usize,
}

impl AuditQuery {
    pub fn new(guild_id: GuildId) -> Self {
        Self {
            guild_id,
            actor_id: None,
            action: None,
            before: None,
            limit: DEFAULT_PAGE,
        }
    }

    pub fn clamped_limit(&self) -> usize {
        self.limit.clamp(1, MAX_PAGE)
    }

    pub fn matches(&self, e: &AuditEntry) -> bool {
        e.guild_id == self.guild_id
            && self.actor_id.is_none_or(|a| a == e.actor_id)
            && self.action.is_none_or(|a| a == e.action)
            && self.before.is_none_or(|b| e.id < b)
    }
}

/// Retention policy (open question: docs/09 sets none; Discord keeps 45 days).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditRetention {
    pub keep_days: u32,
}

impl Default for AuditRetention {
    fn default() -> Self {
        Self { keep_days: 45 }
    }
}

impl AuditRetention {
    /// Entries created before this instant may be purged. `None` = keep forever.
    pub fn cutoff_ms(&self, now_ms: i64) -> Option<i64> {
        (self.keep_days > 0).then(|| now_ms - i64::from(self.keep_days) * DAY_MS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_codes_round_trip() {
        for a in AuditAction::ALL {
            assert_eq!(AuditAction::from_code(a.code()), Some(*a));
        }
        assert_eq!(AuditAction::MemberBanAdd.code(), 22);
        assert_eq!(AuditAction::AutoModerationRuleCreate.code(), 140);
        assert_eq!(AuditAction::from_code(9999), None);
    }

    #[test]
    fn reason_rules() {
        assert_eq!(
            normalize_reason(Some("  spam  ")).unwrap().as_deref(),
            Some("spam")
        );
        assert_eq!(normalize_reason(Some("   ")).unwrap(), None);
        assert_eq!(normalize_reason(None).unwrap(), None);
        assert!(normalize_reason(Some(&"x".repeat(513))).is_err());
        assert!(normalize_reason(Some(&"x".repeat(512))).is_ok());
    }

    #[test]
    fn query_filters() {
        let g = GuildId::new(1);
        let e = AuditEntry::new(
            50,
            g,
            UserId::new(7),
            AuditAction::RoleCreate,
            Some(9),
            None,
            0,
        )
        .unwrap();
        let mut q = AuditQuery::new(g);
        assert!(q.matches(&e));
        q.actor_id = Some(UserId::new(8));
        assert!(!q.matches(&e));
        q.actor_id = Some(UserId::new(7));
        q.action = Some(AuditAction::RoleDelete);
        assert!(!q.matches(&e));
        q.action = Some(AuditAction::RoleCreate);
        q.before = Some(50);
        assert!(!q.matches(&e));
        q.before = Some(51);
        assert!(q.matches(&e));
        assert!(!AuditQuery::new(GuildId::new(2)).matches(&e));
    }

    #[test]
    fn retention_cutoff() {
        let r = AuditRetention { keep_days: 45 };
        assert_eq!(r.cutoff_ms(100 * DAY_MS), Some(55 * DAY_MS));
        assert_eq!(AuditRetention { keep_days: 0 }.cutoff_ms(5), None);
    }
}
