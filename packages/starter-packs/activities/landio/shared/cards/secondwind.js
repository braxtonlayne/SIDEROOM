// @ts-check
// Second Wind (passive, epic): once, instead of dying, the head comes back on the largest piece of its land and
// the rest of its land goes neutral. No kill is credited to whoever caused the death.

import { clearBase, setTrail } from './util.js';
import { DX, DY } from '../grid.js';

/** @typedef {import('../types.js').GameState} GameState */
/** @typedef {import('../types.js').Player} Player */

/**
 * p's land split into 4-connected regions, found by scanning only p's bounding box. Regions come out in
 * ascending order of their lowest tile, so a tie in size goes to the region with the lower index.
 * @param {GameState} s @param {Player} p @returns {number[][]}
 */
function regionsOf(s, p) {
  const { w, h } = s.cfg;
  const [x0, y0, x1, y1] = p.bbox;
  /** @type {Set<number>} */
  const seen = new Set();
  /** @type {number[][]} */
  const out = [];
  for (let y = y0; y <= y1; y++)
    for (let x = x0; x <= x1; x++) {
      const i = y * w + x;
      if (s.base[i] !== p.id || seen.has(i)) continue;
      /** @type {number[]} */
      const region = [];
      const stack = [i];
      seen.add(i);
      while (stack.length) {
        const j = /** @type {number} */ (stack.pop());
        region.push(j);
        const jx = j % w;
        const jy = (j - jx) / w;
        for (let d = 0; d < 4; d++) {
          const nx = jx + DX[d];
          const ny = jy + DY[d];
          if (nx < 0 || ny < 0 || nx >= w || ny >= h) continue;
          const k = ny * w + nx;
          if (s.base[k] === p.id && !seen.has(k)) {
            seen.add(k);
            stack.push(k);
          }
        }
      }
      out.push(region);
    }
  return out;
}

/**
 * The tile of `tiles` with the most own-base 4-neighbours among those `ok`; ties go to the lowest index. -1 if none.
 * @param {GameState} s @param {Player} p @param {number[]} tiles @param {(i: number) => boolean} ok
 */
function bestHead(s, p, tiles, ok) {
  const { w, h } = s.cfg;
  let best = -1;
  let bestN = -1;
  for (const i of tiles.slice().sort((a, b) => a - b)) {
    if (!ok(i)) continue;
    const x = i % w;
    const y = (i - x) / w;
    let n = 0;
    for (let d = 0; d < 4; d++) {
      const nx = x + DX[d];
      const ny = y + DY[d];
      if (nx >= 0 && ny >= 0 && nx < w && ny < h && s.base[ny * w + nx] === p.id) n++;
    }
    if (n > bestN) {
      bestN = n;
      best = i;
    }
  }
  return best;
}

/** @type {import('../cards.js').CardDef} */
export default {
  id: 'secondwind',
  name: 'Second Wind',
  text: 'Once: instead of dying, come back on your largest piece of land. The rest of your land goes neutral.',
  kind: 'passive',
  rarity: 'epic',
  maxLevel: 1,
  notForLeader: true,
  preventDeath(s, p, cause, _killer, level) {
    if (level <= 0 || p.fx.secondwindUsed || cause === 'removed') return false;
    // Everything is checked before anything changes: with no land left, p dies as usual.
    const regions = regionsOf(s, p);
    if (!regions.length) return false;
    let keep = regions[0];
    for (const r of regions) if (r.length > keep.length) keep = r;

    p.fx.secondwindUsed = 1;
    for (const i of p.trail) if (s.trail[i] === p.id) setTrail(s, i, 0);
    p.trail = [];
    for (const r of regions) if (r !== keep) for (const i of r) clearBase(s, i);

    // The head goes to the kept tile with the most own-base neighbours, preferring a tile with no foreign trail.
    const { w } = s.cfg;
    let at = bestHead(s, p, keep, (i) => s.trail[i] === 0);
    if (at < 0) at = bestHead(s, p, keep, () => true);
    p.x = at % w;
    p.y = (at - (at % w)) / w;
    p.stopped = true;
    p.queue = [];

    s.events.push({ k: 'fx', card: 'secondwind', id: p.id, x: p.x, y: p.y });
    return true;
  },
};
