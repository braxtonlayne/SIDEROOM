//! Applying an AutoMod `timeout` action to a member.
//!
//! The gate ([`super::automod_gate`]) resolves the longest timeout a message asks
//! for and hands it to a [`MemberTimeouter`]. The composition root registers
//! [`AdminMemberTimeouter`], which sets `communication_disabled_until` on the
//! member row (never shortening a timeout that already runs longer) and announces
//! the new member object as `GUILD_MEMBER_UPDATE` (`DomainEvent::MemberUpdated`).

use crate::routes::guilds::dto::MemberObject;
use async_trait::async_trait;
use sr_chat::admin_repo::GuildAdminRepository;
use sr_chat::repo::{ChatRepository, UserSummary};
use sr_core::{ApiError, ErrorCode, GuildId, UserId};
use sr_events::{DomainEvent, EventBus};
use std::collections::HashMap;
use std::sync::Arc;

#[async_trait]
pub trait MemberTimeouter: Send + Sync {
    /// Make `user` unable to communicate in `guild` until `until_ms` (unix ms).
    /// Returns the new `communication_disabled_until`, or `None` when the user is
    /// no member or already timed out for longer.
    async fn timeout_member(
        &self,
        guild: GuildId,
        user: UserId,
        until_ms: i64,
    ) -> Result<Option<i64>, ApiError>;
}

/// Does nothing; the default until the composition root wires a real one.
pub struct UnwiredMemberTimeouter;

#[async_trait]
impl MemberTimeouter for UnwiredMemberTimeouter {
    async fn timeout_member(&self, _: GuildId, _: UserId, _: i64) -> Result<Option<i64>, ApiError> {
        tracing::warn!("AutoMod timeout requested but no MemberTimeouter is wired");
        Ok(None)
    }
}

pub struct AdminMemberTimeouter<R> {
    repo: Arc<R>,
    bus: Arc<dyn EventBus>,
}

impl<R> AdminMemberTimeouter<R> {
    pub fn new(repo: Arc<R>, bus: Arc<dyn EventBus>) -> Self {
        Self { repo, bus }
    }
}

#[async_trait]
impl<R: ChatRepository + GuildAdminRepository + 'static> MemberTimeouter
    for AdminMemberTimeouter<R>
{
    async fn timeout_member(
        &self,
        guild: GuildId,
        user: UserId,
        until_ms: i64,
    ) -> Result<Option<i64>, ApiError> {
        let internal = |e: sr_chat::RepoError| {
            tracing::error!(error = %e, "AutoMod timeout failed");
            ApiError::new(ErrorCode::SrInternal)
        };
        let Some(mut member) = self.repo.get_member(guild, user).await.map_err(internal)? else {
            return Ok(None);
        };
        if member
            .communication_disabled_until_ms
            .is_some_and(|cur| cur >= until_ms)
        {
            return Ok(None);
        }
        member.communication_disabled_until_ms = Some(until_ms);
        self.repo.update_member(&member).await.map_err(internal)?;
        let users: HashMap<UserId, UserSummary> = match self.repo.users(&[user]).await {
            Ok(v) => v.into_iter().map(|u| (u.id, u)).collect(),
            Err(e) => {
                tracing::warn!(error = %e, "user lookup for GUILD_MEMBER_UPDATE failed");
                HashMap::new()
            }
        };
        let ev = DomainEvent::MemberUpdated {
            guild,
            user,
            payload: serde_json::to_value(MemberObject::new(&member, true, &users))
                .unwrap_or_default(),
        };
        for topic in ev.topics() {
            if let Err(e) = self.bus.publish(topic, ev.clone()).await {
                tracing::warn!(error = %e, "failed to publish MemberUpdated");
            }
        }
        Ok(Some(until_ms))
    }
}
