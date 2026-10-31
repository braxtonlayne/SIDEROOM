// @ts-check
// Wall (item, rare): a 5-tile wall across the path behind you for 8 s. Every head that drives into it stops,
// its owner included; the owner is safe as long as it walks away, since the wall is behind it. The wall never
// touches land or trails.

import { DX, DY } from '../grid.js';
import { addObject } from './util.js';

/** @typedef {import('../types.js').GameState} GameState */
/** @typedef {import('../types.js').Player} Player */
/** @typedef {import('../types.js').WorldObject} WorldObject */

const LIFE = 56;
const HALF = 2;

/** @type {import('../cards.js').CardDef} */
export default {
  id: 'wall',
  name: 'Wall',
  text: 'Q/E: raise a 5-tile wall across your path behind you for 8 s. Heads that hit it stop.',
  kind: 'item',
  rarity: 'rare',
  maxLevel: 1,
  use(s, p) {
    if (p.dir < 0) return false;
    // Centred on the tile behind the head. Pushed directly rather than through addObject (cards.js), because
    // cards.js imports this module and a runtime import back would be a cycle.
    const x = p.x - DX[p.dir];
    const y = p.y - DY[p.dir];
    addObject(s, { kind: 'wall', owner: p.id, x, y, dir: p.dir, until: s.tick + LIFE });
    const e = { k: 'fx', card: 'wall', id: p.id, x, y, dir: p.dir };
    s.events.push(e);
    return true;
  },
  // Off-grid tiles of a wall at the map edge need no skipping: no head can stand on them.
  objectBlocks(s, o, x, y) {
    // The engine removes an object after the tick it ends on has moved the heads, so the wall stops blocking
    // on that tick itself.
    if (s.tick >= o.until) return false;
    if (o.dir === 0 || o.dir === 2) return y === o.y && Math.abs(x - o.x) <= HALF;
    return x === o.x && Math.abs(y - o.y) <= HALF;
  },
  botScore(s, p) {
    if (p.bot?.mode !== 'flee' || p.dir < 0) return 0;
    // An enemy head within 4 tiles, on the far side of the bot from the way it faces.
    for (const e of s.players) {
      if (e === p || !e.alive) continue;
      const dx = e.x - p.x;
      const dy = e.y - p.y;
      const along = dx * DX[p.dir] + dy * DY[p.dir];
      if (along < 0 && Math.abs(dx) + Math.abs(dy) <= 4) return 0.8;
    }
    return 0;
  },
};
