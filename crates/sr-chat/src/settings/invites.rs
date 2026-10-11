//! Guild invites (LNG-SSET-038, D-022): create, use, expire, max uses, revoke, pause.
//! Platform invites (account creation) live in `sr-auth`; these only join a guild.

use super::error::{SettingsError, SettingsResult};
use serde::{Deserialize, Serialize};
use sr_core::{ChannelId, GuildId, UserId};

/// Discord allows at most 7 days and 100 uses.
pub const MAX_AGE_SECS: u32 = 604_800;
pub const MAX_USES: u32 = 100;
/// Discord cap on live invites per guild.
pub const MAX_INVITES_PER_GUILD: usize = 1000;
pub const CODE_LEN: usize = 8;
const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuildInvite {
    pub code: String,
    pub guild_id: GuildId,
    pub channel_id: ChannelId,
    pub inviter_id: UserId,
    /// 0 = never expires.
    pub max_age_secs: u32,
    /// 0 = unlimited.
    pub max_uses: u32,
    pub uses: u32,
    /// Temporary membership: kicked on disconnect unless a role is assigned.
    pub temporary: bool,
    pub created_at_ms: i64,
    pub revoked_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteStatus {
    Active,
    Expired,
    Exhausted,
    Revoked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InviteOptions {
    pub max_age_secs: u32,
    pub max_uses: u32,
    pub temporary: bool,
}

impl Default for InviteOptions {
    /// Discord defaults: 24 hours, unlimited uses.
    fn default() -> Self {
        Self {
            max_age_secs: 86_400,
            max_uses: 0,
            temporary: false,
        }
    }
}

impl InviteOptions {
    /// D-022: an invite meant to leave the platform defaults to single use.
    pub fn external_default() -> Self {
        Self {
            max_uses: 1,
            ..Self::default()
        }
    }

    /// Options for `POST /channels/{id}/invites`. `external: true` (the dialog's
    /// "Copy link") uses [`Self::external_default`]; in-platform shares use the
    /// server default (open question 134).
    pub fn for_request(external: bool) -> Self {
        if external {
            Self::external_default()
        } else {
            Self::default()
        }
    }

    pub fn validate(&self) -> SettingsResult<()> {
        if self.max_age_secs > MAX_AGE_SECS {
            return Err(SettingsError::invalid(
                "max_age",
                "NUMBER_TYPE_MAX",
                format!("must be at most {MAX_AGE_SECS}"),
            ));
        }
        if self.max_uses > MAX_USES {
            return Err(SettingsError::invalid(
                "max_uses",
                "NUMBER_TYPE_MAX",
                format!("must be at most {MAX_USES}"),
            ));
        }
        Ok(())
    }
}

/// Random 8-character alphanumeric code from a source of 64-bit entropy.
pub fn generate_code(mut entropy: impl FnMut() -> u64) -> String {
    let mut out = String::with_capacity(CODE_LEN);
    let mut bits = 0u64;
    while out.len() < CODE_LEN {
        if bits == 0 {
            bits = entropy();
            if bits == 0 {
                // Degenerate source: still make progress deterministically.
                bits = out.len() as u64 + 1;
            }
        }
        out.push(ALPHABET[(bits % 62) as usize] as char);
        bits /= 62;
    }
    out
}

/// Whether `code` has the shape of a guild invite code (vanity codes are longer).
pub fn is_valid_code(code: &str) -> bool {
    (2..=32).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

impl GuildInvite {
    pub fn new(
        code: String,
        guild_id: GuildId,
        channel_id: ChannelId,
        inviter_id: UserId,
        opts: InviteOptions,
        now_ms: i64,
    ) -> SettingsResult<Self> {
        opts.validate()?;
        Ok(Self {
            code,
            guild_id,
            channel_id,
            inviter_id,
            max_age_secs: opts.max_age_secs,
            max_uses: opts.max_uses,
            uses: 0,
            temporary: opts.temporary,
            created_at_ms: now_ms,
            revoked_at_ms: None,
        })
    }

    pub fn expires_at_ms(&self) -> Option<i64> {
        (self.max_age_secs > 0).then(|| self.created_at_ms + i64::from(self.max_age_secs) * 1000)
    }

    pub fn status(&self, now_ms: i64) -> InviteStatus {
        if self.revoked_at_ms.is_some() {
            InviteStatus::Revoked
        } else if self.expires_at_ms().is_some_and(|e| now_ms >= e) {
            InviteStatus::Expired
        } else if self.max_uses > 0 && self.uses >= self.max_uses {
            InviteStatus::Exhausted
        } else {
            InviteStatus::Active
        }
    }

    /// Check that the invite can be used right now; revoked reads as unknown.
    pub fn check_usable(&self, now_ms: i64) -> SettingsResult<()> {
        match self.status(now_ms) {
            InviteStatus::Active => Ok(()),
            InviteStatus::Expired => Err(SettingsError::InviteExpired),
            InviteStatus::Exhausted => Err(SettingsError::InviteExhausted),
            InviteStatus::Revoked => Err(SettingsError::UnknownInvite),
        }
    }

    /// Record one use in memory (the repository does the same atomically).
    pub fn consume(&mut self, now_ms: i64) -> SettingsResult<()> {
        self.check_usable(now_ms)?;
        self.uses += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invite(opts: InviteOptions) -> GuildInvite {
        GuildInvite::new(
            "abc12345".into(),
            GuildId::new(1),
            ChannelId::new(2),
            UserId::new(3),
            opts,
            1_000,
        )
        .unwrap()
    }

    #[test]
    fn validates_options() {
        let bad_age = InviteOptions {
            max_age_secs: MAX_AGE_SECS + 1,
            ..Default::default()
        };
        assert!(bad_age.validate().is_err());
        let bad_uses = InviteOptions {
            max_uses: 101,
            ..Default::default()
        };
        assert!(bad_uses.validate().is_err());
        assert_eq!(InviteOptions::external_default().max_uses, 1);
    }

    #[test]
    fn expiry_and_max_uses() {
        let mut inv = invite(InviteOptions {
            max_age_secs: 10,
            max_uses: 2,
            temporary: false,
        });
        assert_eq!(inv.expires_at_ms(), Some(11_000));
        assert!(inv.consume(2_000).is_ok());
        assert!(inv.consume(3_000).is_ok());
        assert_eq!(inv.status(3_000), InviteStatus::Exhausted);
        assert!(matches!(
            inv.consume(3_000),
            Err(SettingsError::InviteExhausted)
        ));
        let fresh = invite(InviteOptions {
            max_age_secs: 10,
            ..Default::default()
        });
        assert_eq!(fresh.status(10_999), InviteStatus::Active);
        assert_eq!(fresh.status(11_000), InviteStatus::Expired);
    }

    #[test]
    fn never_expires_and_revoked() {
        let mut inv = invite(InviteOptions {
            max_age_secs: 0,
            max_uses: 0,
            temporary: true,
        });
        assert_eq!(inv.expires_at_ms(), None);
        assert_eq!(inv.status(i64::MAX / 2), InviteStatus::Active);
        inv.revoked_at_ms = Some(5);
        assert!(matches!(
            inv.check_usable(6),
            Err(SettingsError::UnknownInvite)
        ));
    }

    #[test]
    fn code_generation() {
        let mut n = 0x1234_5678_9abc_def0_u64;
        let code = generate_code(|| {
            n = n
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            n
        });
        assert_eq!(code.len(), CODE_LEN);
        assert!(is_valid_code(&code));
        // Degenerate entropy still terminates.
        assert_eq!(generate_code(|| 0).len(), CODE_LEN);
    }
}
