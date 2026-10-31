// @ts-check
// The round framework (D-51): rounds end and restart on a fresh map, the dead come back (or not), teams.
import assert from 'node:assert/strict';
import { RESULTS_TICKS, JOIN_GRACE_TICKS, createState, maySpawn, spawnPlayerAt, step, startRound } from '../shared/rules.js';
import { MODES, allied, results, scoreOf } from '../shared/modes/index.js';
import { setBase } from '../shared/grid.js';

/** @typedef {import('../shared/types.js').GameState} GameState */

const U = 0;
const R = 1;
const D = 2;
const L = 3;

/** @param {Partial<import('../shared/types.js').Config>} cfg */
const arena = (cfg) => createState({ w: 40, h: 40, targetHeads: 0, ...cfg }, 'modes');
/** @param {GameState} s @param {string} name @param {number} x @param {number} y */
const human = (s, name, x, y) => spawnPlayerAt(s, { kind: 'human', name }, x, y);

test('every mode has an id, a name, a blurb and a round length', () => {
  for (const m of MODES) {
    assert.match(m.id, /^[a-z]+$/);
    assert.ok(m.name && m.blurb, m.id);
    assert.ok(m.roundTicks >= 0, m.id);
  }
});

test('classic never ends a round', () => {
  const s = arena({ mode: 'classic' });
  human(s, 'A', 10, 10);
  step(s, [], 5000);
  assert.equal(s.round.phase, 'play');
  assert.equal(s.round.n, 1);
});

test('timed rounds: the round ends on time, freezes, shows results, then a new round revives everyone', () => {
  const s = arena({ mode: 'rounds' });
  const a = human(s, 'A', 10, 10);
  const b = human(s, 'B', 30, 30);
  // A grabs some land so the ranking is clear.
  setBase(s, 20 * 40 + 20, a.id);
  assert.equal(s.round.end, 2100);
  step(s, [], 2099);
  assert.equal(s.round.phase, 'play');
  step(s);
  assert.equal(s.round.phase, 'over');
  assert.equal(s.round.result?.winner, a.id, 'most land wins');
  assert.deepEqual(s.round.result?.players.map((p) => p.id).slice(0, 2), [a.id, b.id]);
  // Frozen: input does nothing.
  const x = a.x;
  step(s, [{ id: a.id, turns: [R] }], 3);
  assert.equal(a.x, x);
  step(s, [], RESULTS_TICKS);
  assert.equal(s.round.phase, 'play');
  assert.equal(s.round.n, 2);
  assert.equal(s.round.start, 2100 + RESULTS_TICKS, 'the next round starts once the results have shown');
  // Same players, fresh lives with 3x3 bases and nothing else.
  for (const p of [a, b]) {
    assert.equal(p.alive, true);
    assert.equal(p.land, 9);
    assert.equal(p.kills, 0);
  }
  assert.ok(s.players.includes(a) && s.players.includes(b));
});

test('a dead human is revived by the next round', () => {
  const s = arena({ mode: 'rounds' });
  const a = human(s, 'A', 2, 10);
  step(s, [{ id: a.id, turns: [L] }], 3); // out of the map
  assert.equal(a.alive, false);
  step(s, [], 2100 + RESULTS_TICKS);
  assert.equal(a.alive, true);
});

test('modes without respawns: one life per round, joining only early on', () => {
  const s = arena({ mode: 'lastland' });
  assert.equal(maySpawn(s, undefined), true, 'at the start anyone may join');
  const a = human(s, 'A', 2, 10);
  step(s, [{ id: a.id, turns: [L] }], 3);
  assert.equal(a.alive, false);
  assert.equal(maySpawn(s, a), false, 'no second life');
  step(s, [], JOIN_GRACE_TICKS);
  assert.equal(maySpawn(s, undefined), false, 'too late to join this round');
  const c = arena({ mode: 'rounds' });
  step(c, [], 1000);
  assert.equal(maySpawn(c, undefined), true, 'respawn modes: any time');
});

test('a round restart rebuilds the map from the round number', () => {
  const s = arena({ mode: 'rounds', map: 'procedural', w: 60, h: 60 });
  const first = [...s.terrain];
  startRound(s);
  const second = [...s.terrain];
  assert.notDeepEqual(first, second, 'a procedural map changes between rounds');
  assert.equal(s.round.n, 2);
});

test('teams: players are split evenly and coloured by team', () => {
  const s = arena({ mode: 'teams' });
  const ps = [human(s, 'A', 6, 6), human(s, 'B', 30, 6), human(s, 'C', 6, 30), human(s, 'D', 30, 30)];
  const counts = [0, 0, 0];
  for (const p of ps) counts[p.team]++;
  assert.deepEqual(counts.slice(1), [2, 2]);
  const sameTeam = ps.filter((p) => p.team === ps[0].team);
  assert.equal(sameTeam[0].look % 8, sameTeam[1].look % 8, 'one colour per team');
  assert.notEqual(sameTeam[0].look, sameTeam[1].look, 'patterns tell teammates apart');
});

test("teams: crossing a teammate's trail is harmless, and neither trail is broken", () => {
  const s = arena({ mode: 'teams' });
  const a = human(s, 'A', 10, 10);
  const b = human(s, 'B', 30, 30); // the other team
  const c = human(s, 'C', 20, 20); // A's team (teams fill evenly: A 1, B 2, C 1)
  assert.equal(c.team, a.team);
  assert.ok(allied(s, a, c));
  assert.ok(!allied(s, a, b));
  // A goes right along y=10 to x=16 (a trail); C goes up from (20,20) to cross it at x=... C walks to (14, 10).
  step(s, [{ id: a.id, turns: [R] }], 6); // A at (16,10), trail 11..16
  assert.equal(a.trail.length, 5);
  // C: left 6 to x=14, then up 10 to y=10 (onto A's trail at (14,10)), then one more up.
  step(s, [{ id: c.id, turns: [L] }]);
  step(s, [], 5);
  step(s, [{ id: c.id, turns: [U] }]);
  step(s, [], 10);
  assert.equal(a.alive, true, 'A survives C crossing its trail');
  assert.equal(c.alive, true);
  assert.ok(a.trail.includes(14 + 10 * 40), "A's trail is intact where C crossed");
  void b;
  void D;
});

test("teams: closing a loop never takes a teammate's land", () => {
  const s = arena({ mode: 'teams' });
  const a = human(s, 'A', 10, 10);
  human(s, 'B', 30, 30);
  const c = human(s, 'C', 14, 10); // A's teammate, base 13..15 x 9..11
  assert.equal(c.team, a.team);
  // A loops around C's base: up 3, right 7, down 6, left 7, up 3 (back home).
  const path = [U, U, U, R, R, R, R, R, R, R, D, D, D, D, D, D, L, L, L, L, L, L, L, U, U];
  for (const d of path) step(s, [{ id: a.id, turns: [d] }]);
  assert.equal(c.alive, true, 'an enclosed teammate lives');
  assert.equal(c.land, 9, "the teammate's land is untouched");
  assert.ok(a.land > 9);
});

test('results: team totals rank the teams; the winner is minus the team number', () => {
  const s = arena({ mode: 'teams' });
  const a = human(s, 'A', 6, 6);
  const b = human(s, 'B', 30, 30);
  for (let i = 0; i < 20; i++) setBase(s, 20 * 40 + i, b.id);
  const res = results(s);
  assert.equal(res.teams[0].team, b.team);
  assert.equal(res.winner, -b.team);
  assert.equal(res.teams[0].score, scoreOf(s, b));
  void a;
});

test('draft night: kills hand out no cards', () => {
  const s = arena({ mode: 'draft' });
  const a = human(s, 'A', 10, 8);
  const b = human(s, 'B', 20, 10);
  // B walks up 3 and stops, leaving a trail at (20, 8) and (20, 7); A runs right along y = 8 into it.
  // (Draft Night drafts on spawn, so compare what the kill adds.)
  step(s, [{ id: b.id, turns: [U] }], 3);
  step(s, [{ id: b.id, toggleStop: true }]);
  const offered = () => a.offers.length + (a.fx.drafted ?? 0);
  const before = offered();
  step(s, [{ id: a.id, turns: [R] }], 10);
  assert.equal(b.alive, false);
  assert.equal(a.kills, 1);
  assert.ok(offered() - before <= 1, 'no bounty draft on top of the spawn drafts');
});

test('gold counts three times in the default score', () => {
  const s = arena({ mode: 'rounds' });
  const a = human(s, 'A', 10, 10);
  s.terrain[30 * 40 + 30] = 2;
  setBase(s, 30 * 40 + 30, a.id);
  assert.equal(a.land, 10);
  assert.equal(scoreOf(s, a), 12);
});

/**
 * A on the left, its base widened to x 9..14 (rows 9..11); the other player's land a block at x 15..25, y 5..15.
 * A walks up from (10, 9) to y = 6, then right onto the block at (15, 6).
 * @param {string} mode @param {boolean} sameTeam
 */
function closeAgainst(mode, sameTeam) {
  const s = arena({ mode });
  const a = human(s, 'A', 10, 10);
  const b = human(s, 'B', 30, 30); // the other team (teams fill evenly)
  const c = human(s, 'C', 20, 20); // A's team
  const other = sameTeam ? c : b;
  for (let y = 9; y <= 11; y++) for (let x = 12; x <= 14; x++) setBase(s, y * 40 + x, a.id);
  for (let y = 5; y <= 15; y++) for (let x = 15; x <= 25; x++) setBase(s, y * 40 + x, other.id);
  const otherLand = other.land;
  for (const d of [U, U, U, U, R, R, R, R, R]) step(s, [{ id: a.id, turns: [d] }]);
  return { s, a, other, otherLand };
}

test("teams: a loop closes against a teammate's land, and the pocket it walls in becomes yours", () => {
  const { s, a, other, otherLand } = closeAgainst('teams', true);
  assert.equal(a.alive, true);
  assert.equal(a.x, 15);
  assert.equal(a.trail.length, 0, 'arriving on teammate land closed the loop');
  for (let y = 7; y <= 8; y++) for (let x = 11; x <= 14; x++) assert.equal(s.base[y * 40 + x], a.id, `(${x}, ${y}) is A's`);
  assert.equal(other.land, otherLand, "the teammate's land is untouched");
  // Walking on across the teammate's land lays no trail.
  step(s, [{ id: a.id, turns: [R] }], 4);
  assert.equal(a.trail.length, 0);
});

test("teams: an enemy's land does not close a loop (the trail runs on over it)", () => {
  const { s, a } = closeAgainst('teams', false);
  assert.equal(a.alive, true);
  assert.ok(a.trail.length > 0, 'still trailing on enemy land');
  assert.notEqual(s.base[8 * 40 + 12], a.id);
});
