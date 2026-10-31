// @ts-check
// Rewind (active, rare): slide back along your own trail, erasing up to 8 tiles of it. The head lands on the
// new last trail tile. When the whole trail goes, it lands on its base next to where the trail began, with no
// trail. Either way the head stops and waits for a key: the erased part is gone, so there is nothing to reverse
// into.

import { DX, DY, idx, inBounds } from '../grid.js';
import { setTrail } from './util.js';

/** @typedef {import('../types.js').GameState} GameState */
/** @typedef {import('../types.js').Player} Player */

const MAX_BACK = 8;

/**
 * The base tile of `p` to land on when its whole trail is gone: the first of up, right, down, left that is
 * its base and touches the trail's first tile. Once another head has cut the start of the trail away, that
 * tile may be nowhere near the base, so then the nearest base tile is used. -1 when `p` holds no land.
 * @param {GameState} s @param {Player} p @param {number} first
 */
function homeFor(s, p, first) {
  const { w } = s.cfg;
  const x0 = first % w;
  const y0 = (first - x0) / w;
  for (let d = 0; d < 4; d++) {
    const x = x0 + DX[d];
    const y = y0 + DY[d];
    if (inBounds(s, x, y) && s.base[idx(s, x, y)] === p.id) return idx(s, x, y);
  }
  if (!p.bbox.length) return -1;
  let best = -1;
  let bestDist = Infinity;
  const [bx0, by0, bx1, by1] = p.bbox;
  for (let y = by0; y <= by1; y++)
    for (let x = bx0; x <= bx1; x++) {
      const i = idx(s, x, y);
      if (s.base[i] !== p.id) continue;
      const dist = Math.abs(x - x0) + Math.abs(y - y0);
      if (dist < bestDist) {
        bestDist = dist;
        best = i;
      }
    }
  return best;
}

/** @type {import('../cards.js').CardDef} */
export default {
  id: 'rewind',
  name: 'Rewind',
  text: 'Shift: slide back along your trail up to 8 tiles, erasing it. Recharges in 15 s.',
  kind: 'active',
  rarity: 'rare',
  maxLevel: 1,
  cooldown: () => 105,
  use(s, p) {
    const len = p.trail.length;
    const n = Math.min(MAX_BACK, len);
    if (!n) return false;
    const { w } = s.cfg;
    const from = p.y * w + p.x;
    // Decide where the head lands before erasing anything, so a failed use changes nothing.
    const dest = n < len ? p.trail[len - n - 1] : homeFor(s, p, p.trail[0]);
    if (dest < 0) return false;
    for (let k = len - n; k < len; k++) setTrail(s, p.trail[k], 0);
    p.trail = p.trail.slice(0, len - n);
    p.x = dest % w;
    p.y = (dest - p.x) / w;
    p.stopped = true;
    p.queue = [];
    s.events.push(/** @type {import('../types.js').GameEvent} */ ({ k: 'fx', card: 'rewind', id: p.id, x: p.x, y: p.y, from }));
    return true;
  },
  botScore(_s, p) {
    return p.bot?.mode === 'flee' && p.trail.length >= 4 ? 0.85 : 0;
  },
};
