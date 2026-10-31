// @ts-check
// Mirage (item, common): movement, expiry, use rules, no effect on the grid, use event and the bot score.
import assert from 'node:assert/strict';
import { CARDS } from '../../shared/cards.js';
import { DX, DY } from '../../shared/grid.js';
import { spawnPlayerAt, step, takeEvents } from '../../shared/rules.js';
import { arena, human, give, run, broken, ACT, U, R, D, L } from './helpers.js';

/** @typedef {import('../../shared/types.js').GameState} GameState */
/** @typedef {import('../../shared/types.js').WorldObject} WorldObject */

/** @param {GameState} s @returns {WorldObject | undefined} */
const ghost = (s) => s.objects.find((o) => o.kind === 'mirage');

test('the card text is the spec text', () => {
  assert.equal(CARDS.mirage.text, 'Q/E: a ghost of you runs straight on for 4 s, to fool the people chasing you.');
});

test('the ghost starts on the owner head, runs 1 tile a tick in the owner direction, every direction', () => {
  for (const d of [U, R, D, L]) {
    const s = arena(30, 30);
    const o = human(s, 'O', 15, 15);
    give(s, o, 'mirage');
    run(s, [[o, [d]]]); // one tile in direction d
    run(s, [[o, ['stop']]]); // hold still on the new tile
    const hx = o.x;
    const hy = o.y;
    run(s, [[o, [ACT.item0]]]); // the ghost is put at the head and moves on this same tick
    const g = ghost(s);
    assert.ok(g, `dir ${d}: a ghost is in the world`);
    assert.equal(g.owner, o.id);
    assert.equal(g.dir, d);
    assert.deepEqual([g.x, g.y], [hx + DX[d], hy + DY[d]], `dir ${d}: first tile`);
    for (let k = 2; k <= 5; k++) {
      step(s);
      assert.deepEqual([g.x, g.y], [hx + k * DX[d], hy + k * DY[d]], `dir ${d}: tick ${k}`);
    }
    assert.deepEqual([o.x, o.y], [hx, hy], `dir ${d}: the owner stays put`);
    assert.equal(broken(s), null);
  }
});

test('the ghost lasts 28 ticks from the use tick, and is gone after the 28th', () => {
  const s = arena(60, 30);
  const o = human(s, 'O', 5, 15);
  give(s, o, 'mirage');
  run(s, [[o, [R]]]);
  run(s, [[o, ['stop']]]);
  run(s, [[o, [ACT.item0]]]);
  const g = ghost(s);
  assert.ok(g);
  assert.equal(g.until, s.tick + 28);
  for (let k = 0; k < 27; k++) step(s);
  assert.equal(ghost(s), g, 'still alive on the 27th tick after use');
  step(s);
  assert.equal(ghost(s), undefined, 'gone on the tick it ends on');
  assert.equal(broken(s), null);
});

test('the ghost is gone at the map edge: it stops there and disappears that tick', () => {
  const s = arena(30, 30);
  const o = human(s, 'O', 26, 15);
  give(s, o, 'mirage');
  run(s, [[o, [R]]]); // to (27, 15)
  run(s, [[o, ['stop']]]);
  run(s, [[o, [ACT.item0]]]); // the ghost is put at (27, 15) and moves on to (28, 15) on this tick
  const g = ghost(s);
  assert.ok(g);
  assert.deepEqual([g.x, g.y], [28, 15]);
  step(s);
  assert.deepEqual([g.x, g.y], [29, 15]);
  step(s); // the next tile is off the map: it stays and is removed
  assert.equal(ghost(s), undefined);
  assert.equal(broken(s), null);
});

test('the ghost does not change the grid: base and trail are identical with and without it over 28 ticks', () => {
  /** A scene where nobody else moves: E has laid a trail up column 20 that the ghost will cross. */
  const setup = () => {
    const s = arena(30, 30);
    const o = human(s, 'O', 5, 5);
    const e = human(s, 'E', 20, 10);
    give(s, o, 'mirage');
    run(s, [
      [e, [U, null, null, null, null, null]],
      [o, [R, null, null, null]],
    ]);
    run(s, [[e, ['stop']], [o, ['stop']]]);
    return { s, o };
  };
  /** @param {boolean} withGhost */
  const play = (withGhost) => {
    const { s, o } = setup();
    let reached = -1;
    // 29 ticks: the use is the first, the ghost is gone on the 29th (it lives 28 ticks from the use).
    for (let k = 1; k <= 29; k++) {
      if (k === 1 && withGhost) run(s, [[o, [ACT.item0]]]);
      else step(s);
      const g = ghost(s);
      if (g) reached = Math.max(reached, g.x);
    }
    return { s, reached };
  };
  const plain = play(false);
  const ghosted = play(true);
  assert.equal(ghost(ghosted.s), undefined);
  assert.ok(ghosted.reached >= 20, 'the ghost ran across the other trail');
  assert.deepEqual(ghosted.s.base, plain.s.base);
  assert.deepEqual(ghosted.s.trail, plain.s.trail);
  assert.equal(broken(ghosted.s), null);
});

test('a ghost lays no trail and kills nobody: heads it passes through live, and no trail is cut', () => {
  const s = arena(30, 30);
  const o = human(s, 'O', 5, 10);
  const e = human(s, 'E', 9, 10);
  give(s, o, 'mirage');
  run(s, [[e, [U, null, null, null]]]); // E lays a short trail up from (9, 10), then stands still
  run(s, [[e, ['stop']]]);
  const eTrail = e.trail.slice();
  run(s, [[o, [R]]]); // O to (6, 10)
  run(s, [[o, ['stop']]]);
  run(s, [[o, [ACT.item0]]]); // the ghost starts at (6, 10) and runs east through E
  for (let k = 0; k < 6; k++) step(s);
  assert.equal(ghost(s)?.x, 13);
  assert.equal(e.alive, true);
  assert.deepEqual(e.trail, eTrail);
  assert.equal(broken(s), null);
});

test('a stopped head on the ghost path is neither hit nor blocked: the ghost passes over it', () => {
  const s = arena(30, 30);
  const o = human(s, 'O', 5, 12);
  const e = human(s, 'E', 9, 12);
  give(s, o, 'mirage');
  run(s, [[o, [R]]]);
  run(s, [[o, ['stop']]]);
  run(s, [[o, [ACT.item0]]]); // the ghost is at (7, 12)
  step(s);
  step(s); // the ghost is now on E's tile (9, 12)
  assert.equal(ghost(s)?.x, 9);
  assert.equal(e.alive, true);
  assert.deepEqual([e.x, e.y], [9, 12]);
  assert.equal(broken(s), null);
});

test('a head with no direction cannot use the mirage: nothing is placed and the card stays in hand', () => {
  const s = arena(30, 30);
  const p = human(s, 'P', 10, 10);
  give(s, p, 'mirage');
  run(s, [[p, [ACT.item0]]]);
  assert.equal(ghost(s), undefined);
  assert.equal(p.cards.items.length, 1);
  assert.equal(broken(s), null);
});

test('using the mirage consumes the card and sends a fx event at the head', () => {
  const s = arena(30, 30);
  const o = human(s, 'O', 10, 10);
  give(s, o, 'mirage');
  run(s, [[o, [R]]]);
  run(s, [[o, ['stop']]]);
  takeEvents(s);
  run(s, [[o, [ACT.item0]]]);
  assert.equal(o.cards.items.length, 0);
  const fx = takeEvents(s).filter((e) => e.k === 'fx');
  assert.deepEqual(fx, [{ k: 'fx', card: 'mirage', id: o.id, x: 11, y: 10 }]);
  assert.equal(broken(s), null);
});

test('botScore: 0.75 for a fleeing bot with an enemy head within 6 tiles, else 0', () => {
  const s = arena(30, 30);
  const b = spawnPlayerAt(s, { kind: 'bot', name: 'B' }, 15, 15);
  b.dir = R;
  b.stopped = false;
  const mode = b.bot;
  assert.ok(mode);
  const score = () => CARDS.mirage.botScore?.(s, b, 1) ?? 0;
  const e = human(s, 'E', 15, 21); // 6 tiles straight down
  assert.equal(score(), 0, 'not fleeing');
  mode.mode = 'flee';
  assert.equal(score(), 0.75, '6 tiles away');
  e.x = 18;
  e.y = 18; // 3 + 3 = 6 tiles, diagonal
  assert.equal(score(), 0.75, 'diagonal 6 tiles');
  e.x = 18;
  e.y = 19; // 3 + 4 = 7 tiles
  assert.equal(score(), 0, 'too far');
  e.x = 15;
  e.y = 21;
  e.alive = false; // dead enemies do not count
  assert.equal(score(), 0, 'dead enemy');
});
