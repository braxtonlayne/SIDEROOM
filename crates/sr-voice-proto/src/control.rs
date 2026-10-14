//! Gateway -> SFU control plane. The gateway owns voice state; the SFU enforces it
//! on the media path. When both run in one process the relay is a direct call.
//! TODO(multi-node): an HTTP implementation for SFU nodes in other processes.

use async_trait::async_trait;
use sr_core::{ChannelId, UserId};

/// A change the SFU must apply to a participant already in a room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceCommand {
    /// Refresh the media-path flags of `user` in `channel`.
    SetFlags {
        channel: ChannelId,
        user: UserId,
        /// Allowed to send audio (SPEAK permission, not server muted).
        speak: bool,
        /// Server deafened or self deafened: nothing is sent to them.
        deaf: bool,
        ptt_required: bool,
        priority: bool,
        /// May publish a camera.
        camera: bool,
        /// May publish a screen share and its audio.
        screen: bool,
    },
    /// Remove `user` from `channel` (leave, move, kick, session loss).
    Disconnect { channel: ChannelId, user: UserId },
    /// Who may receive `owner`'s screen share in `channel`. The SFU forwards the screen
    /// (and its audio) to nobody else. `live: false` ends it. The gateway sends this when
    /// a stream starts, when a viewer starts or stops watching, and when it ends.
    StreamViewers {
        channel: ChannelId,
        owner: UserId,
        viewers: Vec<UserId>,
        live: bool,
    },
    /// Drop a whole room (channel deleted).
    CloseRoom { channel: ChannelId },
}

#[async_trait]
pub trait VoiceRelay: Send + Sync {
    async fn apply(&self, command: VoiceCommand);
}
