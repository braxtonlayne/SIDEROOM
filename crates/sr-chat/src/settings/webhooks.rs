//! Webhooks model (LNG-SSET-044/045). Incoming webhooks post into a channel using
//! `id` + secret token; only a hash of the token is stored.

use serde::{Deserialize, Serialize};
use sr_core::{ApplicationId, ChannelId, GuildId, UserId, WebhookId};

use super::error::{SettingsError, SettingsResult};

pub const MAX_WEBHOOKS_PER_CHANNEL: usize = 15;
pub const TOKEN_LEN: usize = 68;
const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz-_";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebhookKind {
    /// Created by a user, executed with a token.
    Incoming,
    /// Announcement-channel follower (cross-posts).
    ChannelFollower,
    /// Owned by an application (interaction responses).
    Application,
}

impl WebhookKind {
    pub const fn code(self) -> i16 {
        match self {
            Self::Incoming => 1,
            Self::ChannelFollower => 2,
            Self::Application => 3,
        }
    }

    pub const fn from_code(code: i16) -> Option<Self> {
        Some(match code {
            1 => Self::Incoming,
            2 => Self::ChannelFollower,
            3 => Self::Application,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Webhook {
    pub id: WebhookId,
    pub guild_id: GuildId,
    pub channel_id: ChannelId,
    pub kind: WebhookKind,
    pub creator_id: Option<UserId>,
    pub name: String,
    pub avatar_key: Option<String>,
    /// Hash of the execution token; `None` for follower webhooks.
    pub token_hash: Option<String>,
    pub application_id: Option<ApplicationId>,
    /// Followed channel, for [`WebhookKind::ChannelFollower`].
    pub source_channel_id: Option<ChannelId>,
    pub created_at_ms: i64,
}

/// Discord: 1-80 characters; the names `clyde` is reserved there. Here: no
/// `@everyone`/`@here` look-alikes in the display name.
pub fn validate_name(name: &str) -> SettingsResult<String> {
    let n = name.trim();
    if !(1..=80).contains(&n.chars().count()) {
        return Err(SettingsError::invalid(
            "name",
            "BASE_TYPE_BAD_LENGTH",
            "must be between 1 and 80 in length",
        ));
    }
    let lower = n.to_lowercase();
    if lower.contains("@everyone") || lower.contains("@here") {
        return Err(SettingsError::invalid(
            "name",
            "USERNAME_INVALID_CONTAINS",
            "name cannot contain mass mentions",
        ));
    }
    Ok(n.to_owned())
}

/// Random execution token from 64-bit entropy words.
pub fn generate_token(mut entropy: impl FnMut() -> u64) -> String {
    let mut out = String::with_capacity(TOKEN_LEN);
    let mut bits = 0u64;
    let mut left = 0;
    while out.len() < TOKEN_LEN {
        if left == 0 {
            bits = entropy();
            left = 10;
        }
        out.push(ALPHABET[(bits & 63) as usize] as char);
        bits >>= 6;
        left -= 1;
    }
    out
}

impl Webhook {
    /// Reject a new webhook when the channel is already at its cap.
    pub fn check_channel_quota(existing_in_channel: usize) -> SettingsResult<()> {
        if existing_in_channel >= MAX_WEBHOOKS_PER_CHANNEL {
            return Err(SettingsError::LimitReached("webhooks in this channel"));
        }
        Ok(())
    }

    /// New incoming webhook (check [`Self::check_channel_quota`] first).
    pub fn new_incoming(
        id: WebhookId,
        guild_id: GuildId,
        channel_id: ChannelId,
        creator_id: UserId,
        name: &str,
        token_hash: String,
        now_ms: i64,
    ) -> SettingsResult<Self> {
        Ok(Self {
            id,
            guild_id,
            channel_id,
            kind: WebhookKind::Incoming,
            creator_id: Some(creator_id),
            name: validate_name(name)?,
            avatar_key: None,
            token_hash: Some(token_hash),
            application_id: None,
            source_channel_id: None,
            created_at_ms: now_ms,
        })
    }

    /// Constant-time comparison of a presented token hash.
    pub fn token_matches(&self, presented_hash: &str) -> bool {
        let Some(stored) = &self.token_hash else {
            return false;
        };
        if stored.len() != presented_hash.len() {
            return false;
        }
        stored
            .bytes()
            .zip(presented_hash.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
    }

    /// Moving a webhook between channels is allowed only within its guild.
    pub fn move_to(&mut self, channel: ChannelId) {
        self.channel_id = channel;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wh(existing: usize) -> SettingsResult<Webhook> {
        Webhook::check_channel_quota(existing)?;
        Webhook::new_incoming(
            WebhookId::new(1),
            GuildId::new(2),
            ChannelId::new(3),
            UserId::new(4),
            "  Deploy bot ",
            "hash".into(),
            100,
        )
    }

    #[test]
    fn creates_and_trims() {
        let w = wh(0).unwrap();
        assert_eq!(w.name, "Deploy bot");
        assert_eq!(w.kind, WebhookKind::Incoming);
        assert!(w.token_matches("hash"));
        assert!(!w.token_matches("hasH"));
        assert!(!w.token_matches("hash2"));
    }

    #[test]
    fn limits_and_names() {
        assert!(wh(MAX_WEBHOOKS_PER_CHANNEL).is_err());
        assert!(validate_name("").is_err());
        assert!(validate_name(&"x".repeat(81)).is_err());
        assert!(validate_name("hi @everyone").is_err());
    }

    #[test]
    fn tokens() {
        let mut n = 7u64;
        let t = generate_token(|| {
            n = n.wrapping_mul(6364136223846793005).wrapping_add(1);
            n
        });
        assert_eq!(t.len(), TOKEN_LEN);
        assert!(t.bytes().all(|b| ALPHABET.contains(&b)));
        assert_eq!(
            WebhookKind::from_code(WebhookKind::Application.code()),
            Some(WebhookKind::Application)
        );
    }
}
