// @ts-check
// Procedural: a different fair layout for every seed.
// Generation: pick a symmetry from the seed (4-fold mirror, left/right, top/bottom or 180-degree point symmetry)
// and fill a smooth 2-octave value-noise field that reads the same at every mirrored tile. Walls are the top
// share (12-25%) of that field, with a 3-tile open margin along the edge. A clean-up loop then keeps the shared
// rules true while preserving the symmetry (every change is mirrored): wall blobs under 9 tiles go, passages
// narrower than 3 tiles are opened, and any separate open region is joined to the main one by a 3-wide carved
// corridor. In about half the seeds a second, broader noise field marks gold patches (2-8% of the tiles) on
// open ground, mirrored the same way.
import { OPEN, WALL, GOLD } from '../grid.js';

const MARGIN = 3; // open tiles along the edge (a gap to the edge also counts as a passage, so 3 not 2)
const MIN_BLOB = 9;
const MIN_GAP = 3;

/**
 * Smooth lattice value noise, two octaves.
 * @param {number} w @param {number} h @param {number} scale @param {() => number} rand
 */
function makeNoise(w, h, scale, rand) {
  const octaves = [scale, scale / 2].map((s) => {
    const lw = Math.ceil(w / s) + 3;
    const lh = Math.ceil(h / s) + 3;
    const lat = new Float32Array(lw * lh);
    for (let i = 0; i < lat.length; i++) lat[i] = rand();
    return { s, lw, lat };
  });
  return (/** @type {number} */ x, /** @type {number} */ y) => {
    let sum = 0;
    let amp = 1;
    let norm = 0;
    for (const { s, lw, lat } of octaves) {
      const fx = x / s;
      const fy = y / s;
      const ix = Math.floor(fx);
      const iy = Math.floor(fy);
      let tx = fx - ix;
      let ty = fy - iy;
      tx = tx * tx * (3 - 2 * tx);
      ty = ty * ty * (3 - 2 * ty);
      const a = lat[iy * lw + ix];
      const b = lat[iy * lw + ix + 1];
      const c = lat[(iy + 1) * lw + ix];
      const d = lat[(iy + 1) * lw + ix + 1];
      sum += amp * (a + (b - a) * tx + (c - a) * ty + (a - b - c + d) * tx * ty);
      norm += amp;
      amp *= 0.5;
    }
    return sum / norm;
  };
}

/** @type {Array<Array<(x: number, y: number, w: number, h: number) => [number, number]>>} */
const SYMMETRIES = [
  [(x, y, w) => [w - 1 - x, y], (x, y, w, h) => [x, h - 1 - y], (x, y, w, h) => [w - 1 - x, h - 1 - y]], // 4-fold
  [(x, y, w) => [w - 1 - x, y]], // left/right
  [(x, y, w, h) => [x, h - 1 - y]], // top/bottom
  [(x, y, w, h) => [w - 1 - x, h - 1 - y]], // point
];

/** @type {import('./index.js').MapDef} */
export default {
  id: 'procedural',
  name: 'Procedural',
  blurb: 'A new mirrored layout every arena.',
  build(w, h, rand) {
    const n = w * h;
    const mode = Math.min(3, Math.floor(rand() * 4));
    const maps = SYMMETRIES[mode];
    const scale = (8 + rand() * 8) * Math.min(1, Math.max(0.6, Math.min(w, h) / 100)) + 2;
    const wallNoise = makeNoise(w, h, scale, rand);
    const wantGold = rand() < 0.5;
    const goldNoise = makeNoise(w, h, scale * 1.2, rand);
    const goldShare = 0.02 + rand() * 0.06;
    let target = 0.12 + rand() * 0.13;

    /** Canonical tile (the smallest index in the mirror orbit) of each tile. */
    const canon = new Int32Array(n);
    for (let y = 0; y < h; y++)
      for (let x = 0; x < w; x++) {
        let best = y * w + x;
        for (const f of maps) {
          const [mx, my] = f(x, y, w, h);
          best = Math.min(best, my * w + mx);
        }
        canon[y * w + x] = best;
      }
    const field = new Float32Array(n);
    for (let i = 0; i < n; i++) {
      const c = canon[i];
      field[i] = wallNoise(c % w, (c - (c % w)) / w);
    }
    const inMargin = (/** @type {number} */ i) => {
      const x = i % w;
      const y = (i - x) / w;
      return x < MARGIN || y < MARGIN || x >= w - MARGIN || y >= h - MARGIN;
    };
    const interior = [];
    for (let i = 0; i < n; i++) if (!inMargin(i)) interior.push(field[i]);
    interior.sort((a, b) => a - b);

    const W = new Uint8Array(n); // 1 = wall
    const seen = new Int32Array(n);
    const queue = new Int32Array(n);
    /** Call fn for each 4-neighbour of i inside the map. @param {number} i @param {(j: number) => void} fn */
    const eachNb = (i, fn) => {
      const x = i % w;
      if (x + 1 < w) fn(i + 1);
      if (x > 0) fn(i - 1);
      if (i + w < n) fn(i + w);
      if (i - w >= 0) fn(i - w);
    };

    /** Mirror every opening into its orbit. */
    const symmetrize = () => {
      for (let i = 0; i < n; i++) {
        if (W[i]) continue;
        const x = i % w;
        const y = (i - x) / w;
        for (const f of maps) {
          const [mx, my] = f(x, y, w, h);
          W[my * w + mx] = 0;
        }
      }
    };

    /** Label connected components of tiles where W === v into `seen` (1-based); returns the count. @param {number} v */
    const label = (v) => {
      seen.fill(0);
      let count = 0;
      for (let s = 0; s < n; s++) {
        if (seen[s] || W[s] !== v) continue;
        count++;
        let a = 0;
        let b = 0;
        queue[b++] = s;
        seen[s] = count;
        while (a < b) {
          const i = queue[a++];
          eachNb(i, (j) => {
            if (!seen[j] && W[j] === v) {
              seen[j] = count;
              queue[b++] = j;
            }
          });
        }
      }
      return count;
    };

    const removeSmallBlobs = () => {
      const count = label(1);
      if (!count) return false;
      const size = new Int32Array(count + 1);
      for (let i = 0; i < n; i++) if (seen[i]) size[seen[i]]++;
      let changed = false;
      for (let i = 0; i < n; i++)
        if (seen[i] && size[seen[i]] < MIN_BLOB) {
          W[i] = 0;
          changed = true;
        }
      return changed;
    };

    const openNarrowGaps = () => {
      const mark = new Uint8Array(n);
      for (let y = 0; y < h; y++) {
        let run = 0;
        for (let x = 0; x <= w; x++) {
          if (x < w && !W[y * w + x]) run++;
          else {
            if (run > 0 && run < MIN_GAP && x < w && x - run - 1 >= 0 && W[y * w + x - run - 1]) mark[y * w + x] = 1;
            run = 0;
          }
        }
      }
      for (let x = 0; x < w; x++) {
        let run = 0;
        for (let y = 0; y <= h; y++) {
          if (y < h && !W[y * w + x]) run++;
          else {
            if (run > 0 && run < MIN_GAP && y < h && y - run - 1 >= 0 && W[(y - run - 1) * w + x]) mark[y * w + x] = 1;
            run = 0;
          }
        }
      }
      let changed = false;
      for (let i = 0; i < n; i++)
        if (mark[i]) {
          W[i] = 0;
          changed = true;
        }
      if (changed) symmetrize();
      return changed;
    };

    /** Join the smallest separate open region to its nearest other region with a 3-wide corridor. */
    const connectOne = () => {
      const count = label(0);
      if (count <= 1) return false;
      const size = new Int32Array(count + 1);
      for (let i = 0; i < n; i++) if (seen[i]) size[seen[i]]++;
      let small = 1;
      for (let r = 2; r <= count; r++) if (size[r] < size[small]) small = r;
      const lab = Int32Array.from(seen);
      const parent = new Int32Array(n).fill(-2);
      let a = 0;
      let b = 0;
      for (let i = 0; i < n; i++)
        if (lab[i] === small) {
          queue[b++] = i;
          parent[i] = -1;
        }
      let hit = -1;
      while (a < b && hit < 0) {
        const i = queue[a++];
        eachNb(i, (j) => {
          if (hit >= 0 || parent[j] !== -2) return;
          parent[j] = i;
          if (!W[j] && lab[j] !== small) hit = j;
          else queue[b++] = j;
        });
      }
      if (hit < 0) return false;
      for (let i = parent[hit]; i >= 0 && lab[i] !== small; i = parent[i]) {
        const x = i % w;
        const y = (i - x) / w;
        for (let dy = -1; dy <= 1; dy++)
          for (let dx = -1; dx <= 1; dx++) {
            const xx = x + dx;
            const yy = y + dy;
            if (xx >= 0 && yy >= 0 && xx < w && yy < h) W[yy * w + xx] = 0;
          }
      }
      symmetrize();
      return true;
    };

    const wallShare = () => {
      let c = 0;
      for (let i = 0; i < n; i++) c += W[i];
      return c / n;
    };

    for (let attempt = 0; attempt < 8; attempt++) {
      const cut = interior[Math.min(interior.length - 1, Math.floor((1 - target) * interior.length))];
      for (let i = 0; i < n; i++) W[i] = !inMargin(i) && field[i] >= cut ? 1 : 0;
      symmetrize();
      for (let guard = 0; guard < 400; guard++) {
        let changed = removeSmallBlobs();
        if (changed) symmetrize();
        changed = openNarrowGaps() || changed;
        if (!changed) changed = connectOne();
        if (!changed) break;
      }
      const s = wallShare();
      if (s >= 0.12 && s <= 0.28) break;
      target = s < 0.12 ? target + 0.05 : target - 0.05;
    }

    const t = new Uint8Array(n);
    for (let i = 0; i < n; i++) t[i] = W[i] ? WALL : OPEN;
    if (wantGold) {
      /** @param {number} i */
      const g = (i) => goldNoise(canon[i] % w, Math.floor(canon[i] / w));
      const vals = [];
      for (let i = 0; i < n; i++) if (!W[i]) vals.push(g(i));
      vals.sort((a, b) => a - b);
      const cut = vals[Math.min(vals.length - 1, Math.floor((1 - (goldShare * n) / vals.length) * vals.length))];
      for (let i = 0; i < n; i++) if (!W[i] && g(i) >= cut) t[i] = GOLD;
    }
    return t;
  },
};
