// @ts-check
// EMP (item, epic): enemies whose heads are within 6 tiles cannot use cards for 3 s. The engine blocks card
// use while fx.emp is on and holds an active's recharge in place (cardsTick), so this card only sets the timer.

import { enemies } from './util.js';

/** @typedef {import('../types.js').GameState} GameState */
/** @typedef {import('../types.js').Player} Player */

const RANGE = 6;
const TICKS = 21;

/** Living enemies whose head is within RANGE (Chebyshev) of p's head. @param {GameState} s @param {Player} p */
function inRange(s, p) {
  return enemies(s, p).filter((q) => Math.max(Math.abs(q.x - p.x), Math.abs(q.y - p.y)) <= RANGE);
}

/** @type {import('../cards.js').CardDef} */
export default {
  id: 'emp',
  name: 'EMP',
  text: 'Q/E: enemies within 6 tiles cannot use cards for 3 s.',
  kind: 'item',
  rarity: 'epic',
  maxLevel: 1,
  use(s, p) {
    const hit = inRange(s, p);
    if (!hit.length) return false;
    // A later EMP never shortens one already on the enemy.
    for (const q of hit) q.fx.emp = Math.max(q.fx.emp ?? 0, s.tick + TICKS);
    s.events.push({ k: 'fx', card: 'emp', id: p.id, x: p.x, y: p.y, n: hit.length });
    return true;
  },
  botScore(s, p) {
    return inRange(s, p).some((q) => q.cards.items.length > 0 || !!q.cards.active) ? 0.7 : 0;
  },
};
