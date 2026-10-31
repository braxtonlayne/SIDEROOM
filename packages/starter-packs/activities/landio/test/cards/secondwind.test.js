// @ts-check
// Second Wind: a death is prevented once, the head comes back on the largest land, the rest goes neutral.
import assert from 'node:assert/strict';
import { CARDS, effectFlags } from '../../shared/cards.js';
import { killPlayer, removePlayer } from '../../shared/rules.js';
import { setBase } from '../../shared/grid.js';
import { arena, human, give, run, rep, baseAt, tilesOf, broken, U, R, L, D } from './helpers.js';

/**
 * The standard cut: A walks up out of its base (trail at x=10, y 18..15); B walks right and steps on it at (10,17).
 * @param {import('../../shared/types.js').GameState} s @param {import('../../shared/types.js').Player} a @param {import('../../shared/types.js').Player} b
 */
function trailCut(s, a, b) {
  run(s, [
    [a, rep(U, 5)],
    [b, [...rep(R, 5), 'stop']],
  ]);
}

test('Second Wind prevents a trail-cut death: alive, no trail, on its own base, stopped, no kill for the killer', () => {
  const s = arena();
  const a = human(s, 'A', 10, 20);
  give(s, a, 'secondwind');
  const b = human(s, 'B', 5, 17);
  assert.notEqual(effectFlags(a, s.tick) & 64, 0, 'halo while unused');
  trailCut(s, a, b);
  assert.equal(a.alive, true);
  assert.equal(a.death, null);
  assert.equal(a.trail.length, 0);
  assert.equal(tilesOf(s, a.id).t, 0);
  assert.equal(baseAt(s, a.x, a.y), a.id, 'head on its own base');
  assert.equal(a.stopped, true);
  assert.deepEqual(a.queue, []);
  assert.equal(a.fx.secondwindUsed, 1);
  assert.equal(effectFlags(a, s.tick) & 64, 0, 'halo gone once used');
  assert.ok(s.events.some((e) => e.k === 'fx' && e.card === 'secondwind' && e.id === a.id));
  assert.equal(b.kills, 0);
  assert.equal(b.offers.length, 0);
  assert.equal(b.land, 9, 'the killer inherits nothing');
  assert.equal(broken(s), null);
});

test('with two separate regions the largest is kept and the smaller goes neutral', () => {
  const s = arena();
  const a = human(s, 'A', 10, 20);
  give(s, a, 'secondwind');
  const b = human(s, 'B', 5, 17);
  // A second, smaller region of 4 tiles far above the main base (its tiles sit lower in the grid).
  for (const [x, y] of [[20, 5], [21, 5], [20, 6], [21, 6]]) setBase(s, y * s.cfg.w + x, a.id);
  assert.equal(a.land, 13);
  trailCut(s, a, b);
  assert.equal(a.alive, true);
  assert.equal(a.land, 9);
  assert.equal(baseAt(s, 20, 5), 0, 'the smaller region is neutral');
  assert.equal(baseAt(s, 21, 6), 0);
  assert.equal(baseAt(s, 10, 20), a.id, 'the largest region is kept');
  assert.equal(baseAt(s, a.x, a.y), a.id, 'head back on the kept land');
  assert.equal(broken(s), null);
});

test('a tie in size keeps the region with the lowest tile index', () => {
  const s = arena();
  const a = human(s, 'A', 10, 20);
  give(s, a, 'secondwind');
  const b = human(s, 'B', 5, 17);
  // A 3×3 region above the main 3×3 base: equal size, lower tile index.
  for (let y = 4; y <= 6; y++) for (let x = 19; x <= 21; x++) setBase(s, y * s.cfg.w + x, a.id);
  trailCut(s, a, b);
  assert.equal(a.alive, true);
  assert.equal(a.land, 9);
  assert.equal(baseAt(s, 10, 20), 0, 'the main 3×3 is the one that goes neutral');
  assert.equal(baseAt(s, 20, 5), a.id, 'the upper 3×3 is kept');
  assert.deepEqual([a.x, a.y], [20, 5], 'head on the middle of the kept 3×3');
  assert.equal(broken(s), null);
});

test('the second death is final', () => {
  const s = arena();
  const a = human(s, 'A', 10, 20);
  give(s, a, 'secondwind');
  const b = human(s, 'B', 5, 17);
  trailCut(s, a, b);
  assert.equal(a.alive, true);
  // Now A loops onto its own trail on the left: a self-cut, which is not prevented again.
  run(s, [[a, [L, L, L, U, R, D]]]);
  assert.equal(a.alive, false);
  assert.equal(a.death?.cause, 'self');
  assert.equal(broken(s), null);
});

test('without the card the same cut kills A; B takes the land, the kill and a draft', () => {
  const s = arena();
  const a = human(s, 'A', 10, 20);
  const b = human(s, 'B', 5, 17);
  trailCut(s, a, b);
  assert.equal(a.alive, false);
  assert.equal(a.death?.cause, 'trail');
  assert.equal(b.kills, 1);
  assert.equal(b.offers.length, 1);
  assert.equal(b.land, 18, 'A\'s 9 tiles go to B');
  assert.equal(broken(s), null);
});

test('a player with no land left dies even with the card', () => {
  const s = arena();
  const a = human(s, 'A', 10, 20);
  give(s, a, 'secondwind');
  const b = human(s, 'B', 5, 17);
  for (let y = 19; y <= 21; y++) for (let x = 9; x <= 11; x++) setBase(s, y * s.cfg.w + x, 0);
  assert.equal(a.land, 0);
  assert.equal(CARDS.secondwind.preventDeath?.(s, a, 'trail', b, 1), false);
  assert.equal(a.fx.secondwindUsed, undefined);
  killPlayer(s, a, 'trail', b);
  assert.equal(a.alive, false);
  assert.equal(a.death?.cause, 'trail');
  assert.equal(broken(s), null);
});

test('a removed player (leaving) is never saved', () => {
  const s = arena();
  const a = human(s, 'A', 10, 20);
  give(s, a, 'secondwind');
  removePlayer(s, a.id);
  assert.equal(s.players.includes(a), false);
  assert.equal(a.alive, false);
  assert.equal(a.death?.cause, 'removed');
  assert.equal(a.fx.secondwindUsed, undefined);
  assert.equal(broken(s), null);
});
