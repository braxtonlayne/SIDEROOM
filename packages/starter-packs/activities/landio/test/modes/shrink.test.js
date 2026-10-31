// @ts-check
// Shrinking Arena: the border closes in steps, tiles outside become walls, heads outside die.
import assert from 'node:assert/strict';
import { RESULTS_TICKS, createState, findSpawn, spawnPlayerAt, step, stateHash, takeEvents } from '../../shared/rules.js';
import { WALL, setBase } from '../../shared/grid.js';
import { MODE_BY_ID } from '../../shared/modes/index.js';
import { FIRST_STEP, STEP_EVERY, WARN_TICKS, geometry, zoneAt } from '../../shared/modes/shrink.js';

/** @typedef {import('../../shared/types.js').GameState} GameState */

const arena = (w = 100, h = 100) => createState({ w, h, targetHeads: 0, mode: 'shrink' }, 'shrink');
/** @param {GameState} s */
const zone = (s) => /** @type {number[]} */ (s.md.zone);
/** @param {GameState} s @param {number} x @param {number} y */
const walled = (s, x, y) => s.terrain[y * s.cfg.w + x] === WALL;

test('nothing closes in the first 30 seconds', () => {
  const s = arena();
  step(s, [], FIRST_STEP - 1);
  assert.deepEqual(zone(s), [0, 0, 100, 100]);
  assert.equal(s.md.k, 0);
  assert.ok(s.terrain.every((t) => t !== WALL));
});

test('the border steps in every 20 s after that, centred on the map', () => {
  const s = arena();
  assert.equal(geometry(100, 100).sx, 4);
  step(s, [], FIRST_STEP);
  assert.deepEqual(zone(s), [4, 4, 96, 96]);
  step(s, [], STEP_EVERY - 1);
  assert.equal(s.md.k, 1);
  step(s);
  assert.deepEqual(zone(s), [8, 8, 92, 92]);
  assert.equal(s.md.at, FIRST_STEP + 2 * STEP_EVERY);
});

test('tiles outside the new border become walls and lose their owners', () => {
  const s = arena();
  const a = spawnPlayerAt(s, { kind: 'human', name: 'A' }, 50, 50);
  const b = spawnPlayerAt(s, { kind: 'human', name: 'B' }, 20, 20);
  setBase(s, 1 * 100 + 50, b.id);
  setBase(s, 2 * 100 + 50, b.id);
  const landBefore = b.land;
  step(s, [], FIRST_STEP);
  assert.equal(walled(s, 50, 1), true);
  assert.equal(walled(s, 50, 3), true);
  assert.equal(walled(s, 50, 4), false);
  assert.equal(walled(s, 0, 0), true);
  assert.equal(s.base[1 * 100 + 50], 0);
  assert.equal(b.land, landBefore - 2);
  assert.equal(a.alive && b.alive, true);
  let n = 0;
  for (let i = 0; i < s.terrain.length; i++) if (s.terrain[i] === WALL) n++;
  assert.equal(n, 100 * 100 - 92 * 92);
});

test('a trail cut by the closing is cleared, its owner lives', () => {
  const s = arena();
  const a = spawnPlayerAt(s, { kind: 'human', name: 'A' }, 50, 50);
  const t = [3 * 100 + 50, 4 * 100 + 50, 5 * 100 + 50, 6 * 100 + 50];
  a.x = 50;
  a.y = 6;
  a.trail = t.slice();
  for (const i of t) s.trail[i] = a.id;
  step(s, [], FIRST_STEP);
  assert.equal(a.alive, true);
  assert.equal(s.trail[3 * 100 + 50], 0);
  assert.equal(a.trail.includes(3 * 100 + 50), false);
  assert.ok(a.trail.includes(6 * 100 + 50));
});

test('a head outside the new border dies, cause wall', () => {
  const s = arena();
  const a = spawnPlayerAt(s, { kind: 'human', name: 'A' }, 2, 50);
  const b = spawnPlayerAt(s, { kind: 'human', name: 'B' }, 50, 50);
  step(s, [], FIRST_STEP);
  assert.equal(a.alive, false);
  assert.equal(a.death?.cause, 'wall');
  assert.equal(b.alive, true);
});

test('the warning data appears 35 ticks before each step', () => {
  const s = arena();
  step(s, [], FIRST_STEP - WARN_TICKS - 1);
  assert.equal(s.md.next, undefined);
  assert.equal(s.md.at, FIRST_STEP);
  step(s);
  assert.deepEqual(s.md.next, [4, 4, 96, 96]);
  assert.deepEqual(zone(s), [0, 0, 100, 100], 'announced, not yet closed');
  assert.equal(walled(s, 0, 0), false);
  step(s, [], WARN_TICKS);
  assert.equal(s.md.next, undefined, 'the warning is cleared once the step happens');
  step(s, [], STEP_EVERY - WARN_TICKS);
  assert.deepEqual(s.md.next, [8, 8, 92, 92]);
});

test('the final zone is about 30% of the map and the border stops there', () => {
  const s = arena(100, 80);
  const g = geometry(100, 80);
  step(s, [], FIRST_STEP + (g.steps - 1) * STEP_EVERY);
  assert.equal(s.md.k, g.steps);
  assert.equal(s.md.at, undefined, 'no more steps');
  const z = zone(s);
  assert.equal(z[2] - z[0], 30);
  assert.equal(z[3] - z[1], 24);
  assert.ok(FIRST_STEP + (g.steps - 1) * STEP_EVERY <= 3.6 * 60 * 7, 'done by about 3:30');
  const before = s.terrain.slice();
  step(s, [], 150);
  assert.deepEqual(zone(s), z);
  assert.deepEqual(s.terrain, before);
  for (const [w, h] of [[40, 40], [200, 200], [60, 120]]) {
    const gg = geometry(w, h);
    const zz = zoneAt(w, h, gg.steps);
    assert.ok(Math.abs((zz[2] - zz[0]) / w - 0.3) < 0.05, `${w}x${h} width`);
    assert.ok(Math.abs((zz[3] - zz[1]) / h - 0.3) < 0.05, `${w}x${h} height`);
    assert.equal(zoneAt(w, h, gg.steps + 5).join(), zz.join());
  }
});

test('new players spawn inside the open area', () => {
  const s = arena();
  step(s, [], FIRST_STEP + 3 * STEP_EVERY);
  const z = zone(s);
  for (let k = 0; k < 40; k++) {
    const at = findSpawn(s);
    assert.ok(at, 'a spot is found');
    assert.ok(at.x >= z[0] && at.x < z[2] && at.y >= z[1] && at.y < z[3], `${at.x},${at.y}`);
    spawnPlayerAt(s, { kind: 'human', name: `P${k}` }, at.x, at.y);
  }
});

test('bot goal: the centre when near the next border, else none', () => {
  const s = arena();
  const p = spawnPlayerAt(s, { kind: 'bot', name: 'B' }, 50, 50);
  const goal = MODE_BY_ID.shrink.botGoal;
  assert.ok(goal);
  assert.equal(goal(s, p), null);
  p.x = 6;
  assert.deepEqual(goal(s, p), { x: 50, y: 50 });
  p.x = 2;
  assert.deepEqual(goal(s, p), { x: 50, y: 50 });
});

test('bot game: rounds advance, winners are sensible, bots rarely die to the border, deterministic', () => {
  /** @param {string} seed */
  const run = (seed) => {
    const s = createState({ w: 80, h: 80, targetHeads: 10, mode: 'shrink' }, seed, { fill: true });
    const rounds = new Set([1]);
    const winners = [];
    let deaths = 0;
    let wallDeaths = 0;
    const total = 2 * (4 * 60 * 7 + RESULTS_TICKS) + 5;
    let lastPhase = s.round.phase;
    for (let t = 0; t < total; t++) {
      step(s);
      rounds.add(s.round.n);
      if (s.round.phase === 'over' && lastPhase !== 'over') {
        const r = s.round.result;
        assert.ok(r && r.players.length > 0 && r.winner === r.players[0].id);
        winners.push(r.winner);
      }
      lastPhase = s.round.phase;
      for (const e of takeEvents(s))
        if (e.k === 'kill') {
          deaths++;
          if (e.c === 'wall') wallDeaths++;
        }
      // Invariants: nothing outside the zone is anything but wall, and no living head is outside it.
      if (s.round.phase === 'play' && t % 20 === 0) {
        const z = zone(s);
        const { w } = s.cfg;
        for (let i = 0; i < s.terrain.length; i++) {
          const x = i % w;
          const y = (i / w) | 0;
          if (x < z[0] || x >= z[2] || y < z[1] || y >= z[3]) {
            assert.equal(s.terrain[i], WALL);
            assert.equal(s.base[i] | s.trail[i], 0);
          }
        }
        for (const p of s.players) if (p.alive) assert.ok(p.x >= z[0] && p.x < z[2] && p.y >= z[1] && p.y < z[3]);
      }
    }
    return { rounds, winners, deaths, wallDeaths, hash: stateHash(s) };
  };
  const a = run('trial');
  assert.ok(a.rounds.size >= 3, `rounds ${[...a.rounds]}`);
  assert.ok(a.winners.length >= 2);
  assert.ok(a.deaths > 5, `deaths ${a.deaths}`);
  assert.ok(a.wallDeaths < a.deaths * 0.25, `${a.wallDeaths} wall deaths of ${a.deaths}`);
  console.log(`bot game: rounds ${a.rounds.size}, winners ${a.winners}, deaths ${a.deaths}, wall ${a.wallDeaths}`);
  assert.equal(run('trial').hash, a.hash);
});

test('timing: the mode costs under 0.3 ms per tick at 200x200 with 30 bots', () => {
  /** @param {string} mode */
  const time = (mode) => {
    const s = createState({ w: 200, h: 200, targetHeads: 30, mode }, 'timing', { fill: true });
    const t0 = performance.now();
    step(s, [], 2000);
    return (performance.now() - t0) / 2000;
  };
  time('shrink');
  const base = time('rounds');
  const mine = time('shrink');
  console.log(`ms/tick: rounds ${base.toFixed(3)}, shrink ${mine.toFixed(3)}`);
  assert.ok(mine - base < 0.3, `${(mine - base).toFixed(3)} ms extra per tick`);
});
