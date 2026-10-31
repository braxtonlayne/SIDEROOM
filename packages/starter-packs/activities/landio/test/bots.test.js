// @ts-check
// Bot behaviour (shared/bots.js): no self or border deaths over seeded games, a bot flees home from a racing
// enemy, games are deterministic, and a bot does not step into its own trail when another move exists.
import assert from 'node:assert/strict';
import { botThink } from '../shared/bots.js';
import { setTrail } from '../shared/grid.js';
import { createState, removePlayer, spawnPlayer, spawnPlayerAt, step, stateHash, takeEvents } from '../shared/rules.js';
import { broken } from './cards/helpers.js';

/** @typedef {import('../shared/types.js').GameState} GameState */

test('bots never die to themselves or the border: at most 1% of 20 games of 3,000 ticks', () => {
  const causes = new Map();
  let deaths = 0;
  for (let g = 0; g < 20; g++) {
    const s = createState({ w: 100, h: 100, targetHeads: 12 }, `bots-safe-${g}`, { fill: true });
    for (let t = 0; t < 3000; t++) {
      step(s);
      for (const e of takeEvents(s)) {
        if (e.k !== 'kill' || !e.c) continue;
        deaths++;
        causes.set(e.c, (causes.get(e.c) ?? 0) + 1);
      }
    }
  }
  // The games were real games: bots died to trails and heads, and every death was counted.
  assert.ok(deaths > 100, `only ${deaths} deaths in 20 games`);
  const own = (causes.get('self') ?? 0) + (causes.get('border') ?? 0);
  assert.ok(own <= deaths * 0.01, `${own} self or border deaths of ${deaths} (${JSON.stringify([...causes])})`);
});

test('a bot with an enemy racing to its trail goes home and captures before the enemy arrives', () => {
  // The bot (B) has laid a trail of 7 tiles east of its base, and is 8 steps from home round it. The racer (H)
  // comes down towards that trail, then turns away after two ticks; a bot that kept walking its loop would be cut.
  const s = createState({ w: 30, h: 30, targetHeads: 2, cards: true }, 'bots-race');
  s.humans = 1;
  const B = spawnPlayerAt(s, { kind: 'bot', name: 'B' }, 5, 15);
  const H = spawnPlayerAt(s, { kind: 'human', name: 'H' }, 13, 8);
  H.stopped = false;
  H.dir = 2;
  const trail = [];
  for (let x = 7; x <= 13; x++) {
    setTrail(s, 15 * 30 + x, B.id);
    trail.push(15 * 30 + x);
  }
  B.trail = trail;
  B.x = 13;
  B.y = 15;
  B.dir = 1;
  if (B.bot) {
    B.bot.mode = 'expand';
    B.bot.legs = [1, 5, 2, 3];
  }

  let captured = -1;
  for (let t = 1; t <= 8; t++) {
    if (t === 3) H.dir = 1;
    step(s);
    assert.equal(broken(s), null, `tick ${t}`);
    assert.ok(B.alive, `bot died at tick ${t}`);
    if (t === 1) assert.equal(B.bot?.mode, 'flee', 'the racer is in reach: the bot flees');
    for (const e of takeEvents(s)) if (e.k === 'capture' && e.id === B.id && captured < 0) captured = t;
  }
  // Eight steps is its way home along the shortest path that avoids its own trail.
  assert.equal(captured, 8, 'captured on the 8th tick, when the bot reaches home');
  assert.equal(B.trail.length, 0);
});

test('same seed, same game: identical state after 2,000 ticks; another seed differs', () => {
  const run = (/** @type {string} */ seed) => {
    const s = createState({ w: 100, h: 100, targetHeads: 12 }, seed, { fill: true });
    for (let t = 0; t < 2000; t++) step(s);
    return stateHash(s);
  };
  const a = run('bots-determinism');
  assert.equal(run('bots-determinism'), a);
  assert.notEqual(run('bots-other'), a);
});

test('a bot does not step into its own trail when another move exists', () => {
  // A small loop of its own trail around the tile above the head: up is its own trail, right is its own trail,
  // down is its own trail; left is free. The only move is left.
  /** @type {GameState} */
  const s = createState({ w: 30, h: 30, targetHeads: 0 }, 'bots-own-trail');
  const bot = spawnPlayerAt(s, { kind: 'bot', name: 'Loop' }, 5, 15);
  const path = [10 + 9 * 30, 11 + 9 * 30, 11 + 10 * 30, 11 + 11 * 30, 10 + 11 * 30, 10 + 10 * 30];
  for (const i of path) setTrail(s, i, bot.id);
  bot.trail = path;
  bot.x = 10;
  bot.y = 10;
  bot.dir = 0;
  if (bot.bot) bot.bot.mode = 'home';
  const d = botThink(s, bot);
  assert.equal(d, 3, `chose ${d}: 0 is up into its own trail, 1 right and 2 down are its trail too`);
});

test('bot skill: hard bots catch a looping player more often than easy ones', () => {
  // A scripted player loops out 8 tiles and back, over and over, in an arena of 10 bots; respawned when caught.
  const LOOP = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 3, 3];
  /** @param {'easy' | 'hard'} skill */
  const caught = (skill) => {
    let n = 0;
    for (let seed = 0; seed < 6; seed++) {
      const s = createState({ w: 80, h: 80, targetHeads: 10, botSkill: skill }, 'dummy' + seed, { fill: true });
      s.humans = 1;
      /** @type {import('../shared/types.js').Player | null} */
      let me = null;
      let k = 0;
      for (let t = 0; t < 4000; t++) {
        if (!me || !me.alive) {
          if (me) removePlayer(s, me.id);
          me = spawnPlayer(s, { kind: 'human', name: 'Dummy', uid: 'd' });
          k = 0;
        }
        step(s, me ? [{ id: me.id, turns: [LOOP[k++ % LOOP.length]] }] : []);
        for (const e of s.events) if (e.k === 'kill' && e.vn === 'Dummy' && e.id) n++;
        s.events = [];
      }
    }
    return n;
  };
  const easy = caught('easy');
  const hard = caught('hard');
  assert.ok(hard >= easy * 1.4, `hard ${hard} vs easy ${easy}`);
});
