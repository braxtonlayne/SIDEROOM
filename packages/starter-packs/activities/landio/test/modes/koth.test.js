// @ts-check
// King of the Hill: the hill, the points, the moves.
import assert from 'node:assert/strict';
import { RESULTS_TICKS, createState, spawnPlayerAt, step, stateHash, removePlayer, killPlayer } from '../../shared/rules.js';
import { setBase, setTerrain, GOLD, WALL } from '../../shared/grid.js';
import { scoreOf, results, modeOf } from '../../shared/modes/index.js';
import koth from '../../shared/modes/koth.js';

/** @typedef {import('../../shared/types.js').GameState} GameState */

const SEC = 7;
const MOVE = 75 * 7;

/** @param {Partial<import('../../shared/types.js').Config>} [cfg] */
const arena = (cfg = {}) => createState({ w: 100, h: 100, targetHeads: 0, mode: 'koth', ...cfg }, 'hill');
/** @param {GameState} s @returns {number[]} */
const hillOf = (s) => /** @type {number[]} */ (s.md.hill);
/** Give player `id` a row of `n` tiles inside the hill, from its centre. @param {GameState} s @param {number} id @param {number} n @param {number} [dy] */
function row(s, id, n, dy = 0) {
  const [cx, cy] = hillOf(s);
  for (let k = 0; k < n; k++) setBase(s, (cy + dy) * s.cfg.w + cx + k, id);
}
/** Share of the hill's tiles that are not walls. @param {GameState} s @param {number[]} hill */
function openShare(s, hill) {
  const [cx, cy, r] = hill;
  let t = 0;
  let o = 0;
  for (let y = cy - r; y <= cy + r; y++)
    for (let x = cx - r; x <= cx + r; x++) {
      if ((x - cx) ** 2 + (y - cy) ** 2 > r * r) continue;
      t++;
      if (s.terrain[y * s.cfg.w + x] !== WALL) o++;
    }
  return o / t;
}

test('the hill sits at the centre of an open map, 12% of the shorter side wide (at least 5)', () => {
  const s = arena({ w: 100, h: 80 });
  assert.deepEqual(hillOf(s), [50, 40, 10]);
  assert.deepEqual(hillOf(arena({ w: 40, h: 40 })), [20, 20, 5]);
  assert.equal(hillOf(arena({ w: 200, h: 200 }))[2], 24);
});

test('the hill avoids a wall-heavy centre (Donut) and stays at least 70% open', () => {
  const s = arena({ map: 'donut' });
  const hill = hillOf(s);
  assert.ok(openShare(s, hill) >= 0.7);
  assert.ok(Math.hypot(hill[0] - 50, hill[1] - 50) > 5, 'moved off the void');
  for (const map of ['islands', 'hourglass', 'maze', 'goldrush', 'procedural']) {
    const t = arena({ map });
    assert.ok(openShare(t, hillOf(t)) >= 0.7, map);
  }
});

test('points: one per hill tile each second, gold tiles three', () => {
  const s = arena();
  const a = spawnPlayerAt(s, { kind: 'human', name: 'A' }, 10, 10);
  const [cx, cy] = hillOf(s);
  row(s, a.id, 10);
  setTerrain(s, cy * 100 + cx, GOLD);
  setTerrain(s, cy * 100 + cx + 1, GOLD);
  // Land outside the hill does not count.
  setBase(s, 60 * 100 + 90, a.id);
  step(s, [], SEC - 1);
  assert.equal(scoreOf(s, a), 0);
  step(s);
  assert.equal(scoreOf(s, a), 8 + 2 * 3);
  step(s, [], SEC);
  assert.equal(scoreOf(s, a), 2 * 14);
  // Two players share the hill tile by tile.
  const b = spawnPlayerAt(s, { kind: 'human', name: 'B' }, 90, 90);
  row(s, b.id, 4, 3);
  step(s, [], SEC);
  assert.equal(scoreOf(s, a), 3 * 14);
  assert.equal(scoreOf(s, b), 4);
});

test('points survive death and a respawn (humans by uid) and count in the results', () => {
  const s = arena();
  const a = spawnPlayerAt(s, { kind: 'human', name: 'A', uid: 'u-a' }, 10, 10);
  const b = spawnPlayerAt(s, { kind: 'human', name: 'B', uid: 'u-b' }, 90, 90);
  row(s, a.id, 10);
  row(s, b.id, 3, 2);
  step(s, [], SEC);
  assert.equal(scoreOf(s, a), 10);
  killPlayer(s, a, 'wall', null);
  assert.equal(a.alive, false);
  step(s, [], 2 * SEC);
  assert.equal(scoreOf(s, a), 10, 'a dead player keeps their points');
  assert.equal(scoreOf(s, b), 9);
  assert.equal(results(s).players[0].id, a.id, 'and ranks by them');
  removePlayer(s, a.id);
  step(s, [], SEC);
  const a2 = spawnPlayerAt(s, { kind: 'human', name: 'A', uid: 'u-a' }, 10, 90);
  assert.equal(scoreOf(s, a2), 10, 'a respawn continues the total');
  row(s, a2.id, 5, -3);
  step(s, [], SEC);
  assert.equal(scoreOf(s, a2), 15);
});

test('a new round starts everyone from nothing', () => {
  const s = arena();
  const a = spawnPlayerAt(s, { kind: 'human', name: 'A', uid: 'u-a' }, 10, 10);
  row(s, a.id, 10);
  step(s, [], SEC);
  assert.equal(scoreOf(s, a), 10);
  step(s, [], modeOf(s).roundTicks - SEC + RESULTS_TICKS);
  assert.equal(s.round.n, 2);
  assert.equal(scoreOf(s, a), 0);
});

test('the hill is announced 5 s ahead, then jumps at least a quarter of the shorter side away', () => {
  const s = arena({ w: 120, h: 100 });
  const first = [...hillOf(s)];
  step(s, [], MOVE - 35 - 1);
  assert.equal(s.md.nextHill, undefined);
  step(s);
  const next = /** @type {number[]} */ (s.md.nextHill);
  assert.ok(next, 'announced');
  assert.equal(s.md.moveAt, MOVE);
  assert.deepEqual(hillOf(s), first, 'not moved yet');
  assert.ok(Math.hypot(next[0] - first[0], next[1] - first[1]) >= 25);
  assert.ok(openShare(s, next) >= 0.7);
  step(s, [], 34);
  assert.deepEqual(hillOf(s), first);
  step(s);
  assert.deepEqual(hillOf(s), next);
  assert.equal(s.md.nextHill, undefined);
  assert.equal(s.md.moveAt, undefined);
  // Points follow the new hill.
  const a = spawnPlayerAt(s, { kind: 'human', name: 'A' }, 5, 5);
  row(s, a.id, 3);
  step(s, [], SEC);
  assert.equal(scoreOf(s, a), 3);
  // And again, several times, on a wall-heavy map.
  const d = arena({ map: 'maze' });
  let prev = [...hillOf(d)];
  for (let k = 1; k <= 3; k++) {
    step(d, [], MOVE);
    const h = hillOf(d);
    assert.ok(Math.hypot(h[0] - prev[0], h[1] - prev[1]) >= 25, `move ${k}`);
    assert.ok(openShare(d, h) >= 0.7);
    prev = [...h];
  }
});

test('bots head for the hill', () => {
  const s = arena();
  const bot = spawnPlayerAt(s, { kind: 'bot', name: 'Bot' }, 10, 10);
  const [cx, cy] = hillOf(s);
  assert.deepEqual(koth.botGoal?.(s, bot), { x: cx, y: cy });
});

/** Run a bot game, checking the mode's invariants every tick. @param {string} seed @param {number} ticks */
function botGame(seed, ticks) {
  const s = createState({ w: 80, h: 80, targetHeads: 10, mode: 'koth' }, seed, { fill: true });
  /** @type {import('../../shared/types.js').RoundResult[]} */
  const finished = [];
  let seenOver = 0;
  for (let t = 0; t < ticks; t++) {
    step(s);
    const hill = hillOf(s);
    assert.equal(hill[2], 10);
    assert.ok(openShare(s, hill) >= 0.7);
    const pts = /** @type {number[]} */ (s.md.pts);
    assert.equal(pts.length, /** @type {string[]} */ (s.md.keys).length);
    assert.ok(pts.length < 60);
    for (const v of pts) assert.ok(Number.isInteger(v) && v >= 0);
    if (s.round.phase === 'over' && s.round.n > seenOver && s.round.result) {
      seenOver = s.round.n;
      finished.push(s.round.result);
    }
  }
  return { s, finished };
}

test('a bot game plays rounds, scores points, crowns the top scorer, and is deterministic', () => {
  const ticks = 2 * (2100 + RESULTS_TICKS) + 50;
  const { s, finished } = botGame('trial', ticks);
  assert.ok(s.round.n >= 3, `rounds advanced (${s.round.n})`);
  assert.ok(finished.length >= 2);
  for (const r of finished) {
    const top = r.players[0];
    assert.equal(r.winner, top.id);
    assert.ok(top.score > 0, 'the top bot has points');
    for (const p of r.players) assert.ok(p.score <= top.score);
  }
  console.log(`bot game: ${finished.length} rounds, winners ${finished.map((r) => `${r.winner} (${r.players[0].score} pts)`).join(', ')}`);
  assert.equal(stateHash(botGame('trial', ticks).s), stateHash(s));
});

test('timing: the mode tick stays under 0.3 ms at 200x200 with 30 bots', () => {
  const s = createState({ w: 200, h: 200, targetHeads: 30, mode: 'koth' }, 'timing', { fill: true });
  const orig = koth.tick;
  let spent = 0;
  koth.tick = (st) => {
    const t0 = performance.now();
    orig?.(st);
    spent += performance.now() - t0;
  };
  try {
    step(s, [], 2000);
  } finally {
    koth.tick = orig;
  }
  const per = spent / 2000;
  console.log(`koth tick: ${per.toFixed(4)} ms`);
  assert.ok(per < 0.3, `${per} ms per tick`);
});
