// @ts-check
// Shield: a cut on your trail by an enemy does not kill you (10 s); the cutter is stunned and your trail is cut
// back to the crossing. Head-on crashes, hits on the head itself and every other cause still kill.
import assert from 'node:assert/strict';
import { CARDS } from '../../shared/cards.js';
import { takeEvents } from '../../shared/rules.js';
import { arena, human, give, run, rep, broken, trailAt, ACT, D, L, R } from './helpers.js';

/** @typedef {(number|'stop'|null|(number|'stop')[])[]} Script */

const W = 30;
/** Tile index of (x, y) on the default 30 × 30 arena. @param {number} x @param {number} y */
const at = (x, y) => y * W + x;

/**
 * P (shielded unless told otherwise) lays a trail east along y = 20 from its base at x = 10 and stops at x = 16
 * (trail 12..16). C walks down from (cutX, 8), stops above the trail and steps onto it at tick `cut`.
 * @param {{cut?: number, shield?: boolean, cutX?: number}} [o]
 */
function fight({ cut = 13, shield = true, cutX = 15 } = {}) {
  const s = arena();
  const p = human(s, 'P', 10, 20);
  const c = human(s, 'C', cutX, 8);
  if (shield) give(s, p, 'shield');
  /** @type {Script} */
  const P = [shield ? [R, ACT.item0] : R, R, R, R, R, R, 'stop'];
  // Ticks 1-11 down its column, stopped at tick 12, stepping on at `cut`.
  /** @type {Script} */
  const C = [...rep(D, 11), 'stop', ...rep(null, cut - 13), 'stop'];
  run(s, [
    [p, P],
    [c, C],
  ]);
  return { s, p, c };
}

/** @param {import('../../shared/types.js').GameState} s @param {import('../../shared/types.js').Player} p @param {string} cause @param {import('../../shared/types.js').Player | null} killer */
const preventDeath = (s, p, cause, killer) => !!CARDS.shield.preventDeath?.(s, p, cause, killer, 0);

test('shield: an enemy cutting your trail does not kill you, and the shield is spent', () => {
  const { s, p, c } = fight();
  assert.equal(p.alive, true);
  assert.equal(p.death, null);
  assert.equal(p.fx.shield, 0);
  assert.equal(p.cards.items.length, 0);
  assert.equal(c.alive, true);
  assert.equal(broken(s), null);
});

test('shield: your trail is cut back to the crossing; you live with the part behind your head', () => {
  const { s, p, c } = fight();
  assert.deepEqual(p.trail, [at(16, 20)]);
  assert.equal(trailAt(s, 12, 20), 0);
  assert.equal(trailAt(s, 13, 20), 0);
  assert.equal(trailAt(s, 14, 20), 0);
  assert.equal(trailAt(s, 15, 20), c.id, 'the cutter keeps the crossing tile');
  assert.equal(broken(s), null);
});

test('shield: the cutter gets no kill, no draft and no death', () => {
  const { s, c } = fight();
  assert.equal(c.kills, 0);
  assert.equal(c.offers.length, 0);
  const events = takeEvents(s);
  assert.equal(
    events.some((e) => e.k === 'kill'),
    false,
  );
  assert.ok(events.some((e) => e.k === 'fx' && e.card === 'shield' && e.v === c.id));
});

test('shield: the cutter is stunned (fx.stun = cut tick + 5): it does not move for the 4 ticks after the cut', () => {
  const { s, c } = fight();
  assert.equal(c.fx.stun, 13 + 5);
  run(s, [[c, [null, null, null, null]]]); // ticks 14-17
  assert.deepEqual([c.x, c.y], [15, 20]);
  run(s, [[c, [null]]]); // tick 18: free again, moves on
  assert.deepEqual([c.x, c.y], [15, 21]);
  assert.equal(broken(s), null);
});

test('shield: after the shield is used up, a second cut kills you', () => {
  const { s, p, c } = fight();
  // Queued during the stun, the cutter turns right and steps onto your head at tick 18 (the shield is gone).
  run(s, [[c, [R, null, null, null, null]]]);
  assert.equal(p.alive, false);
  assert.equal(p.death?.cause, 'trail');
  assert.equal(p.death?.killer, 'C');
  assert.equal(c.kills, 1);
  assert.equal(broken(s), null);
});

test('shield: lasts 70 ticks from use; a cut at tick 70 is blocked, at tick 71 it kills', () => {
  const late = fight({ cut: 70 });
  assert.equal(late.p.alive, true);
  assert.equal(broken(late.s), null);

  const expired = fight({ cut: 71 });
  assert.equal(expired.p.alive, false);
  assert.equal(expired.p.death?.cause, 'trail');
  assert.equal(expired.c.kills, 1);
  assert.equal(broken(expired.s), null);
});

test('shield: a head-on crash is not prevented', () => {
  const s = arena();
  const p = human(s, 'P', 10, 20);
  const h = human(s, 'H', 14, 20);
  give(s, p, 'shield');
  run(s, [
    [p, [[R, ACT.item0], R]],
    [h, [L]],
  ]);
  assert.equal(p.alive, false);
  assert.equal(p.death?.cause, 'head');
  assert.equal(h.alive, false);
  assert.equal(broken(s), null);
});

test('shield: a cut onto your head itself is a normal cut (D-39): the shield does not cover it', () => {
  const { s, p, c } = fight({ cutX: 16 });
  assert.equal(p.alive, false);
  assert.equal(p.death?.cause, 'trail');
  assert.equal(c.kills, 1);
  assert.equal(broken(s), null);
});

test('shield: without a shield the same cut kills you', () => {
  const { s, p, c } = fight({ shield: false });
  assert.equal(p.alive, false);
  assert.equal(c.kills, 1);
  assert.equal(broken(s), null);
});

test('shield: use() shields you for 70 ticks from the tick it is used', () => {
  const s = arena();
  const p = human(s, 'P', 10, 20);
  assert.equal(CARDS.shield.use?.(s, p, 1), true);
  assert.equal(p.fx.shield, s.tick + 70);
});

test('shield: preventDeath blocks only a trail cut by a living enemy, once per shield', () => {
  const s = arena();
  const p = human(s, 'P', 10, 20);
  const c = human(s, 'C', 15, 8);
  p.fx.shield = s.tick + 70;
  for (const cause of ['border', 'head', 'self', 'enclosed', 'landless', 'removed']) assert.equal(preventDeath(s, p, cause, c), false, cause);
  assert.equal(preventDeath(s, p, 'trail', null), false, 'no killer');
  assert.equal(preventDeath(s, p, 'trail', p), false, 'self');
  assert.equal(p.fx.shield, 70, 'still shielded after the refusals');
  c.alive = false;
  assert.equal(preventDeath(s, p, 'trail', c), false, 'dead killer');
  c.alive = true;
  assert.equal(preventDeath(s, p, 'trail', c), true);
  assert.equal(p.fx.shield, 0);
  assert.equal(c.fx.stun, s.tick + 5);
  assert.equal(preventDeath(s, p, 'trail', c), false, 'used up');
});

test('shield: preventDeath does nothing without a shield', () => {
  const s = arena();
  const p = human(s, 'P', 10, 20);
  const c = human(s, 'C', 15, 8);
  assert.equal(preventDeath(s, p, 'trail', c), false);
  assert.equal(c.fx.stun, undefined);
});

test('shield: botScore is 0.75 with a trail of 6+ tiles and an enemy head within 5 tiles of it, else 0', () => {
  /** @param {number} n tiles moved east from (5, 5) (trail n - 1 tiles) @param {number} ex @param {number} ey @param {boolean} [alive] */
  const score = (n, ex, ey, alive = true) => {
    const s = arena();
    const p = human(s, 'P', 5, 5);
    const e = human(s, 'E', ex, ey);
    e.alive = alive;
    run(s, [[p, rep(R, n)]]);
    assert.equal(broken(s), null);
    return CARDS.shield.botScore?.(s, p, 0);
  };
  assert.equal(score(8, 18, 5), 0.75, 'trail 7, enemy 5 tiles from its head');
  assert.equal(score(8, 19, 5), 0, 'enemy 6 tiles away');
  assert.equal(score(7, 17, 5), 0.75, 'trail of exactly 6, head at x 12');
  assert.equal(score(6, 16, 5), 0, 'trail of 5 is too short, even with an enemy in reach');
  assert.equal(score(1, 18, 5), 0, 'no trail');
  assert.equal(score(8, 18, 5, false), 0, 'a dead enemy does not count');
});
