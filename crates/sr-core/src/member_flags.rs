//! Discord's `GuildMemberFlags` (D-245). `guild_members.flags` and the `flags` field of the
//! member object use these bits. Only [`COMPLETED_ONBOARDING`] has a setter today; the other
//! three stay 0 until they do.

/// The member left and rejoined the server.
pub const DID_REJOIN: u32 = 1 << 0;
/// The member answered the onboarding questions (set by `POST /guilds/{id}/onboarding-responses`).
pub const COMPLETED_ONBOARDING: u32 = 1 << 1;
/// The member bypasses verification requirements.
pub const BYPASSES_VERIFICATION: u32 = 1 << 2;
/// The member started onboarding.
pub const STARTED_ONBOARDING: u32 = 1 << 3;

/// Every defined bit.
pub const ALL: u32 = DID_REJOIN | COMPLETED_ONBOARDING | BYPASSES_VERIFICATION | STARTED_ONBOARDING;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_match_discord() {
        assert_eq!(COMPLETED_ONBOARDING, 2);
        assert_eq!(ALL, 0b1111);
    }
}
