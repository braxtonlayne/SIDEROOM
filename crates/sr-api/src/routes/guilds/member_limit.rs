//! The members-per-server limit in force, for the guild object's `max_members` (D-660).

use std::sync::Arc;

use async_trait::async_trait;

/// Where the limit in force comes from. The composition root registers a [`MemberLimitSource`] that reads
/// the `guilds.max_members` setting; without one the default applies.
#[async_trait]
pub trait MemberLimit: Send + Sync {
    /// The number of members one server may have right now.
    async fn members_limit(&self) -> u64;
}

/// Registered on the `AppState` as an extension.
pub struct MemberLimitSource(pub Arc<dyn MemberLimit>);

/// The default, used when no source is registered (tests).
pub fn default_limit() -> u64 {
    u64::try_from(sr_core::guild_caps::DEFAULT_GUILD_MEMBERS).unwrap_or(25_000)
}
