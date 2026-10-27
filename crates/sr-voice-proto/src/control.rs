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
    /// `user` has taken part in the call's MLS handshake (op 101 `key_package`, or `ready` for the member
    /// that starts the group), so the SFU may forward their media (OPS-IMM-001). Until then a sender's
    /// frames are dropped and counted (`sr_voice_unkeyed_frames_total`): end-to-end encryption is the only
    /// voice mode, and a client or bot that never speaks it joins but is never heard. `keyed: false`
    /// withdraws it. The gateway sends this; it is forgotten with `Disconnect` and `CloseRoom`.
    E2eeKeyed {
        channel: ChannelId,
        user: UserId,
        keyed: bool,
    },
    /// Drop a whole room (channel deleted).
    CloseRoom { channel: ChannelId },
}

#[async_trait]
pub trait VoiceRelay: Send + Sync {
    async fn apply(&self, command: VoiceCommand);
}
