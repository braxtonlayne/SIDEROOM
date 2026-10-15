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
    /// Flag likely spam from non-friends into Message Requests' spam tab. Stored and shown;
    /// no detector reads it yet.
    pub dm_spam_filter: bool,
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
