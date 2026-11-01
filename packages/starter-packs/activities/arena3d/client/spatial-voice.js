// @ts-check
// Spatial voice hooks: where the listener (this player) and each speaker (other players) are, every frame.
//
// SIDE ROOM's `ingame_voice` capability is reserved (D-399): a bundle may declare it, but the runtime refuses it,
// so there is no in-game voice yet and the implementation below does nothing. The game already calls these
// hooks every frame, so when the platform ships the capability, the SDK side plugs in here (for example a
// WebAudio PannerNode per speaker fed by the call's audio tracks) and no game code changes.
//
// Coordinates are world metres, +Y up; `forward` is a unit vector (where the listener looks).

/**
 * @typedef {object} SpatialVoice
 * @property {(pos: [number, number, number], forward: [number, number, number]) => void} onListenerMove
 * @property {(memberId: string, pos: [number, number, number]) => void} onSpeakerMove
 * @property {(memberId: string) => void} onSpeakerGone
 * @property {boolean} active   false until a real implementation is attached
 */

/** The no-op implementation the game uses today. @returns {SpatialVoice} */
export function createNoopSpatialVoice() {
  return {
    active: false,
    onListenerMove() {},
    onSpeakerMove() {},
    onSpeakerGone() {},
  };
}

/**
 * Pick the implementation for a capability list. Always the no-op today: `ingame_voice` is reserved, and the
 * SDK has no voice routing command to call. Kept as the single switch point for the future capability.
 * @param {readonly string[]} _capabilities
 * @returns {SpatialVoice}
 */
export function spatialVoiceFor(_capabilities) {
  return createNoopSpatialVoice();
}
