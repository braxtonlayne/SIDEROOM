//! [`SrStructuralPatch`]: the only thing the Discord-to-SIDE ROOM applier accepts (design section 5.2).
//!
//! The type has no permission word, no overwrite and no role position field, so the compiler, not a review,
//! keeps authorization inputs out of the path that Discord events take. Role order is an authorization input here
//! (`actor.outranks_position`) and so is every overwrite, which is why they are absent.
//!
//! ```compile_fail
//! // A role patch cannot carry a permission word.
//! let _ = sr_bridge::sync::RoleStructurePatch { permissions: Some(8), ..Default::default() };
//! ```
//!
//! ```compile_fail
//! // Nor a position.
//! let _ = sr_bridge::sync::RoleStructurePatch { position: Some(1), ..Default::default() };
//! ```
//!
//! ```compile_fail
//! // Nor overwrites on a channel.
//! let _ = sr_bridge::sync::ChannelPatch { permission_overwrites: Some(vec![]), ..Default::default() };
//! ```
//!
//! Fields that exist compile:
//!
//! ```
//! let p = sr_bridge::sync::RoleStructurePatch { name: Some("Mods".into()), ..Default::default() };
//! assert!(!p.is_empty());
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `None` leaves a field alone. For a clearable field `Some(None)` clears it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleStructurePatch {
    pub name: Option<String>,
    /// Primary colour only (`0xRRGGBB`); SIDE ROOM's secondary and tertiary colours are SIDE ROOM-only.
    pub color: Option<i32>,
    pub hoist: Option<bool>,
    pub mentionable: Option<bool>,
    pub unicode_emoji: Option<Option<String>>,
}

impl RoleStructurePatch {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Channel, category and forum fields that flow Discord to SIDE ROOM. Parent and position are channel order
/// (two-way); they are NOT role positions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelPatch {
    pub name: Option<String>,
    pub topic: Option<Option<String>>,
    pub nsfw: Option<bool>,
    pub rate_limit_per_user: Option<i32>,
    /// The SIDE ROOM id of the new parent category (`Some(None)`: top level).
    pub parent_id: Option<Option<i64>>,
    pub position: Option<i32>,
    pub bitrate: Option<i32>,
    pub user_limit: Option<i32>,
    pub video_quality_mode: Option<i16>,
    pub default_auto_archive_duration: Option<i32>,
    pub default_thread_rate_limit: Option<i32>,
    /// Forum tags, by name (ids differ per side).
    pub available_tags: Option<Value>,
    pub default_reaction_emoji: Option<Value>,
    pub default_sort_order: Option<i16>,
    pub default_forum_layout: Option<i16>,
}

impl ChannelPatch {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadPatch {
    pub name: Option<String>,
    pub archived: Option<bool>,
    pub locked: Option<bool>,
    pub auto_archive_duration: Option<i32>,
    pub rate_limit_per_user: Option<i32>,
    /// Applied forum tags, as SIDE ROOM tag ids.
    pub applied_tags: Option<Vec<i64>>,
}

impl ThreadPatch {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A change to apply on SIDE ROOM because Discord changed something two-way. The target object is named by its
/// SIDE ROOM id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SrStructuralPatch {
    Role {
        role_id: i64,
        patch: RoleStructurePatch,
    },
    Channel {
        channel_id: i64,
        patch: ChannelPatch,
    },
    Thread {
        thread_id: i64,
        patch: ThreadPatch,
    },
}

impl SrStructuralPatch {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Role { patch, .. } => patch.is_empty(),
            Self::Channel { patch, .. } => patch.is_empty(),
            Self::Thread { patch, .. } => patch.is_empty(),
        }
    }
}
