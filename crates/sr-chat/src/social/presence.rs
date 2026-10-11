//! Persisted presence preferences: chosen status, DND expiry and custom status.
//! Live presence (sessions, activities) is memory-only (docs/04 section 4); only
//! what the user picked survives a restart.

use super::{SocialError, SocialLimits, SocialResult, clean_text};
use sr_core::UserId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Status {
    Online,
    Idle,
    Dnd,
    Invisible,
    Offline,
}

impl Status {
    pub const fn code(self) -> i16 {
        match self {
            Self::Online => 0,
            Self::Idle => 1,
            Self::Dnd => 2,
            Self::Invisible => 3,
            Self::Offline => 4,
        }
    }

    pub const fn from_code(code: i16) -> Option<Self> {
        Some(match code {
            0 => Self::Online,
            1 => Self::Idle,
            2 => Self::Dnd,
            3 => Self::Invisible,
            4 => Self::Offline,
            _ => return None,
        })
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Idle => "idle",
            Self::Dnd => "dnd",
            Self::Invisible => "invisible",
            Self::Offline => "offline",
        }
    }

    /// What other users see: invisible looks offline.
    pub const fn visible(self) -> Self {
        match self {
            Self::Invisible => Self::Offline,
            s => s,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CustomStatus {
    pub text: Option<String>,
    pub emoji_name: Option<String>,
    pub emoji_id: Option<u64>,
    pub expires_ms: Option<i64>,
}

impl CustomStatus {
    pub fn is_empty(&self) -> bool {
        self.text.is_none() && self.emoji_name.is_none() && self.emoji_id.is_none()
    }

    pub fn is_expired(&self, now_ms: i64) -> bool {
        self.expires_ms.is_some_and(|e| e <= now_ms)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresencePrefs {
    pub user: UserId,
    pub status: Status,
    /// "Do Not Disturb for 1 hour": status reverts to online at this time.
    pub status_expires_ms: Option<i64>,
    pub custom: CustomStatus,
}

impl PresencePrefs {
    pub fn new(user: UserId) -> Self {
        Self {
            user,
            status: Status::Online,
            status_expires_ms: None,
            custom: CustomStatus::default(),
        }
    }

    /// Apply expiries; returns the prefs a reader should act on.
    pub fn resolved(mut self, now_ms: i64) -> Self {
        if self.status_expires_ms.is_some_and(|e| e <= now_ms) {
            self.status = Status::Online;
            self.status_expires_ms = None;
        }
        if self.custom.is_expired(now_ms) {
            self.custom = CustomStatus::default();
        }
        self
    }
}

/// Validate a status change. Only user-selectable statuses are accepted.
pub fn plan_set_status(
    mut prefs: PresencePrefs,
    status: Status,
    duration_ms: Option<i64>,
    now_ms: i64,
) -> SocialResult<PresencePrefs> {
    if status == Status::Offline {
        return Err(SocialError::invalid(
            "status",
            "ENUM_TYPE_BASE_TYPE_BAD_VALUE",
            "offline is not selectable; use invisible",
        ));
    }
    if duration_ms.is_some_and(|d| d <= 0) {
        return Err(SocialError::invalid(
            "duration",
            "NUMBER_TYPE_MIN",
            "must be positive",
        ));
    }
    prefs.status = status;
    prefs.status_expires_ms = duration_ms.map(|d| now_ms.saturating_add(d));
    Ok(prefs)
}

/// Validate and set (or clear, with an empty status) the custom status.
pub fn plan_set_custom_status(
    mut prefs: PresencePrefs,
    mut custom: CustomStatus,
    limits: &SocialLimits,
    now_ms: i64,
) -> SocialResult<PresencePrefs> {
    custom.text = clean_text(
        "text",
        custom.text.as_deref(),
        limits.max_custom_status_chars,
    )?;
    custom.emoji_name = clean_text("emoji_name", custom.emoji_name.as_deref(), 64)?;
    if custom.emoji_name.is_none() {
        custom.emoji_id = None;
    }
    if custom.is_empty() || custom.is_expired(now_ms) {
        custom = CustomStatus::default();
    }
    prefs.custom = custom;
    Ok(prefs)
}

/// Merge a user's per-session statuses into one visible status
/// (LNG-PRF-015): online beats do-not-disturb beats idle; invisible sessions
/// contribute nothing; no sessions means offline.
pub fn merge_sessions(statuses: &[Status]) -> Status {
    let mut best = Status::Offline;
    let rank = |s: Status| match s {
        Status::Online => 3,
        Status::Dnd => 2,
        Status::Idle => 1,
        Status::Invisible | Status::Offline => 0,
    };
    for s in statuses {
        if rank(*s) > rank(best) {
            best = *s;
        }
    }
    best
}
