// @ts-check
// Bot backfill (ACT-LND-002). Bots keep an arena playable: they fill the seats people leave empty and step out,
// one per tick, as people join (rules.js `manageBots`). Every mode can ask for a minimum number of heads (a team
// game needs two a side, Last Land needs someone to outlast); with `cfg.backfill` on (the activity's arenas),
// bots fill up to that minimum even when the host set fewer bots.
//
// Bots are named `Lumen bot` and `Tofu bot`, then `Lumen bot 2`, `Tofu bot 2`, and so on: the lowest free name.
// Names are picked without the RNG (it is a pure function of who is present), every other bot choice (spawn
// spot, style, boldness, moves, cards) goes through the state's seeded RNG, so a game replays identically.

import { modeOf } from './modes/index.js';

/** @typedef {import('./types.js').GameState} GameState */

/** The bot names, in the order they are handed out. */
export const BOT_BASE_NAMES = ['Lumen bot', 'Tofu bot'];

/** The n-th bot name (0-based): Lumen bot, Tofu bot, Lumen bot 2, Tofu bot 2, ... @param {number} n */
export function botNameAt(n) {
  const base = BOT_BASE_NAMES[n % BOT_BASE_NAMES.length];
  const round = Math.floor(n / BOT_BASE_NAMES.length);
  return round ? `${base} ${round + 1}` : base;
}

/** The first bot name nobody present has. @param {Iterable<string>} taken */
export function nextBotName(taken) {
  const used = new Set(taken);
  for (let n = 0; ; n++) {
    const name = botNameAt(n);
    if (!used.has(name)) return name;
  }
}

/** The fewest heads the arena's mode wants; 0 unless backfill is on. @param {GameState} s */
export function modeMinHeads(s) {
  return s.cfg.backfill ? Math.max(0, modeOf(s).minHeads ?? 0) : 0;
}

/** How many heads (people and bots) the arena aims for. @param {GameState} s */
export const wantedHeads = (s) => Math.max(s.cfg.targetHeads, modeMinHeads(s));

/** How many bots the arena should have now: the wanted heads minus the seated people. @param {GameState} s */
export const botQuota = (s) => Math.max(0, wantedHeads(s) - s.humans);
