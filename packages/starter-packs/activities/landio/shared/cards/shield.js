// @ts-check
// Shield (item, rare). STUB: the card's data is final; its behaviour is implemented in wave 1.
// See shared/cards.js (CardDef) for the hooks available.

import { enemies } from './util.js';
import { DX, DY, idx } from '../grid.js';

/** @typedef {import('../types.js').GameState} GameState */
/** @typedef {import('../types.js').Player} Player */

/**
 * Whether the cutter's next tile is p's own head. A hit on the head is a normal cut (as D-39 says of Phase):
 * the head stays where it is, so the trail could not be cut back to anything.
 * @param {GameState} s @param {Player} p @param {Player} killer
 */
function hitsHead(s, p, killer) {
  if (killer.dir < 0) return false;
  return idx(s, killer.x + DX[killer.dir], killer.y + DY[killer.dir]) === idx(s, p.x, p.y);
}

/** @type {import('../cards.js').CardDef} */
export default {
  id: 'shield',
  name: 'Shield',
  text: 'Q/E: for 10 s, the first cut on your trail does not kill you and stuns the cutter.',
  kind: 'item',
  rarity: 'rare',
  maxLevel: 1,
  use(s, p) {
    p.fx.shield = s.tick + 70;
    return true;
  },
  preventDeath(s, p, cause, killer) {
    // Only a cut by a living enemy; a head-on crash, the border, enclosure and self kills still count.
    if (cause !== 'trail' || !killer || killer === p || !killer.alive) return false;
    if (!((p.fx.shield ?? 0) > s.tick)) return false;
    if (hitsHead(s, p, killer)) return false;
    p.fx.shield = 0;
    // Stun ends on the tick given; the cut tick's own move has already happened, so it stalls 3 ticks after.
    // fx ends are exclusive and the cutter's move this tick already happened: +5 freezes it for 4 ticks.
    killer.fx.stun = s.tick + 5;
    s.events.push({ k: 'fx', card: 'shield', id: p.id, x: p.x, y: p.y, v: killer.id });
    return true;
  },
  botScore(s, p) {
    if (p.trail.length < 6) return 0;
    const { w } = s.cfg;
    for (const e of enemies(s, p))
      for (const i of p.trail) {
        const tx = i % w;
        const ty = (i - tx) / w;
        if (Math.abs(tx - e.x) + Math.abs(ty - e.y) <= 5) return 0.75;
      }
    return 0;
  },
};
