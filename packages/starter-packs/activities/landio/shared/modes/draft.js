// @ts-check
// Draft Night (ACT-LND-002): a roguelike run of RUN_ROUNDS rounds.
//
//  - Every life starts with DRAFTS card drafts in a row (the next appears once you pick, or the timer picks for
//    you). Kills give no cards.
//  - The first draft of your first life in each round is a KEEPER pick: the card you take stays with you for
//    the rest of the run, handed to you again at every spawn. It is the pick between rounds. Round 1's keeper
//    draft is fair (every card equally likely); later ones lean to rare and epic cards for whoever trails the
//    run, so a bad round buys a better keeper.
//  - The other drafts are weighted by rarity (commons most often, epics least), with the same catch-up lean.
//  - Cards come in three families. Drafting three cards of one family in a life (keepers count, and so does a
//    used item: the bonus does not flicker as items are thrown) turns on its set bonus, five its
//    stronger version (only reachable with keepers, so it is the late-run payoff):
//      Runner    dash, haste, phase, bloodlust, rewind     your ability recharges 20% (II: 33%) faster
//      Trickster grenade, snare, mirage, emp, wall         leaving your land wards you for 3 s (II: 5 s): the first
//                                                          cut only stuns the cutter (as Shield); recharges in 20 s
//      Builder   claimflag, overgrowth, shield, secondwind your land scores 8% (II: 15%) more
//  - Each round pays run points by place (5, 3, 2, then 1 for everyone else listed). After the last round the
//    results are the end-of-run summary: the standings, everyone's keepers, the champion. Then a new run.
//
// Run state lives in s.md (sent to clients with the round) and survives the round's reset through `carry`. The
// rules engine owns rounds; this file only fills ModeDef hooks. Everything goes through the seeded RNG.

import { CARDS, REGISTRY, choose, grant } from '../cards.js';
import { rand } from '../rng.js';

/** @typedef {import('../types.js').GameState} GameState */
/** @typedef {import('../types.js').Player} Player */
/** @typedef {'common' | 'rare' | 'epic'} Rarity */

/** Drafts per life. */
export const DRAFTS = 3;
/** Rounds per run. */
export const RUN_ROUNDS = 3;
/** Run points by place in a round; everyone further down who is listed gets PLACE_REST. */
export const PLACE_POINTS = [5, 3, 2];
export const PLACE_REST = 1;
/** Ticks a human has to pick, as the engine's OFFER_TICKS; bots pick at once. */
const OFFER_TICKS = 56;
const BOT_OFFER_TICKS = 4;

/** The three families and their cards. @type {Record<string, string[]>} */
export const FAMILIES = {
  runner: ['dash', 'haste', 'phase', 'bloodlust', 'rewind'],
  trickster: ['grenade', 'snare', 'mirage', 'emp', 'wall'],
  builder: ['claimflag', 'overgrowth', 'shield', 'secondwind'],
};
/** Display names and what each set bonus does, for the HUD. */
export const FAMILY_INFO = {
  runner: { name: 'Runner', bonus: ['ability recharges 20% faster', 'ability recharges 33% faster'] },
  trickster: { name: 'Trickster', bonus: ['a 3 s ward when you leave your land', 'a 5 s ward when you leave your land'] },
  builder: { name: 'Builder', bonus: ['land scores +8%', 'land scores +15%'] },
};
/** Card id -> family. @type {Record<string, string>} */
const FAMILY_OF = Object.fromEntries(Object.entries(FAMILIES).flatMap(([f, ids]) => ids.map((id) => [id, f])));
/** @param {string} id */
export const familyOf = (id) => FAMILY_OF[id] ?? '';

/** Trickster: the ward's length by tier (ticks), and its recharge. It is Shield's effect (p.fx.shield). */
const WARD_TICKS = [0, 21, 35];
const WARD_RECHARGE = 140;
/** Runner: every this many ticks the ability's recharge skips a tick, by tier. */
const RUNNER_EVERY = [0, 4, 2];
/** Builder: score bonus in percent, by tier. */
const BUILDER_PCT = [0, 8, 15];

/** Rarity weights. Fair: every card alike. Normal drafts lean to commons; the catch-up lean helps the trailing. */
const W_FAIR = { common: 1, rare: 1, epic: 1 };
const W_LEAD = { common: 6, rare: 2.5, epic: 0.8 };
const W_MID = { common: 5, rare: 3, epic: 1.4 };
const W_TRAIL = { common: 3.5, rare: 3.5, epic: 2.2 };

/** Md keys that survive from one round of the run to the next (rules.js startRound). */
const CARRY = ['runN', 'leg', 'keys', 'names', 'pts', 'keep', 'kleg'];

// ---- the run record (in s.md) ----------------------------------------------------------------------------

/** A stable key for whoever `p` is across lives: a person by user id (or name), a bot by its seat name. @param {Player} p */
export const runKey = (p) => (p.kind === 'bot' ? `b:${p.name}` : p.uid ? `u:${p.uid}` : `n:${p.name}`);

/**
 * The run record, made on first use. @param {GameState} s
 * @returns {{runN: number, leg: number, keys: string[], names: string[], pts: number[], keep: string[], kleg: number[]}}
 */
export function runOf(s) {
  const md = /** @type {any} */ (s.md);
  if (typeof md.leg !== 'number') {
    md.runN = (typeof md.runN === 'number' ? md.runN : 0) + 1;
    md.leg = 1;
    md.keys = [];
    md.names = [];
    md.pts = [];
    md.keep = [];
    md.kleg = [];
  }
  return md;
}

/** `p`'s row in the run record, made on first use. @param {GameState} s @param {Player} p */
function rowOf(s, p) {
  const r = runOf(s);
  const k = runKey(p);
  let i = r.keys.indexOf(k);
  if (i < 0) {
    i = r.keys.length;
    r.keys.push(k);
    r.names.push(p.name);
    r.pts.push(0);
    r.keep.push('');
    r.kleg.push(0);
  }
  r.names[i] = p.name;
  return i;
}

/** The keepers of row `i`. @param {GameState} s @param {number} i @returns {string[]} */
const keepersAt = (s, i) => (runOf(s).keep[i] ? runOf(s).keep[i].split(',') : []);

/** `p`'s keepers this run. @param {GameState} s @param {Player} p */
export const keepersOf = (s, p) => keepersAt(s, rowOf(s, p));

/** `p`'s place in the run standings (1 = most points), and how many are in it. @param {GameState} s @param {Player} p */
function runStanding(s, p) {
  const r = runOf(s);
  const mine = r.pts[rowOf(s, p)];
  let place = 1;
  for (const v of r.pts) if (v > mine) place++;
  return { place, of: r.pts.length };
}

/** The rarity weights for `p`'s next draft. @param {GameState} s @param {Player} p @param {boolean} keeper */
function weightsFor(s, p, keeper) {
  const r = runOf(s);
  // Round 1's keeper draft is fair: nobody has a standing yet.
  if (keeper && r.leg === 1) return W_FAIR;
  const { place, of } = runStanding(s, p);
  if (of < 2 || r.pts.every((v) => v === 0)) return W_MID;
  if (place === 1) return W_LEAD;
  return place > of / 2 ? W_TRAIL : W_MID;
}

// ---- families and set bonuses ----------------------------------------------------------------------------

/** Where a life's family counts live in p.fx (wiped on death and placement, sent to the player's HUD). */
const FX = { runner: 'famRunner', trickster: 'famTrick', builder: 'famBuild' };

/** How many cards of each family `p` drafted this life, keepers included. @param {Player} p @returns {Record<string, number>} */
export function familyCounts(p) {
  return { runner: p.fx[FX.runner] ?? 0, trickster: p.fx[FX.trickster] ?? 0, builder: p.fx[FX.builder] ?? 0 };
}

/** Count card `id` towards `p`'s family this life. @param {Player} p @param {string} id */
function countFamily(p, id) {
  const f = /** @type {keyof typeof FX | undefined} */ (FAMILY_OF[id]);
  if (f) p.fx[FX[f]] = (p.fx[FX[f]] ?? 0) + 1;
}

/** Cards of one family a life needs for its set bonus, and for the stronger version. */
export const SET_AT = [3, 5];
/** Set bonus tier from a family count: 0 none, 1 from SET_AT[0] cards, 2 from SET_AT[1]. @param {number} n */
export const tierOf = (n) => (n >= SET_AT[1] ? 2 : n >= SET_AT[0] ? 1 : 0);

/** The set bonuses `p` has on now. @param {Player} p @returns {Record<string, number>} family -> tier */
export function setBonuses(p) {
  const c = familyCounts(p);
  return { runner: tierOf(c.runner), trickster: tierOf(c.trickster), builder: tierOf(c.builder) };
}

// ---- drafts ----------------------------------------------------------------------------------------------

/**
 * Offer `p` three different cards, drawn by rarity weight. @param {GameState} s @param {Player} p
 * @param {Record<Rarity, number>} w @param {boolean} keep
 */
function offer(s, p, w, keep) {
  /** @type {string[]} */
  const picks = [];
  while (picks.length < 3) {
    const left = REGISTRY.filter((c) => !picks.includes(c.id) && w[c.rarity] > 0);
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
  /** @type {{cards: string[], until: number, keep?: boolean}} */
  const o = { cards: picks, until: s.tick + (p.kind === 'bot' ? BOT_OFFER_TICKS : OFFER_TICKS) };
  if (keep) o.keep = true;
  p.offers.push(o);
}

/**
 * Offer `p` its next draft: the keeper draft when it has not had this round's yet, else a normal one.
 * @param {GameState} s @param {Player} p
 */
function offerNext(s, p) {
  if (!s.cfg.cards) return;
  p.fx.drafted = (p.fx.drafted ?? 0) + 1;
  const r = runOf(s);
  const i = rowOf(s, p);
  const keeper = r.kleg[i] !== r.leg;
  if (keeper) r.kleg[i] = r.leg;
  offer(s, p, weightsFor(s, p, keeper), keeper);
}

/** How much a bot wants card `id` now: rarity, the family it is building, an upgrade. @param {Player} p @param {string} id */
export function botValue(p, id) {
  const def = CARDS[id];
  if (!def) return 0;
  const rarity = def.rarity === 'epic' ? 2.2 : def.rarity === 'rare' ? 1.6 : 1;
  const fam = familyCounts(p)[FAMILY_OF[id]] ?? 0;
  // A duplicate of something it holds at level I upgrades it; a third family card turns the bonus up.
  const have = (p.cards.active?.id === id && p.cards.active.level < def.maxLevel) || p.cards.passives.some((c) => c.id === id && c.level < def.maxLevel);
  // An ability or a passive lasts the whole life; an item is gone after one use.
  const lasting = def.kind === 'item' ? 0 : 0.5;
  return rarity + lasting + 0.9 * Math.min(fam, 2) + (have ? 0.8 : 0);
}

/** A bot picks from its draft (best value, the seeded RNG breaks near-ties). @param {GameState} s @param {Player} p */
function botPick(s, p) {
  const o = p.offers[0];
  let best = 0;
  let bestV = -Infinity;
  o.cards.forEach((id, i) => {
    const v = botValue(p, id) + rand(s) * 0.3;
    if (v > bestV) {
      bestV = v;
      best = i;
    }
  });
  // choose() calls onPick below, which writes a keeper into the run.
  choose(s, p, best);
}

/** @param {GameState} s @param {Player} p @param {string} id */
function addKeeper(s, p, id) {
  const r = runOf(s);
  const i = rowOf(s, p);
  r.keep[i] = [...keepersAt(s, i), id].join(',');
}

// ---- results ---------------------------------------------------------------------------------------------

/**
 * Pay run points for the round just over and, after the last round, turn the results into the run summary and
 * set up the next run. @param {GameState} s @param {import('../types.js').RoundResult} res
 */
function roundOver(s, res) {
  const r = runOf(s);
  /** @type {Map<number, number>} row index -> points gained */
  const gained = new Map();
  res.players.forEach((line, k) => {
    const p = s.players.find((q) => q.id === line.id);
    if (!p) return;
    const i = rowOf(s, p);
    const g = k < PLACE_POINTS.length ? PLACE_POINTS[k] : PLACE_REST;
    r.pts[i] += g;
    gained.set(i, g);
  });
  const final = r.leg >= RUN_ROUNDS;
  const order = r.keys.map((_, i) => i).sort((a, b) => r.pts[b] - r.pts[a] || r.keys[a].localeCompare(r.keys[b]));
  res.run = {
    n: r.runN,
    leg: r.leg,
    of: RUN_ROUNDS,
    final,
    lines: order.slice(0, 8).map((i) => ({
      name: r.names[i],
      bot: r.keys[i].startsWith('b:'),
      pts: r.pts[i],
      gain: gained.get(i) ?? 0,
      keep: keepersAt(s, i),
    })),
  };
  if (final) {
    const champ = order[0];
    if (champ !== undefined) {
      res.run.champion = r.names[champ];
      const key = r.keys[champ];
      const p = s.players.find((q) => runKey(q) === key);
      if (p) res.winner = p.id;
    }
    // The next round starts a new run.
    const md = /** @type {any} */ (s.md);
    delete md.leg;
  } else r.leg++;
}

/** @type {import('./index.js').ModeDef} */
export default {
  id: 'draft',
  name: 'Draft Night',
  blurb: 'A three-round run: draft at every life, keep one card each round, build a set.',
  roundTicks: 5 * 60 * 7,
  respawn: true,
  minHeads: 4,
  cards: 'start',
  carry: CARRY,
  setup(s) {
    runOf(s);
  },
  // p.fx is wiped on every placement and death, so `fx.drafted` counts the drafts offered this life.
  onSpawn(s, p) {
    const i = rowOf(s, p);
    for (const id of keepersAt(s, i)) {
      grant(s, p, id);
      countFamily(p, id);
    }
    offerNext(s, p);
  },
  tick(s) {
    for (const p of s.players) {
      if (!p.alive) continue;
      // Bots pick at once, by value; a person's pick came with the input (or the timer took the first).
      if (p.kind === 'bot' && p.offers.length) botPick(s, p);
      if (!p.offers.length && (p.fx.drafted ?? 0) < DRAFTS) offerNext(s, p);
      bonuses(s, p);
    }
  },
  onPick(s, p, id, o) {
    countFamily(p, id);
    if (o.keep) addKeeper(s, p, id);
  },
  score(s, p) {
    const base = p.land + 2 * p.gold;
    const t = tierOf(familyCounts(p).builder);
    return t ? base + Math.floor((base * BUILDER_PCT[t]) / 100) : base;
  },
  onRoundOver: roundOver,
};

/** The Runner and Trickster set bonuses, once per tick (Builder's is in score). @param {GameState} s @param {Player} p */
function bonuses(s, p) {
  const c = familyCounts(p);
  const run = tierOf(c.runner);
  const a = p.cards.active;
  if (run && a && a.ready > s.tick + 1 && s.tick % RUNNER_EVERY[run] === 0) a.ready--;
  // Trickster: the step off your land (the first trail tile) raises the ward, when it has recharged.
  const trick = tierOf(c.trickster);
  if (trick && p.trail.length === 1 && s.tick >= (p.fx.wardReady ?? 0)) {
    p.fx.shield = Math.max(p.fx.shield ?? 0, s.tick + WARD_TICKS[trick]);
    p.fx.wardReady = s.tick + WARD_RECHARGE;
  }
}
