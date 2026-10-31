// @ts-check
// Last Land Standing: no respawns. The last one alive wins. The arena fills with bots at the round start and
// nobody joins after the first 10 seconds. The round ends early when one player or none is left (once it is
// 10 seconds old and two were alive at some point), or at the 5 minute cap (then the most land wins). Sudden death
// keeps rounds from running out the clock: from 2:00 the border closes in like Shrinking Arena (every 15 s, down
// to about 30% of the map by about 4:30), so the survivors have to meet.

import { closing } from './shrink.js';

/** Ticks after the round start in which bots may still join, and before which the round cannot end early. */
const GRACE = 70;
/** Sudden death: when the border starts closing, and the time between its steps. */
export const SUDDEN_DEATH = 2 * 60 * 7;
const CLOSE_EVERY = 15 * 7;

/** @param {import('../types.js').GameState} s */
const aliveCount = (s) => {
  let n = 0;
  for (const p of s.players) if (p.alive) n++;
  return n;
};

/**
 * Survival ranking (negative when `a` ranks above `b`, for Array.sort): alive before dead, the later death
 * first among the dead, more land first among the living, then the lower id.
 * @param {import('../types.js').GameState} _s @param {import('../types.js').Player} a @param {import('../types.js').Player} b
 */
export function rank(_s, a, b) {
  if (a.alive !== b.alive) return a.alive ? -1 : 1;
  if (!a.alive) return (b.death?.tick ?? 0) - (a.death?.tick ?? 0) || b.land - a.land || a.id - b.id;
  return b.land - a.land || a.id - b.id;
}

/** @type {import('./index.js').ModeDef} */
export default {
  id: 'lastland',
  name: 'Last Land Standing',
  blurb: 'No respawns. The last one alive wins.',
  roundTicks: 5 * 60 * 7,
  respawn: false,
  rank,
  setup(s) {
    const { w, h } = s.cfg;
    s.md = { peak: 0, zone: [0, 0, w, h], k: 0, at: s.round.start + SUDDEN_DEATH };
  },
  tick(s) {
    const n = aliveCount(s);
    if (n > /** @type {number} */ (s.md.peak ?? 0)) s.md.peak = n;
    closing(s, CLOSE_EVERY);
  },
  botGoal(s, p) {
    // Near the closing border, head for the middle.
    const z = /** @type {number[]} */ (s.md.zone);
    if (s.md.at === undefined || s.tick < Number(s.md.at) - 2 * CLOSE_EVERY) return null;
    const m = 10;
    if (p.x >= z[0] + m && p.x < z[2] - m && p.y >= z[1] + m && p.y < z[3] - m) return null;
    return { x: Math.floor((z[0] + z[2]) / 2), y: Math.floor((z[1] + z[3]) / 2) };
  },
  botsJoin(s) {
    return s.tick - s.round.start <= GRACE;
  },
  roundOver(s) {
    if (s.tick - s.round.start < GRACE) return false;
    const n = aliveCount(s);
    const peak = /** @type {number} */ (s.md.peak ?? 0);
    return n === 0 ? peak >= 1 : n === 1 && peak >= 2;
  },
};
