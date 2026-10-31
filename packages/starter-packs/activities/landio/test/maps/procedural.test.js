// @ts-check
// Procedural: the shared map rules, plus what makes it procedural: seeds differ, every map is mirror or point
// symmetric, walls are 10-30%, the edge margin is clear, wall blobs are chunky, and gold comes and goes by seed.
import assert from 'node:assert/strict';
import procedural from '../../shared/maps/procedural.js';
import { WALL, GOLD } from '../../shared/grid.js';
import { build, botTrial, checkMap, share } from './helpers.js';

const SIZES = [
  [40, 40],
  [100, 100],
  [60, 50],
  [200, 200],
];
const SEEDS = ['alpha', 'bravo', 'charlie', 'delta', 'echo', 'foxtrot', 'golf', 'hotel'];

test('procedural: passes the shared map rules', () => {
  checkMap(procedural);
});

test('procedural: botTrial keeps bots off the walls', () => {
  const r = botTrial('procedural', { w: 100, h: 100, heads: 12, ticks: 3000 });
  assert.ok(r.deaths > 0 && r.captured > 0, `deaths ${r.deaths}, captured ${r.captured}`);
  assert.ok(r.wallDeaths <= r.deaths * 0.05, `wall deaths ${r.wallDeaths} of ${r.deaths}`);
});

test('procedural: different seeds give different maps', () => {
  for (const [w, h] of SIZES) {
    const keys = new Set(SEEDS.map((s) => build(procedural, w, h, s).join('')));
    assert.equal(keys.size, SEEDS.length, `${w}x${h}: every seed differs`);
  }
});

/** @param {Uint8Array} t @param {number} w @param {number} h @param {(x: number, y: number) => [number, number]} f */
const symmetric = (t, w, h, f) => {
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const [mx, my] = f(x, y);
      if (t[y * w + x] !== t[my * w + mx]) return false;
    }
  return true;
};

test('procedural: every map is mirrored (left-right, top-bottom or point)', () => {
  for (const [w, h] of SIZES)
    for (const s of SEEDS) {
      const t = build(procedural, w, h, s);
      const ok =
        symmetric(t, w, h, (x, y) => [w - 1 - x, y]) ||
        symmetric(t, w, h, (x, y) => [x, h - 1 - y]) ||
        symmetric(t, w, h, (x, y) => [w - 1 - x, h - 1 - y]);
      assert.ok(ok, `${w}x${h} ${s}: some symmetry`);
    }
});

test('procedural: wall share 10-30%, clear edge margin, chunky wall pieces', () => {
  for (const [w, h] of SIZES)
    for (const s of SEEDS) {
      const t = build(procedural, w, h, s);
      const where = `${w}x${h} ${s}`;
      const wall = share(t, WALL);
      assert.ok(wall >= 0.1 && wall <= 0.3, `${where}: walls ${(wall * 100).toFixed(1)}%`);
      for (let y = 0; y < h; y++)
        for (let x = 0; x < w; x++)
          if (x < 2 || y < 2 || x >= w - 2 || y >= h - 2) assert.notEqual(t[y * w + x], WALL, `${where}: wall at ${x},${y} by the edge`);
      // Wall pieces (4-connected) have at least 9 tiles.
      const seen = new Uint8Array(w * h);
      for (let i = 0; i < w * h; i++) {
        if (seen[i] || t[i] !== WALL) continue;
        const stack = [i];
        seen[i] = 1;
        let size = 0;
        while (stack.length) {
          const j = /** @type {number} */ (stack.pop());
          size++;
          const x = j % w;
          for (const k of [x + 1 < w ? j + 1 : -1, x > 0 ? j - 1 : -1, j + w, j - w]) {
            if (k < 0 || k >= w * h || seen[k] || t[k] !== WALL) continue;
            seen[k] = 1;
            stack.push(k);
          }
        }
        assert.ok(size >= 9, `${where}: a wall piece of ${size} tiles`);
      }
    }
});

test('procedural: gold appears in some seeds and not in others (2-8% when present)', () => {
  let withGold = 0;
  let without = 0;
  for (let k = 0; k < 10; k++) {
    const g = share(build(procedural, 100, 100, `gold${k}`), GOLD);
    if (g > 0) {
      withGold++;
      assert.ok(g >= 0.015 && g <= 0.085, `seed gold${k}: gold ${(g * 100).toFixed(1)}%`);
    } else without++;
  }
  assert.ok(withGold >= 3, `${withGold} seeds with gold`);
  assert.ok(without >= 3, `${without} seeds without gold`);
});

test('procedural: generation is fast at 200x200', () => {
  const t0 = performance.now();
  build(procedural, 200, 200, 'speed');
  assert.ok(performance.now() - t0 < 200);
});
