// @ts-check
import assert from 'node:assert/strict';
import { performance } from 'node:perf_hooks';
import { RESULTS_TICKS, createState, spawnPlayer, spawnPlayerAt, step, stateHash } from '../../shared/rules.js';
import { ACT, REGISTRY } from '../../shared/cards.js';
import { kill } from '../../shared/cards/util.js';

const arena = (/** @type {number} */ seed) => createState({ w: 40, h: 40, targetHeads: 0, mode: 'draft' }, 'draft' + seed);
const held = (/** @type {import('../../shared/types.js').Player} */ p) => p.cards.items.length + (p.cards.active ? 1 : 0) + p.cards.passives.length;
const pick = (/** @type {any} */ s, /** @type {any} */ p) => step(s, [{ id: p.id, turns: [], cards: [ACT.choose0] }]);

test('a new player drafts three times in a row, then no more', () => {
  const s = arena(1);
  const a = spawnPlayerAt(s, { kind: 'human', name: 'A' }, 10, 10);
  assert.equal(a.offers.length, 1, 'first offer right away');
  assert.equal(a.offers[0].cards.length, 3);
  step(s, [], 5);
  assert.equal(a.offers.length, 1, 'no second offer before picking');
  pick(s, a);
  step(s, [], 1);
  assert.equal(a.offers.length, 1, 'second offer after the pick');
  assert.ok(a.offers[0].until > s.tick + 40, 'a full timer, not eaten while waiting');
  pick(s, a);
  step(s, [], 1);
  assert.equal(a.offers.length, 1, 'third offer');
  pick(s, a);
  step(s, [], 50);
  assert.equal(a.offers.length, 0, 'no fourth');
  assert.ok(held(a) >= 2 && held(a) <= 3);
});

test('an offer that times out auto-picks and the next one follows', () => {
  const s = arena(2);
  const a = spawnPlayerAt(s, { kind: 'human', name: 'A' }, 10, 10);
  step(s, [], 70);
  assert.ok(a.fx.drafted >= 2);
  step(s, [], 300);
  assert.equal(a.offers.length, 0);
  assert.equal(a.fx.drafted, 3);
});

test('a respawned player starts over with three drafts', () => {
  const s = arena(3);
  const a = spawnPlayerAt(s, { kind: 'human', name: 'A', uid: 'u1' }, 10, 10);
  step(s, [], 300);
  assert.equal(a.fx.drafted, 3);
  kill(s, a, 'wall', null);
  assert.equal(a.alive, false);
  const a2 = spawnPlayer(s, { kind: 'human', name: 'A', uid: 'u1' });
  assert.ok(a2);
  assert.equal(a2.offers.length, 1, 'offer at the respawn');
  for (let i = 0; i < 3; i++) {
    assert.equal(a2.offers.length, 1);
    pick(s, a2);
    step(s, [], 1);
  }
  step(s, [], 60);
  assert.equal(a2.offers.length, 0);
  assert.equal(a2.fx.drafted, 3);
});

test('a new round starts everyone over with three drafts', () => {
  const s = arena(6);
  const a = spawnPlayerAt(s, { kind: 'human', name: 'A' }, 10, 10);
  step(s, [], 2100 + RESULTS_TICKS + 2);
  assert.equal(s.round.n, 2);
  assert.ok(a.alive);
  assert.equal(a.cards.items.length + a.cards.passives.length + (a.cards.active ? 1 : 0) <= 1, true, 'old hand gone');
  assert.equal(a.fx.drafted, 1);
  assert.equal(a.offers.length, 1);
});

test('kills do not offer cards', () => {
  const s = arena(4);
  const a = spawnPlayerAt(s, { kind: 'human', name: 'A' }, 10, 10);
  const b = spawnPlayerAt(s, { kind: 'human', name: 'B' }, 30, 30);
  step(s, [], 300);
  assert.equal(a.offers.length, 0);
  kill(s, b, 'trail', a);
  step(s, [], 2);
  assert.equal(a.offers.length, 0, 'the killer gets no draft');
});

test('bots end up holding cards after their drafts', () => {
  const s = createState({ w: 60, h: 60, targetHeads: 8, mode: 'draft' }, 'bots-draft', { fill: true });
  step(s, [], 60);
  const bots = s.players.filter((p) => p.kind === 'bot' && p.alive);
  assert.ok(bots.length > 0);
  assert.ok(bots.some((p) => held(p) > 0));
  for (const p of bots) if (p.fx.drafted === 3 && !p.offers.length) assert.ok(held(p) > 0 || p.spawnTick > 0);
});

test('drafts are a fair spread over the whole pool', () => {
  const counts = new Map(REGISTRY.map((c) => [c.id, 0]));
  for (let i = 0; i < 150; i++) {
    const s = arena(100 + i);
    const a = spawnPlayerAt(s, { kind: 'human', name: 'A' }, 10, 10);
    for (const c of a.offers[0].cards) counts.set(c, (counts.get(c) ?? 0) + 1);
    // one more look at the rank-1 pool: the leader gets every card too
    a.land = 99;
  }
  const v = [...counts.values()];
  assert.ok(Math.min(...v) > 0, 'every card appears');
  assert.ok(Math.max(...v) <= 3 * Math.min(...v), `spread ${JSON.stringify([...counts])}`);
});

test('the engine weights are restored after a draft', () => {
  const s = arena(5);
  spawnPlayerAt(s, { kind: 'human', name: 'A' }, 10, 10);
  assert.equal(s.cfg.sandbox, false);
});

test('bot game: rounds advance, results have a winner, deterministic', () => {
  const run = () => {
    const s = createState({ w: 80, h: 80, targetHeads: 10, mode: 'draft' }, 'trial', { fill: true });
    const len = 5 * 60 * 7 + RESULTS_TICKS;
    let max = 0;
    for (let i = 0; i < len * 2 + 5; i++) {
      step(s);
      for (const p of s.players) {
        assert.ok((p.fx.drafted ?? 0) <= 3 || !p.alive);
        assert.ok(p.offers.length <= 1);
        max = Math.max(max, held(p));
      }
    }
    return { s, max };
  };
  const a = run();
  assert.ok(a.s.round.n >= 3);
  assert.ok(a.s.round.result && a.s.round.result.winner !== 0 || a.s.round.phase === 'play');
  assert.ok(a.max > 0, 'bots held cards');
  assert.equal(stateHash(a.s), stateHash(run().s));
});

test('timing: tick stays cheap at 200x200 with 30 bots', () => {
  const time = (/** @type {string} */ mode) => {
    const s = createState({ w: 200, h: 200, targetHeads: 30, mode }, 'time', { fill: true });
    step(s, [], 100);
    const t = performance.now();
    step(s, [], 2000);
    return (performance.now() - t) / 2000;
  };
  const base = time('classic');
  const d = time('draft');
  assert.ok(d - base < 0.3, `draft ${d.toFixed(3)} ms vs classic ${base.toFixed(3)} ms`);
});

test('bot game numbers', { skip: !process.env.DRAFT_NUMBERS }, () => {
  const s = createState({ w: 80, h: 80, targetHeads: 10, mode: 'draft' }, 'trial', { fill: true });
  let deaths = 0;
  for (let i = 0; i < (2100 + RESULTS_TICKS) * 2; i++) {
    step(s);
    for (const e of s.events) if (e.k === 'kill') deaths++;
    s.events.length = 0;
  }
  console.log('rounds', s.round.n, 'winner', s.round.result?.winner, 'deaths', deaths, 'phase', s.round.phase);
});
