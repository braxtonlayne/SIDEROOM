// @ts-check
// Snare (item, common). Hide a snare on your own base, standing still with no trail, at most 2 at a time and
// never two on one tile. The first enemy head to step on it is stunned for 11 ticks (it makes no moves for the
// first 10 of them) and the snare is spent. Snares are secret: only their owner's client draws them.

import { idx } from '../grid.js';
import { addObject } from './util.js';

/** @typedef {import('../types.js').GameState} GameState */
/** @typedef {import('../types.js').Player} Player */

const LIFE = 420;
const STUN = 11;
const MAX_PER_OWNER = 2;
/** Chebyshev distance within which a bot wants to set a snare. */
const BOT_RANGE = 10;

/** Whether p may hide a snare on its head's tile now. @param {GameState} s @param {Player} p */
function canPlace(s, p) {
  if (p.trail.length || s.base[idx(s, p.x, p.y)] !== p.id) return false;
  let mine = 0;
  for (const o of s.objects) {
    if (o.kind !== 'snare') continue;
    if (o.x === p.x && o.y === p.y) return false;
    if (o.owner === p.id) mine++;
  }
  return mine < MAX_PER_OWNER;
}

/** @type {import('../cards.js').CardDef} */
export default {
  id: 'snare',
  name: 'Snare',
  text: 'Q/E: hide a snare on your land. The first enemy to step on it is stuck for 1.5 s.',
  kind: 'item',
  rarity: 'common',
  maxLevel: 1,
  use(s, p) {
    if (!canPlace(s, p)) return false;
    // Pushed directly rather than via addObject(): cards.js imports this module, so importing it back is a cycle.
    addObject(s, { kind: 'snare', owner: p.id, x: p.x, y: p.y, dir: 0, until: s.tick + LIFE });
    return true;
  },
  objectTick(s, o) {
    // Removed after this pass, so a head arriving on its last tick must not trigger it.
    if (s.tick >= o.until) return;
    for (const q of s.players) {
      if (!q.alive || q.id === o.owner || q.x !== o.x || q.y !== o.y) continue;
      q.fx.stun = s.tick + STUN;
      o.dead = true;
      s.events.push({ k: 'fx', card: 'snare', id: o.owner, x: o.x, y: o.y, v: q.id });
      return;
    }
  },
  botScore(s, p) {
    if (!canPlace(s, p)) return 0;
    for (const q of s.players) {
      if (q.alive && q !== p && Math.max(Math.abs(q.x - p.x), Math.abs(q.y - p.y)) <= BOT_RANGE) return 0.7;
    }
    return 0;
  },
};
