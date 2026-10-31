// @ts-check
// EMP (item, epic): enemies within 6 tiles cannot use cards for 21 ticks, and their recharges wait.
import assert from 'node:assert/strict';
import { ACT, CARDS, fxOn } from '../../shared/cards.js';
import { takeEvents } from '../../shared/rules.js';
import { arena, human, give, run, rep, broken, R } from './helpers.js';

test('EMP hits only living enemies within 6 tiles (Chebyshev), not the owner', () => {
  const s = arena(30, 30);
  const a = human(s, 'A', 10, 10);
  const near = human(s, 'Near', 16, 10); // 6 tiles to the right
  const diag = human(s, 'Diag', 4, 4); // 6 tiles, diagonal
  const far = human(s, 'Far', 10, 17); // 7 tiles down
  give(s, a, 'emp');
  run(s, [[a, [ACT.item0]]]);
  const until = s.tick + 21;
  assert.equal(near.fx.emp, until);
  assert.equal(diag.fx.emp, until);
  assert.equal(far.fx.emp, undefined);
  assert.equal(a.fx.emp, undefined);
  assert.equal(a.cards.items.length, 0);
  const fx = takeEvents(s).filter((e) => e.k === 'fx');
  assert.deepEqual(fx, [{ k: 'fx', card: 'emp', id: a.id, x: 10, y: 10, n: 2 }]);
  assert.equal(broken(s), null);
});

test('a hit enemy cannot use its item or active for 21 ticks, then can', () => {
  const s = arena(30, 30);
  const a = human(s, 'A', 9, 16);
  const b = human(s, 'B', 6, 10);
  give(s, a, 'emp');
  give(s, b, 'claimflag');
  give(s, b, 'dash');
  run(s, [[b, [R]]]); // tick 1: B heads right
  run(s, [[b, ['stop']]]); // tick 2: B waits
  // Tick 3: A fires EMP; B then tries its item and its active on each of the 21 ticks it is blocked.
  run(s, [
    [a, [ACT.item0]],
    [b, rep([ACT.item0, ACT.active], 21)],
  ]);
  assert.equal(s.tick, 23);
  assert.equal(b.cards.items.length, 1, 'item kept while blocked');
  assert.equal(b.cards.active?.id, 'dash');
  assert.equal(b.fx.dash, undefined, 'active did nothing while blocked');
  assert.equal(broken(s), null);
  run(s, [[b, [ACT.item0, ACT.active]]]); // tick 24: EMP is over
  assert.equal(b.cards.items.length, 0, 'item used');
  assert.ok(fxOn(b, 'dash', s.tick), 'active used');
  assert.equal(broken(s), null);
});

test("an EMP'd enemy's active recharge does not run down; it is ready 21 ticks later", () => {
  const s = arena(30, 30);
  const a = human(s, 'A', 9, 16);
  const b = human(s, 'B', 6, 10);
  give(s, a, 'emp');
  give(s, b, 'dash');
  run(s, [[b, [R]]]); // tick 1: B heads right
  run(s, [[b, [ACT.active]]]); // tick 2: dash, recharging from here
  run(s, [[b, ['stop']]]); // tick 3: B stops
  const before = b.cards.active?.ready ?? 0;
  assert.ok(before > s.tick, 'dash is recharging');
  run(s, [[a, [ACT.item0]]]); // tick 4: EMP
  run(s, [[b, rep(null, 25)]]); // ticks 5..29, EMP lasts through tick 24
  assert.equal(b.cards.active?.ready, before + 21);
  assert.equal(fxOn(b, 'emp', s.tick), false);
  assert.equal(broken(s), null);
});

test('with nobody in range the EMP is kept', () => {
  const s = arena(30, 30);
  const a = human(s, 'A', 10, 10);
  const far = human(s, 'Far', 10, 17); // 7 tiles away
  give(s, a, 'emp');
  run(s, [[a, [ACT.item0]]]);
  assert.equal(a.cards.items.length, 1);
  assert.equal(far.fx.emp, undefined);
  assert.equal(takeEvents(s).filter((e) => e.k === 'fx').length, 0);
  assert.equal(broken(s), null);
});

test('botScore is 0.7 only when an enemy within 6 holds an item or an active', () => {
  const s = arena(30, 30);
  const a = human(s, 'A', 10, 10);
  const far = human(s, 'Far', 10, 20);
  give(s, far, 'claimflag');
  const score = () => CARDS.emp.botScore?.(s, a, 1);
  assert.equal(score(), 0, 'the item holder is out of range');
  const near = human(s, 'Near', 16, 10);
  give(s, near, 'secondwind'); // a passive does not count
  assert.equal(score(), 0);
  give(s, near, 'dash'); // an active does
  assert.equal(score(), 0.7);
});
