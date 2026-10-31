// @ts-check
// Maze Ruins: the wreck of a maze. A coarse cell grid (cells about 10-14 tiles) gets a randomized depth-first maze;
// every wall between two cells the maze did NOT open is drawn 2 tiles thick (no outer frame). The maze is then
// ruined: about half of the wall segments are removed whole, the survivors get 6+ tile gaps punched in them, and
// 2-4 plazas (about 2 cells across) are swept clear so there is room to build big. Finally walls are only ever
// removed: pieces over 80 tiles are cut up, specks under 6 tiles go, narrow gaps (<3) are opened, and any sealed
// pocket is opened. What is left is a field of L, T and short straight pieces. No gold.
import { OPEN, WALL } from '../grid.js';

const MIN_PIECE = 6;
const MAX_PIECE = 80;
const KEEP = 0.66; // share of closed wall segments that survive the ruin

/** @type {import('./index.js').MapDef} */
export default {
  id: 'maze',
  name: 'Maze Ruins',
  blurb: 'Broken walls everywhere: short loops, tight escapes.',
  build(w, h, rand) {
    const t = new Uint8Array(w * h);
    const nx = Math.max(3, Math.round(w / 11));
    const ny = Math.max(3, Math.round(h / 11));
    const cw = w / nx;
    const ch = h / ny;
    const ci = (/** @type {number} */ i) => Math.round(i * cw);
    const cj = (/** @type {number} */ j) => Math.round(j * ch);

    // Randomized depth-first maze. vOpen[j][i]: passage between cell (i,j) and (i+1,j); hOpen[j][i]: (i,j)-(i,j+1).
    const vOpen = Array.from({ length: ny }, () => new Uint8Array(nx));
    const hOpen = Array.from({ length: ny }, () => new Uint8Array(nx));
    const seen = new Uint8Array(nx * ny);
    const stack = [0];
    seen[0] = 1;
    while (stack.length) {
      const c = stack[stack.length - 1];
      const i = c % nx;
      const j = (c - i) / nx;
      /** @type {number[][]} */
      const opts = [];
      if (i + 1 < nx && !seen[c + 1]) opts.push([i + 1, j, 0]);
      if (i > 0 && !seen[c - 1]) opts.push([i - 1, j, 1]);
      if (j + 1 < ny && !seen[c + nx]) opts.push([i, j + 1, 2]);
      if (j > 0 && !seen[c - nx]) opts.push([i, j - 1, 3]);
      if (!opts.length) {
        stack.pop();
        continue;
      }
      const [a, b, d] = opts[Math.floor(rand() * opts.length)];
      if (rand() < 0.5) {
        // an unopened (kept) wall: the maze is only half carved, so more ruins survive
      } else if (d === 0) vOpen[j][i] = 1;
      else if (d === 1) vOpen[j][a] = 1;
      else if (d === 2) hOpen[j][i] = 1;
      else hOpen[b][i] = 1;
      seen[b * nx + a] = 1;
      stack.push(b * nx + a);
    }

    /** @param {number} x0 @param {number} y0 @param {number} x1 @param {number} y1 @param {number} v */
    const paint = (x0, y0, x1, y1, v) => {
      if (v === WALL) (x0 = Math.max(x0, 3)), (y0 = Math.max(y0, 3)), (x1 = Math.min(x1, w - 3)), (y1 = Math.min(y1, h - 3));
      for (let y = Math.max(0, y0); y < Math.min(h, y1); y++)
        for (let x = Math.max(0, x0); x < Math.min(w, x1); x++) t[y * w + x] = v;
    };

    // Closed walls as segments (2 thick), each kept with probability KEEP; long kept ones get a 6+ tile gap.
    /** @param {boolean} vertical @param {number} line @param {number} from @param {number} to */
    const segment = (vertical, line, from, to) => {
      if (rand() > KEEP) return;
      const len = to - from;
      /** @type {number[][]} */
      const parts = [];
      if (len >= 16 && rand() < 0.55) {
        const gl = 6 + Math.floor(rand() * 4);
        const gs = from + 3 + Math.floor(rand() * Math.max(1, len - gl - 6));
        parts.push([from, gs], [gs + gl, to]);
      } else parts.push([from, to]);
      for (const [a, b] of parts) {
        if (b - a < 3) continue;
        if (vertical) paint(line - 1, a, line + 1, b, WALL);
        else paint(a, line - 1, b, line + 1, WALL);
      }
    };
    for (let j = 0; j < ny; j++)
      for (let i = 0; i + 1 < nx; i++) if (!vOpen[j][i]) segment(true, ci(i + 1), cj(j) - 1, cj(j + 1) + 1);
    for (let j = 0; j + 1 < ny; j++)
      for (let i = 0; i < nx; i++) if (!hOpen[j][i]) segment(false, cj(j + 1), ci(i) - 1, ci(i + 1) + 1);

    // Plazas: 2-4 cleared squares about 2 cells across, spread apart.
    const plazas = w * h < 3600 ? 1 : Math.max(2, Math.min(4, Math.round((w * h) / 9000) + 2));
    const pw = Math.min(Math.ceil(2 * cw), w - 8);
    const ph = Math.min(Math.ceil(2 * ch), h - 8);
    /** @type {number[][]} */
    const spots = [];
    for (let k = 0; k < plazas; k++) {
      let best = [0, 0];
      let bestD = -1;
      for (let tries = 0; tries < 8; tries++) {
        const px = Math.floor(rand() * (w - pw + 1));
        const py = Math.floor(rand() * (h - ph + 1));
        let d = Infinity;
        for (const [sx, sy] of spots) d = Math.min(d, Math.hypot(sx - px, sy - py));
        if (d > bestD) {
          bestD = d;
          best = [px, py];
        }
      }
      spots.push(best);
      paint(best[0], best[1], best[0] + pw, best[1] + ph, OPEN);
    }

    cleanup(t, w, h);
    return t;
  },
};

/** Remove wall tiles until pieces are 6..80 tiles, every gap is 3+, and the open ground is one region. */
function cleanup(/** @type {Uint8Array} */ t, /** @type {number} */ w, /** @type {number} */ h) {
  for (let round = 0; round < 80; round++) {
    let changed = false;
    // Pieces: label 4-connected wall components.
    const comp = new Uint8Array(w * h);
    const q = new Int32Array(w * h);
    for (let s = 0; s < w * h; s++) {
      if (t[s] !== WALL || comp[s]) continue;
      let a = 0;
      let b = 0;
      q[b++] = s;
      comp[s] = 1;
      while (a < b) {
        const i = q[a++];
        const x = i % w;
        const y = (i - x) / w;
        if (x + 1 < w && t[i + 1] === WALL && !comp[i + 1]) (comp[i + 1] = 1), (q[b++] = i + 1);
        if (x > 0 && t[i - 1] === WALL && !comp[i - 1]) (comp[i - 1] = 1), (q[b++] = i - 1);
        if (y + 1 < h && t[i + w] === WALL && !comp[i + w]) (comp[i + w] = 1), (q[b++] = i + w);
        if (y > 0 && t[i - w] === WALL && !comp[i - w]) (comp[i - w] = 1), (q[b++] = i - w);
      }
      if (b < MIN_PIECE) {
        for (let k = 0; k < b; k++) t[q[k]] = OPEN;
        changed = true;
      } else if (b > MAX_PIECE) {
        // Cut a 7x7 hole out of the piece around the middle of its BFS order.
        const m = q[b >> 1];
        const mx = m % w;
        const my = (m - mx) / w;
        for (let k = 0; k < b; k++) {
          const x = q[k] % w;
          const y = (q[k] - x) / w;
          if (Math.abs(x - mx) <= 3 && Math.abs(y - my) <= 3) t[q[k]] = OPEN;
        }
        changed = true;
      }
    }
    // Gaps under 3 between walls, in rows and columns: open them by removing the walls at both ends.
    /** @param {(k: number) => number} at @param {number} n */
    const gaps = (at, n) => {
      let run = 0;
      let start = 0;
      for (let k = 0; k <= n; k++) {
        const wall = k === n || t[at(k)] === WALL;
        if (wall) {
          if (run > 0 && run < 3 && k < n) {
            t[at(k)] = OPEN;
            if (start > 0) t[at(start - 1)] = OPEN;
            changed = true;
          }
          run = 0;
          start = k + 1;
        } else run++;
      }
    };
    for (let y = 0; y < h; y++) gaps((k) => y * w + k, w);
    for (let x = 0; x < w; x++) gaps((k) => k * w + x, h);
    // Sealed pockets: open the walls around every region but the largest.
    const reg = new Int32Array(w * h).fill(-1);
    /** @type {number[][]} */
    const regions = [];
    for (let s = 0; s < w * h; s++) {
      if (t[s] === WALL || reg[s] >= 0) continue;
      const id = regions.length;
      const cells = [s];
      reg[s] = id;
      for (let a = 0; a < cells.length; a++) {
        const i = cells[a];
        const x = i % w;
        const y = (i - x) / w;
        for (const j of [x + 1 < w ? i + 1 : -1, x > 0 ? i - 1 : -1, y + 1 < h ? i + w : -1, y > 0 ? i - w : -1])
          if (j >= 0 && t[j] !== WALL && reg[j] < 0) (reg[j] = id), cells.push(j);
      }
      regions.push(cells);
    }
    if (regions.length > 1) {
      let big = 0;
      for (let r = 1; r < regions.length; r++) if (regions[r].length > regions[big].length) big = r;
      for (let r = 0; r < regions.length; r++) {
        if (r === big) continue;
        for (const i of regions[r]) {
          const x = i % w;
          const y = (i - x) / w;
          for (let dy = -1; dy <= 1; dy++)
            for (let dx = -1; dx <= 1; dx++) {
              const px = x + dx;
              const py = y + dy;
              if (px >= 0 && py >= 0 && px < w && py < h) t[py * w + px] = OPEN;
            }
        }
      }
      changed = true;
    }
    if (!changed) return;
  }
}
