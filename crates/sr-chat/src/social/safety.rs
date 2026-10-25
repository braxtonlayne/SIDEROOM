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

/// Distinct link hosts in one stranger's message that make it look like spam (the automod
/// `link flood` rule uses five; a first message from someone you do not know gets fewer).
pub const DM_SPAM_LINK_HOSTS: usize = 3;

/// RDY-PAR-004: why a first message from a stranger looks like spam, by the same heuristics
/// the server automod uses (character flood, repeated word, link flood, see
/// `settings::automod::spam_reason`) plus a lower link threshold for strangers.
pub fn dm_spam_reason(content: &str) -> Option<&'static str> {
    use crate::settings::automod::{extract_hosts, spam_reason};
    if let Some(r) = spam_reason(content) {
        return Some(r);
    }
    let mut hosts = extract_hosts(content);
    hosts.sort();
    hosts.dedup();
    (hosts.len() >= DM_SPAM_LINK_HOSTS).then_some("several links")
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
    fn dm_spam_heuristics() {
        assert_eq!(dm_spam_reason("hi, how are you?"), None);
        assert_eq!(dm_spam_reason("see https://a.example.com/x"), None);
        assert_eq!(dm_spam_reason(&"a".repeat(30)), Some("character flood"));
        assert_eq!(dm_spam_reason(&"free ".repeat(9)), Some("repeated word"));
        assert_eq!(
            dm_spam_reason("https://a.example.com https://b.example.org https://c.example.net"),
            Some("several links")
        );
        // The same host three times is one link target, not a flood.
        assert_eq!(
            dm_spam_reason(
                "https://a.example.com/1 https://a.example.com/2 https://a.example.com/3"
            ),
            None
        );
    }

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
