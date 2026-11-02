// @ts-check
// What a player can do in one tick, as small integers. The simulation never sees a raw mouse delta or a float:
// the client accumulates look into an integer yaw/pitch (TURN units per turn) and the buttons into a bit set.
// The same object is what the wire carries (net/codec.js) and what replays record (replay/script.json).

import { TURN } from './dmath.js';

/** Button bits. */
export const BTN = Object.freeze({
  FORWARD: 1,
  BACK: 2,
  LEFT: 4,
  RIGHT: 8,
  JUMP: 16,
  CROUCH: 32,
});

/** Pitch is clamped to just short of straight up/down (TURN/4 is 90 degrees). */
export const PITCH_LIMIT = TURN / 4 - 512;

/**
 * @typedef {object} PlayerInput
 * @property {number} buttons  a BTN bit set (0..63)
 * @property {number} yaw      integer, TURN units per turn, wrapped to int16 (-32768..32767)
 * @property {number} pitch    integer, clamped to +-PITCH_LIMIT
 */

/** @returns {PlayerInput} */
export const idleInput = () => ({ buttons: 0, yaw: 0, pitch: 0 });

/** Wrap any integer angle into int16. @param {number} units */
export function wrapYaw(units) {
  return (((Math.round(units) + 32768) % TURN) + TURN) % TURN - 32768;
}

/** @param {number} units */
export function clampPitch(units) {
  return Math.max(-PITCH_LIMIT, Math.min(PITCH_LIMIT, Math.round(units)));
}

/**
 * Normalise an input from anywhere (a peer, a script) to the legal integer ranges.
 * @param {Partial<PlayerInput> | null | undefined} raw
 * @returns {PlayerInput}
 */
export function sanitiseInput(raw) {
  return {
    buttons: (Number(raw?.buttons) | 0) & 63,
    yaw: wrapYaw(Number(raw?.yaw) || 0),
    pitch: clampPitch(Number(raw?.pitch) || 0),
  };
}

/** @param {PlayerInput} a @param {PlayerInput} b */
export const sameInput = (a, b) => a.buttons === b.buttons && a.yaw === b.yaw && a.pitch === b.pitch;
