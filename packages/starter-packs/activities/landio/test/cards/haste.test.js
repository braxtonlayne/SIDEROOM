// @ts-check
// Haste: extra tiles on the ticks it names, and a hasted head still cannot jump a trail.
import assert from 'node:assert/strict';
import { CARDS } from '../../shared/cards.js';
import { arena, human, give, run, rep, broken, U, R } from './helpers.js';

/** Extra tiles the card gives over `n` ticks starting after tick `from`: one for each tick that is a multiple of `period`. */
const extrasOver = (/** @type {number} */ from, /** @type {number} */ n, /** @type {number} */ period) => {
  let k = 0;
  for (let t = from + 1; t <= from + n; t++) if (t % period === 0) k++;
  return k;
};

/** Tiles a head covers in a straight run of 60 ticks east, from a known tick. */
function straightRun(/** @type {number} */ levels) {
  const s = arena(200, 200);
  const p = human(s, 'P', 5, 100);
  for (let k = 0; k < levels; k++) give(s, p, 'haste');
  const x0 = p.x;
  const from = s.tick;
  run(s, [[p, rep(R, 60)]]);
  assert.equal(broken(s), null);
  return { moved: p.x - x0, from, p, s };
}

test('Haste is a passive common card with two levels', () => {
  assert.equal(CARDS.haste.kind, 'passive');
  assert.equal(CARDS.haste.rarity, 'common');
  assert.equal(CARDS.haste.maxLevel, 2);
});

test('speed: no extra tile at level 0; level I every 6 ticks; level II every 3; pure', () => {
  const speed = /** @type {(level: number, fx: Record<string, number>, tick: number) => number} */ (CARDS.haste.speed);
  const none = /** @type {Record<string, number>} */ ({});
  for (let t = 0; t < 30; t++) {
    assert.equal(speed(0, none, t), 0, `level 0 at ${t}`);
    assert.equal(speed(1, none, t), t % 6 === 0 ? 1 : 0, `level I at ${t}`);
    assert.equal(speed(2, none, t), t % 3 === 0 ? 1 : 0, `level II at ${t}`);
  }
  // Pure: the same arguments give the same answer and the fx record is not touched.
  const fx = { dash: 99 };
  assert.equal(speed(2, fx, 9), 1);
  assert.deepEqual(fx, { dash: 99 });
});

test('without Haste a straight run of 60 ticks covers exactly 60 tiles', () => {
  const { moved } = straightRun(0);
  assert.equal(moved, 60);
});

test('Haste I: a straight run of 60 ticks covers 60 + the extra tiles of its ticks', () => {
  const { moved, from } = straightRun(1);
  const extra = extrasOver(from, 60, 6);
  assert.equal(extra, 10, 'a 60-tick window holds 10 multiples of 6 from any start tick');
  assert.equal(moved, 60 + extra);
});

test('Haste II: a straight run of 60 ticks covers 60 + the extra tiles of its ticks', () => {
  const { moved, from } = straightRun(2);
  const extra = extrasOver(from, 60, 3);
  assert.equal(extra, 20);
  assert.equal(moved, 60 + extra);
});

test('a hasted head still cannot jump a trail: it dies on the trail it steps onto mid-jump', () => {
  const s = arena(40, 40);
  // A's spawn base is 5x5 around (10, 28); moving up it lays a trail at x = 10 from y = 25 upward, so the tile
  // (10, 24) is trail from tick 4 on.
  const a = human(s, 'A', 10, 28);
  const b = human(s, 'B', 4, 24);
  give(s, b, 'haste'); // level I: the first hasted tick is tick 6
  assert.equal(s.tick, 0);
  // B walks east along y = 24: ticks 1-5 take it to x = 9; tick 6 is hasted, so it steps onto (10, 24) and
  // would carry on to x = 11 in the same tick if the extra tile were not a sub-step.
  run(s, [
    [a, rep(U, 8)],
    [b, rep(R, 8)],
  ]);
  assert.equal(a.alive, false);
  assert.equal(a.death?.killer, 'B');
  assert.equal(a.death?.tick, 6);
  assert.equal(broken(s), null);
});
