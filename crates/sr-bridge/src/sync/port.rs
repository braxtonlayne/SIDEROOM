//! The ports of the SIDE ROOM side (design sections 5.2, 6.2 and 11.3).
//!
//! `sr-bridge` does not depend on `sr-chat`. Everything the mirror changes on SIDE ROOM goes through
//! [`SrStructure`], which `sr-server` implements over the `sr-chat` role, channel and thread services with
//! `Origin::Bridge { op_id }` (the dispatch carries the origin, so the outbound listener drops its own echo).
//!
//! Note what is missing: there is no method to set a role's permission word, an overwrite or a role position from
//! Discord. The only permission write is [`SrStructure::import_role_permissions`], reachable only from the seed
//! of a plan the owner confirmed (design 9.4), and its argument is a [`Permissions`] value that has already been
//! cut down to the shared bits.

use async_trait::async_trait;
use sr_core::permissions::Permissions;

use super::patch::SrStructuralPatch;
use crate::error::BridgeResult;

/// Who is behind a change the bridge applies here (the audit row's `options.sr_bridge`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeOrigin {
    /// The sync op (or seed step) that caused it.
    pub op_id: i64,
    /// The Discord user who made the change, when the audit log said so.
    pub discord_user_id: Option<i64>,
    /// A person on SIDE ROOM acted (the owner confirming a seed): they are the audit actor, not the bridge user.
    pub sr_actor: Option<i64>,
}

impl BridgeOrigin {
    pub fn bridge(op_id: i64) -> Self {
        Self {
            op_id,
            discord_user_id: None,
            sr_actor: None,
        }
    }
}

/// A role created here from Discord: no permissions (the importing step adds the owner-confirmed ones).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrNewRole {
    pub name: String,
    pub color: i32,
    pub hoist: bool,
    pub mentionable: bool,
    pub unicode_emoji: Option<String>,
}

/// A channel, category or thread created here from Discord.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrNewChannel {
    /// The SIDE ROOM channel type (0 text, 2 voice, 4 category, 5 announcement, 10 to 12 thread, 13 stage, 15, 16).
    pub kind: i16,
    pub name: String,
    pub topic: Option<String>,
    pub parent_id: Option<i64>,
    pub position: i32,
    pub nsfw: bool,
    pub rate_limit_per_user: i32,
    pub bitrate: Option<i32>,
    pub user_limit: Option<i32>,
}

#[async_trait]
pub trait SrStructure: Send + Sync {
    /// Create a role with no permissions, directly above `@everyone`. Returns its id.
    async fn create_role(
        &self,
        guild: i64,
        new: SrNewRole,
        origin: &BridgeOrigin,
    ) -> BridgeResult<i64>;

    /// Create a channel, category or thread; for a child it copies the SIDE ROOM parent's overwrites (as Discord's
    /// "synced to category" does) and nothing more. Returns its id.
    async fn create_channel(
        &self,
        guild: i64,
        new: SrNewChannel,
        origin: &BridgeOrigin,
    ) -> BridgeResult<i64>;

    /// Apply a Discord-originated structural change (two-way fields only; see [`SrStructuralPatch`]).
    async fn apply(
        &self,
        guild: i64,
        patch: &SrStructuralPatch,
        origin: &BridgeOrigin,
    ) -> BridgeResult<()>;

    /// Delete a role (the guarded Discord deletion, design 6.3).
    async fn delete_role(&self, guild: i64, role: i64, origin: &BridgeOrigin) -> BridgeResult<()>;

    /// Delete a channel, category or thread through the normal path (the hold job and "Delete now").
    async fn delete_channel(
        &self,
        guild: i64,
        channel: i64,
        origin: &BridgeOrigin,
    ) -> BridgeResult<()>;

    /// OWNER-CONFIRMED import (design 9.4): add `shared` bits to a role created from Discord. `origin.sr_actor`
    /// is the owner who confirmed the plan; the audit row is theirs.
    async fn import_role_permissions(
        &self,
        guild: i64,
        role: i64,
        shared: Permissions,
        origin: &BridgeOrigin,
    ) -> BridgeResult<()>;

    /// Whether a role denies something in any channel overwrite here (the role-deletion guard, design 2.3).
    async fn role_has_denies(&self, guild: i64, role: i64) -> BridgeResult<bool>;
}

/// Pushes SIDE ROOM's overwrites of mapped channels to Discord. Owned by BRG-SYN-002 (`sync::overwrites`); the
/// seed calls it after the structure exists.
#[async_trait]
pub trait OverwritePush: Send + Sync {
    /// Make the Discord channel's overwrites equal SIDE ROOM's (shared bits only, Discord-only bits preserved).
    async fn push_channel(
        &self,
        guild: i64,
        sr_channel: i64,
        discord_channel: i64,
        reason: &str,
    ) -> BridgeResult<()>;
}

/// Does nothing (selective mode, tests that look elsewhere).
pub struct NoOverwrites;

#[async_trait]
impl OverwritePush for NoOverwrites {
    async fn push_channel(&self, _: i64, _: i64, _: i64, _: &str) -> BridgeResult<()> {
        Ok(())
    }
}
