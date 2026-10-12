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
            // The row is committed; the payload is best effort (bare ids if the
            // user lookup fails) and a bus failure never fails the request.
            let users: HashMap<UserId, UserSummary> = match self.chat.users(&[user]).await {
                Ok(v) => v.into_iter().map(|u| (u.id, u)).collect(),
                Err(e) => {
                    tracing::warn!(error = %e, "user lookup for MEMBER_JOINED failed");
                    HashMap::new()
                }
            };
            let payload =
                serde_json::to_value(MemberObject::new(member, true, &users)).unwrap_or_default();
            let ev = DomainEvent::MemberJoined {
                guild: member.guild_id,
                user,
                payload,
            };
            for topic in ev.topics() {
                if let Err(e) = self.bus.publish(topic, ev.clone()).await {
                    tracing::warn!(error = %e, "failed to publish MemberJoined");
                }
            }
        }
        Ok(outcome)
    }
}
