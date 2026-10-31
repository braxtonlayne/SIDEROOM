// @ts-check
// Grenade: the throw (distance, clamping, direction), the fuse, the blast on land, trails and heads, bot score.
// A turn is applied at the next tile, so a throw comes a tick after the turn that sets the direction.
import assert from 'node:assert/strict';
import { ACT, CARDS } from '../../shared/cards.js';
import { idx, setBase } from '../../shared/grid.js';
import { arena, human, give, run, rep, baseAt, trailAt, broken, U, R, L } from './helpers.js';

/** @typedef {import('../../shared/types.js').GameState} GameState */
/** @typedef {import('../../shared/types.js').Player} Player */

/** Paint `p`'s land on the square of radius r around (x, y). @param {GameState} s @param {Player} p @param {number} x @param {number} y @param {number} r */
function paint(s, p, x, y, r) {
  for (let dy = -r; dy <= r; dy++) for (let dx = -r; dx <= r; dx++) setBase(s, idx(s, x + dx, y + dy), p.id);
}

/** The card effect events of a given phase ('throw' | 'boom') in the last tick. @param {GameState} s @param {string} phase */
const fxPhase = (s, phase) => s.events.filter((ev) => ev.k === 'fx' && /** @type {any} */ (ev).phase === phase);

/** Thrower A starts at (17, 9) holding a grenade; enemy E's land painted around the landing tile (10, 9). */
function setup() {
  const s = arena(30, 30);
  const a = human(s, 'A', 17, 9);
  const e = human(s, 'E', 25, 25);
  give(s, a, 'grenade');
  paint(s, e, 10, 9, 1);
  return { s, a, e };
}

/** Tick 1 turns left (the head reaches (16, 9)); tick 2 throws to (10, 9) and stops. The blast is on tick 7 (FUSE 5). */
/** @type {(number | 'stop' | (number | 'stop')[])[]} */
const THROW = [[L], [ACT.item0, 'stop']];

test('a grenade lands 6 tiles ahead of the head, and the item is used up', () => {
  const { s, a } = setup();
  run(s, [[a, THROW]]);
  const o = s.objects.find((x) => x.kind === 'grenade');
  assert.ok(o, 'a grenade is in the air');
  assert.deepEqual([o.x, o.y, o.owner], [10, 9, a.id]);
  assert.equal(a.cards.items.length, 0);
  assert.ok(fxPhase(s, 'throw').some((ev) => ev.card === 'grenade' && ev.x === 10 && ev.y === 9));
  assert.equal(broken(s), null);
});

test('the landing tile is clamped to the grid edge', () => {
  const east = arena(30, 30);
  const a = human(east, 'A', 27, 3);
  give(east, a, 'grenade');
  run(east, [[a, [[R], [ACT.item0, 'stop']]]]); // lands at x 34, clamped to 29
  const o = east.objects.find((x) => x.kind === 'grenade');
  assert.deepEqual([o?.x, o?.y], [29, 3]);
  assert.equal(broken(east), null);

  const north = arena(30, 30);
  const b = human(north, 'B', 4, 2);
  give(north, b, 'grenade');
  run(north, [[b, [[U], [ACT.item0, 'stop']]]]); // lands at y -4, clamped to 0
  const n = north.objects.find((x) => x.kind === 'grenade');
  assert.deepEqual([n?.x, n?.y], [4, 0]);
  assert.equal(broken(north), null);
});

test('nothing happens for 4 ticks after the throw; the blast comes on the 5th', () => {
  const { s, a, e } = setup();
  run(s, [[a, THROW]]);
  run(s, [[a, rep(null, 4)]]);
  assert.equal(s.objects.filter((x) => x.kind === 'grenade').length, 1, 'still in the air after 4 ticks');
  assert.equal(baseAt(s, 10, 9), e.id, 'no blast yet');
  assert.equal(fxPhase(s, 'boom').length, 0);
  run(s, [[a, [null]]]);
  assert.equal(baseAt(s, 10, 9), 0, 'the blast clears the enemy land at the centre');
  assert.ok(fxPhase(s, 'boom').some((ev) => ev.card === 'grenade' && ev.x === 10 && ev.y === 9));
  assert.equal(s.objects.filter((x) => x.kind === 'grenade').length, 0, 'the grenade is gone');
  assert.equal(broken(s), null);
});

test('enemy land in the radius goes neutral; the thrower keeps its own; land outside stays', () => {
  const { s, a, e } = setup();
  paint(s, e, 10, 11, 0); // in the blast (distance 2)
  paint(s, e, 10, 12, 0); // outside it (distance 3)
  paint(s, a, 10, 7, 0); // the thrower's own land, in the blast (distance 2)
  const before = e.land;
  run(s, [[a, THROW]]);
  run(s, [[a, rep(null, 7)]]);
  assert.equal(baseAt(s, 9, 8), 0, 'enemy land at distance 2 goes neutral');
  assert.equal(baseAt(s, 10, 11), 0, 'enemy land at distance 2 goes neutral');
  assert.equal(baseAt(s, 10, 12), e.id, 'enemy land at distance 3 stays');
  assert.equal(baseAt(s, 10, 7), a.id, 'the thrower keeps its own land in the blast');
  assert.ok(e.land < before);
  assert.equal(broken(s), null);
});

test('an enemy trail through the blast is cut, and the enemy lives when its head is 2+ away', () => {
  const s = arena(30, 30);
  const a = human(s, 'A', 17, 9);
  const e = human(s, 'E', 10, 20);
  give(s, a, 'grenade');
  // E runs straight up x = 10 from tick 2: after 15 ticks its head is at (10, 6), with trail through (10, 7)..(10, 11).
  // A throws on tick 11 (landing (10, 9)), so the blast is on tick 16.
  run(s, [
    [e, [null, U, ...rep(null, 13)]],
    [a, [...rep(null, 9), [L], [ACT.item0, 'stop']]],
  ]);
  assert.equal(trailAt(s, 10, 8), e.id, 'the trail runs through the blast');
  assert.equal(broken(s), null);
  run(s, [[e, [null]]]); // tick 16: E moves to (10, 5) and the blast lands
  assert.equal(e.alive, true);
  assert.deepEqual([e.x, e.y], [10, 5]);
  assert.deepEqual(e.trail, [idx(s, 10, 6), idx(s, 10, 5)], 'the trail is cut back past the blast');
  assert.equal(trailAt(s, 10, 7), 0);
  assert.equal(trailAt(s, 10, 6), e.id);
  assert.equal(broken(s), null);
});

test('an enemy head 1 tile from the centre, off its base, dies and the thrower gets the kill', () => {
  const s = arena(30, 30);
  const a = human(s, 'A', 17, 9);
  const e = human(s, 'E', 11, 20);
  give(s, a, 'grenade');
  // Throw on tick 6 (blast on tick 11); E runs up x = 11 and is at (11, 9) on tick 11: 1 tile from (10, 9).
  run(s, [
    [a, [null, null, null, null, [L], [ACT.item0, 'stop'], ...rep(null, 7)]],
    [e, rep(U, 11)],
  ]);
  assert.equal(e.alive, false);
  assert.equal(e.death?.cause, 'grenade');
  assert.equal(e.death?.killer, 'A');
  assert.equal(a.kills, 1);
  assert.equal(a.alive, true);
  assert.ok(s.events.some((ev) => ev.k === 'kill' && ev.c === 'grenade' && ev.v === e.id));
  assert.equal(broken(s), null);
});

test('an enemy head 1 tile from the centre but on its own base survives', () => {
  const s = arena(30, 30);
  const a = human(s, 'A', 17, 9);
  const e = human(s, 'E', 11, 9);
  give(s, a, 'grenade');
  run(s, [[a, [null, null, [L], [ACT.item0, 'stop'], ...rep(null, 7)]]]);
  assert.equal(e.alive, true);
  assert.equal(baseAt(s, 11, 9), e.id, 'its own tile is kept');
  assert.equal(a.kills, 0);
  assert.equal(broken(s), null);
});

test('an enemy head 2 tiles from the centre on its own base keeps its tile', () => {
  const s = arena(30, 30);
  const a = human(s, 'A', 17, 9);
  const e = human(s, 'E', 12, 9);
  give(s, a, 'grenade');
  run(s, [[a, [null, null, [L], [ACT.item0, 'stop'], ...rep(null, 7)]]]);
  assert.equal(e.alive, true);
  assert.equal(baseAt(s, 12, 9), e.id);
  assert.equal(broken(s), null);
});

test('the thrower dies to its own blast when it walks into it, with no killer credited', () => {
  const s = arena(30, 30);
  const a = human(s, 'A', 17, 9);
  give(s, a, 'grenade');
  // Throw on tick 2 (blast on tick 7) while walking left: the head reaches the centre (10, 9) on tick 7.
  run(s, [[a, [[L], [ACT.item0], ...rep(null, 6)]]]);
  assert.equal(a.alive, false);
  assert.equal(a.death?.cause, 'grenade');
  assert.equal(a.death?.killer, null);
  assert.equal(a.kills, 0);
  assert.equal(broken(s), null);
});

test('no direction yet: the grenade is not thrown and the item is kept', () => {
  const s = arena(30, 30);
  const a = human(s, 'A', 17, 9);
  give(s, a, 'grenade');
  run(s, [[a, [ACT.item0]]]);
  assert.equal(a.cards.items.length, 1);
  assert.equal(s.objects.length, 0);
  assert.equal(broken(s), null);
});

test('bot score: 0.8 when an enemy head will be within 1 of the landing tile at the blast, 0.65 for an enemy trail within 2', () => {
  /** @param {GameState} s @param {Player} p */
  const score = (s, p) => CARDS.grenade.botScore?.(s, p, 1) ?? 0;

  const far = arena(30, 30);
  const a = human(far, 'A', 16, 9);
  human(far, 'E', 25, 25);
  a.dir = L; // lands on (10, 9)
  assert.equal(score(far, a), 0, 'no enemy near the landing tile');

  // A head 2 tiles from the landing tile and standing still is out of the blast: no throw.
  const still = arena(30, 30);
  const b = human(still, 'B', 16, 9);
  human(still, 'F', 8, 9);
  b.dir = L;
  assert.equal(score(still, b), 0, 'a still head 2 away survives the blast');

  // A head that keeps going right from (6, 9) is at (11, 9) when the blast comes (FUSE tiles on): 1 from (10, 9).
  const moving = arena(30, 30);
  const c = human(moving, 'C', 16, 9);
  const f = human(moving, 'F', 6, 9);
  c.dir = L;
  f.dir = R;
  f.stopped = false; // humans spawn stopped
  assert.equal(score(moving, c), 0.8, 'the head will be in the blast');
  f.stopped = true;
  assert.equal(score(moving, c), 0, 'a head that stops is not');

  // An enemy trail (not a head) within 2: E runs up to (10, 11), then right to (11, 11).
  const trail = arena(30, 30);
  const d = human(trail, 'D', 16, 9);
  const e = human(trail, 'E', 10, 20);
  run(trail, [[e, [...rep(U, 9), R]]]);
  assert.equal(broken(trail), null);
  d.dir = L;
  assert.equal(score(trail, d), 0.65, 'the enemy trail at (10, 11) is 2 from the landing tile');

  d.dir = -1;
  assert.equal(score(trail, d), 0, 'no direction: cannot throw');
});
