// @ts-check
// Bloodlust: a kill recharges the killer's active ability and hastes it for the 20 ticks after the kill.
import assert from 'node:assert/strict';
import { ACT, CARDS, fxOn } from '../../shared/cards.js';
import { takeEvents } from '../../shared/rules.js';
import { arena, human, give, run, rep, broken, U, R } from './helpers.js';

/**
 * A duel on an 80x80 grid. A (the victim) runs east along y = 66 from (4, 66) and lays a trail there. B (the
 * killer) starts at (12, 70), dashes east along its own row (Dash used on tick 2), stops at (27, 70) and turns
 * north on tick 26: on tick 29 it steps onto A's trail at (27, 66). A dies, credited to B. Ticks are played one
 * at a time, so the kill tick is known.
 * @param {boolean} withCard
 */
function duel(withCard) {
  const s = arena(80, 80);
  const a = human(s, 'A', 4, 66);
  const b = human(s, 'B', 12, 70);
  if (withCard) give(s, b, 'bloodlust');
  give(s, b, 'dash');
  /** @type {(number | 'stop' | null)[]} */
  const bScript = [R, ACT.active, ...rep(null, 6), 'stop', ...rep(null, 16), U, null, null, null];
  /** @type {import('../../shared/types.js').GameEvent[]} */
  let events = [];
  for (let t = 0; t < bScript.length && a.alive; t++) {
    run(s, [
      [a, [R]],
      [b, [bScript[t]]],
    ]);
    events = takeEvents(s);
  }
  assert.equal(a.alive, false, 'A dies on B\'s trail');
  assert.equal(a.death?.killer, 'B');
  assert.equal(s.tick, 29, 'the kill is on tick 29');
  assert.equal(broken(s), null);
  return { s, a, b, kill: s.tick, events };
}

test('Bloodlust is a rare passive with one level', () => {
  assert.equal(CARDS.bloodlust.kind, 'passive');
  assert.equal(CARDS.bloodlust.rarity, 'rare');
  assert.equal(CARDS.bloodlust.maxLevel, 1);
});

test('speed: an extra tile on every third tick while fx.bloodlust is ahead of the tick; pure', () => {
  const speed = /** @type {(level: number, fx: Record<string, number>, tick: number) => number} */ (CARDS.bloodlust.speed);
  assert.equal(speed(1, { bloodlust: 10 }, 9), 1, 'on, multiple of 3');
  assert.equal(speed(1, { bloodlust: 10 }, 10), 0, 'the end tick is off');
  assert.equal(speed(1, { bloodlust: 13 }, 11), 0, 'on, but not a multiple of 3');
  assert.equal(speed(1, { bloodlust: 20 }, 18), 1);
  assert.equal(speed(1, { bloodlust: 20 }, 19), 0);
  assert.equal(speed(1, {}, 9), 0, 'no effect without fx');
  const fx = { dash: 99, bloodlust: 30 };
  assert.equal(speed(1, fx, 12), 1);
  assert.deepEqual(fx, { dash: 99, bloodlust: 30 });
});

test('a kill with Bloodlust recharges the killer active to the kill tick, and marks the kill', () => {
  const { b, kill, events } = duel(true);
  // Dash (used on tick 2) would be ready on tick 58; the kill makes it ready at once.
  assert.equal(b.cards.active?.ready, kill);
  assert.ok(!fxOn(b, 'dash', kill), 'Dash itself has run out');
  const fx = events.filter((e) => e.k === 'fx');
  // The event sits on the killer's tile as the kill resolves (its last step of the tick has not landed yet).
  assert.deepEqual(fx, [{ k: 'fx', card: 'bloodlust', id: b.id, x: 27, y: 67 }]);
});

test('after a Bloodlust kill the killer moves 1/3 faster for 20 ticks, then normally again', () => {
  const { s, b, kill } = duel(true);
  const y0 = b.y;
  run(s, [[b, rep(U, 21)]]);
  assert.equal(broken(s), null);
  // The burst covers ticks kill+1 .. kill+20; the tiles it adds are the ticks that are multiples of 3.
  let extra = 0;
  for (let t = kill + 1; t < kill + 21; t++) if (t % 3 === 0) extra++;
  assert.equal(extra, 7, 'ticks 30..49 hold seven multiples of 3');
  assert.equal(y0 - b.y, 21 + 7, '21 ticks at one tile plus the 7 extra');
  const y1 = b.y;
  run(s, [[b, rep(U, 9)]]);
  assert.equal(y1 - b.y, 9, 'normal speed again');
  assert.equal(broken(s), null);
});

test('without Bloodlust a kill changes neither the active ability nor the speed', () => {
  const { s, b, events } = duel(false);
  assert.equal(b.cards.active?.ready, 2 + 56, 'Dash ready 56 ticks after its use on tick 2');
  assert.equal(b.fx.bloodlust, undefined);
  assert.ok(!events.some((e) => e.k === 'fx'), 'no card effect event');
  const y0 = b.y;
  run(s, [[b, rep(U, 21)]]);
  assert.equal(y0 - b.y, 21, 'exactly one tile a tick');
  assert.equal(broken(s), null);
});
