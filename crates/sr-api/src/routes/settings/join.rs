//! Adding a member when an invite is accepted.
//!
//! The chat repository traits have no "insert member" operation yet (the guild
//! backend only creates the owner member), so the accept endpoint goes through
//! [`MemberJoiner`]. The composition root provides the real one once the guild
//! join path exists; until then [`UnwiredMemberJoiner`] answers 503 before any
//! invite use is consumed.

use async_trait::async_trait;
use sr_core::{ApiError, ErrorCode, GuildId, UserId};

#[async_trait]
pub trait MemberJoiner: Send + Sync {
    /// Whether joins can be performed at all. Checked before the invite is used.
    fn is_wired(&self) -> bool;

    /// Add `user` to `guild`, emit the member-join event and apply onboarding
    /// defaults. `temporary` is the invite's flag (kick on disconnect, Phase 2).
    async fn join(&self, guild: GuildId, user: UserId, temporary: bool) -> Result<(), ApiError>;
}

/// TODO(join): replace with an adapter over the guild-membership write path.
pub struct UnwiredMemberJoiner;

#[async_trait]
impl MemberJoiner for UnwiredMemberJoiner {
    fn is_wired(&self) -> bool {
        false
    }

    async fn join(&self, _: GuildId, _: UserId, _: bool) -> Result<(), ApiError> {
        Err(ApiError::with_message(
            ErrorCode::FeatureDisabled,
            "Joining by invite is not available yet",
        ))
    }
}
