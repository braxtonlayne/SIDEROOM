// @ts-check
// Mirage (item, common): a ghost of your head runs straight on for 4 s (28 ticks) in your direction. It lays
// no trail, kills nobody, cannot be killed and never touches the grid, so it only fools the heads chasing you.

import { DX, DY, passable } from '../grid.js';
import { addObject } from './util.js';

const LIFE = 28;

/** @type {import('../cards.js').CardDef} */
export default {
  id: 'mirage',
  name: 'Mirage',
  text: 'Q/E: a ghost of you runs straight on for 4 s, to fool the people chasing you.',
  kind: 'item',
  rarity: 'common',
  maxLevel: 1,
  use(s, p) {
    if (p.dir < 0) return false;
    // Pushed directly, as wall.js does: cards.js imports this module, so a runtime import back would be a cycle.
    addObject(s, { kind: 'mirage', owner: p.id, x: p.x, y: p.y, dir: p.dir, until: s.tick + LIFE });
    s.events.push({ k: 'fx', card: 'mirage', id: p.id, x: p.x, y: p.y });
    return true;
  },
  objectTick(s, o) {
    // Like the wall, an object is still alive on the tick it ends on, so the ghost stands still on that tick.
    if (o.dead || s.tick >= o.until) return;
    const x = o.x + DX[o.dir];
    const y = o.y + DY[o.dir];
    if (!passable(s, x, y)) {
      o.dead = true;
      return;
    }
    o.x = x;
    o.y = y;
  },
  botScore(s, p) {
    if (p.bot?.mode !== 'flee') return 0;
    for (const e of s.players) {
      if (e === p || !e.alive) continue;
      if (Math.abs(e.x - p.x) + Math.abs(e.y - p.y) <= 6) return 0.75;
    }
    return 0;
  },
};
