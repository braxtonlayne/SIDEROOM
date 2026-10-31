// @ts-check
import assert from 'node:assert/strict';
import { RESULTS_TICKS, createState, spawnPlayerAt, step, stateHash } from '../../shared/rules.js';
import { results } from '../../shared/modes/index.js';
import teams, { teamCount } from '../../shared/modes/teams.js';
import { kill } from '../../shared/cards/util.js';
import { setBase } from '../../shared/grid.js';

/** @param {number} targetHeads */
const arena = (targetHeads) => createState({ w: 60, h: 60, targetHeads, mode: 'teams' }, 'teams');
/** @param {any} s @param {string} name @param {number} x @param {number} y */
const human = (s, name, x, y) => spawnPlayerAt(s, { kind: 'human', name, uid: 'u-' + name }, x, y);

test('team count follows targetHeads: 2, 3 from 18, 4 from 28', () => {
  for (const [heads, n] of [[0, 2], [17, 2], [18, 3], [27, 3], [28, 4], [40, 4]]) {
    assert.equal(teamCount(arena(heads)), n, `heads ${heads}`);
  }
  assert.equal(teams.teams, 4);
});

test('newcomers join the smallest team, only among the first N teams', () => {
  for (const [heads, n] of [[0, 2], [18, 3], [28, 4]]) {
    const s = arena(heads);
    const counts = Array(5).fill(0);
    for (let i = 0; i < 12; i++) counts[spawnPlayerAt(s, { kind: i % 2 ? 'bot' : 'human', name: 'P' + i, uid: i % 2 ? null : 'u' + i }, 5 + (i % 6) * 8, 5 + Math.floor(i / 6) * 8).team]++;
    assert.equal(counts[0], 0);
    for (let t = n + 1; t <= 4; t++) assert.equal(counts[t], 0, `no team ${t} with ${heads} heads`);
    const live = counts.slice(1, n + 1);
    assert.ok(Math.max(...live) - Math.min(...live) <= 1, `balanced ${live}`);
  }
});

test('a human keeps their team after dying and respawning in the same round', () => {
  const s = arena(0);
  const a = human(s, 'A', 10, 10);
  human(s, 'B', 30, 30);
  const c = human(s, 'C', 45, 10);
  const team = a.team;
  kill(s, a, 'wall', null);
  assert.equal(a.alive, false);
  // Others change the balance while A is dead: A's side would otherwise flip.
  const d = human(s, 'D', 20, 45);
  assert.ok(d && c);
  const again = spawnPlayerAt(s, { kind: 'human', name: 'A', uid: 'u-A' }, 10, 40);
  assert.equal(again.team, team);
});

test('a new round re-balances: the memory is cleared', () => {
  const s = arena(0);
  human(s, 'A', 10, 10);
  assert.ok((/** @type {string[]} */ (s.md.uids)).length === 1);
  step(s, [], 5 * 60 * 7 + RESULTS_TICKS);
  assert.equal(s.round.n, 2);
  assert.ok(!s.md.uids || /** @type {string[]} */ (s.md.uids).length <= 1);
});

test('results rank teams by total and list only teams with players', () => {
  const s = arena(0);
  const a = human(s, 'A', 10, 10);
  const b = human(s, 'B', 30, 30);
  const c = human(s, 'C', 45, 10);
  assert.notEqual(a.team, b.team);
  // Give B's team more land than A's.
  for (let i = 0; i < 20; i++) setBase(s, (35 + (i % 5)) * 60 + 40 + Math.floor(i / 5), b.id);
  const r = results(s);
  assert.equal(r.teams.length, 2, 'empty teams are not listed');
  assert.equal(r.teams[0].team, b.team);
  assert.equal(r.winner, -b.team);
  assert.ok(r.teams[0].score > r.teams[1].score);
  assert.ok(c);
});

test('botGoal pulls a far bot to its team centre and leaves near bots alone', () => {
  const s = arena(0);
  const t1 = spawnPlayerAt(s, { kind: 'bot', name: 'x1' }, 5, 5);
  spawnPlayerAt(s, { kind: 'bot', name: 'y1' }, 50, 5);
  const t2 = spawnPlayerAt(s, { kind: 'bot', name: 'x2' }, 50, 50);
  assert.equal(t1.team, t2.team);
  assert.deepEqual(teams.botGoal?.(s, t1), { x: 28, y: 28 });
  // The centre is cached for 35 ticks, then recomputed.
  t2.x = 10;
  t2.y = 10;
  assert.deepEqual(teams.botGoal?.(s, t1), { x: 28, y: 28 }, 'cached');
  s.tick += 35;
  assert.equal(teams.botGoal?.(s, t1), null, 'recomputed: the centre is now near');
});

test('bot game: rounds advance, teams are valid, results make sense, deterministic', () => {
  const run = () => {
    const s = createState({ w: 80, h: 80, targetHeads: 20, mode: 'teams' }, 'trial', { fill: true });
    const winners = [];
    const len = 5 * 60 * 7 + RESULTS_TICKS;
    for (let t = 0; t < 2 * len + 10; t++) {
      step(s);
      if (t % 25 === 0) for (const p of s.players) assert.ok(p.team >= 1 && p.team <= 3, `team ${p.team}`);
      if (s.round.phase === 'over' && s.round.result && winners.length < s.round.n) winners.push(s.round.result);
    }
    return { s, winners };
  };
  const a = run();
  assert.ok(a.s.round.n >= 3);
  assert.equal(a.winners.length, 2);
  for (const r of a.winners) {
    assert.ok(r.winner < 0, 'a team wins');
    assert.ok(r.teams.length >= 2 && r.teams.length <= 3);
    assert.ok(r.teams[0].score >= r.teams[1].score);
  }
  assert.equal(stateHash(run().s), stateHash(a.s));
});

test('timing: the mode costs under 0.3 ms per tick at 200x200 with 30 bots', () => {
  /** @param {string} mode */
  const time = (mode) => {
    const s = createState({ w: 200, h: 200, targetHeads: 30, mode }, 'timing', { fill: true });
    step(s, [], 100);
    const t0 = performance.now();
    step(s, [], 2000);
    return (performance.now() - t0) / 2000;
  };
  time('teams');
  const base = time('rounds');
  const mine = time('teams');
  console.log(`teams ${mine.toFixed(3)} ms/tick, rounds baseline ${base.toFixed(3)}`);
  assert.ok(mine - base < 0.3);
});
