// @ts-check
// Rewind: slide back along your trail (up to 8 tiles), landing on the new last tile or, when the whole trail
// goes, on your base next to where the trail began.
import assert from 'node:assert/strict';
import { CARDS, useCard } from '../../shared/cards.js';
import { spawnPlayerAt, step, takeEvents } from '../../shared/rules.js';
import { arena, human, give, run, rep, baseAt, trailAt, broken, U, R, D, L, ACT } from './helpers.js';

/** @typedef {import('../../shared/types.js').GameState} GameState */
/** @typedef {import('../../shared/types.js').Player} Player */

/** Press Shift (the active ability) at the next tick. @param {GameState} s @param {Player} p */
const shift = (s, p) => step(s, [{ id: p.id, turns: [], cards: [ACT.active] }]);

/**
 * V starts on its base at (10, 20) and walks up 11 tiles (the first lands on its base): its trail runs
 * y = 18 down to y = 9 in walk order, trail[0] = (10,18), head (10,9). Nothing else is used yet.
 */
function walkUp10() {
  const s = arena(30, 30);
  const v = human(s, 'V', 10, 20);
  give(s, v, 'rewind');
  run(s, [[v, rep(U, 11)]]);
  return { s, v };
}

test('after 10 tiles out, rewind puts the head 8 tiles back along its path, trail length 2, stopped', () => {
  const { s, v } = walkUp10();
  assert.equal(v.trail.length, 10);
  assert.deepEqual([v.x, v.y], [10, 9]);
  run(s, [[v, [ACT.active]]]);
  assert.deepEqual([v.x, v.y], [10, 17], '8 tiles back: trail[1] of the 10');
  assert.equal(v.trail.length, 2);
  assert.equal(v.stopped, true);
  assert.deepEqual(v.queue, []);
  assert.equal(trailAt(s, 10, 9), 0);
  assert.equal(trailAt(s, 10, 16), 0, 'erased');
  assert.equal(trailAt(s, 10, 18), v.id, 'kept');
  assert.equal(broken(s), null);
  // The fx event: the landing tile, and the old head.
  // `from` is not in GameEvent (the card adds it), so the event is read loosely here.
  const fx = /** @type {any} */ (takeEvents(s).find((e) => e.k === 'fx' && e.card === 'rewind'));
  assert.ok(fx, 'an fx event');
  assert.deepEqual([fx.id, fx.x, fx.y, fx.from], [v.id, 10, 17, 9 * 30 + 10]);
});

test('with a 3-tile trail, rewind lands on its own base with no trail, not trailing', () => {
  const s = arena(30, 30);
  const v = human(s, 'V', 10, 20);
  give(s, v, 'rewind');
  // Out to the right: (11,20) is base, then trail at (12,20), (13,20), (14,20).
  run(s, [[v, [...rep(R, 4), ACT.active]]]);
  // Its first trail tile (12,20) touches base at (11,20) to the left; up/right/down are not base.
  assert.deepEqual([v.x, v.y], [11, 20]);
  assert.equal(v.trail.length, 0);
  assert.equal(baseAt(s, 11, 20), v.id);
  assert.equal(trailAt(s, 12, 20), 0);
  assert.equal(trailAt(s, 14, 20), 0);
  assert.equal(v.stopped, true);
  assert.equal(broken(s), null);
});

test('after a rewind home, moving out again lays a fresh trail, and coming back captures it', () => {
  const s = arena(30, 30);
  const v = human(s, 'V', 10, 20);
  give(s, v, 'rewind');
  run(s, [[v, [...rep(R, 4), ACT.active]]]);
  const landed = v.land;
  // A key resumes the stopped head at once: right, right, then down and back left to the base.
  run(s, [[v, [R, R]]]);
  assert.deepEqual([v.x, v.y], [13, 20]);
  assert.deepEqual(v.trail, [12 + 20 * 30, 13 + 20 * 30]);
  assert.equal(broken(s), null);
  run(s, [[v, [D, L, L]]]);
  assert.deepEqual([v.x, v.y], [11, 21]);
  assert.equal(v.trail.length, 0, 'back on the base: the trail is captured');
  assert.ok(v.land > landed, `land ${v.land} vs ${landed}`);
  assert.equal(broken(s), null);
});

test('a trail of exactly 8 tiles is all erased and lands on the base too', () => {
  const s = arena(30, 30);
  const v = human(s, 'V', 10, 20);
  give(s, v, 'rewind');
  // Down out of the base: (10,21) is base, then trail (10,22), (10,23), (10,24), then right to (15,24).
  run(s, [[v, [...rep(D, 4), ...rep(R, 5), ACT.active]]]);
  assert.equal(v.trail.length, 0);
  // The first trail tile (10,22) touches the base at (10,21) above it.
  assert.deepEqual([v.x, v.y], [10, 21]);
  assert.equal(v.stopped, true);
  assert.equal(broken(s), null);
});

test('recharge: a second use before 105 ticks does nothing; it works again on the tick it ends', () => {
  const { s, v } = walkUp10();
  shift(s, v); // tick 12
  assert.deepEqual([v.x, v.y], [10, 17]);
  const ready = v.cards.active?.ready ?? 0;
  assert.equal(ready, s.tick + 105, 'recharge of 105 ticks started');
  // The next tick: nothing happens, the trail stays.
  shift(s, v);
  assert.deepEqual([v.x, v.y], [10, 17]);
  assert.equal(v.trail.length, 2);
  // Up to the tick before the recharge ends: still nothing.
  step(s, [], ready - 2 - s.tick); // so the next shift is at tick ready - 1
  shift(s, v);
  assert.equal(s.tick, ready - 1);
  assert.deepEqual([v.x, v.y], [10, 17]);
  assert.equal(v.trail.length, 2);
  // The tick the recharge ends (a use happens when tick >= ready): the 2-tile trail goes, and the head
  // lands on its base beside (10,18), the tile below it.
  shift(s, v);
  assert.equal(s.tick, ready);
  assert.deepEqual([v.x, v.y], [10, 19]);
  assert.equal(v.trail.length, 0);
  assert.equal(baseAt(s, 10, 19), v.id);
  assert.equal(broken(s), null);
});

test('with no trail, rewind fails and starts no recharge; it can be used at once after getting a trail', () => {
  const s = arena(30, 30);
  const v = human(s, 'V', 10, 20);
  give(s, v, 'rewind');
  const ready = v.cards.active?.ready ?? -1;
  step(s, []);
  assert.equal(useCard(s, v, -1), false, 'no trail');
  assert.equal(v.cards.active?.ready, ready, 'no recharge');
  takeEvents(s);
  // Out to (13,20) and back home: the trail is 2 tiles, and the rewind works straight away.
  run(s, [[v, [R, R, R]]]);
  assert.equal(v.trail.length, 2);
  run(s, [[v, [ACT.active]]]);
  assert.deepEqual([v.x, v.y], [11, 20]);
  assert.equal(v.trail.length, 0);
  const uses = takeEvents(s).filter((e) => e.k === 'use');
  assert.equal(uses.length, 1, 'one use, not two');
  assert.equal(broken(s), null);
});

test('an enemy moving onto a trail tile that the rewind erased is not killed (rewind runs first in the tick)', () => {
  // V's trail runs up x = 10 from y = 18 to y = 9. E walks left along y = 12 and steps onto (10,12) at the
  // tick V presses Shift. That tile is one of the 8 erased, so after the rewind it is neutral: no kill.
  const s = arena(30, 30);
  const v = human(s, 'V', 10, 20);
  const e = human(s, 'E', 14, 12);
  give(s, v, 'rewind');
  run(s, [
    [v, [...rep(U, 11), ACT.active]],
    [e, [...rep(null, 8), L, L, L, L]],
  ]);
  assert.equal(v.alive, true);
  assert.equal(e.alive, true);
  assert.deepEqual([v.x, v.y], [10, 17]);
  assert.deepEqual([e.x, e.y], [10, 12]);
  assert.equal(trailAt(s, 10, 12), e.id, 'E lays its trail on the erased tile');
  assert.equal(broken(s), null);
});

test('an enemy stepping onto a trail tile the rewind kept still kills the rewound head', () => {
  // E walks left along y = 18 and steps onto (10,18), V's first trail tile. Rewind leaves that tile in the
  // trail (V's head is now (10,17)), so the step is a trail hit: V dies, credited to E. The rewind did not
  // save V; it only moved the head back, and the kept trail stays dangerous.
  const s = arena(30, 30);
  const v = human(s, 'V', 10, 20);
  const e = human(s, 'E', 14, 18);
  give(s, v, 'rewind');
  run(s, [
    [v, [...rep(U, 11), ACT.active]],
    [e, [...rep(null, 8), L, L, L, L]],
  ]);
  assert.equal(v.alive, false);
  assert.equal(v.death?.cause, 'trail');
  assert.equal(v.death?.killer, 'E');
  assert.equal(e.alive, true);
  assert.equal(e.kills, 1);
  assert.equal(broken(s), null);
});

test('botScore: 0.85 when a bot flees with a trail of 4 or more, 0 otherwise', () => {
  const s = arena(30, 30);
  const b = spawnPlayerAt(s, { kind: 'bot', name: 'Bot' }, 20, 20);
  const score = () => CARDS.rewind?.botScore?.(s, b, 1);
  assert.equal(score(), 0, 'no trail, idle');
  b.trail = [1, 2, 3, 4];
  assert.equal(score(), 0, 'trail of 4 but not fleeing');
  if (b.bot) b.bot.mode = 'flee';
  assert.equal(score(), 0.85);
  b.trail = [1, 2, 3];
  assert.equal(score(), 0, 'fleeing with only 3 tiles');
  b.trail = [1, 2, 3, 4];
  if (b.bot) b.bot.mode = 'attack';
  assert.equal(score(), 0, 'attacking, not fleeing');
});

test('rewind is the active ability with a 105-tick cooldown', () => {
  const card = CARDS.rewind;
  assert.equal(card?.kind, 'active');
  assert.equal(card?.cooldown?.(1), 105);
  assert.equal(card?.maxLevel, 1);
});
