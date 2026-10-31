// @ts-check
// Snare (item, common): where it can be set, what it does to an enemy head, what it leaves the owner alone,
// how long it lasts, and how bots score it.
import assert from 'node:assert/strict';
import { CARDS, fxOn } from '../../shared/cards.js';
import { step, takeEvents } from '../../shared/rules.js';
import { arena, human, give, run, rep, baseAt, broken, ACT, R, L } from './helpers.js';

/** @typedef {import('../../shared/types.js').GameState} GameState */

/** @param {GameState} s */
const snares = (s) => s.objects.filter((o) => o.kind === 'snare');

test('a snare needs your own base with no trail: trailing fails and the item is kept', () => {
  const s = arena();
  const p = human(s, 'P', 10, 10);
  give(s, p, 'snare');
  run(s, [[p, [R, R, R]]]); // out to (13, 10), trailing
  assert.equal(p.trail.length, 2);
  assert.notEqual(baseAt(s, p.x, p.y), p.id, 'off its base');
  run(s, [[p, [ACT.item0]]]);
  assert.equal(snares(s).length, 0);
  assert.equal(p.cards.items.length, 1, 'kept');
  assert.equal(broken(s), null);
});

test('at most two snares per owner, and never two on one tile', () => {
  const s = arena();
  const p = human(s, 'P', 10, 10);
  give(s, p, 'snare');
  give(s, p, 'snare');
  run(s, [[p, [ACT.item0]]]);
  assert.equal(snares(s).length, 1);
  assert.equal(p.cards.items.length, 1);
  // Same tile: a snare already sits there.
  run(s, [[p, [ACT.item0]]]);
  assert.equal(snares(s).length, 1);
  assert.equal(p.cards.items.length, 1, 'kept');
  // Second tile: stop after one step right, then place.
  run(s, [[p, [R]]]);
  run(s, [[p, ['stop']]]);
  run(s, [[p, [ACT.item0]]]);
  assert.equal(snares(s).length, 2);
  // A third is refused while two are live.
  give(s, p, 'snare');
  run(s, [[p, [ACT.item0]]]);
  assert.equal(snares(s).length, 2);
  assert.equal(p.cards.items.length, 1, 'kept');
  assert.equal(broken(s), null);
});

test('an enemy head that steps on a snare is stuck for 10 ticks, then moves on; the snare is spent', () => {
  const s = arena();
  const a = human(s, 'A', 10, 10);
  const b = human(s, 'B', 5, 10);
  give(s, a, 'snare');
  run(s, [[a, [ACT.item0]]]); // placed on tick 1
  run(s, [[b, [R, R, R, R, R]]]); // ticks 2-6: B reaches (10, 10) on tick 6
  assert.equal(s.tick, 6);
  assert.deepEqual([b.x, b.y], [10, 10]);
  assert.equal(b.fx.stun, 17);
  assert.equal(snares(s).length, 0, 'spent');
  const ev = takeEvents(s).filter((e) => e.k === 'fx' && e.card === 'snare');
  assert.equal(ev.length, 1);
  assert.equal(ev[0].v, b.id);
  assert.equal(ev[0].id, a.id);
  run(s, [[b, rep(null, 10)]]); // ticks 7-16: frozen
  assert.deepEqual([b.x, b.y], [10, 10]);
  assert.ok(fxOn(b, 'stun', s.tick));
  run(s, [[b, [null]]]); // tick 17: moving again
  assert.deepEqual([b.x, b.y], [11, 10]);
  assert.ok(b.alive);
  assert.equal(broken(s), null);
});

test('the owner walking over its own snare is not stunned', () => {
  const s = arena();
  const a = human(s, 'A', 10, 10);
  give(s, a, 'snare');
  run(s, [[a, [ACT.item0]]]);
  run(s, [[a, [R]]]); // (11, 10)
  run(s, [[a, [L]]]); // back over the snare at (10, 10)
  assert.equal(a.x, 10);
  assert.equal(snares(s).length, 1, 'still there');
  assert.ok(!fxOn(a, 'stun', s.tick));
  run(s, [[a, [null]]]);
  assert.equal(a.x, 9, 'keeps moving');
  assert.equal(broken(s), null);
});

test('a snare lasts 420 ticks: a head arriving on its last live tick is caught, one arriving after is not', () => {
  for (const [arrive, caught] of /** @type {[number, boolean][]} */ ([
    [420, true],
    [421, false],
  ])) {
    const s = arena();
    const a = human(s, 'A', 10, 10);
    const b = human(s, 'B', 7, 10);
    give(s, a, 'snare');
    run(s, [[a, [ACT.item0]]]); // placed on tick 1, expires on tick 421
    step(s, [], arrive - 3 - s.tick);
    run(s, [[b, [R, R, R]]]); // B reaches (10, 10) on tick `arrive`
    assert.equal(s.tick, arrive);
    assert.equal(fxOn(b, 'stun', s.tick), caught, `arriving on tick ${arrive}`);
    assert.equal(snares(s).length, 0);
    assert.equal(broken(s), null);
  }
});

test('bot score: 0.7 only when it can set a snare now and an enemy is within 10 tiles; else 0', () => {
  const near = arena();
  const a = human(near, 'A', 10, 10);
  human(near, 'B', 20, 10); // Chebyshev distance 10
  assert.equal(CARDS.snare.botScore?.(near, a, 1), 0.7);

  const far = arena();
  const c = human(far, 'A', 10, 10);
  human(far, 'B', 21, 10); // distance 11
  assert.equal(CARDS.snare.botScore?.(far, c, 1), 0);

  // Trailing: no snare possible, even with an enemy close.
  run(near, [[a, [R, R, R]]]);
  assert.equal(CARDS.snare.botScore?.(near, a, 1), 0);
  assert.equal(broken(near), null);

  // Two live snares of its own: no room.
  const full = arena();
  const d = human(full, 'A', 10, 10);
  human(full, 'B', 20, 10);
  full.objects.push(
    { id: 1, kind: 'snare', owner: d.id, x: 9, y: 9, dir: 0, until: 999 },
    { id: 2, kind: 'snare', owner: d.id, x: 11, y: 11, dir: 0, until: 999 },
  );
  assert.equal(CARDS.snare.botScore?.(full, d, 1), 0);

  // A snare already on its tile: none can go there.
  const taken = arena();
  const e = human(taken, 'A', 10, 10);
  human(taken, 'B', 20, 10);
  taken.objects.push({ id: 1, kind: 'snare', owner: 99, x: 10, y: 10, dir: 0, until: 999 });
  assert.equal(CARDS.snare.botScore?.(taken, e, 1), 0);
});
