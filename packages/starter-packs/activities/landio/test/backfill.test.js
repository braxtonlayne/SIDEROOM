// @ts-check
// Bot backfill (ACT-LND-002, shared/backfill.js): bots are Lumen bot, Tofu bot, then numbered; they fill the seats a
// mode needs and leave as people join; every bot choice stays on the seeded RNG.
import assert from 'node:assert/strict';
import { BOT_BASE_NAMES, botNameAt, botQuota, nextBotName, wantedHeads } from '../shared/backfill.js';
import { MODES } from '../shared/modes/index.js';
import { createState, removePlayer, spawnPlayer, step, stateHash } from '../shared/rules.js';
import { LandioHost } from '../host/engine.js';

const bots = (/** @type {import('../shared/types.js').GameState} */ s) => s.players.filter((p) => p.kind === 'bot');

test('bot names: Lumen bot, Tofu bot, then numbered variants, lowest free first', () => {
  assert.deepEqual(BOT_BASE_NAMES, ['Lumen bot', 'Tofu bot']);
  assert.deepEqual([0, 1, 2, 3, 4, 5].map(botNameAt), ['Lumen bot', 'Tofu bot', 'Lumen bot 2', 'Tofu bot 2', 'Lumen bot 3', 'Tofu bot 3']);
  assert.equal(nextBotName([]), 'Lumen bot');
  assert.equal(nextBotName(['Lumen bot', 'Tofu bot', 'Tofu bot 2']), 'Lumen bot 2');
  assert.equal(nextBotName(['Tofu bot']), 'Lumen bot');
  assert.equal(nextBotName(['Lumen bot', 'Tofu bot', 'Lumen bot 2']), 'Tofu bot 2', 'a gap left by a bot that went home is reused');
});

test('every bot in a filled arena has a distinct Lumen/Tofu name', () => {
  const s = createState({ w: 100, h: 100, targetHeads: 12 }, 'names', { fill: true });
  const names = bots(s).map((p) => p.name);
  assert.equal(names.length, 12);
  assert.equal(new Set(names).size, 12);
  for (const n of names) assert.match(n, /^(Lumen|Tofu) bot( \d+)?$/);
  assert.ok(names.every((n) => n.length <= 24));
});

test('every round mode asks for a minimum; backfill tops a lobby up to it whatever the bot setting', () => {
  for (const m of MODES) {
    if (m.id === 'classic') {
      assert.equal(m.minHeads ?? 0, 0, 'classic plays fine alone');
      continue;
    }
    assert.ok((m.minHeads ?? 0) >= 2, `${m.id} has a minimum`);
    const s = createState({ w: 60, h: 60, targetHeads: 0, mode: m.id, backfill: true }, 'min-' + m.id, { fill: true });
    assert.equal(wantedHeads(s), m.minHeads);
    assert.equal(bots(s).length, m.minHeads, `${m.id}: filled to its minimum`);
    const off = createState({ w: 60, h: 60, targetHeads: 0, mode: m.id }, 'min-' + m.id, { fill: true });
    assert.equal(bots(off).length, 0, `${m.id}: no backfill unless asked (rules default, tests)`);
  }
});

test('bots leave as people join and come back when people leave', () => {
  const s = createState({ w: 60, h: 60, targetHeads: 0, mode: 'teams', backfill: true, botRespawnTicks: 3 }, 'come-go', { fill: true });
  assert.equal(bots(s).length, 4);
  /** @type {import('../shared/types.js').Player[]} */
  const people = [];
  for (let i = 0; i < 3; i++) {
    s.humans++;
    const p = spawnPlayer(s, { kind: 'human', name: `P${i}`, uid: `u${i}` });
    assert.ok(p);
    people.push(p);
    step(s, [], 1);
    assert.equal(botQuota(s), 4 - s.humans);
    assert.equal(bots(s).length, 4 - s.humans, `after ${s.humans} joined`);
  }
  // A full lobby of people (4 or more): no bots at all.
  s.humans = 5;
  step(s, [], 2);
  assert.equal(bots(s).length, 0);
  // Everyone leaves: the bots return, one per cooldown.
  for (const p of people) removePlayer(s, p.id);
  s.humans = 0;
  step(s, [], 40);
  assert.equal(bots(s).length, 4);
  assert.deepEqual(bots(s).map((p) => p.name).sort(), ['Lumen bot', 'Lumen bot 2', 'Tofu bot', 'Tofu bot 2']);
});

test('the host setting more bots than the minimum wins; backfill never removes bots the host asked for', () => {
  const s = createState({ w: 80, h: 80, targetHeads: 9, mode: 'draft', backfill: true }, 'more', { fill: true });
  assert.equal(bots(s).length, 9);
  s.humans = 2;
  step(s, [], 3);
  assert.equal(bots(s).length, 7);
});

test('backfilled games are deterministic: same seed and joins, same state', () => {
  const run = () => {
    const s = createState({ w: 60, h: 60, targetHeads: 0, mode: 'infection', backfill: true }, 'det', { fill: true });
    for (let t = 0; t < 900; t++) {
      if (t === 100) {
        s.humans = 1;
        spawnPlayer(s, { kind: 'human', name: 'A', uid: 'a' });
      }
      if (t === 500) s.humans = 0;
      step(s);
    }
    return stateHash(s);
  };
  assert.equal(run(), run());
});

test('the activity host backfills a mode lobby and sends bots home as members sit down', () => {
  let t = 1000;
  const eng = new LandioHost({ instance: 'bf', hostId: 'h1', send() {}, publish() {}, now: () => t, seed: 'bf', cfg: { targetHeads: 0, mode: 'teams' } });
  assert.equal(bots(eng.state).length, 4, 'a host alone in a team lobby still gets a game');
  eng.handle('h1', 'Alice', { t: 'hello' });
  eng.handle('g1', 'Bob', { t: 'hello' });
  for (let i = 0; i < 3; i++) {
    t += 143;
    eng.tick();
  }
  assert.equal(bots(eng.state).length, 2);
  eng.close();
});
