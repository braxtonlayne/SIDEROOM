// @ts-check
// Wall (item, rare): geometry, blocking heads, the owner walking away, expiry, use rules and the bot score.
import assert from 'node:assert/strict';
import { CARDS } from '../../shared/cards.js';
import { spawnPlayerAt, step } from '../../shared/rules.js';
import { arena, human, give, run, rep, broken, ACT, U, R, D, L } from './helpers.js';

/** @typedef {import('../../shared/types.js').GameState} GameState */
/** @typedef {import('../../shared/types.js').WorldObject} WorldObject */

/** @param {GameState} s @param {WorldObject} o @param {number} x @param {number} y */
const blocks = (s, o, x, y) => CARDS.wall.objectBlocks?.(s, o, x, y) ?? false;

/**
 * A head at (15, 15) heading `d`, with a wall raised on the tile behind it (the head then holds still).
 * @param {number} d
 */
function raise(d) {
  const s = arena(30, 30);
  const p = human(s, 'P', 15, 15);
  give(s, p, 'wall');
  run(s, [[p, [d]]]); // the first direction moves the head one tile
  const t = s.tick + 1; // the tick the wall goes up on
  run(s, [[p, [[ACT.item0, 'stop']]]]); // one tick: raise, then hold still
  const o = s.objects.find((x) => x.kind === 'wall');
  assert.ok(o, 'a wall is in the world');
  return { s, p, o, t };
}

/**
 * Owner O at (20, 19) heading up with a wall raised at (20, 20) on tick 2 (until tick 58), and an enemy E
 * at (21, 10), not yet moving.
 */
function scene() {
  const s = arena(40, 40);
  const o = human(s, 'O', 20, 20);
  const e = human(s, 'E', 21, 10);
  give(s, o, 'wall');
  run(s, [[o, [U]]]); // O heads up to (20, 19)
  run(s, [[o, [[ACT.item0, 'stop']]]]); // the wall goes up on (20, 20); O holds still
  const wall = s.objects.find((x) => x.kind === 'wall');
  assert.ok(wall, 'a wall is in the world');
  return { s, o, e, wall };
}

test('the wall is 5 tiles, centred on the tile behind the head, across its path (every direction)', () => {
  for (const d of [U, R, D, L]) {
    const { s, p, o, t } = raise(d);
    assert.deepEqual([o.x, o.y], [15, 15], `dir ${d}`);
    assert.equal(o.dir, d);
    assert.equal(o.owner, p.id);
    assert.equal(o.until, t + 56);
    const horizontal = d === U || d === D;
    for (let k = -3; k <= 3; k++) {
      const x = horizontal ? 15 + k : 15;
      const y = horizontal ? 15 : 15 + k;
      assert.equal(blocks(s, o, x, y), Math.abs(k) <= 2, `dir ${d}, offset ${k}`);
    }
    // The head's own tile and the lane it came from are not wall.
    assert.equal(blocks(s, o, p.x, p.y), false, `dir ${d}: head tile`);
    assert.equal(broken(s), null);
  }
});

test('a wall on the map edge blocks only the tiles that are on the map', () => {
  const s = arena(30, 30);
  const p = human(s, 'P', 1, 1);
  give(s, p, 'wall');
  run(s, [[p, [U]]]); // to (1, 0), the top row; the wall goes up on (1, 1)
  run(s, [[p, [[ACT.item0, 'stop']]]]);
  const o = s.objects.find((x) => x.kind === 'wall');
  assert.ok(o);
  assert.equal(blocks(s, o, 0, 1), true);
  assert.equal(blocks(s, o, 3, 1), true);
  assert.equal(blocks(s, o, 4, 1), false);
  assert.equal(broken(s), null);
});

test('the use event and the owner are recorded, and the card leaves the hand', () => {
  const { s, p, o } = raise(R);
  assert.equal(p.cards.items.length, 0);
  const e = s.events.find((x) => x.k === 'fx' && x.card === 'wall');
  assert.ok(e, 'a fx event for the wall');
  assert.deepEqual([e.id, e.x, e.y], [p.id, o.x, o.y]);
});

test('a head with no direction cannot use the wall: nothing is placed and the card stays in hand', () => {
  const s = arena(30, 30);
  const p = human(s, 'P', 10, 10);
  give(s, p, 'wall');
  run(s, [[p, [ACT.item0]]]);
  assert.equal(s.objects.length, 0);
  assert.equal(p.cards.items.length, 1);
  assert.equal(broken(s), null);
});

test('an enemy head driving into the wall stops (alive, stopped) and can turn away on the next tick', () => {
  const { s, e } = scene();
  // Tick 3 starts the walk down column 21 and tick 12 is stopped by the wall at (21, 20).
  run(s, [[e, [D, ...rep(null, 9)]]]);
  assert.deepEqual([e.x, e.y], [21, 19]);
  assert.equal(e.stopped, true);
  assert.equal(e.alive, true);
  assert.equal(broken(s), null);
  // Still held: another push down does nothing new.
  run(s, [[e, [D]]]);
  assert.deepEqual([e.x, e.y], [21, 19]);
  assert.equal(e.alive, true);
  // Turning away: the next tick it moves sideways.
  run(s, [[e, [R]]]);
  assert.deepEqual([e.x, e.y], [22, 19]);
  assert.equal(e.stopped, false);
  assert.equal(e.alive, true);
  assert.equal(broken(s), null);
});

test('the owner walking away from its wall is unaffected', () => {
  const { s, o } = scene();
  // Up again (out of the hold, one step), then three more: away from the wall, never blocked by it.
  run(s, [[o, [U, null, null, null]]]);
  assert.deepEqual([o.x, o.y], [20, 15]);
  assert.equal(o.alive, true);
  assert.equal(broken(s), null);
});

test('the owner cannot walk back through its own wall', () => {
  const { s, o } = scene();
  // O is still on its base (no trail), so turning around is allowed, but the wall stops it.
  run(s, [[o, [D]]]);
  assert.deepEqual([o.x, o.y], [20, 19]);
  assert.equal(o.stopped, true);
  assert.equal(o.alive, true);
  assert.equal(broken(s), null);
});

test('the wall is gone after 56 ticks: the stopped move passes on the expiry tick', () => {
  const { s, e, wall } = scene();
  assert.equal(wall.until, 58);
  run(s, [[e, [D, ...rep(null, 9)]]]); // E stopped at (21, 19) from tick 12
  step(s, [], wall.until - 2 - s.tick); // to tick 56
  run(s, [[e, [D]]]); // tick 57: still blocked
  assert.deepEqual([e.x, e.y], [21, 19]);
  run(s, [[e, [D]]]); // tick 58: the wall has expired
  assert.deepEqual([e.x, e.y], [21, 20]);
  assert.equal(s.objects.some((x) => x.kind === 'wall'), false);
  assert.equal(broken(s), null);
});

test('botScore: 0.8 for a fleeing bot with an enemy within 4 tiles behind it, else 0', () => {
  const s = arena(30, 30);
  const b = spawnPlayerAt(s, { kind: 'bot', name: 'B' }, 15, 15);
  b.dir = R;
  b.stopped = false;
  const mode = b.bot;
  assert.ok(mode);
  const score = () => CARDS.wall.botScore?.(s, b, 1) ?? 0;
  const e = human(s, 'E', 12, 15); // 3 tiles behind, heading right
  assert.equal(score(), 0, 'not fleeing');
  mode.mode = 'flee';
  assert.equal(score(), 0.8, '3 tiles behind');
  e.x = 11; // 4 tiles behind: the limit
  assert.equal(score(), 0.8, '4 tiles behind');
  e.x = 10; // 5 tiles behind
  assert.equal(score(), 0, 'too far');
  e.x = 18; // ahead of the bot
  assert.equal(score(), 0, 'ahead');
  e.x = 12;
  e.alive = false; // dead enemies do not count
  assert.equal(score(), 0, 'dead enemy');
});
