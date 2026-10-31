// @ts-check
// Capture by inverse flood fill (GDD §4):
//   A. the trail becomes base;
//   B. take the bounding box of the player's base, padded by one tile;
//   C. BFS from the padded box's edge, marking every tile reachable without crossing the player's base as
//      outside;
//   D. every tile in the box that is not outside becomes the player's base (enemy base included: stolen).
// The box is kept as a superset of the base (it only grows), which is still correct: its padded edge is
// always outside the territory.

import { WALL, setBase, setTrail } from './grid.js';

/** @typedef {import('./types.js').GameState} GameState */
/** @typedef {import('./types.js').Player} Player */

/** How far past its own land a loop closed against a teammate's land may reach (tiles). */
const REACH = 16;

// Scratch buffers reused between captures (sized to the largest box seen). They hold no state between calls.
let seen = new Uint8Array(0);
let queue = new Int32Array(0);

/**
 * Close `p`'s loop. Returns the tiles that became `p`'s base (trail tiles included). `keep(owner)` true means
 * that owner is a teammate: their land is never taken, and it walls in the loop like `p`'s own land (so a loop
 * may be closed against a teammate's territory). `allies` are those teammates (their land widens the box).
 * @param {GameState} s @param {Player} p @param {(owner: number) => boolean} [keep] @param {Player[]} [allies]
 * @returns {number[]}
 */
export function captureLoop(s, p, keep = () => false, allies = []) {
  const { w, h } = s.cfg;
  const gained = [];
  // A. Trail → base.
  for (const i of p.trail) {
    setTrail(s, i, 0);
    if (s.base[i] !== p.id && !(s.base[i] && keep(s.base[i]))) {
      setBase(s, i, p.id);
      gained.push(i);
    }
  }
  p.trail = [];
  if (p.bbox.length === 0) return gained;

  // B. Padded box, in box coordinates (bx, by) = (x - x0, y - y0). Tiles outside the grid are outside. With
  //    teammates, the box also covers the land of those whose land touches our box (a loop closed against a
  //    teammate always touches it); far-off teammates would only make the fill slower.
  let [minX, minY, maxX, maxY] = p.bbox;
  const [px0, py0, px1, py1] = p.bbox;
  for (const a of allies)
    if (a.bbox.length && a.bbox[0] <= px1 + 1 && a.bbox[2] >= px0 - 1 && a.bbox[1] <= py1 + 1 && a.bbox[3] >= py0 - 1) {
      minX = Math.min(minX, a.bbox[0]);
      minY = Math.min(minY, a.bbox[1]);
      maxX = Math.max(maxX, a.bbox[2]);
      maxY = Math.max(maxY, a.bbox[3]);
    }
  // ...but never more than REACH tiles past our own box: a bigger pocket is simply not taken (it errs towards
  // leaving land neutral), and the fill stays small.
  minX = Math.max(minX, px0 - REACH);
  minY = Math.max(minY, py0 - REACH);
  maxX = Math.min(maxX, px1 + REACH);
  maxY = Math.min(maxY, py1 + REACH);
  const x0 = minX - 1;
  const y0 = minY - 1;
  const bw = maxX - minX + 3;
  const bh = maxY - minY + 3;
  const n = bw * bh;
  if (seen.length < n) {
    seen = new Uint8Array(n);
    queue = new Int32Array(n);
  } else seen.fill(0, 0, n);

  const id = p.id;
  // Our land, or a teammate's: the flood from outside stops at both.
  const mine = (/** @type {number} */ bx, /** @type {number} */ by) => {
    const x = bx + x0;
    const y = by + y0;
    if (x < 0 || y < 0 || x >= w || y >= h) return false;
    const o = s.base[y * w + x];
    return o === id || (o !== 0 && keep(o));
  };

  // C. BFS from every edge tile of the padded box (they are never the player's base).
  let head = 0;
  let tail = 0;
  const push = (/** @type {number} */ bx, /** @type {number} */ by) => {
    const k = by * bw + bx;
    if (seen[k] || mine(bx, by)) return;
    seen[k] = 1;
    queue[tail++] = k;
  };
  for (let bx = 0; bx < bw; bx++) {
    push(bx, 0);
    push(bx, bh - 1);
  }
  for (let by = 1; by < bh - 1; by++) {
    push(0, by);
    push(bw - 1, by);
  }
  while (head < tail) {
    const k = queue[head++];
    const bx = k % bw;
    const by = (k - bx) / bw;
    if (bx > 0) push(bx - 1, by);
    if (bx < bw - 1) push(bx + 1, by);
    if (by > 0) push(bx, by - 1);
    if (by < bh - 1) push(bx, by + 1);
  }

  // D. Everything not reached is enclosed.
  for (let by = 1; by < bh - 1; by++) {
    const y = by + y0;
    if (y < 0 || y >= h) continue;
    for (let bx = 1; bx < bw - 1; bx++) {
      const x = bx + x0;
      if (x < 0 || x >= w || seen[by * bw + bx]) continue;
      const i = y * w + x;
      // A wall is never owned (the flood can only miss one that is walled in by this player's own land).
      if (s.base[i] !== id && s.terrain[i] !== WALL && !(s.base[i] && keep(s.base[i]))) {
        setBase(s, i, id);
        gained.push(i);
      }
    }
  }
  return gained;
}
