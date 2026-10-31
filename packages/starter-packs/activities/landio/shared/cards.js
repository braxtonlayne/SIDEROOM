// @ts-check
// The card engine (D-37). Cards are data plus a few optional hooks, one module per card in shared/cards/.
// The simulation calls the hooks at fixed points of a tick, always in REGISTRY order, so every outcome is
// deterministic. A player holds up to MAX_ITEMS single-use items (Q, E), one active ability (Shift, with a
// recharge) and up to MAX_PASSIVES passives. A kill drafts a choice of cards (1/2/3 or click).
//
// Timed effects live in `p.fx` as the tick they END on (exclusive), so the client can predict them too.

import { rand } from './rng.js';
import { addObject } from './cards/util.js';

import grenade from './cards/grenade.js';
import dash from './cards/dash.js';
import shield from './cards/shield.js';
import rewind from './cards/rewind.js';
import snare from './cards/snare.js';
import claimflag from './cards/claimflag.js';
import phase from './cards/phase.js';
import wall from './cards/wall.js';
import overgrowth from './cards/overgrowth.js';
import bloodlust from './cards/bloodlust.js';
import mirage from './cards/mirage.js';
import emp from './cards/emp.js';
import secondwind from './cards/secondwind.js';
import haste from './cards/haste.js';

/** @typedef {import('./types.js').GameState} GameState */
/** @typedef {import('./types.js').Player} Player */
/** @typedef {import('./types.js').WorldObject} WorldObject */

/**
 * One card. Every hook is optional; hooks run for EVERY card on EVERY player (in registry order) and decide
 * themselves whether they apply (from `level`, 0 when the player does not hold the card, or from `p.fx`).
 * @typedef {object} CardDef
 * @property {string} id
 * @property {string} name
 * @property {string} text                 one line for the card face
 * @property {'item'|'active'|'passive'} kind
 * @property {'common'|'rare'|'epic'} rarity
 * @property {number} maxLevel             1, or 2 when a duplicate upgrades it
 * @property {boolean} [notForLeader]      never offered to the player with the most land
 * @property {(level: number) => number} [cooldown]   active: ticks before it can be used again
 * @property {(s: GameState, p: Player, level: number) => boolean} [use]   item/active: do it; false = nothing happened (item kept, no recharge)
 * @property {(level: number, fx: Record<string, number>, tick: number) => number} [speed]   extra tiles this tick (pure: the client predicts with it)
 * @property {(level: number, fx: Record<string, number>, at: number) => void} [predictUse]   the client's guess of what using it at tick `at` does to `fx`
 * @property {(s: GameState, p: Player, level: number) => void} [onTick]   start of each tick, before moving
 * @property {(s: GameState, p: Player, gained: number[], level: number) => void} [onCapture]
 * @property {(s: GameState, p: Player, victim: Player, level: number) => void} [onKill]
 * @property {(s: GameState, p: Player, cause: string, killer: Player|null, level: number) => boolean} [preventDeath]   true = p survives (the card handles what happens instead)
 * @property {(s: GameState, mover: Player, owner: Player, level: number) => boolean} [trailImmune]   true = mover stepping on owner's trail kills nobody (level is the MOVER's)
 * @property {(s: GameState, o: WorldObject) => void} [objectTick]          for objects of kind === id, after moving
 * @property {(s: GameState, o: WorldObject, x: number, y: number) => boolean} [objectBlocks]   a head cannot enter (x, y): it stops instead
 * @property {(s: GameState, p: Player, level: number) => number} [botScore]   0..1: how much a bot wants to use it now
 */

/** @type {CardDef[]} the fixed resolution order */
export const REGISTRY = [grenade, dash, shield, rewind, snare, claimflag, phase, wall, overgrowth, bloodlust, mirage, emp, secondwind, haste];
/** @type {Record<string, CardDef>} */
export const CARDS = Object.fromEntries(REGISTRY.map((c) => [c.id, c]));

export const MAX_ITEMS = 2;
export const MAX_PASSIVES = 2;
/** Most extra tiles per tick from all speed effects together (so at most 3 tiles a tick). */
export const MAX_EXTRA_MOVES = 2;
/** Ticks a human has to pick before the first card is taken for them; bots decide sooner. */
export const OFFER_TICKS = 56;
export const BOT_OFFER_TICKS = 4;
/** A bot uses a card only when its best score clears this. */
export const BOT_THRESHOLD = 0.6;

/** Card actions inside an act (see server/arena.js input): dir codes below -1. */
export const ACT = { item0: -2, item1: -3, active: -4, choose0: -5 };

/** A player's card state, empty. */
export function emptyHand() {
  return {
    /** @type {{id: string, level: number}[]} */
    items: [],
    /** @type {{id: string, level: number, ready: number} | null} */
    active: null,
    /** @type {{id: string, level: number}[]} */
    passives: [],
  };
}

/** The level at which `p` holds card `id` (as active or passive), 0 if not. @param {Player} p @param {string} id */
export function levelOf(p, id) {
  if (p.cards.active?.id === id) return p.cards.active.level;
  for (const c of p.cards.passives) if (c.id === id) return c.level;
  return 0;
}

/** Whether timed effect `name` is on for `p` at tick `t`. @param {Player} p @param {string} name @param {number} t */
export const fxOn = (p, name, t) => (p.fx[name] ?? 0) > t;

/** Rank of `p` by land among living players: 1 = most land. @param {GameState} s @param {Player} p */
export function rankOf(s, p) {
  let r = 1;
  for (const q of s.players) if (q.alive && q !== p && q.land > p.land) r++;
  return r;
}

/**
 * Draft after a kill: 3 different cards (4 when the victim had the most land: the bounty), weighted by rarity
 * and by the killer's rank so the leader draws mostly commons and the trailing players better ones (D-38).
 * @param {GameState} s @param {Player} killer @param {boolean} bounty
 */
export function offerCards(s, killer, bounty) {
  if (!s.cfg.cards) return;
  const rank = rankOf(s, killer);
  const w = s.cfg.sandbox
    ? { common: 1, rare: 1, epic: 1 }
    : rank === 1
      ? { common: 6, rare: 1.5, epic: 0 }
      : rank <= 3
        ? { common: 6, rare: 3, epic: 1 }
        : { common: 5, rare: 4, epic: 2 };
  const pool = REGISTRY.filter((c) => s.cfg.sandbox || !(rank === 1 && c.notForLeader));
  /** @type {string[]} */
  const picks = [];
  const n = bounty ? 4 : 3;
  while (picks.length < n) {
    const left = pool.filter((c) => !picks.includes(c.id) && w[c.rarity] > 0);
    if (!left.length) break;
    const total = left.reduce((a, c) => a + w[c.rarity], 0);
    let roll = rand(s) * total;
    let chosen = left[left.length - 1];
    for (const c of left) {
      roll -= w[c.rarity];
      if (roll < 0) {
        chosen = c;
        break;
      }
    }
    picks.push(chosen.id);
  }
  killer.offers.push({ cards: picks, until: s.tick + (killer.kind === 'bot' ? BOT_OFFER_TICKS : OFFER_TICKS) });
}

/**
 * Take card `id`: an item goes in a free slot (the oldest goes when both are full), a duplicate active or
 * passive levels up, a new active replaces the old one, a new passive replaces the oldest when both are full.
 * @param {GameState} s @param {Player} p @param {string} id
 */
export function grant(s, p, id) {
  const def = CARDS[id];
  if (!def) return;
  const h = p.cards;
  if (def.kind === 'item') {
    h.items.push({ id, level: 1 });
    if (h.items.length > MAX_ITEMS) h.items.shift();
  } else if (def.kind === 'active') {
    if (h.active?.id === id) h.active.level = Math.min(def.maxLevel, h.active.level + 1);
    else h.active = { id, level: 1, ready: s.tick };
  } else {
    const have = h.passives.find((c) => c.id === id);
    if (have) have.level = Math.min(def.maxLevel, have.level + 1);
    else {
      h.passives.push({ id, level: 1 });
      if (h.passives.length > MAX_PASSIVES) h.passives.shift();
    }
  }
  s.events.push({ k: 'card', id: p.id, name: p.name, card: id });
}

/** Choose card `i` of the oldest offer. @param {GameState} s @param {Player} p @param {number} i */
export function choose(s, p, i) {
  const o = p.offers[0];
  if (!o || i < 0 || i >= o.cards.length) return;
  p.offers.shift();
  grant(s, p, o.cards[i]);
}

/**
 * Use item slot `slot` (0/1) or the active ability (slot -1). Atomic: nothing changes unless the card did
 * something. EMP stops all use.
 * @param {GameState} s @param {Player} p @param {number} slot
 */
export function useCard(s, p, slot) {
  if (!p.alive || fxOn(p, 'emp', s.tick)) return false;
  if (slot >= 0) {
    const it = p.cards.items[slot];
    const def = it && CARDS[it.id];
    if (!def?.use || !def.use(s, p, it.level)) return false;
    p.cards.items.splice(slot, 1);
    s.events.push({ k: 'use', id: p.id, card: it.id });
    return true;
  }
  const a = p.cards.active;
  const def = a && CARDS[a.id];
  if (!a || !def?.use || s.tick < a.ready || !def.use(s, p, a.level)) return false;
  a.ready = s.tick + (def.cooldown ? def.cooldown(a.level) : 0);
  s.events.push({ k: 'use', id: p.id, card: a.id });
  return true;
}

/** Apply one card act code (ACT.*) for `p`. @param {GameState} s @param {Player} p @param {number} code */
export function cardAct(s, p, code) {
  if (code === ACT.item0) useCard(s, p, 0);
  else if (code === ACT.item1) useCard(s, p, 1);
  else if (code === ACT.active) useCard(s, p, -1);
  else if (code <= ACT.choose0 && code > ACT.choose0 - 4) choose(s, p, ACT.choose0 - code);
}

/** Extra tiles `p` moves this tick (all speed effects, capped). @param {Player} p @param {number} tick */
export function extraMoves(p, tick) {
  let n = 0;
  for (const def of REGISTRY) if (def.speed) n += def.speed(levelOf(p, def.id), p.fx, tick);
  return Math.min(MAX_EXTRA_MOVES, n);
}

/** Start-of-tick card work: expire offers, EMP holds recharges, onTick hooks, bot card use. @param {GameState} s */
export function cardsTick(s) {
  for (const p of s.players) {
    if (!p.alive) continue;
    while (p.offers.length && p.offers[0].until <= s.tick) {
      // Not chosen in time: a bot takes a random one, a human the first.
      const i = p.kind === 'bot' ? Math.floor(rand(s) * p.offers[0].cards.length) : 0;
      choose(s, p, i);
    }
    if (fxOn(p, 'emp', s.tick) && p.cards.active) p.cards.active.ready = Math.max(p.cards.active.ready, s.tick) + 1;
    for (const def of REGISTRY) def.onTick?.(s, p, levelOf(p, def.id));
    if (p.kind === 'bot') botCards(s, p);
  }
}

/**
 * Utility-scored card use for a bot: each held card scores 0..1 for "use it now" and the best one is used if
 * it clears BOT_THRESHOLD. At most one card per tick, and none for a few ticks after one (no spamming).
 * @param {GameState} s @param {Player} p
 */
function botCards(s, p) {
  if ((p.fx.botCardWait ?? 0) > s.tick) return;
  let best = -1;
  let bestScore = BOT_THRESHOLD;
  const h = p.cards;
  h.items.forEach((it, i) => {
    const sc = CARDS[it.id]?.botScore?.(s, p, it.level) ?? 0;
    if (sc > bestScore) {
      bestScore = sc;
      best = i;
    }
  });
  if (h.active && s.tick >= h.active.ready) {
    const sc = CARDS[h.active.id]?.botScore?.(s, p, h.active.level) ?? 0;
    if (sc > bestScore) {
      bestScore = sc;
      best = -2; // the active ability
    }
  }
  if (best === -1) return;
  if (useCard(s, p, best === -2 ? -1 : best)) p.fx.botCardWait = s.tick + 7;
}

/** Hooks run by the simulation at fixed points. */
export const hooks = {
  /** @param {GameState} s @param {Player} p @param {number[]} gained */
  capture(s, p, gained) {
    for (const def of REGISTRY) def.onCapture?.(s, p, gained, levelOf(p, def.id));
  },
  /** @param {GameState} s @param {Player} killer @param {Player} victim */
  kill(s, killer, victim) {
    for (const def of REGISTRY) def.onKill?.(s, killer, victim, levelOf(killer, def.id));
  },
  /** @param {GameState} s @param {Player} p @param {string} cause @param {Player|null} killer */
  preventDeath(s, p, cause, killer) {
    for (const def of REGISTRY) if (def.preventDeath?.(s, p, cause, killer, levelOf(p, def.id))) return true;
    return false;
  },
  /** @param {GameState} s @param {Player} mover @param {Player} owner */
  trailImmune(s, mover, owner) {
    for (const def of REGISTRY) if (def.trailImmune?.(s, mover, owner, levelOf(mover, def.id))) return true;
    return false;
  },
  /** @param {GameState} s @param {number} x @param {number} y */
  blocked(s, x, y) {
    for (const o of s.objects) if (o.until > s.tick && !o.dead && CARDS[o.kind]?.objectBlocks?.(s, o, x, y)) return true;
    return false;
  },
  /** Objects whose time is up are gone before the live ones run (`until` is exclusive). @param {GameState} s */
  objects(s) {
    s.objects = s.objects.filter((o) => !o.dead && o.until > s.tick);
    for (const o of s.objects.slice()) CARDS[o.kind]?.objectTick?.(s, o);
    s.objects = s.objects.filter((o) => !o.dead);
  },
};

export { addObject };

/** Head flags for effects other people see (bit values in shared/codec.js). @param {Player} p @param {number} t */
export function effectFlags(p, t) {
  let f = 0;
  if (fxOn(p, 'shield', t)) f |= 4;
  if (fxOn(p, 'phase', t)) f |= 8;
  if (fxOn(p, 'dash', t)) f |= 16;
  if (fxOn(p, 'stun', t)) f |= 32;
  if (levelOf(p, 'secondwind') && !p.fx.secondwindUsed) f |= 64;
  if (fxOn(p, 'emp', t)) f |= 128;
  if (fxOn(p, 'spawn', t)) f |= 256;
  return f;
}
