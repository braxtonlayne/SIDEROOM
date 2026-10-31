// @ts-check
// The card engine: drafting on kills, slots, use rules, speed sub-steps, and the 3 reference cards.
import assert from 'node:assert/strict';
import { ACT, CARDS, REGISTRY, offerCards, fxOn } from '../shared/cards.js';
import { createState, step, stateHash, takeEvents } from '../shared/rules.js';
import { arena, human, give, run, rep, baseAt, broken, U, R, D, L } from './cards/helpers.js';

test('all 14 cards are registered with complete card data', () => {
  assert.equal(REGISTRY.length, 14);
  for (const c of REGISTRY) {
    assert.ok(c.id && c.name && c.text, c.id);
    assert.ok(['item', 'active', 'passive'].includes(c.kind), c.id);
    assert.ok(['common', 'rare', 'epic'].includes(c.rarity), c.id);
    assert.ok(c.maxLevel === 1 || c.maxLevel === 2, c.id);
  }
});

test('a kill drafts 3 different cards; 1/2/3 picks one; an unpicked offer is taken for you', () => {
  const s = arena();
  const a = human(s, 'A', 5, 10);
  const b = human(s, 'B', 12, 8);
  run(s, [
    [a, rep(U, 7)],
    [b, rep(L, 7)],
  ]);
  assert.equal(a.alive, false);
  assert.equal(b.offers.length, 1);
  const offer = b.offers[0].cards;
  assert.ok(offer.length >= 3);
  assert.equal(new Set(offer).size, offer.length);
  run(s, [[b, [[ACT.choose0 - 1, 'stop']]]]); // the second card; and stop before the wall
  assert.equal(b.offers.length, 0);
  const has = (/** @type {string} */ x) =>
    b.cards.items.some((c) => c.id === x) || b.cards.active?.id === x || b.cards.passives.some((c) => c.id === x);
  assert.ok(has(offer[1]), `got ${offer[1]}`);

  offerCards(s, b, false);
  const first = b.offers[0].cards[0];
  step(s, [], 60); // a human gets 8 s, then the first card is taken
  assert.equal(b.offers.length, 0);
  assert.ok(has(first));
});

test('the leader never draws epics; killing the leader drafts 4', () => {
  const s = createState({ w: 40, h: 40, targetHeads: 0 }, 'draft');
  const a = human(s, 'A', 8, 8);
  const b = human(s, 'B', 30, 30);
  b.land = 1; // A leads
  for (let k = 0; k < 300; k++) offerCards(s, a, false);
  for (const o of a.offers) for (const id of o.cards) assert.notEqual(CARDS[id].rarity, 'epic', id);
  a.offers = [];
  offerCards(s, a, true);
  assert.equal(a.offers[0].cards.length, 4);
});

test('slots: items cap at 2 (oldest goes), a duplicate active or passive levels up, a 3rd passive replaces the oldest', () => {
  const s = arena();
  const p = human(s, 'P', 10, 10);
  give(s, p, 'claimflag');
  give(s, p, 'grenade');
  give(s, p, 'snare');
  assert.deepEqual(p.cards.items.map((c) => c.id), ['grenade', 'snare']);
  give(s, p, 'dash');
  give(s, p, 'dash');
  give(s, p, 'dash');
  assert.deepEqual([p.cards.active?.id, p.cards.active?.level], ['dash', 2]);
  give(s, p, 'rewind');
  assert.equal(p.cards.active?.id, 'rewind');
  give(s, p, 'haste');
  give(s, p, 'overgrowth');
  give(s, p, 'haste');
  assert.deepEqual(p.cards.passives.map((c) => [c.id, c.level]), [['haste', 2], ['overgrowth', 1]]);
  give(s, p, 'bloodlust');
  assert.deepEqual(p.cards.passives.map((c) => c.id), ['overgrowth', 'bloodlust']);
});

test('Dash: two tiles a tick for 7 ticks, then a recharge; trail on every tile; invariants hold', () => {
  const s = arena(40, 40);
  const p = human(s, 'P', 5, 20);
  give(s, p, 'dash');
  run(s, [[p, [R, R]]]);
  assert.deepEqual([p.x, p.y], [7, 20]);
  run(s, [[p, [ACT.active]]]);
  assert.ok(fxOn(p, 'dash', s.tick));
  assert.deepEqual([p.x, p.y], [9, 20]);
  run(s, [[p, rep(null, 6)]]);
  assert.deepEqual([p.x, p.y], [21, 20]);
  run(s, [[p, [null]]]);
  assert.deepEqual([p.x, p.y], [22, 20]);
  assert.equal(broken(s), null);
  assert.equal(p.trail.length, 22 - 6);
  // Recharging: Shift again does nothing yet.
  run(s, [[p, [ACT.active]]]);
  assert.ok(!fxOn(p, 'dash', s.tick));
  assert.ok((p.cards.active?.ready ?? 0) > s.tick);
});

test('a dashing head cannot jump a trail: crossing it still cuts', () => {
  const s = arena(40, 40);
  const a = human(s, 'A', 10, 30);
  const b = human(s, 'B', 4, 20);
  give(s, b, 'dash');
  // A lays a vertical trail at x = 10 up past y = 20 (there by tick 10); then B dashes right across it.
  run(s, [
    [a, rep(U, 15)],
    [b, [...rep(null, 10), R, ACT.active, null, null, null]],
  ]);
  assert.equal(a.alive, false);
  assert.equal(a.death?.killer, 'B');
});

test('Claim Flag claims neutral land around you only, touching your land; kept when nothing to claim', () => {
  const s = arena();
  const p = human(s, 'P', 10, 10);
  const q = human(s, 'Q', 15, 10);
  give(s, p, 'claimflag');
  run(s, [[p, [R, ACT.item0]]]);
  // The flag moves p to (11, 10): the 7×7 is x 8..14, y 7..13 (radius 3).
  assert.equal(baseAt(s, 8, 7), p.id, 'the corner of the 7×7 is claimed');
  assert.equal(baseAt(s, 14, 13), p.id, 'the opposite corner too');
  assert.equal(baseAt(s, 7, 10), 0, 'a tile just outside the 7×7 stays neutral');
  assert.equal(baseAt(s, 14, 10), q.id, 'enemy land untouched');
  assert.equal(p.cards.items.length, 0);
  assert.equal(broken(s), null);
  // Far from its land: the flag does nothing and stays in hand.
  const s2 = arena();
  const r = human(s2, 'R', 5, 15);
  give(s2, r, 'claimflag');
  run(s2, [[r, [...rep(R, 8), ACT.item0]]]);
  assert.equal(r.cards.items.length, 1);
});

test('Overgrowth: a capture also claims the neutral ring around it', () => {
  const loop = [...rep(U, 3), ...rep(R, 3), ...rep(D, 3), ...rep(L, 2)];
  const plain = arena(20, 20);
  const a = human(plain, 'A', 5, 5);
  run(plain, [[a, loop]]);
  const s = arena(20, 20);
  const b = human(s, 'B', 5, 5);
  give(s, b, 'overgrowth');
  run(s, [[b, loop]]);
  assert.ok(b.land > a.land + 10, `${b.land} vs ${a.land}`);
  assert.equal(broken(s), null);
});

test('EMP blocks card use', () => {
  const s = arena();
  const p = human(s, 'P', 10, 10);
  give(s, p, 'claimflag');
  p.fx.emp = s.tick + 10;
  run(s, [[p, [R, ACT.item0]]]);
  assert.equal(p.cards.items.length, 1);
});

test('cards are lost on death', () => {
  const s = arena();
  const p = human(s, 'P', 5, 5);
  give(s, p, 'dash');
  give(s, p, 'claimflag');
  run(s, [[p, rep(U, 6)]]);
  assert.equal(p.alive, false);
  assert.equal(p.cards.active, null);
  assert.equal(p.cards.items.length, 0);
});

test('bots draft and use cards; games with cards stay deterministic and keep every invariant', () => {
  const play = () => {
    const s = createState({ w: 60, h: 60, targetHeads: 12 }, 'bots-cards', { fill: true });
    let used = 0;
    let taken = 0;
    for (let t = 0; t < 3000; t++) {
      step(s);
      for (const e of takeEvents(s)) {
        if (e.k === 'use') used++;
        if (e.k === 'card') taken++;
      }
      const bad = broken(s);
      if (bad) assert.fail(`tick ${s.tick}: ${bad}`);
    }
    return { used, taken, hash: stateHash(s) };
  };
  const r1 = play();
  assert.ok(r1.taken > 5, `cards taken ${r1.taken}`);
  assert.ok(r1.used > 0, `cards used ${r1.used}`);
  assert.equal(play().hash, r1.hash);
});
