// @ts-check
// Draft Night as a roguelike run (ACT-LND-002): keeper picks between rounds, rarity with catch-up, family set
// bonuses, run points and the end-of-run summary.
import assert from 'node:assert/strict';
import { RESULTS_TICKS, createState, endRound, spawnPlayer, spawnPlayerAt, startRound, step, stateHash } from '../../shared/rules.js';
import { ACT, CARDS, fxOn } from '../../shared/cards.js';
import { kill } from '../../shared/cards/util.js';
import { FAMILIES, PLACE_POINTS, RUN_ROUNDS, SET_AT, familyCounts, keepersOf, runOf, setBonuses } from '../../shared/modes/draft.js';
import { scoreOf } from '../../shared/modes/index.js';

/** @typedef {import('../../shared/types.js').GameState} GameState */
/** @typedef {import('../../shared/types.js').Player} Player */

const ROUND = 140;
const arena = (/** @type {string} */ seed, extra = {}) => createState({ w: 40, h: 40, targetHeads: 0, mode: 'draft', roundTicks: ROUND, ...extra }, seed);
const pick = (/** @type {GameState} */ s, /** @type {Player} */ p, i = 0) => step(s, [{ id: p.id, turns: [], cards: [ACT.choose0 - i] }]);
/** Run until the round is over (results showing). @param {GameState} s */
const toResults = (s) => {
  while (s.round.phase === 'play') step(s);
};
/** Run until the next round has started. @param {GameState} s */
const toNextRound = (s) => {
  const n = s.round.n;
  while (s.round.n === n) step(s);
};

test('a run is three rounds; the last round shows the end-of-run summary and the next round starts a new run', () => {
  const s = arena('run');
  const a = spawnPlayerAt(s, { kind: 'human', name: 'A', uid: 'ua' }, 10, 10);
  assert.equal(RUN_ROUNDS, 3);
  for (let leg = 1; leg <= RUN_ROUNDS; leg++) {
    assert.equal(runOf(s).leg, leg, `leg ${leg}`);
    assert.equal(runOf(s).runN, 1);
    toResults(s);
    const run = s.round.result?.run;
    assert.ok(run);
    assert.equal(run.leg, leg);
    assert.equal(run.of, RUN_ROUNDS);
    assert.equal(run.final, leg === RUN_ROUNDS);
    assert.equal(run.lines[0].name, 'A');
    assert.equal(run.lines[0].pts, PLACE_POINTS[0] * leg, 'alone: first every round');
    if (run.final) {
      assert.equal(run.champion, 'A');
      assert.equal(s.round.result?.winner, a.id);
    }
    toNextRound(s);
  }
  assert.equal(runOf(s).runN, 2, 'a new run');
  assert.equal(runOf(s).leg, 1);
  assert.deepEqual(runOf(s).pts.filter((v) => v > 0), [], 'points start over');
});

test('run points by place: 5, 3, 2, then 1', () => {
  const s = arena('pts', { w: 60, h: 60 });
  const ps = ['A', 'B', 'C', 'D', 'E'].map((n, k) => spawnPlayerAt(s, { kind: 'human', name: n, uid: n }, 6 + k * 11, 30));
  // Land decides the places: A most, E least.
  ps.forEach((p, k) => (p.land = 100 - k * 10));
  endRound(s);
  const lines = /** @type {NonNullable<import('../../shared/types.js').RoundResult['run']>} */ (s.round.result?.run).lines;
  assert.deepEqual(lines.map((l) => [l.name, l.pts, l.gain]), [['A', 5, 5], ['B', 3, 3], ['C', 2, 2], ['D', 1, 1], ['E', 1, 1]]);
});

test('the first draft of each round is a keeper: it stays for the rest of the run, at every spawn', () => {
  const s = arena('keep');
  const a = spawnPlayerAt(s, { kind: 'human', name: 'A', uid: 'ua' }, 10, 10);
  assert.equal(a.offers[0].keep, true, 'round 1, first life: keeper draft');
  const first = a.offers[0].cards[1];
  pick(s, a, 1);
  assert.deepEqual(keepersOf(s, a), [first]);
  step(s, [], 1);
  assert.equal(a.offers[0].keep, undefined, 'the next drafts are normal');
  pick(s, a);
  step(s, [], 1);
  pick(s, a);
  // A death and a respawn in the same round: the keeper comes back, the first draft is NOT another keeper.
  kill(s, a, 'wall', null);
  const a2 = /** @type {Player} */ (spawnPlayer(s, { kind: 'human', name: 'A', uid: 'ua' }));
  const held = (/** @type {Player} */ p) => [...p.cards.items.map((c) => c.id), p.cards.active?.id, ...p.cards.passives.map((c) => c.id)].filter(Boolean);
  assert.ok(held(a2).includes(first), 'keeper handed back at the respawn');
  assert.equal(a2.offers[0].keep, undefined);
  // Next round: the keeper is held from the start and a new keeper draft is offered.
  toNextRound(s);
  const a3 = /** @type {Player} */ (s.players.find((p) => p.uid === 'ua'));
  assert.ok(held(a3).includes(first));
  assert.equal(a3.offers[0].keep, true, 'round 2: a new keeper pick');
  const second = a3.offers[0].cards[0];
  // The timer picks the first card when the person does not.
  step(s, [], 60);
  assert.deepEqual(keepersOf(s, a3), [first, second]);
  // The summary lists keepers.
  toResults(s);
  const line = s.round.result?.run?.lines.find((l) => l.name === 'A');
  assert.deepEqual(line?.keep, [first, second]);
});

test('a host restart (fresh round) starts a new run; a natural new round carries it', () => {
  const s = arena('restart');
  spawnPlayerAt(s, { kind: 'human', name: 'A', uid: 'ua' }, 10, 10);
  toResults(s);
  toNextRound(s);
  assert.equal(runOf(s).leg, 2);
  startRound(s, { fresh: true });
  assert.equal(runOf(s).leg, 1);
  assert.deepEqual(runOf(s).pts, [0]);
});

test('later keeper drafts lean to rare and epic cards for whoever trails the run', () => {
  let lead = 0;
  let trail = 0;
  const rare = (/** @type {string[]} */ ids) => ids.filter((id) => CARDS[id].rarity !== 'common').length;
  for (let i = 0; i < 150; i++) {
    const s = arena('lean' + i, { w: 60, h: 60 });
    const md = runOf(s);
    md.leg = 2;
    md.keys = ['u:lead', 'u:mid', 'u:trail'];
    md.names = ['L', 'M', 'T'];
    md.pts = [10, 5, 2];
    md.keep = ['', '', ''];
    md.kleg = [1, 1, 1];
    const L = spawnPlayerAt(s, { kind: 'human', name: 'L', uid: 'lead' }, 10, 10);
    const T = spawnPlayerAt(s, { kind: 'human', name: 'T', uid: 'trail' }, 40, 40);
    assert.equal(L.offers[0].keep, true);
    lead += rare(L.offers[0].cards);
    trail += rare(T.offers[0].cards);
  }
  assert.ok(trail > lead * 1.25, `rare+epic offered: trailing ${trail}, leading ${lead}`);
});

test('families: every card is in exactly one, and the counts include keepers and used items', () => {
  const all = Object.values(FAMILIES).flat();
  assert.equal(new Set(all).size, all.length);
  assert.deepEqual([...all].sort(), Object.keys(CARDS).sort());
  const s = arena('fam');
  const a = spawnPlayerAt(s, { kind: 'human', name: 'A' }, 10, 10);
  a.offers[0].cards = ['grenade', 'dash', 'shield'];
  pick(s, a, 0);
  assert.equal(familyCounts(a).trickster, 1);
  step(s, [{ id: a.id, turns: [1] }]);
  step(s, [{ id: a.id, turns: [], cards: [ACT.item0] }], 2);
  assert.equal(a.cards.items.length, 0, 'the grenade was thrown');
  assert.equal(familyCounts(a).trickster, 1, 'a used item still counts');
});

/** A player with `n` drafted cards of family `f` this life. @param {Player} p @param {string} f @param {number} n */
const withFamily = (p, f, n) => {
  p.fx[{ runner: 'famRunner', trickster: 'famTrick', builder: 'famBuild' }[f] ?? ''] = n;
};

test('Runner set: the ability recharges faster', () => {
  const recharge = (/** @type {number} */ n) => {
    const s = arena('runner', { cards: true });
    const a = spawnPlayerAt(s, { kind: 'human', name: 'A' }, 10, 10);
    a.offers = [];
    a.fx.drafted = 3;
    withFamily(a, 'runner', n);
    a.cards.active = { id: 'dash', level: 1, ready: s.tick + 100 };
    let t = 0;
    while (a.cards.active.ready > s.tick) {
      step(s);
      t++;
    }
    return t;
  };
  const none = recharge(0);
  const one = recharge(SET_AT[0]);
  const two = recharge(SET_AT[1]);
  assert.ok(none >= 99 && none <= 100, `no bonus ${none}`);
  assert.ok(one <= 84 && one >= 76, `tier 1 ${one}`);
  assert.ok(two <= 70 && two >= 62, `tier 2 ${two}`);
});

test('Builder set: land scores more', () => {
  const s = arena('builder');
  const a = spawnPlayerAt(s, { kind: 'human', name: 'A' }, 10, 10);
  a.land = 200;
  assert.equal(scoreOf(s, a), 200);
  withFamily(a, 'builder', SET_AT[0]);
  assert.equal(scoreOf(s, a), 216);
  withFamily(a, 'builder', SET_AT[1]);
  assert.equal(scoreOf(s, a), 230);
});

test('Trickster set: stepping off your land raises a short ward that recharges', () => {
  const s = arena('trick');
  const a = spawnPlayerAt(s, { kind: 'human', name: 'A' }, 10, 10);
  a.offers = [];
  a.fx.drafted = 3;
  withFamily(a, 'trickster', SET_AT[0]);
  step(s, [{ id: a.id, turns: [1] }]); // (11,10): still home
  step(s); // (12,10): first trail tile
  assert.equal(a.trail.length, 1);
  assert.ok(fxOn(a, 'shield', s.tick), 'warded');
  assert.ok(!fxOn(a, 'shield', s.tick + 21));
  assert.deepEqual(setBonuses(a), { runner: 0, trickster: 1, builder: 0 });
});

test('a whole bot run is deterministic and every player stays within the draft rules', () => {
  const run = () => {
    const s = createState({ w: 60, h: 60, targetHeads: 8, mode: 'draft', roundTicks: 350 }, 'det-run', { fill: true });
    let finals = 0;
    for (let t = 0; t < (350 + RESULTS_TICKS) * 3 + 5; t++) {
      const was = s.round.phase;
      step(s);
      s.events.length = 0;
      for (const p of s.players) {
        assert.ok(p.offers.length <= 1);
        assert.ok((p.fx.drafted ?? 0) <= 3 || !p.alive);
      }
      if (was === 'play' && s.round.phase === 'over' && s.round.result?.run?.final) finals++;
    }
    return { hash: stateHash(s), finals, md: JSON.stringify(s.md) };
  };
  const a = run();
  assert.equal(a.finals, 1, 'one run finished');
  assert.deepEqual(run(), a);
});

test('balance guard: over bot runs no family wins most rounds, and catch-up lets different bots win runs', () => {
  const fams = Object.keys(FAMILIES);
  /** @type {Record<string, number>} */
  const wins = { runner: 0, trickster: 0, builder: 0, none: 0 };
  const champs = new Set();
  let rounds = 0;
  for (let r = 0; r < 8; r++) {
    const s = createState({ w: 70, h: 70, targetHeads: 10, mode: 'draft', roundTicks: 420 }, `guard-${r}`, { fill: true });
    for (let t = 0; t < (420 + RESULTS_TICKS) * 3; t++) {
      const was = s.round.phase;
      step(s);
      s.events.length = 0;
      if (was !== 'play' || s.round.phase !== 'over') continue;
      rounds++;
      const w = s.players.find((p) => p.id === s.round.result?.players[0]?.id);
      if (w) {
        const c = familyCounts(w);
        const top = fams.filter((f) => c[f] > 0 && c[f] === Math.max(...fams.map((g) => c[g])));
        wins[top.length === 1 ? top[0] : 'none']++;
      }
      if (s.round.result?.run?.champion) champs.add(s.round.result.run.champion);
    }
  }
  assert.equal(rounds, 24);
  for (const f of fams) assert.ok(wins[f] <= rounds * 0.6, `${f} won ${wins[f]} of ${rounds}: ${JSON.stringify(wins)}`);
  assert.ok(champs.size >= 3, `champions: ${[...champs]}`);
});
