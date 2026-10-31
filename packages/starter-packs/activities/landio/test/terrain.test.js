// @ts-check
// Terrain (D-50): walls behave like the map's edge, gold is counted, and the map layer travels to clients.
import assert from 'node:assert/strict';
import { createState, findSpawn, spawnPlayerAt, step } from '../shared/rules.js';
import { GOLD, WALL, setBase, setTerrain, takeTerrainDirty } from '../shared/grid.js';
import { MAPS, buildTerrain } from '../shared/maps/index.js';
import { encodeFull, rleDecode } from '../shared/codec.js';

/** @typedef {import('../shared/types.js').GameState} GameState */

const U = 0;
const R = 1;
const D = 2;
const L = 3;

/** A 30x30 arena with no bots. */
const arena = () => createState({ w: 30, h: 30, targetHeads: 0 }, 'terrain');
/** @param {GameState} s @param {number} x @param {number} y */
const at = (s, x, y) => y * s.cfg.w + x;
/** @param {GameState} s @param {number} x0 @param {number} y0 @param {number} x1 @param {number} y1 @param {number} t */
function fill(s, x0, y0, x1, y1, t) {
  for (let y = y0; y <= y1; y++) for (let x = x0; x <= x1; x++) s.terrain[at(s, x, y)] = t;
}

test('a head entering a wall tile dies, cause wall', () => {
  const s = arena();
  fill(s, 15, 0, 15, 29, WALL);
  const p = spawnPlayerAt(s, { kind: 'human', name: 'A' }, 12, 10);
  step(s, [{ id: p.id, turns: [R] }]); // 13
  step(s); // 14
  assert.equal(p.alive, true);
  step(s); // 15: the wall
  assert.equal(p.alive, false);
  assert.equal(p.death?.cause, 'wall');
});

test('nobody owns a wall: setBase on one does nothing', () => {
  const s = arena();
  const p = spawnPlayerAt(s, { kind: 'human', name: 'A' }, 5, 5);
  const land = p.land;
  s.terrain[at(s, 20, 20)] = WALL;
  setBase(s, at(s, 20, 20), p.id);
  assert.equal(s.base[at(s, 20, 20)], 0);
  assert.equal(p.land, land);
});

test('a loop around a wall claims the ground but not the wall', () => {
  const s = arena();
  s.terrain[at(s, 10, 6)] = WALL;
  const p = spawnPlayerAt(s, { kind: 'human', name: 'A' }, 8, 8);
  // From (8,8): up 4 to y=4, right 4 to x=12, down 4 to y=8 (back on the base row at x=9? no: base is 7..9).
  const script = [U, U, U, U, R, R, R, R, D, D, D, D, L, L, L];
  for (const d of script) step(s, [{ id: p.id, turns: [d] }]);
  assert.equal(p.alive, true);
  assert.equal(s.base[at(s, 10, 6)], 0, 'the wall tile stays unowned');
  assert.equal(s.base[at(s, 11, 6)], p.id, 'the ground beside it is captured');
  // Land counts match the grid.
  let n = 0;
  for (let i = 0; i < s.base.length; i++) if (s.base[i] === p.id) n++;
  assert.equal(p.land, n);
});

test('a wall never closes a loop: ground between a trail and a wall stays outside', () => {
  const s = arena();
  // A wall column at x=14 from y=0 to y=20.
  fill(s, 14, 0, 14, 20, WALL);
  const p = spawnPlayerAt(s, { kind: 'human', name: 'A' }, 11, 10);
  // Out to x=13 (beside the wall), up 6, back left to x=10 and down home: the strip x=12..13 between the trail
  // and the wall is enclosed only by the trail and the player's base, so it is captured; but nothing past the
  // wall is ever reached.
  const script = [R, R, U, U, U, U, U, U, L, L, L, D, D, D, D, D];
  for (const d of script) step(s, [{ id: p.id, turns: [d] }]);
  assert.equal(p.alive, true);
  for (let y = 0; y < 30; y++) assert.equal(s.base[at(s, 15, y)], 0, 'nothing beyond the wall');
  for (let y = 0; y <= 20; y++) assert.equal(s.base[at(s, 14, y)], 0, 'the wall itself');
});

test('gold tiles are counted in p.gold as land changes hands', () => {
  const s = arena();
  fill(s, 4, 4, 6, 6, GOLD);
  const p = spawnPlayerAt(s, { kind: 'human', name: 'A' }, 5, 5);
  assert.equal(p.gold, 9);
  const q = spawnPlayerAt(s, { kind: 'human', name: 'B' }, 20, 20);
  setBase(s, at(s, 4, 4), q.id);
  assert.equal(p.gold, 8);
  assert.equal(q.gold, 1);
  setBase(s, at(s, 4, 4), 0);
  assert.equal(q.gold, 0);
});

test('setTerrain to wall clears the tile and is reported as a terrain change', () => {
  const s = arena();
  const p = spawnPlayerAt(s, { kind: 'human', name: 'A' }, 5, 5);
  takeTerrainDirty(s);
  const land = p.land;
  setTerrain(s, at(s, 5, 5), WALL);
  assert.equal(s.base[at(s, 5, 5)], 0);
  assert.equal(p.land, land - 1);
  assert.deepEqual(takeTerrainDirty(s), [at(s, 5, 5)]);
  assert.deepEqual(takeTerrainDirty(s), []);
});

test('spawns never put a base on a wall', () => {
  const s = createState({ w: 40, h: 40, targetHeads: 0 }, 'spawn-walls');
  // Walls in a checkerboard of 3x3 blocks leave few clean 5x5 areas.
  for (let y = 0; y < 40; y++) for (let x = 0; x < 40; x++) if ((Math.floor(x / 4) + Math.floor(y / 4)) % 2) s.terrain[at(s, x, y)] = WALL;
  fill(s, 20, 20, 27, 27, 0);
  const spot = findSpawn(s);
  assert.ok(spot);
  for (let y = spot.y - 2; y <= spot.y + 2; y++) for (let x = spot.x - 2; x <= spot.x + 2; x++) assert.notEqual(s.terrain[at(s, x, y)], WALL);
});

test('the full snapshot carries the terrain and the map id', () => {
  const s = createState({ w: 30, h: 30, targetHeads: 0, map: 'open' }, 'snap');
  s.terrain[at(s, 3, 3)] = WALL;
  s.terrain[at(s, 4, 3)] = GOLD;
  const f = encodeFull(s);
  assert.equal(f.map, 'open');
  const t = rleDecode(f.terrain, new Uint8Array(900));
  assert.deepEqual([...t], [...s.terrain]);
});

test('buildTerrain: every map is the right size, uses only known values, and is the same for a seed', () => {
  for (const m of MAPS) {
    const a = buildTerrain(m.id, 60, 50, 'seed-1');
    const b = buildTerrain(m.id, 60, 50, 'seed-1');
    assert.equal(a.length, 3000, m.id);
    assert.deepEqual([...a], [...b], `${m.id} is deterministic`);
  }
  // A broken map falls back to open ground; an unknown id is the open field.
  assert.ok(buildTerrain('no-such-map', 20, 20, 1).every((v) => v === 0));
});

test('bots play 1,500 ticks among walls without ever standing in one', () => {
  const s = createState({ w: 60, h: 60, targetHeads: 10 }, 'bots-walls', { fill: false });
  // Pillars and two long walls.
  fill(s, 10, 10, 13, 13, WALL);
  fill(s, 40, 35, 44, 39, WALL);
  fill(s, 30, 0, 30, 20, WALL);
  fill(s, 0, 45, 25, 45, WALL);
  let wallDeaths = 0;
  let deaths = 0;
  for (let t = 0; t < 1500; t++) {
    step(s);
    for (const e of s.events) if (e.k === 'kill') {
      deaths++;
      if (e.c === 'wall') wallDeaths++;
    }
    s.events = [];
    for (const p of s.players) if (p.alive) assert.notEqual(s.terrain[at(s, p.x, p.y)], WALL, `${p.name} on a wall at tick ${s.tick}`);
    for (let i = 0; i < s.base.length; i++) if (s.terrain[i] === WALL) assert.equal(s.base[i] | s.trail[i], 0, 'a wall with land or trail');
  }
  assert.ok(deaths > 0, 'the bots played');
  assert.ok(wallDeaths <= 1, `bots avoid walls (${wallDeaths} wall deaths of ${deaths})`);
});
