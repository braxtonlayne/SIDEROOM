// @ts-check
// Maze Ruins: the shared map rules, plus broken wall pieces, no frame, open plazas, more pieces on bigger maps.
import assert from 'node:assert/strict';
import maze from '../../shared/maps/maze.js';
import { WALL, GOLD } from '../../shared/grid.js';
import { botTrial, build, checkMap, share } from './helpers.js';

test('maze: passes the shared map rules', () => {
  checkMap(maze);
});

test('maze: bots play it without dying on walls', () => {
  const r = botTrial('maze', { w: 100, h: 100, heads: 12, ticks: 3000 });
  assert.ok(r.deaths > 0, 'some deaths');
  assert.ok(r.captured > 0, 'some captures');
  assert.ok(r.wallDeaths <= r.deaths * 0.05, `wall deaths ${r.wallDeaths} of ${r.deaths}`);
});

/** Sizes of the 4-connected wall pieces. @param {Uint8Array} t @param {number} w @param {number} h */
function pieces(t, w, h) {
  const seen = new Uint8Array(w * h);
  /** @type {number[]} */
  const out = [];
  for (let s = 0; s < w * h; s++) {
    if (t[s] !== WALL || seen[s]) continue;
    const q = [s];
    seen[s] = 1;
    for (let a = 0; a < q.length; a++) {
      const i = q[a];
      const x = i % w;
      for (const j of [x + 1 < w ? i + 1 : -1, x > 0 ? i - 1 : -1, i + w, i - w])
        if (j >= 0 && j < w * h && t[j] === WALL && !seen[j]) (seen[j] = 1), q.push(j);
    }
    out.push(q.length);
  }
  return out;
}

const DIMS = [[40, 40], [60, 50], [100, 100], [200, 200]];

test('maze: wall pieces are 6 to 80 tiles, walls 3-20%, no gold', () => {
  for (const [w, h] of DIMS)
    for (const seed of ['alpha', 'bravo']) {
      const t = build(maze, w, h, seed);
      const ps = pieces(t, w, h);
      assert.ok(ps.length > 0, `${w}x${h}: has walls`);
      for (const n of ps) assert.ok(n >= 6 && n <= 80, `${w}x${h} ${seed}: a piece of ${n} tiles`);
      const wall = share(t, WALL);
      assert.ok(wall >= 0.03 && wall <= 0.2, `${w}x${h} ${seed}: walls ${(wall * 100).toFixed(1)}%`);
      assert.equal(share(t, GOLD), 0);
    }
});

test('maze: no wall within 1 tile of the map edge', () => {
  for (const [w, h] of DIMS) {
    const t = build(maze, w, h, 'alpha');
    for (let y = 0; y < h; y++)
      for (let x = 0; x < w; x++)
        if (x < 2 || y < 2 || x >= w - 2 || y >= h - 2) assert.notEqual(t[y * w + x], WALL, `${w}x${h}: wall at ${x},${y}`);
  }
});

/** Count disjoint all-open squares of side `s` (greedy scan). @param {Uint8Array} t @param {number} w @param {number} h @param {number} s */
function openSquares(t, w, h, s) {
  // prefix sums of wall tiles
  const P = new Int32Array((w + 1) * (h + 1));
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++)
      P[(y + 1) * (w + 1) + x + 1] = P[y * (w + 1) + x + 1] + P[(y + 1) * (w + 1) + x] - P[y * (w + 1) + x] + (t[y * w + x] === WALL ? 1 : 0);
  /** @type {number[][]} */
  const found = [];
  for (let y = 0; y + s <= h; y++)
    for (let x = 0; x + s <= w; x++) {
      const n = P[(y + s) * (w + 1) + x + s] - P[y * (w + 1) + x + s] - P[(y + s) * (w + 1) + x] + P[y * (w + 1) + x];
      if (n === 0 && found.every(([fx, fy]) => Math.abs(fx - x) >= s || Math.abs(fy - y) >= s)) found.push([x, y]);
    }
  return found.length;
}

test('maze: open plazas about two cells across', () => {
  for (const [w, h] of DIMS)
    for (const seed of ['alpha', 'bravo']) {
      const t = build(maze, w, h, seed);
      const cell = Math.min(w, h) / Math.max(3, Math.round(Math.min(w, h) / 11));
      const side = Math.floor(2 * cell) - 2;
      const n = openSquares(t, w, h, side);
      assert.ok(n >= (w * h >= 3600 ? 2 : 1), `${w}x${h} ${seed}: ${n} open squares of side ${side}`);
    }
});

test('maze: more wall pieces on bigger maps', () => {
  const counts = [40, 80, 160].map((n) => pieces(build(maze, n, n, 'alpha'), n, n).length);
  assert.ok(counts[0] < counts[1] && counts[1] < counts[2], `piece counts ${counts}`);
});
