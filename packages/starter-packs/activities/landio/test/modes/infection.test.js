// @ts-check
import assert from 'node:assert/strict';
import { RESULTS_TICKS, createState, spawnPlayerAt, step, stateHash } from '../../shared/rules.js';
import { allied, results, TEAMS } from '../../shared/modes/index.js';
import infection, { winner } from '../../shared/modes/infection.js';
import { LOOK_COLORS } from '../../shared/look.js';
import { kill } from '../../shared/cards/util.js';

const R = 1;
const L = 3;
const ROUND = 2100;

/** @param {Partial<import('../../shared/types.js').Config>} [cfg] */
const arena = (cfg = {}) => createState({ w: 40, h: 40, targetHeads: 0, mode: 'infection', ...cfg }, 'inf');
/** @param {any} s @param {string} name @param {number} x @param {number} y @param {string} [uid] */
const human = (s, name, x, y, uid = name) => spawnPlayerAt(s, { kind: 'human', name, uid }, x, y);
const team = (/** @type {{team: number}} */ p) => p.team;

test('exactly one virus at the start, a human preferred', () => {
  const s = createState({ w: 60, h: 60, targetHeads: 8, mode: 'infection' }, 'one', { fill: true });
  const a = human(s, 'A', 30, 30);
  step(s, [], 1);
  assert.equal(s.players.filter((p) => p.team === 2).length, 1);
  // A new round re-seeds it from setup (look on the start tick itself: bots joining later may be infected).
  step(s, [], ROUND + RESULTS_TICKS - 1);
  assert.equal(s.round.n, 2);
  assert.equal(s.players.filter((p) => p.team === 2).length, 1);
  assert.equal(s.players.find((p) => p.kind === 'human')?.team, 2);
});

test('with only bots, a bot is the virus', () => {
  const s = createState({ w: 60, h: 60, targetHeads: 6, mode: 'infection' }, 'bots', { fill: true });
  step(s, [], 1);
  assert.equal(s.players.filter((p) => p.team === 2).length, 1);
});

test('a Clean victim killed by the Virus respawns as Virus, by uid; look and team update', () => {
  const s = arena();
  const a = human(s, 'A', 10, 10); // first virus (random among humans)
  const b = human(s, 'B', 30, 30);
  step(s, [], 1);
  const [v, c] = a.team === 2 ? [a, b] : [b, a];
  assert.equal(c.team, 1);
  const before = c.look;
  assert.equal(before % LOOK_COLORS, TEAMS[0].color);
  kill(s, c, 'trail', v);
  assert.equal(c.alive, false);
  assert.equal(c.team, 2);
  assert.equal(c.look % LOOK_COLORS, TEAMS[1].color);
  assert.deepEqual(/** @type {string[]} */ (s.md.inf).includes(/** @type {string} */ (c.uid)), true);
  // Respawn: the engine places the human again via a new round-less spawn.
  const again = spawnPlayerAt(s, { kind: 'human', name: 'B2', uid: c.uid }, 20, 20);
  assert.equal(again.team, 2);
});

test('a Clean player who crashes stays Clean', () => {
  const s = arena();
  const a = human(s, 'A', 20, 20);
  const b = human(s, 'B', 2, 10);
  step(s, [], 1);
  const clean = a.team === 1 ? a : b;
  const virus = a.team === 1 ? b : a;
  assert.equal(virus.team, 2);
  clean.x = 1;
  clean.y = 10;
  kill(s, clean, 'border', null);
  assert.equal(clean.team, 1);
  assert.equal(/** @type {string[]} */ (s.md.inf).includes(/** @type {string} */ (clean.uid)), false);
  const again = spawnPlayerAt(s, { kind: 'human', name: 'X', uid: clean.uid }, 30, 30);
  assert.equal(again.team, 1);
});

test('allies cannot hurt each other', () => {
  const s = arena();
  const a = human(s, 'A', 10, 10);
  const b = human(s, 'B', 20, 20);
  const c = human(s, 'C', 30, 30);
  step(s, [], 1);
  const clean = [a, b, c].filter((p) => p.team === 1);
  assert.equal(clean.length, 2);
  assert.equal(allied(s, clean[0], clean[1]), true);
  const v = [a, b, c].find((p) => p.team === 2);
  assert.ok(v);
  assert.equal(allied(s, v, clean[0]), false);
});

test('the virus wins early, but not before 30 s', () => {
  const s = arena();
  const a = human(s, 'A', 10, 10);
  const b = human(s, 'B', 30, 30);
  step(s, [], 1);
  const [v, c] = a.team === 2 ? [a, b] : [b, a];
  kill(s, c, 'trail', v);
  step(s, [], 100);
  assert.equal(s.round.phase, 'play', 'too early');
  step(s, [], 110);
  assert.equal(s.round.phase, 'over');
  assert.ok(s.tick < ROUND);
  assert.equal(s.round.result?.winner, -2);
});

test('Clean win at time while any Clean is alive', () => {
  const s = arena();
  human(s, 'A', 10, 10);
  human(s, 'B', 30, 30);
  step(s, [], ROUND);
  assert.equal(s.round.phase, 'over');
  assert.equal(s.round.result?.winner, -1);
});

test('winner(): Clean while any is alive, else Virus; results() uses it', () => {
  const s = arena();
  const a = human(s, 'A', 10, 10);
  const b = human(s, 'B', 30, 30);
  step(s, [], 1);
  assert.equal(winner(s), -1);
  assert.equal(infection.winner?.(s), -1);
  assert.equal(results(s).winner, -1);
  const v = a.team === 2 ? a : b;
  const c = a.team === 2 ? b : a;
  kill(s, c, 'trail', v);
  assert.equal(winner(s), -2);
  assert.equal(results(s).winner, -2);
});

test('virus bots hunt: botGoal is the nearest Clean head', () => {
  const s = arena();
  const a = human(s, 'A', 10, 10);
  const b = human(s, 'B', 30, 30);
  step(s, [], 1);
  const v = a.team === 2 ? a : b;
  const c = a.team === 2 ? b : a;
  assert.deepEqual(infection.botGoal?.(s, v), { x: c.x, y: c.y });
  assert.equal(infection.botGoal?.(s, c), null);
});

test('bot game: rounds advance, results sensible, one virus at least, deterministic', () => {
  const run = () => {
    const s = createState({ w: 80, h: 80, targetHeads: 10, mode: 'infection' }, 'trial', { fill: true });
    const winners = [];
    let seenRound = 1;
    for (let t = 0; t < 2 * (ROUND + RESULTS_TICKS) + 10; t++) {
      step(s, [], 1);
      for (const p of s.players) assert.ok(p.team === 1 || p.team === 2, 'team set');
      if (s.round.phase === 'play' && s.tick - s.round.start > 20 && s.tick % 50 === 0 && s.players.some((p) => p.alive && p.team === 1))
        assert.ok(s.players.some((p) => p.team === 2), 'a virus exists while clean play');
      if (s.round.n > seenRound) seenRound = s.round.n;
      if (s.round.phase === 'over' && s.round.result && winners.length < s.round.n) winners.push(s.round.result.winner);
    }
    assert.ok(s.round.n >= 3 || winners.length >= 2, 'rounds advance');
    return { hash: stateHash(s), winners };
  };
  const a = run();
  const b = run();
  assert.equal(a.hash, b.hash);
  assert.ok(a.winners.length >= 2);
  for (const w of a.winners) assert.ok(w === -1 || w === -2);
  console.log('winners', a.winners);
});

test('timing: tick cost at 200x200 with 30 bots', () => {
  const time = (/** @type {string} */ mode) => {
    const s = createState({ w: 200, h: 200, targetHeads: 30, mode }, 'time', { fill: true });
    const m = infection;
    step(s, [], 50);
    let sum = 0;
    for (let i = 0; i < 2000; i++) {
      if (mode === 'infection' && s.round.phase === 'play') {
        const t0 = performance.now();
        m.tick?.(s);
        m.roundOver?.(s);
        sum += performance.now() - t0;
      }
      step(s, [], 1);
    }
    return sum / 2000;
  };
  const avg = time('infection');
  console.log('infection tick ms', avg.toFixed(4));
  assert.ok(avg < 0.3);
});

test('an infected human who stays dead does not stall the round: after 10 s someone clean is infected', () => {
  const s = arena();
  const a = human(s, 'A', 8, 8);
  const b = human(s, 'B', 30, 8);
  const c = human(s, 'C', 8, 30);
  step(s, [], 1);
  const virus = s.players.find((p) => p.team === 2);
  assert.ok(virus);
  // The only virus dies and does not come back (its human stays on the summary).
  kill(s, virus, 'wall', null);
  assert.equal(s.players.filter((p) => p.alive && p.team === 2).length, 0);
  step(s, [], 5 * 7);
  assert.equal(s.players.filter((p) => p.alive && p.team === 2).length, 0, 'not at once: the infected may respawn');
  step(s, [], 7 * 7);
  assert.equal(s.players.filter((p) => p.alive && p.team === 2).length, 1, 'a new virus after about 10 s');
  void a;
  void b;
  void c;
});
