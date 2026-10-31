// @ts-check
// Checks every map must pass (D-50). A map test calls checkMap(def) for the shared rules, then tests what is
// special about its own layout.
import assert from 'node:assert/strict';
import { createState, step } from '../../shared/rules.js';
import { OPEN, WALL, GOLD } from '../../shared/grid.js';
import { buildTerrain } from '../../shared/maps/index.js';
import { rand, seedFrom } from '../../shared/rng.js';

/** @typedef {import('../../shared/maps/index.js').MapDef} MapDef */

/** Arena sizes a map must work at, with the bot count an arena of that size runs. */
export const SIZES = [
  { w: 40, h: 40, heads: 4 },
  { w: 80, h: 80, heads: 6 },
  { w: 100, h: 100, heads: 12 },
  { w: 200, h: 200, heads: 30 },
];
export const SEEDS = ['alpha', 'bravo', 'charlie'];

/** Build `def` directly (not through buildTerrain, which hides a broken map behind an open field). @param {MapDef} def @param {number} w @param {number} h @param {string} seed */
export function build(def, w, h, seed) {
  // The same seeded stream buildTerrain uses.
  const st = { rng: seedFrom(`${seed}|map:${def.id}`) };
  return def.build(w, h, () => rand(st));
}

/** Share of tiles with terrain `v`. @param {Uint8Array} t @param {number} v */
export function share(t, v) {
  let n = 0;
  for (const x of t) if (x === v) n++;
  return n / t.length;
}

/** Number of 4-connected regions of non-wall tiles. @param {Uint8Array} t @param {number} w @param {number} h */
export function openRegions(t, w, h) {
  const seen = new Uint8Array(w * h);
  const q = new Int32Array(w * h);
  let regions = 0;
  for (let s = 0; s < w * h; s++) {
    if (seen[s] || t[s] === WALL) continue;
    regions++;
    let a = 0;
    let b = 0;
    q[b++] = s;
    seen[s] = 1;
    while (a < b) {
      const i = q[a++];
      const x = i % w;
      const y = (i - x) / w;
      for (const [nx, ny] of [[x + 1, y], [x - 1, y], [x, y + 1], [x, y - 1]]) {
        if (nx < 0 || ny < 0 || nx >= w || ny >= h) continue;
        const j = ny * w + nx;
        if (seen[j] || t[j] === WALL) continue;
        seen[j] = 1;
        q[b++] = j;
      }
    }
  }
  return regions;
}

/**
 * The narrowest open passage, measured as the smallest run of open tiles between walls (or the map edge) along
 * a row or a column, over runs that touch a wall on both ends. A 1-wide gap is a head-on death trap.
 * @param {Uint8Array} t @param {number} w @param {number} h
 */
export function narrowestGap(t, w, h) {
  let best = Infinity;
  /** @param {(k: number) => number} at @param {number} n */
  const scan = (at, n) => {
    let run = 0;
    let wallBefore = true;
    for (let k = 0; k <= n; k++) {
      const wall = k === n || t[at(k)] === WALL;
      if (wall) {
        if (run > 0 && wallBefore && k < n) best = Math.min(best, run);
        run = 0;
        wallBefore = true;
      } else run++;
    }
  };
  for (let y = 0; y < h; y++) scan((k) => y * w + k, w);
  for (let x = 0; x < w; x++) scan((k) => k * w + x, h);
  return best;
}

/**
 * The shared rules, at every size and seed:
 * - the right length and only OPEN / WALL / GOLD;
 * - deterministic: one seed, one map;
 * - walls at most `maxWall` of the map (default 40%);
 * - every open tile reachable from every other (one region): nobody spawns in a sealed pocket;
 * - no passage between walls narrower than `minGap` tiles (default 3), counted in rows and columns;
 * - an arena of that size fills with its bots (there is room to spawn them).
 * @param {MapDef} def @param {{maxWall?: number, minGap?: number}} [opt]
 */
export function checkMap(def, opt = {}) {
  const { maxWall = 0.4, minGap = 3 } = opt;
  assert.match(def.id, /^[a-z]+$/, 'the id is lower-case letters');
  assert.ok(def.name && def.blurb, 'a name and a blurb');
  for (const { w, h, heads } of SIZES)
    for (const seed of SEEDS) {
      const where = `${def.id} ${w}x${h} seed ${seed}`;
      const t = build(def, w, h, seed);
      assert.ok(t instanceof Uint8Array, `${where}: a Uint8Array`);
      assert.equal(t.length, w * h, `${where}: w*h tiles`);
      assert.ok(t.every((v) => v === OPEN || v === WALL || v === GOLD), `${where}: only OPEN, WALL, GOLD`);
      assert.deepEqual(build(def, w, h, seed), t, `${where}: deterministic`);
      assert.ok(share(t, WALL) <= maxWall, `${where}: walls ${(share(t, WALL) * 100).toFixed(1)}% > ${maxWall * 100}%`);
      assert.equal(openRegions(t, w, h), 1, `${where}: all open ground is connected`);
      const gap = narrowestGap(t, w, h);
      assert.ok(gap >= minGap, `${where}: a ${gap}-tile gap (min ${minGap})`);
      const s = createState({ w, h, targetHeads: heads, map: def.id }, seed, { fill: true });
      assert.equal(s.players.length, heads, `${where}: room for ${heads} bots`);
      assert.deepEqual([...s.terrain], [...buildTerrain(def.id, w, h, seed)], `${where}: the arena uses the map`);
    }
}

/**
 * Bots (with cards) play `ticks` ticks on the map. Nobody may stand on, own or trail a wall; returns counts so a
 * test can bound the wall deaths.
 * @param {string} id @param {{w?: number, h?: number, heads?: number, ticks?: number, seed?: string}} [opt]
 */
export function botTrial(id, opt = {}) {
  const { w = 100, h = 100, heads = 12, ticks = 3000, seed = 'trial' } = opt;
  const s = createState({ w, h, targetHeads: heads, map: id }, seed, { fill: true });
  let deaths = 0;
  let wallDeaths = 0;
  let captured = 0;
  for (let k = 0; k < ticks; k++) {
    step(s);
    for (const e of s.events) {
      if (e.k === 'kill' && e.c !== 'removed') {
        deaths++;
        if (e.c === 'wall') wallDeaths++;
      }
      if (e.k === 'capture') captured += e.n ?? 0;
    }
    s.events = [];
    for (const p of s.players) if (p.alive) assert.notEqual(s.terrain[p.y * w + p.x], WALL, `${p.name} stands on a wall`);
  }
  for (let i = 0; i < w * h; i++) if (s.terrain[i] === WALL) assert.equal(s.base[i] | s.trail[i], 0, 'land or trail on a wall');
  return { deaths, wallDeaths, captured, state: s };
}
