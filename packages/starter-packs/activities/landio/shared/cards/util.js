// @ts-check
// Helpers for card modules: safe ways to change trails and tiles, and area queries.

import { setBase, setTrail, playerById } from '../grid.js';

/** @typedef {import('../types.js').GameState} GameState */
/** @typedef {import('../types.js').Player} Player */

/**
 * Erase the trail tiles of `p` that are in `cut`, keeping the trail one contiguous path that ends at the head:
 * everything from the start of the trail up to the last cut tile goes; the head's own tile always stays.
 * @param {GameState} s @param {Player} p @param {Set<number>} cut
 */
export function cutTrail(s, p, cut) {
  if (!p.trail.length) return;
  let last = -1;
  for (let k = 0; k < p.trail.length - 1; k++) if (cut.has(p.trail[k])) last = k;
  if (last < 0) return;
  for (let k = 0; k <= last; k++) setTrail(s, p.trail[k], 0);
  p.trail = p.trail.slice(last + 1);
}

/** Tile indices within Manhattan radius `r` of (x, y), on the grid. @param {GameState} s @param {number} x @param {number} y @param {number} r */
export function diamond(s, x, y, r) {
  const out = [];
  for (let dy = -r; dy <= r; dy++)
    for (let dx = -(r - Math.abs(dy)); dx <= r - Math.abs(dy); dx++) {
      const tx = x + dx;
      const ty = y + dy;
      if (tx >= 0 && ty >= 0 && tx < s.cfg.w && ty < s.cfg.h) out.push(ty * s.cfg.w + tx);
    }
  return out;
}

/** Tile indices of the square of side 2r+1 around (x, y), on the grid. @param {GameState} s @param {number} x @param {number} y @param {number} r */
export function square(s, x, y, r) {
  const out = [];
  for (let ty = Math.max(0, y - r); ty <= Math.min(s.cfg.h - 1, y + r); ty++)
    for (let tx = Math.max(0, x - r); tx <= Math.min(s.cfg.w - 1, x + r); tx++) out.push(ty * s.cfg.w + tx);
  return out;
}

/** @type {(s: GameState, p: Player, cause: string, killer: Player | null) => void} */
let killImpl = () => {
  throw new Error('kill() used before shared/rules.js loaded');
};
/** Called once by shared/rules.js (avoids an import cycle). @param {typeof killImpl} fn */
export function registerKill(fn) {
  killImpl = fn;
}
/**
 * Kill `p` (cause e.g. 'grenade'), credited to `killer` when given: the same path as any death (cards may
 * prevent it; the killer inherits the land and drafts cards).
 * @param {GameState} s @param {Player} p @param {string} cause @param {Player | null} killer
 */
export const kill = (s, p, cause, killer) => killImpl(s, p, cause, killer);

/**
 * Add a world object (a thrown grenade, a placed snare, a wall, a mirage). It lives until tick `until`
 * (exclusive: gone on that tick) or until its card marks it `dead`.
 * @param {GameState} s @param {Omit<import('../types.js').WorldObject, 'id'>} o
 */
export function addObject(s, o) {
  const obj = { id: ++s.nextObjectId, ...o };
  s.objects.push(obj);
  return obj;
}

/** Living players other than `p`. @param {GameState} s @param {Player} p */
export const enemies = (s, p) => s.players.filter((q) => q.alive && q !== p);

/** Make tile i neutral land, keeping land counts right. @param {GameState} s @param {number} i */
export const clearBase = (s, i) => setBase(s, i, 0);

export { setBase, setTrail, playerById };
