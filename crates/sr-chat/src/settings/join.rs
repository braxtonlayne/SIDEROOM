//! Joining a guild by invite: validate, add the member and consume exactly one
//! use as a single atomic step (open question 134).
//!
//! The `@everyone` role is implicit (a member's `role_ids` lists only explicit
//! assignments), so a join writes one `guild_members` row and nothing in
//! `member_roles`. Discord writes no audit entry for a normal join, and neither do
//! we; `INVITE_CREATE`/`INVITE_DELETE` are the only invite audit actions.

use async_trait::async_trait;
use sr_core::{GuildId, UserId};

use crate::domain::Member;

use super::error::SettingsResult;
use super::invites::GuildInvite;

/// Result of a successful accept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InviteJoin {
    /// The user was added and one use was consumed; `invite` carries the new count.
    Joined { invite: GuildInvite, member: Member },
    /// The user already was a member; nothing changed and no use was consumed.
    AlreadyMember { invite: GuildInvite },
}

impl InviteJoin {
    pub fn invite(&self) -> &GuildInvite {
        match self {
            Self::Joined { invite, .. } | Self::AlreadyMember { invite } => invite,
        }
    }
}

#[async_trait]
pub trait InviteJoinRepository: Send + Sync {
    /// Accept invite `code` for `user` at `now_ms`.
    ///
    /// Checks, in order: the invite exists and is not revoked (`UnknownInvite`),
    /// not expired (`InviteExpired`), not exhausted (`InviteExhausted`), the guild
    /// has not paused invites (`InvitesPaused`) and the user is not banned
    /// (`Banned`). A user who already is a member gets
    /// [`InviteJoin::AlreadyMember`] without consuming a use. Otherwise the
    /// membership row is added and exactly one use consumed atomically: when N
    /// callers race for the last use, exactly one is admitted and the others see
    /// `InviteExhausted` with no membership left behind.
    async fn join_via_invite(
        &self,
        code: &str,
        user: UserId,
        now_ms: i64,
    ) -> SettingsResult<InviteJoin>;

    /// Join a listed Community straight from the Server Directory, without an invite: the
    /// same pause, ban and screening rules as an invite, no use counter. `None` when the user
    /// already is a member. The caller checked that the listing is approved.
    async fn join_listed(
        &self,
        guild: GuildId,
        user: UserId,
        now_ms: i64,
    ) -> SettingsResult<Option<crate::domain::Member>>;

    /// Drop the temporary memberships of `user` that hold no role (LNG-INV-005), as when
    /// they disconnect: an invite marked `temporary` only keeps a member while they are
    /// online or have been given a role. Returns the guilds they were removed from.
    async fn take_temporary_memberships(&self, user: UserId) -> SettingsResult<Vec<GuildId>>;
}
