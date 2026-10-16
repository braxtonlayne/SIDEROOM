//! Content-and-social safety preferences of one user (LNG-USET-012/013): how to show media
//! in DMs from friends, others and servers, the DM spam filter, and per-server DM overrides.
//! They live next to the privacy settings (`user_privacy`) and travel in the same object.

use std::collections::BTreeMap;

/// What to do with sensitive media from one source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaFilter {
    /// Show it as is.
    Show,
    /// Blur it until the viewer clicks.
    Blur,
    /// Hide it.
    Block,
}

impl MediaFilter {
    pub const fn code(self) -> i16 {
        match self {
            Self::Show => 0,
            Self::Blur => 1,
            Self::Block => 2,
        }
    }

    pub const fn from_code(code: i16) -> Option<Self> {
        Some(match code {
            0 => Self::Show,
            1 => Self::Blur,
            2 => Self::Block,
            _ => return None,
        })
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Show => "show",
            Self::Blur => "blur",
            Self::Block => "block",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "show" => Self::Show,
            "blur" => Self::Blur,
            "block" => Self::Block,
            _ => return None,
        })
    }
}

/// How strictly first DMs are routed to Message Requests (D-230).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpamFilter {
    /// Only the `message_requests` toggle decides.
    Off,
    /// First DMs from non-friends go to requests (default).
    #[default]
    NonFriends,
    /// Every first DM, friends included, goes to requests.
    All,
}

impl SpamFilter {
    pub const fn code(self) -> i16 {
        match self {
            Self::Off => 0,
            Self::NonFriends => 1,
            Self::All => 2,
        }
    }

    pub const fn from_code(code: i16) -> Option<Self> {
        Some(match code {
            0 => Self::Off,
            1 => Self::NonFriends,
            2 => Self::All,
            _ => return None,
        })
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::NonFriends => "non_friends",
            Self::All => "all",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "off" => Self::Off,
            "non_friends" => Self::NonFriends,
            "all" => Self::All,
            _ => return None,
        })
    }
}

/// Sensitive-media handling per source (Discord: friends, non-friends, servers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensitiveMedia {
    pub friends: MediaFilter,
    pub others: MediaFilter,
    pub servers: MediaFilter,
}

impl Default for SensitiveMedia {
    /// Friends are trusted; strangers' media is blurred; servers follow their own filter.
    fn default() -> Self {
        Self {
            friends: MediaFilter::Show,
            others: MediaFilter::Blur,
            servers: MediaFilter::Show,
        }
    }
}

/// Most per-server DM overrides one user may keep (the cap on guild memberships is lower).
pub const MAX_SERVER_DM_OVERRIDES: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SafetyPrefs {
    pub sensitive_media: SensitiveMedia,
    pub dm_spam_filter: SpamFilter,
    /// Per-server override of "allow DMs from server members" (guild id to allowed).
    pub server_dm_overrides: BTreeMap<u64, bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_round_trips() {
        for f in [MediaFilter::Show, MediaFilter::Blur, MediaFilter::Block] {
            assert_eq!(MediaFilter::from_code(f.code()), Some(f));
            assert_eq!(MediaFilter::parse(f.as_str()), Some(f));
        }
        assert_eq!(MediaFilter::from_code(3), None);
        assert_eq!(MediaFilter::parse("hide"), None);
    }
}
