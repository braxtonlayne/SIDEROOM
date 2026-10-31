// @ts-check
// Wire format shared by the server and the client. JSON throughout (the proxy relays bodies as they are);
// arrays of numbers keep it small.
//
// A tick record (one per server tick):
//   { t: tick, tiles: [i, base, trail, ...], heads: [id, x, y, dir, flags, ...], ev: GameEvent[] }
//   flags: 1 = stopped, 2 = trailing, then card effects others see: 4 shield, 8 phase, 16 dash, 32 stunned,
//   64 second wind ready, 128 EMP'd, 256 spawn-protected
//   objs: [id, card index (REGISTRY), x, y, dir, until, owner, ...] the card objects in the world
//   ter: [i, terrain, ...] tiles whose terrain changed this tick (only when some did)
// A full snapshot:
//   { t, w, h, map, base: rle, trail: rle, terrain: rle, heads }
// rle: [value, count, value, count, ...] over the tile array.

import { REGISTRY, effectFlags } from './cards.js';
import { modeOf, scoreOf } from './modes/index.js';

/** @typedef {import('./types.js').GameState} GameState */
/** @typedef {import('./types.js').Player} Player */

export const FLAG_STOPPED = 1;
export const FLAG_TRAILING = 2;

/** Run-length encode a tile layer. @param {ArrayLike<number>} a @returns {number[]} */
export function rleEncode(a) {
  /** @type {number[]} */
  const out = [];
  if (!a.length) return out;
  let v = a[0];
  let n = 1;
  for (let i = 1; i < a.length; i++) {
    if (a[i] === v) n++;
    else {
      out.push(v, n);
      v = a[i];
      n = 1;
    }
  }
  out.push(v, n);
  return out;
}

/** Decode into `into` (which must have the right length). @template {Uint16Array | Uint8Array} A @param {number[]} rle @param {A} into @returns {A} */
export function rleDecode(rle, into) {
  let k = 0;
  for (let j = 0; j < rle.length; j += 2) {
    const v = rle[j];
    const n = rle[j + 1];
    into.fill(v, k, k + n);
    k += n;
  }
  if (k !== into.length) throw new Error(`rle covers ${k} of ${into.length} tiles`);
  return into;
}

/** The heads of the live players, compact. @param {GameState} s @returns {number[]} */
export function encodeHeads(s) {
  /** @type {number[]} */
  const out = [];
  for (const p of s.players) {
    if (!p.alive) continue;
    out.push(
      p.id,
      p.x,
      p.y,
      p.dir,
      (p.stopped ? FLAG_STOPPED : 0) | (p.trail.length ? FLAG_TRAILING : 0) | effectFlags(p, s.tick),
    );
  }
  return out;
}

const KIND = new Map(REGISTRY.map((c, k) => [c.id, k]));

/** The card objects in the world, compact. @param {GameState} s @returns {number[]} */
export function encodeObjects(s) {
  /** @type {number[]} */
  const out = [];
  for (const o of s.objects) out.push(o.id, KIND.get(o.kind) ?? -1, o.x, o.y, o.dir, o.until, o.owner);
  return out;
}

/** A player's own cards, drafts and timers (only they get this). @param {Player} p */
export function encodeHand(p) {
  return {
    items: p.cards.items.map((c) => [c.id, c.level]),
    active: p.cards.active ? [p.cards.active.id, p.cards.active.level, p.cards.active.ready] : null,
    passives: p.cards.passives.map((c) => [c.id, c.level]),
    offers: p.offers.map((o) => ({ cards: o.cards, until: o.until })),
    fx: { ...p.fx },
  };
}

/** @param {GameState} s @param {number[]} changed tile indices @returns {number[]} */
export function encodeTerrain(s, changed) {
  /** @type {number[]} */
  const out = [];
  for (const i of changed) out.push(i, s.terrain[i]);
  return out;
}

/** @param {GameState} s @param {number[]} changed tile indices @returns {number[]} */
export function encodeTiles(s, changed) {
  /** @type {number[]} */
  const out = [];
  for (const i of changed) out.push(i, s.base[i], s.trail[i]);
  return out;
}

/** @param {GameState} s */
export function encodeFull(s) {
  return {
    t: s.tick,
    w: s.cfg.w,
    h: s.cfg.h,
    map: s.cfg.map,
    base: rleEncode(s.base),
    trail: rleEncode(s.trail),
    terrain: rleEncode(s.terrain),
    heads: encodeHeads(s),
    objs: encodeObjects(s),
  };
}

/** Who is in the arena: names change rarely, so they travel only when this changes. @param {GameState} s */
export function encodeRoster(s) {
  return s.players.map((p) => ({ id: p.id, name: p.name, bot: p.kind === 'bot', look: p.look, team: p.team }));
}

/** Per-player numbers that change often: [id, land, kills, alive, score, ...]. @param {GameState} s */
export function encodeStats(s) {
  /** @type {number[]} */
  const out = [];
  for (const p of s.players) out.push(p.id, p.land, p.kills, p.alive ? 1 : 0, scoreOf(s, p));
  return out;
}

/**
 * The round, the mode and the map (D-51): small, sent with every poll.
 * @param {GameState} s
 */
export function encodeRound(s) {
  const r = s.round;
  return {
    n: r.n,
    phase: r.phase,
    start: r.start,
    end: r.end,
    next: r.next,
    mode: modeOf(s).id,
    map: s.cfg.map,
    md: s.md,
    ...(r.phase === 'over' && r.result ? { result: r.result } : {}),
  };
}
