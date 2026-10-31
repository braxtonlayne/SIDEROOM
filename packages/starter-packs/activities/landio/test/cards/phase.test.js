// @ts-check
// Phase: crossing trails unharmed for 7 ticks, what it does not cover (a head, the recharge), bot use.
import assert from 'node:assert/strict';
import { ACT, CARDS, effectFlags } from '../../shared/cards.js';
import { arena, human, give, run, rep, trailAt, broken, U, R } from './helpers.js';

/** @typedef {import('../../shared/types.js').GameState} GameState */
/** @typedef {import('../../shared/types.js').Player} Player */
/** @typedef {Parameters<typeof run>[1]} Scripts */

/**
 * Run the scripts one tick at a time, checking the grid invariants after every tick.
 * @param {GameState} s @param {Scripts} scripts
 */
function checked(s, scripts) {
  const n = Math.max(...scripts.map(([, sc]) => sc.length));
  for (let t = 0; t < n; t++) {
    /** @type {Scripts} */
    const part = scripts.map(([p, sc]) => [p, sc.slice(t, t + 1)]);
    run(s, part);
    assert.equal(broken(s), null, `tick ${s.tick}`);
  }
}

/**
 * A runs right along y = 20 for 10 ticks: its trail is x 12..20 (x 14 is laid on tick 4). B waits, phases on
 * tick 1, then goes up x = 14 and steps onto A's trail on tick `wait + 5`.
 * @param {number} wait
 */
function crossing(wait) {
  const s = arena(40, 40);
  const a = human(s, 'A', 10, 20);
  const b = human(s, 'B', 14, 25);
  give(s, b, 'phase');
  /** @type {Scripts} */
  const scripts = [
    [a, rep(R, 10)],
    [b, [ACT.active, ...rep(null, wait - 1), U, U, U, U, U]],
  ];
  return { s, a, b, scripts };
}

test('Phase card data: active, epic, one level; recharge 140 ticks (20 s)', () => {
  const def = CARDS.phase;
  assert.equal(def.kind, 'active');
  assert.equal(def.rarity, 'epic');
  assert.equal(def.maxLevel, 1);
  assert.equal(def.cooldown?.(1), 140);
  assert.equal(def.speed, undefined, 'Phase does not change speed');
});

test('while phased, crossing an enemy trail kills nobody and cuts that trail back to the crossing', () => {
  const { s, a, b, scripts } = crossing(2); // phase on tick 1 (ends 8); the crossing is on tick 7
  checked(s, scripts);
  assert.ok(a.alive, 'the trail owner lives');
  assert.ok(b.alive, 'the phased head lives');
  assert.equal(trailAt(s, 12, 20), 0, 'the trail before the crossing is gone');
  assert.equal(trailAt(s, 13, 20), 0);
  assert.equal(trailAt(s, 14, 20), b.id, 'the crossing tile is now the phased head trail');
  assert.equal(trailAt(s, 15, 20), a.id, 'the part after the crossing is kept');
  assert.equal(a.trail.length, 6, 'A keeps x 15..20');
});

test('7 ticks after use the phase is over: crossing an enemy trail kills its owner as usual', () => {
  const { s, a, b, scripts } = crossing(3); // the crossing is on tick 8, the first tick without phase
  checked(s, scripts);
  assert.equal(a.alive, false);
  assert.equal(a.death?.cause, 'trail');
  assert.equal(a.death?.killer, 'B');
  assert.ok(b.alive);
});

/**
 * P walks a loop that comes back onto its own trail at (13, 20) on tick 9. Phase is used on tick `useAt`.
 * @param {number} useAt
 */
function loop(useAt) {
  const s = arena(40, 40);
  const p = human(s, 'P', 10, 20);
  give(s, p, 'phase');
  const moves = [R, R, R, R, U, U, 3, 2, 2]; // 3 = left, 2 = down
  /** @type {Scripts} */
  const scripts = [[p, moves.map((d, i) => (i + 1 === useAt ? [d, ACT.active] : d))]];
  return { s, p, scripts };
}

test('while phased, crossing your own trail does not kill you and pinches the loop off at the crossing', () => {
  const { s, p, scripts } = loop(3); // phase ends on tick 10: the crossing on tick 9 is still phased
  checked(s, scripts);
  assert.ok(p.alive);
  assert.equal(p.x, 13);
  assert.equal(p.y, 20);
  assert.equal(p.trail.length, 2, 'the trail ends at the crossing');
  assert.equal(trailAt(s, 13, 20), p.id);
  for (const [x, y] of [[14, 20], [14, 19], [14, 18], [13, 18], [13, 19]]) assert.equal(trailAt(s, x, y), 0, `${x},${y}`);
});

test('7 ticks after use the phase is over: crossing your own trail kills you', () => {
  const { s, p, scripts } = loop(2); // phase ends on tick 9, the crossing is on tick 9
  checked(s, scripts);
  assert.equal(p.alive, false);
  assert.equal(p.death?.cause, 'self');
});

test('stepping onto an enemy head while phased still kills that enemy (immunity never covers a head)', () => {
  const s = arena(40, 40);
  const a = human(s, 'A', 10, 20);
  const b = human(s, 'B', 20, 25);
  give(s, b, 'phase');
  checked(s, [
    [a, [...rep(R, 10), 'stop']], // A's head stops on (20, 20) on tick 11
    [b, [...rep(null, 6), [U, ACT.active], U, U, U, U]], // B phases on tick 7 and steps onto that head on tick 11
  ]);
  assert.equal(a.alive, false);
  assert.equal(a.death?.killer, 'B');
  assert.ok(b.alive);
});

test('the phase recharges for 140 ticks: a use on cooldown does nothing; the next one is taken exactly when ready', () => {
  const s = arena();
  const p = human(s, 'P', 10, 10);
  give(s, p, 'phase');
  // Tick 1 used (ready on 141). Tick 140 refused. Tick 141 taken.
  checked(s, [[p, [ACT.active, ...rep(null, 138), ACT.active, ACT.active]]]);
  assert.equal(s.tick, 141);
  assert.equal(p.fx.phase, 148, 'only the tick-141 use counted');
  assert.equal(p.cards.active?.ready, 281);
});

test('Phase works while standing still: the use counts and the head is drawn see-through until the phase ends', () => {
  const s = arena();
  const p = human(s, 'P', 10, 10);
  give(s, p, 'phase');
  checked(s, [[p, [ACT.active]]]);
  assert.equal(p.fx.phase, 8);
  assert.equal(p.cards.active?.ready, 141);
  assert.deepEqual([p.x, p.y], [10, 10]);
  assert.equal(effectFlags(p, s.tick) & 8, 8);
  checked(s, [[p, rep(null, 7)]]); // tick 8: the phase is over
  assert.equal(effectFlags(p, s.tick) & 8, 0);
});

test('predictUse matches the server: the client sets fx.phase to at + 7 and nothing else', () => {
  /** @type {Record<string, number>} */
  const fx = {};
  CARDS.phase.predictUse?.(1, fx, 10);
  assert.deepEqual(fx, { phase: 17 });
});

test('botScore: 0.95 when the tile ahead holds a trail that is not an enemy head; 0 otherwise', () => {
  /** @param {GameState} s @param {Player} p */
  const score = (s, p) => CARDS.phase.botScore?.(s, p, 1) ?? 0;

  // An enemy trail ahead (A's, its head is at x 20 by tick 10).
  const enemy = arena(40, 40);
  const a = human(enemy, 'A', 10, 20);
  const b = human(enemy, 'B', 14, 25);
  checked(enemy, [
    [a, rep(R, 10)],
    [b, [U, U, U, U, 'stop']], // B stops on (14, 21) facing up, with the trail on (14, 20)
  ]);
  assert.equal(score(enemy, b), 0.95);

  // An enemy head ahead: 0 (phasing does not stop the kill).
  const head = arena(40, 40);
  const a2 = human(head, 'A', 10, 20);
  const b2 = human(head, 'B', 14, 25);
  checked(head, [
    [a2, rep(R, 4)],
    [b2, rep(U, 4)],
  ]); // A's head is on (14, 20) when B is on (14, 21)
  assert.equal(score(head, b2), 0);

  // Its own trail ahead: 0.95 (the loop closing onto itself).
  const own = arena(40, 40);
  const p = human(own, 'P', 10, 20);
  checked(own, [[p, [R, R, R, R, U, U, 3, 2]]]); // P faces down at (13, 19); its trail is on (13, 20)
  assert.equal(score(own, p), 0.95);

  // Standing still: 0. Empty ground ahead: 0.
  const empty = arena(40, 40);
  const e = human(empty, 'E', 10, 10);
  assert.equal(score(empty, e), 0, 'not moving');
  checked(empty, [[e, [U]]]);
  assert.equal(score(empty, e), 0, 'empty ground ahead');

  // At the map edge the tile ahead is off the map: 0, no crash.
  const edge = arena(40, 40);
  const q = human(edge, 'Q', 10, 10);
  checked(edge, [[q, rep(3, 10)]]); // walks left to (0, 10)
  assert.equal(score(edge, q), 0);
});

test('effectFlags sees the phase (flag 8) and nothing else for a phased head', () => {
  const s = arena();
  const p = human(s, 'P', 10, 10);
  p.fx.phase = s.tick + 7;
  assert.equal(effectFlags(p, s.tick), 8);
});
