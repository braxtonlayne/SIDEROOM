//! Adding a member when an invite is accepted.
//!
//! The accept endpoint goes through [`MemberJoiner`]. The composition root
//! registers [`InviteMemberJoiner`], which validates the invite, adds the member
//! and consumes one use in a single repository transaction and then announces the
//! join (`GUILD_CREATE` to the joiner's sessions, `GUILD_MEMBER_ADD` to the
//! guild). [`UnwiredMemberJoiner`] remains for tests of the not-available path.

use crate::routes::guilds::dto::MemberObject;
use async_trait::async_trait;
use sr_chat::repo::{ChatRepository, UserSummary};
use sr_chat::settings::join::{InviteJoin, InviteJoinRepository};
use sr_core::{ApiError, ErrorCode, UserId};
use sr_events::{DomainEvent, EventBus};
use std::collections::HashMap;
use std::sync::Arc;

#[async_trait]
pub trait MemberJoiner: Send + Sync {
    /// Accept invite `code` for `user` at `now_ms`: validate it, add the member
    /// and consume exactly one use atomically, and emit the member-join event. A
    /// user who already is a member gets [`InviteJoin::AlreadyMember`] and no use
    /// is consumed.
    async fn join_with_invite(
        &self,
        code: &str,
        user: UserId,
        now_ms: i64,
    ) -> Result<InviteJoin, ApiError>;

    /// Join a listed Community from the Server Directory without an invite (LNG-GLD-013);
    /// emits the member-join event. `None` when the user already is a member.
    async fn join_listed(
        &self,
        guild: sr_core::GuildId,
        user: UserId,
        now_ms: i64,
    ) -> Result<Option<sr_chat::Member>, ApiError>;
}

/// Answers every join with 40006 "not available".
pub struct UnwiredMemberJoiner;

#[async_trait]
impl MemberJoiner for UnwiredMemberJoiner {
    async fn join_with_invite(&self, _: &str, _: UserId, _: i64) -> Result<InviteJoin, ApiError> {
        Err(ApiError::with_message(
            ErrorCode::FeatureDisabled,
            "Joining by invite is not available yet",
        ))
    }

    async fn join_listed(
        &self,
        _: sr_core::GuildId,
        _: UserId,
        _: i64,
    ) -> Result<Option<sr_chat::Member>, ApiError> {
        Err(ApiError::with_message(
            ErrorCode::FeatureDisabled,
            "Joining from the directory is not available yet",
        ))
    }
}

/// The real joiner over [`InviteJoinRepository`]. The `@everyone` role is
/// implicit and, like Discord, a normal join writes no audit entry.
pub struct InviteMemberJoiner {
    repo: Arc<dyn InviteJoinRepository>,
    chat: Arc<dyn ChatRepository>,
    bus: Arc<dyn EventBus>,
}

impl InviteMemberJoiner {
    pub fn new(
        repo: Arc<dyn InviteJoinRepository>,
        chat: Arc<dyn ChatRepository>,
        bus: Arc<dyn EventBus>,
    ) -> Self {
        Self { repo, chat, bus }
    }

    async fn publish(&self, ev: DomainEvent) {
        for topic in ev.topics() {
            if let Err(e) = self.bus.publish(topic, ev.clone()).await {
                tracing::warn!(error = %e, "failed to publish a membership event");
            }
        }
    }

    /// `GUILD_MEMBER_ADD` for a member whose row is committed. The payload is best effort
    /// (bare ids if the user lookup fails) and a bus failure never fails the request.
    async fn announce_join(&self, member: &sr_chat::Member) {
        let users: HashMap<UserId, UserSummary> = match self.chat.users(&[member.user_id]).await {
            Ok(v) => v.into_iter().map(|u| (u.id, u)).collect(),
            Err(e) => {
                tracing::warn!(error = %e, "user lookup for MEMBER_JOINED failed");
                HashMap::new()
            }
        };
        let payload =
            serde_json::to_value(MemberObject::new(member, true, &users)).unwrap_or_default();
        self.publish(DomainEvent::MemberJoined {
            guild: member.guild_id,
            user: member.user_id,
            payload,
        })
        .await;
    }

    /// Remove the temporary memberships of `user` that hold no role (LNG-INV-005) and tell
    /// each server (`GUILD_MEMBER_REMOVE`). Called when the user's last session ends.
    /// Returns the servers they were removed from.
    pub async fn drop_temporary(&self, user: UserId) -> Vec<sr_core::GuildId> {
        let gone = match self.repo.take_temporary_memberships(user).await {
            Ok(g) => g,
            Err(e) => {
                tracing::error!(error = %e, "dropping temporary memberships failed");
                return Vec::new();
            }
        };
        for guild in &gone {
            self.publish(DomainEvent::MemberLeft {
                guild: *guild,
                user,
            })
            .await;
        }
        gone
    }
}

#[async_trait]
impl MemberJoiner for InviteMemberJoiner {
    async fn join_with_invite(
        &self,
        code: &str,
        user: UserId,
        now_ms: i64,
    ) -> Result<InviteJoin, ApiError> {
        let outcome = self
            .repo
            .join_via_invite(code, user, now_ms)
            .await
            .map_err(|e| {
                if matches!(e, sr_chat::settings::SettingsError::Repo(_)) {
                    tracing::error!(error = %e, "invite join failed");
                }
                e.to_api_error()
            })?;
        if let InviteJoin::Joined { member, .. } = &outcome {
            self.announce_join(member).await;
        }
        Ok(outcome)
    }

    async fn join_listed(
        &self,
        guild: sr_core::GuildId,
        user: UserId,
        now_ms: i64,
    ) -> Result<Option<sr_chat::Member>, ApiError> {
        let joined = self
            .repo
            .join_listed(guild, user, now_ms)
            .await
            .map_err(|e| {
                if matches!(e, sr_chat::settings::SettingsError::Repo(_)) {
                    tracing::error!(error = %e, "directory join failed");
                }
                e.to_api_error()
            })?;
        if let Some(member) = &joined {
            self.announce_join(member).await;
        }
        Ok(joined)
    }
}
