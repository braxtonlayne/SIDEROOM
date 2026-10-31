// @ts-check
// Donut: a ring of open ground around a solid void. The void is a centred ellipse (a disc on square maps) with
// semi-axes 26% of the width and height; the outer corners are cut away by a superellipse (exponent 4) that
// follows the map's aspect, so the play area reads as a round ring. A tile is wall when its centre is inside
// the void or outside the outer shape; wall tiles with no wall neighbour are then dropped (no spikes). A thin
// band of GOLD (1 tile at 40x40, 2 at 100x100, 4 at 200x200, so it stays 3-4% of the map) hugs the inner edge: risky ground next to the void.
// Everything is symmetric left-right and top-bottom, and uses no randomness.
import { OPEN, WALL, GOLD } from '../grid.js';

const INNER = 0.26; // void semi-axis as a share of the map side
const OUTER_POW = 4; // superellipse exponent of the outer edge

/** @type {import('./index.js').MapDef} */
export default {
  id: 'donut',
  name: 'Donut',
  blurb: 'A ring around an empty middle. No safe centre.',
  build(w, h) {
    const t = new Uint8Array(w * h);
    const rx = INNER * w;
    const ry = INNER * h;
    const goldW = Math.max(1, Math.round(Math.min(rx, ry) / 13));
    const gq = 1 + goldW / Math.min(rx, ry);
    for (let y = 0; y < h; y++)
      for (let x = 0; x < w; x++) {
        const dx = x + 0.5 - w / 2;
        const dy = y + 0.5 - h / 2;
        const q = Math.hypot(dx / rx, dy / ry);
        const o = Math.pow(Math.abs(dx) / (w / 2), OUTER_POW) + Math.pow(Math.abs(dy) / (h / 2), OUTER_POW);
        t[y * w + x] = q < 1 || o > 1 ? WALL : q <= gq ? GOLD : OPEN;
      }
    // Drop wall tiles that stick out (at most one wall neighbour; the map edge counts as wall).
    /** @param {number} x @param {number} y */
    const wall = (x, y) => x < 0 || y < 0 || x >= w || y >= h || t[y * w + x] === WALL;
    const drop = [];
    for (let y = 0; y < h; y++)
      for (let x = 0; x < w; x++)
        if (t[y * w + x] === WALL) {
          const n = +wall(x + 1, y) + +wall(x - 1, y) + +wall(x, y + 1) + +wall(x, y - 1);
          if (n <= 1) drop.push(y * w + x);
        }
    for (const i of drop) t[i] = OPEN;
    return t;
  },
};
