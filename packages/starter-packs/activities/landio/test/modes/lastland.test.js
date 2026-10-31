// @ts-check
import assert from 'node:assert/strict';
import { RESULTS_TICKS, createState, maySpawn, removePlayer, spawnPlayerAt, step, stateHash } from '../../shared/rules.js';
import { results, MODE_BY_ID } from '../../shared/modes/index.js';
import { setBase } from '../../shared/grid.js';

const L = 3;
const mode = MODE_BY_ID.lastland;
/** @param {Partial<import('../../shared/types.js').Config>} cfg */
const arena = (cfg = {}) => createState({ w: 40, h: 40, targetHeads: 0, mode: 'lastland', ...cfg }, 'last');
/** @param {import('../../shared/types.js').GameState} s @param {string} name @param {number} x @param {number} y */
const human = (s, name, x, y) => spawnPlayerAt(s, { kind: 'human', name }, x, y);

test('no respawn: the dead cannot spawn again', () => {
  const s = arena();
  const a = human(s, 'A', 2, 10);
  human(s, 'B', 30, 30);
  step(s, [{ id: a.id, turns: [L] }], 3);
  assert.equal(a.alive, false);
  assert.equal(maySpawn(s, a), false);
  assert.equal(mode.respawn, false);
});

test('bots fill the arena at the start and stop joining after the grace', () => {
  const s = arena({ targetHeads: 6 });
  human(s, 'H', 5, 5);
  s.humans = 1;
  step(s, [], 69);
  assert.equal(mode.botsJoin?.(s), true);
  const bots = () => s.players.filter((p) => p.kind === 'bot');
  assert.ok(bots().length >= 1);
  step(s, [], 20);
  assert.equal(mode.botsJoin?.(s), false);
  const before = bots().length;
  removePlayer(s, bots()[0].id);
  step(s, [], 30);
  assert.ok(bots().length <= before - 1 + 0, 'nobody joined late');
});

test('the round does not end at the start, even in an empty arena', () => {
  const s = arena();
  human(s, 'A', 10, 10);
  step(s, [], 200);
  assert.equal(s.round.phase, 'play', 'a lone player never had a rival');
  const t = arena();
  const a = human(t, 'A', 2, 10);
  human(t, 'B', 30, 30);
  step(t, [{ id: a.id, turns: [L] }], 3);
  step(t, [], 60);
  assert.equal(t.round.phase, 'play', 'only one left but the round is under 10 s old');
});

test('the round ends once one is left (two were alive) and the survivor wins', () => {
  const s = arena();
  const a = human(s, 'A', 2, 10);
  const b = human(s, 'B', 30, 30);
  step(s, [{ id: a.id, turns: [L] }], 3);
  while (s.tick < 69) step(s);
  assert.equal(s.round.phase, 'play');
  step(s);
  assert.equal(s.round.phase, 'over');
  assert.equal(s.round.result?.winner, b.id);
  assert.equal(s.round.result?.players[0].id, b.id);
  step(s, [], RESULTS_TICKS);
  assert.equal(s.round.n, 2);
});

test('with nobody left the round ends', () => {
  const s = arena();
  const a = human(s, 'A', 2, 10);
  const b = human(s, 'B', 2, 30);
  step(s, [{ id: a.id, turns: [L] }, { id: b.id, turns: [L] }], 3);
  step(s, [], 80);
  assert.equal(s.round.phase, 'over');
});

test('rank: survivor first, then the later death, then more land', () => {
  const s = arena();
  const a = human(s, 'A', 2, 10); // dies first
  const c = human(s, 'C', 2, 30); // dies second, with the most land
  const b = human(s, 'B', 30, 30);
  for (let i = 0; i < 30; i++) setBase(s, 20 * 40 + i, c.id);
  step(s, [{ id: a.id, turns: [L] }], 3);
  step(s, [{ id: c.id, turns: [L] }], 3);
  assert.ok(!a.alive && !c.alive && b.alive);
  assert.ok(mode.rank);
  const res = results(s);
  assert.deepEqual(res.players.map((p) => p.id), [b.id, c.id, a.id]);
  assert.equal(res.winner, b.id);
});

test('the 5 minute cap: most land wins', () => {
  const s = arena();
  // Inside the sudden death's final zone (the middle 30%: x and y 14..25 on 40x40), so the border spares them.
  const a = human(s, 'A', 16, 20);
  const b = human(s, 'B', 23, 20);
  for (let i = 0; i < 12; i++) setBase(s, 23 * 40 + 14 + i, b.id);
  step(s, [], 2099);
  assert.equal(s.round.phase, 'play');
  step(s);
  assert.equal(s.round.phase, 'over');
  assert.equal(s.round.result?.winner, b.id);
  void a;
});

test('bot game: rounds advance, winners are alive, deterministic', () => {
  const run = () => {
    const s = createState({ w: 80, h: 80, targetHeads: 10, mode: 'lastland' }, 'trial', { fill: true });
    const winners = [];
    let seen = 0;
    for (let t = 0; t < 2 * (2100 + RESULTS_TICKS); t++) {
      step(s);
      if (s.round.phase === 'play' && s.tick - s.round.start > 70) {
        const alive = s.players.filter((p) => p.alive).length;
        assert.ok(alive <= s.cfg.targetHeads);
      }
      if (s.round.phase === 'over' && s.round.result && s.round.n > seen) {
        seen = s.round.n;
        const r = s.round.result;
        // The last two can die on the same step (the closing border): then nobody wins (bots leave on death).
        const w = r.players.find((p) => p.id === r.winner);
        if (r.winner) {
          assert.ok(w, 'winner is in the results');
          assert.equal(r.players[0].id, r.winner);
        }
        winners.push(r.winner);
        // Everyone ranked above a dead player is alive.
        const firstDead = r.players.findIndex((p) => !p.alive);
        assert.ok(firstDead === -1 || r.players.slice(firstDead).every((p) => !p.alive));
      }
    }
    assert.ok(s.round.n >= 2, `rounds: ${s.round.n}`);
    return { h: stateHash(s), winners, n: s.round.n };
  };
  const a = run();
  const b = run();
  assert.deepEqual(a, b);
  assert.ok(a.winners.length >= 2);
});

test('timing: the mode tick costs under 0.3 ms at 200x200 with 30 bots', () => {
  const s = createState({ w: 200, h: 200, targetHeads: 30, mode: 'lastland' }, 'time', { fill: true });
  let spent = 0;
  for (let t = 0; t < 2000; t++) {
    step(s);
    if (s.round.phase !== 'play') continue;
    const t0 = performance.now();
    mode.tick?.(s);
    mode.roundOver?.(s);
    mode.botsJoin?.(s);
    spent += performance.now() - t0;
  }
  assert.ok(spent / 2000 < 0.3, `${(spent / 2000).toFixed(4)} ms per tick`);
});

test('sudden death: from 2:00 the border closes in, and bot rounds end before the 5 minute cap', () => {
  const s = createState({ w: 80, h: 80, targetHeads: 10, mode: 'lastland' }, 'sudden', { fill: true });
  const start = s.round.start;
  step(s, [], 2 * 60 * 7 - 1);
  if (s.round.n === 1 && s.round.phase === 'play') {
    assert.deepEqual(s.md.zone, [0, 0, 80, 80], 'nothing closes before 2:00');
    step(s);
    assert.ok(Number(s.md.k) >= 1, 'the first step at 2:00');
    assert.equal(s.terrain[0], 1, 'the corner is wall now');
  }
  // Three rounds: each must end before its cap.
  /** @type {number[]} */
  const lengths = [];
  let n = s.round.n;
  let roundStart = start;
  for (let k = 0; k < 3 * (2100 + RESULTS_TICKS); k++) {
    step(s);
    if (s.round.phase === 'over' && lengths.length < n) lengths.push(s.tick - roundStart);
    if (s.round.n !== n) {
      n = s.round.n;
      roundStart = s.round.start;
    }
  }
  assert.ok(lengths.length >= 2, `rounds finished (${lengths})`);
  for (const len of lengths) assert.ok(len < 2100, `a round ended before the cap (${len} ticks)`);
});
