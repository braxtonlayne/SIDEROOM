// @ts-check
// Simulation unit tests: every rule of brief §2.2, driven through step() directly.
import assert from 'node:assert/strict';
import { createState, spawnPlayer, spawnPlayerAt, step, stateHash } from '../shared/rules.js';
import { setBase as setBaseForTest } from '../shared/grid.js';

/** @typedef {import('../shared/types.js').GameState} GameState */
/** @typedef {import('../shared/types.js').Player} Player */
/** @typedef {import('../shared/types.js').Input} Input */

const U = 0;
const R = 1;
const D = 2;
const L = 3;

/** An arena with no bots. @param {number} [w] @param {number} [h] */
const arena = (w = 30, h = 30) => createState({ w, h, targetHeads: 0 }, 'test');

/** @param {GameState} s @param {string} name @param {number} x @param {number} y */
const human = (s, name, x, y) => spawnPlayerAt(s, { kind: 'human', name }, x, y);

/** Run scripts in parallel: one entry per tick per player, a direction or 'stop' or null (no input). */
function run(/** @type {GameState} */ s, /** @type {[Player, (number|'stop'|null)[]][]} */ scripts) {
  const ticks = Math.max(...scripts.map(([, sc]) => sc.length));
  for (let t = 0; t < ticks; t++) {
    /** @type {Input[]} */
    const inputs = [];
    for (const [p, sc] of scripts) {
      const a = sc[t];
      if (a === 'stop') inputs.push({ id: p.id, toggleStop: true });
      else if (typeof a === 'number') inputs.push({ id: p.id, turns: [a] });
    }
    step(s, inputs);
  }
}

const rep = (/** @type {number} */ d, /** @type {number} */ n) => Array(n).fill(d);
/** @param {GameState} s @param {number} x @param {number} y */
const baseAt = (s, x, y) => s.base[y * s.cfg.w + x];
/** @param {GameState} s @param {number} id */
const tilesOf = (s, id) => {
  let b = 0;
  let t = 0;
  for (let i = 0; i < s.base.length; i++) {
    if (s.base[i] === id) b++;
    if (s.trail[i] === id) t++;
  }
  return { b, t };
};

test('a simple loop captures the trail and what it encloses', () => {
  const s = arena(20, 20);
  const a = human(s, 'A', 5, 5);
  run(s, [[a, [...rep(U, 3), ...rep(R, 3), ...rep(D, 3), ...rep(L, 2)]]]);
  // 9 base + 9 trail + 3 enclosed ((6,4) was base already).
  assert.equal(a.land, 21);
  assert.deepEqual(tilesOf(s, a.id), { b: 21, t: 0 });
  assert.equal(a.trail.length, 0);
  for (const [x, y] of [[6, 3], [7, 3], [7, 4]]) assert.equal(baseAt(s, x, y), a.id);
  assert.equal(baseAt(s, 9, 3), 0);
});

test('a loop that uses its own base edge as a wall', () => {
  const s = arena(20, 20);
  const a = human(s, 'A', 5, 5);
  run(s, [[a, [...rep(R, 3), ...rep(U, 3), ...rep(L, 3), ...rep(D, 2)]]]);
  // 9 base + 9 trail + 3 enclosed between trail and base.
  assert.equal(a.land, 21);
  for (const [x, y] of [[6, 3], [7, 3], [7, 4]]) assert.equal(baseAt(s, x, y), a.id);
  assert.deepEqual(tilesOf(s, a.id), { b: 21, t: 0 });
});

test('a loop that encloses enemy base steals it', () => {
  const s = arena();
  const a = human(s, 'A', 5, 5);
  const b = human(s, 'B', 12, 5);
  // B steps to the right edge of its base and stops there, out of the loop.
  run(s, [
    [a, [...rep(U, 3), ...rep(R, 7), ...rep(D, 6), ...rep(L, 7), ...rep(U, 2)]],
    [b, [R, 'stop']],
  ]);
  assert.equal(b.alive, true);
  assert.equal(b.land, 3, 'B keeps only its column outside the loop');
  assert.equal(a.land, 59);
  for (const [x, y] of [[11, 5], [12, 4], [12, 6]]) assert.equal(baseAt(s, x, y), a.id, `${x},${y}`);
  assert.equal(baseAt(s, 13, 5), b.id);
});

test('a loop that encloses an enemy head inside its base kills it (decision D-7)', () => {
  const s = arena();
  const a = human(s, 'A', 5, 5);
  const b = human(s, 'B', 12, 5);
  // B never moves: it is invulnerable in its base, until its tile is captured.
  run(s, [[a, [...rep(U, 3), ...rep(R, 9), ...rep(D, 6), ...rep(L, 9), ...rep(U, 2)]]]);
  assert.equal(b.alive, false);
  assert.equal(b.death?.cause, 'enclosed');
  assert.equal(b.death?.killer, 'A');
  assert.equal(a.kills, 1);
  assert.equal(a.land, 73);
  assert.deepEqual(tilesOf(s, b.id), { b: 0, t: 0 });
});

test('stepping on your own trail kills you and clears your land', () => {
  const s = arena(20, 20);
  const a = human(s, 'A', 5, 5);
  run(s, [[a, [U, U, U, R, R, D, L, L]]]);
  assert.equal(a.alive, false);
  assert.equal(a.death?.cause, 'self');
  assert.equal(a.death?.killer, null);
  assert.equal(a.death?.land, 9);
  assert.deepEqual(tilesOf(s, a.id), { b: 0, t: 0 });
  assert.equal(a.land, 0);
});

test('stepping on an enemy trail kills its owner and credits the kill', () => {
  const s = arena();
  const a = human(s, 'A', 5, 10);
  const b = human(s, 'B', 12, 8);
  run(s, [
    [a, rep(U, 7)],
    [b, rep(L, 7)],
  ]);
  assert.equal(a.alive, false);
  assert.equal(a.death?.cause, 'trail');
  assert.equal(a.death?.killer, 'B');
  assert.equal(b.alive, true);
  assert.equal(b.kills, 1);
  assert.deepEqual(tilesOf(s, a.id), { b: 0, t: 0 });
  // D-32: the killer takes the victim's whole base.
  assert.equal(b.land, 18);
  for (let y = 9; y <= 11; y++) for (let x = 4; x <= 6; x++) assert.equal(baseAt(s, x, y), b.id, `${x},${y}`);
});

test('a killer inherits the victim base, except tiles under its own trail', () => {
  const s = arena();
  const a = human(s, 'A', 5, 10);
  const b = human(s, 'B', 12, 10);
  // A walks straight up out of its base; B crosses A's base (laying trail on (6,10) and (6,9)), turns up,
  // then left onto A's trail at (5,7).
  run(s, [
    [a, rep(U, 10)],
    [b, [...rep(L, 6), ...rep(U, 3), L]],
  ]);
  assert.equal(a.alive, false);
  assert.equal(a.death?.killer, 'B');
  assert.equal(b.alive, true);
  assert.equal(baseAt(s, 6, 10), 0, 'under the killer trail: neutral');
  assert.equal(baseAt(s, 6, 9), 0);
  assert.equal(s.trail[10 * s.cfg.w + 6], b.id, 'the killer trail is untouched');
  assert.equal(b.land, 9 + 7);
  assert.equal(baseAt(s, 4, 10), b.id);
  // The invariant holds: no tile with the same owner's base and trail.
  for (let i = 0; i < s.base.length; i++) assert.ok(!(s.base[i] && s.base[i] === s.trail[i]));
});

test('head to head outside both bases: both die', () => {
  const s = arena();
  const a = human(s, 'A', 5, 10);
  const b = human(s, 'B', 11, 10);
  run(s, [
    [a, rep(R, 3)],
    [b, rep(L, 3)],
  ]);
  assert.equal(a.alive, false);
  assert.equal(b.alive, false);
  assert.equal(a.death?.cause, 'head');
  assert.equal(b.death?.cause, 'head');
  // Nobody survives to inherit: both bases go neutral.
  assert.equal(s.base.filter((v) => v !== 0).length, 0);
});

test('a head inside its own base is invulnerable; others pass through it', () => {
  const s = arena();
  const a = human(s, 'A', 5, 10);
  const b = human(s, 'B', 12, 10);
  // A turns once (so it has a direction) inside its base and stops at once.
  run(s, [
    [a, [L, 'stop']],
    [b, rep(L, 10)],
  ]);
  // B walked through A's head at (4,10) and on.
  assert.equal(a.alive, true);
  assert.equal(b.alive, true);
  assert.equal(b.x, 2);
});

test('leaving the map kills', () => {
  const s = arena(20, 20);
  const a = human(s, 'A', 5, 5);
  run(s, [[a, rep(U, 5)]]);
  assert.equal(a.alive, true);
  assert.equal(a.y, 0);
  run(s, [[a, [U]]]);
  assert.equal(a.alive, false);
  assert.equal(a.death?.cause, 'border');
});

test('reversal is ignored while trailing, allowed inside the base', () => {
  const s = arena(20, 20);
  const a = human(s, 'A', 5, 10);
  run(s, [[a, [U, U, D]]]);
  assert.equal(a.alive, true);
  assert.deepEqual([a.x, a.y, a.dir], [5, 7, U]);
  const b = human(s, 'B', 14, 10);
  run(s, [[b, [R, L]]]);
  assert.deepEqual([b.x, b.y, b.dir], [14, 10, L]);
  assert.equal(b.alive, true);
});

test('stopping inside your base is safe, stopping on your trail is not', () => {
  const s = arena();
  const a = human(s, 'A', 5, 10);
  const c = human(s, 'C', 12, 10);
  run(s, [
    [a, [U, 'stop']],
    [c, rep(L, 9)],
  ]);
  assert.equal(a.alive, true, 'A sat in its base while C crossed it');
  assert.equal(a.y, 9);

  const s2 = arena();
  const p = human(s2, 'P', 5, 10);
  const q = human(s2, 'Q', 12, 7);
  // P walks out to (5,7) and stops on its trail; Q crosses (5,7).
  run(s2, [
    [p, [U, U, U, 'stop']],
    [q, rep(L, 7)],
  ]);
  assert.equal(p.alive, false);
  assert.equal(p.death?.cause, 'trail');
  assert.equal(p.death?.killer, 'Q');
});

test('a buffered second turn lands on the following tile', () => {
  const s = arena(20, 20);
  const a = human(s, 'A', 10, 10);
  step(s, [{ id: a.id, turns: [U] }], 3);
  // Left then down pressed between two tiles: left is taken now, down on the next tile.
  step(s, [{ id: a.id, turns: [L, D] }]);
  assert.deepEqual([a.x, a.y], [9, 7]);
  step(s);
  assert.deepEqual([a.x, a.y], [9, 8]);
});

test('spawns never overlap and keep their distance', () => {
  const s = createState({ w: 100, h: 100, targetHeads: 0 }, 'spawn');
  /** @type {Player[]} */
  const ps = [];
  for (let k = 0; k < 40; k++) {
    const p = spawnPlayer(s, { kind: 'bot', name: `b${k}` });
    assert.ok(p, 'there is room for 40 bases on 100×100');
    ps.push(p);
  }
  for (const p of ps) {
    assert.equal(p.land, 9);
    for (let y = p.y - 1; y <= p.y + 1; y++)
      for (let x = p.x - 1; x <= p.x + 1; x++) assert.equal(baseAt(s, x, y), p.id);
    for (const q of ps)
      if (q !== p) assert.ok(Math.max(Math.abs(p.x - q.x), Math.abs(p.y - q.y)) > 5, 'heads 6+ tiles apart');
  }
});

test('spawning into a full arena returns null instead of overlapping', () => {
  const s = createState({ w: 12, h: 12, targetHeads: 0 }, 'tiny');
  let n = 0;
  while (spawnPlayer(s, { kind: 'bot', name: 'x' })) n++;
  assert.ok(n >= 1 && n < 10);
  const owned = s.base.filter((v) => v !== 0).length;
  assert.equal(owned, n * 9);
});

test('a human can always spawn on a full map: a base is carved from land; bots wait for room', () => {
  const s = createState({ w: 30, h: 30, targetHeads: 0 }, 'full');
  const big = human(s, 'Big', 15, 15);
  // Big owns every tile; its head stays in the middle.
  for (let i = 0; i < s.base.length; i++) s.base[i] = 0;
  big.land = 0;
  big.bbox = [];
  for (let y = 0; y < 30; y++) for (let x = 0; x < 30; x++) setBaseForTest(s, y * 30 + x, big.id);
  assert.equal(big.land, 900);
  assert.equal(spawnPlayer(s, { kind: 'bot', name: 'bot' }), null, 'a bot does not carve');
  const late = spawnPlayer(s, { kind: 'human', name: 'Late' });
  assert.ok(late, 'the human gets in');
  assert.equal(late.land, 9);
  assert.equal(big.land, 891);
  assert.ok(Math.max(Math.abs(late.x - big.x), Math.abs(late.y - big.y)) > 2, 'away from the other head');
  for (let i = 0; i < s.base.length; i++) assert.ok(!(s.base[i] && s.base[i] === s.trail[i]));
});

test('determinism: same seed and inputs give the same state hash after 1,000 ticks', () => {
  const play = (/** @type {string} */ seed) => {
    const s = createState({}, seed, { fill: true });
    s.humans = 1;
    const me = spawnPlayer(s, { kind: 'human', name: 'me' });
    assert.ok(me);
    let lcg = 12345;
    for (let t = 0; t < 1000; t++) {
      lcg = (Math.imul(lcg, 1103515245) + 12345) >>> 0;
      const inputs = t % 5 === 0 ? [{ id: me.id, turns: [lcg % 4] }] : [];
      step(s, inputs);
    }
    return stateHash(s);
  };
  const h1 = play('alpha');
  assert.equal(play('alpha'), h1);
  assert.notEqual(play('beta'), h1);
});

test('bots fill the arena and leave as humans join', () => {
  const s = createState({ w: 60, h: 60, targetHeads: 6 }, 'fill', { fill: true });
  assert.equal(s.players.length, 6);
  s.humans = 2;
  step(s, [], 3);
  assert.equal(s.players.filter((p) => p.kind === 'bot').length, 4);
  s.humans = 0;
  // Bots also die and respawn meanwhile: the arena must be back to 6 at some point, not at one fixed tick.
  let full = false;
  for (let k = 0; k < 400 && !full; k++) {
    step(s);
    full = s.players.filter((p) => p.kind === 'bot').length === 6;
  }
  assert.ok(full, 'refilled to 6 bots');
});
