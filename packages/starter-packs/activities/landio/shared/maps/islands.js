// @ts-check
// Islands: a grid of islands (2x2 below 70 tiles, 3x3 from 70, 4x4 from 160, counted per axis) split by water
// channels (WALL). Each channel is 3-5 tiles wide and meanders a little (two slow sines with seeded phases, so the
// shores curve instead of running ruler-straight). Every pair of neighbouring islands is joined by a bridge: a
// 4-6 tile gap in the channel at a random point along the shared edge, kept clear of the island corners (long
// edges get a second bridge). The islands reach the map edge: only channels are water. No gold.
import { OPEN, WALL } from '../grid.js';

/** @typedef {{axis: 'v' | 'h', ch: number, a: number, len: number}} Bridge */

/** Grid cells for a side of `n` tiles. @param {number} n */
const cells = (n) => (n < 70 ? 2 : n < 160 ? 3 : 4);

/**
 * The plan for a w x h map: grid size, channel centres and wobble, and bridge positions.
 * Vertical channel i (axis 'v') sits at x = vx[i] + vOff[i][y]; a bridge of it spans rows a..a+len-1.
 * Horizontal channel j (axis 'h') sits at y = hy[j] + hOff[j][x]; a bridge spans columns a..a+len-1.
 * @param {number} w @param {number} h @param {() => number} rand
 */
export function layout(w, h, rand) {
  const nx = cells(w);
  const ny = cells(h);
  const s = Math.min(w, h);
  const cw = s < 60 ? 3 : s < 100 ? 4 : 5;
  const bw = Math.min(6, 4 + Math.floor(s / 80));
  const amp = Math.min(4, 1 + Math.floor(s / 50));
  /** @param {number} len */
  const wobble = (len) => {
    const p1 = rand() * 6.28;
    const p2 = rand() * 6.28;
    const l1 = 60 + rand() * 40;
    const l2 = 30 + rand() * 15;
    const out = [];
    for (let k = 0; k < len; k++)
      out.push(Math.round(amp * (0.6 * Math.sin((k * 6.283) / l1 + p1) + 0.4 * Math.sin((k * 6.283) / l2 + p2))));
    return out;
  };
  /** @type {number[]} */
  const vx = [];
  /** @type {number[]} */
  const hy = [];
  for (let i = 1; i < nx; i++) vx.push(Math.round((i * w) / nx));
  for (let j = 1; j < ny; j++) hy.push(Math.round((j * h) / ny));
  const vOff = vx.map(() => wobble(h));
  const hOff = hy.map(() => wobble(w));
  /** @type {Bridge[]} */
  const bridges = [];
  const clear = Math.floor(cw / 2) + amp + 3;
  /**
   * Bridges for channel `ch` along an axis of length `len`, between the crossing channels `cross`.
   * @param {'v'|'h'} axis @param {number} ch @param {number} len @param {number[]} cross
   */
  const place = (axis, ch, len, cross) => {
    for (let j = 0; j <= cross.length; j++) {
      const lo = j === 0 ? 3 : cross[j - 1] + clear;
      const hi = j === cross.length ? len - 3 : cross[j] - clear;
      const room = hi - lo;
      const ranges = room >= 40 ? [[lo, lo + (room >> 1)], [lo + (room >> 1), hi]] : [[lo, hi]];
      for (const [a, b] of ranges) {
        const m = Math.floor((b - a) * 0.12);
        const from = a + m;
        const span = b - m - bw - from;
        bridges.push({ axis, ch, a: from + Math.floor(rand() * (span + 1)), len: bw });
      }
    }
  };
  for (let i = 0; i < vx.length; i++) place('v', i, h, hy);
  for (let j = 0; j < hy.length; j++) place('h', j, w, vx);
  return { nx, ny, cw, bw, amp, vx, hy, vOff, hOff, bridges };
}

/** @type {import('./index.js').MapDef} */
export default {
  id: 'islands',
  name: 'Islands',
  blurb: 'Water channels split the map; narrow bridges join the islands.',
  build(w, h, rand) {
    const t = new Uint8Array(w * h);
    const L = layout(w, h, rand);
    const half = Math.floor(L.cw / 2);
    for (let i = 0; i < L.vx.length; i++)
      for (let y = 0; y < h; y++) {
        const x0 = L.vx[i] + L.vOff[i][y] - half;
        for (let x = x0; x < x0 + L.cw; x++) if (x >= 0 && x < w) t[y * w + x] = WALL;
      }
    for (let j = 0; j < L.hy.length; j++)
      for (let x = 0; x < w; x++) {
        const y0 = L.hy[j] + L.hOff[j][x] - half;
        for (let y = y0; y < y0 + L.cw; y++) if (y >= 0 && y < h) t[y * w + x] = WALL;
      }
    const reach = half + L.amp + 1;
    for (const b of L.bridges) {
      if (b.axis === 'v') {
        const c = L.vx[b.ch];
        for (let y = b.a; y < b.a + b.len; y++)
          for (let x = Math.max(0, c - reach); x <= Math.min(w - 1, c + reach); x++) t[y * w + x] = OPEN;
      } else {
        const c = L.hy[b.ch];
        for (let x = b.a; x < b.a + b.len; x++)
          for (let y = Math.max(0, c - reach); y <= Math.min(h - 1, c + reach); y++) t[y * w + x] = OPEN;
      }
    }
    // Where a shore wobbles it can leave a 1-2 tile sliver of land between water in a row or column (a death
    // trap): flood those slivers.
    for (let pass = 0; pass < 4; pass++) {
      let changed = false;
      /** @param {number} n @param {(k: number) => number} at */
      const scan = (n, at) => {
        let run = 0;
        for (let k = 0; k <= n; k++) {
          if (k < n && t[at(k)] !== WALL) run++;
          else {
            if (run > 0 && run < 3 && (k === n || t[at(k)] === WALL) && (k - run === 0 || t[at(k - run - 1)] === WALL)) {
              for (let q = k - run; q < k; q++) t[at(q)] = WALL;
              changed = true;
            }
            run = 0;
          }
        }
      };
      for (let y = 0; y < h; y++) scan(w, (k) => y * w + k);
      for (let x = 0; x < w; x++) scan(h, (k) => k * w + x);
      if (!changed) break;
    }
    return t;
  },
};
