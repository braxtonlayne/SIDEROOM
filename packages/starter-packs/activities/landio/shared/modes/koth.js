// @ts-check
// King of the Hill: a circle at the map's centre scores for whoever holds land inside it. Once a second every
// player gains a point per tile of their land in the hill (gold tiles count 3). Every 75 s the hill jumps to
// another spot, announced 5 s ahead. Points live in s.md (md.keys / md.pts, parallel lists) so they survive a
// death: a human is keyed by uid (a respawn continues the total), a bot by id.

import { GOLD, WALL } from '../grid.js';
import { randInt } from '../rng.js';

/** @typedef {import('../types.js').GameState} GameState */
/** @typedef {import('../types.js').Player} Player */

const TICKS_PER_POINT = 7;
const MOVE_TICKS = 75 * 7;
const WARN_TICKS = 5 * 7;
const OPEN_SHARE = 0.7;
const GOLD_POINTS = 3;

/** Hill radius: 12% of the shorter side, at least 5 tiles. @param {GameState} s */
const radiusOf = (s) => Math.max(5, Math.round(0.12 * Math.min(s.cfg.w, s.cfg.h)));

/** The key a player's points are filed under. @param {Player} p */
const keyOf = (p) => (p.uid ? `u:${p.uid}` : p.kind === 'bot' ? `b:${p.id}` : `h:${p.id}`);

/** @param {GameState} s @returns {string[]} */
function keys(s) {
  if (!Array.isArray(s.md.keys)) s.md.keys = [];
  if (!Array.isArray(s.md.pts)) s.md.pts = [];
  return /** @type {string[]} */ (s.md.keys);
}

/** The points of `p`. @param {GameState} s @param {Player} p */
function points(s, p) {
  const k = s.md.keys;
  if (!Array.isArray(k)) return 0;
  const i = /** @type {string[]} */ (k).indexOf(keyOf(p));
  return i < 0 ? 0 : /** @type {number[]} */ (s.md.pts)[i] ?? 0;
}

/** Whether a circle of radius r at (cx, cy) fits the map and is at least 70% non-wall. @param {GameState} s @param {number} cx @param {number} cy @param {number} r */
function fits(s, cx, cy, r) {
  const { w, h } = s.cfg;
  if (cx < r || cy < r || cx > w - 1 - r || cy > h - 1 - r) return false;
  let total = 0;
  let walls = 0;
  const r2 = r * r;
  for (let y = cy - r; y <= cy + r; y++)
    for (let x = cx - r; x <= cx + r; x++) {
      if ((x - cx) * (x - cx) + (y - cy) * (y - cy) > r2) continue;
      total++;
      if (s.terrain[y * w + x] === WALL) walls++;
    }
  return walls <= (1 - OPEN_SHARE) * total;
}

/**
 * The fitting spot nearest (tx, ty) that `accept` allows, searching outward in square rings.
 * @param {GameState} s @param {number} tx @param {number} ty @param {number} r @param {(x: number, y: number) => boolean} accept
 * @returns {[number, number, number] | null}
 */
function findNear(s, tx, ty, r, accept) {
  const reach = Math.max(s.cfg.w, s.cfg.h);
  for (let k = 0; k <= reach; k++) {
    let best = null;
    let bestD = Infinity;
    for (let dy = -k; dy <= k; dy++) {
      const edge = dy === -k || dy === k;
      for (let dx = -k; dx <= k; edge ? dx++ : (dx += 2 * k || 1)) {
        const cx = tx + dx;
        const cy = ty + dy;
        const d = dx * dx + dy * dy;
        if (d >= bestD || !fits(s, cx, cy, r) || !accept(cx, cy)) continue;
        best = [cx, cy, r];
        bestD = d;
      }
    }
    if (best) return /** @type {[number, number, number]} */ (best);
  }
  return null;
}

/** Choose the next hill, at least a quarter of the shorter side from the current one. @param {GameState} s @returns {[number, number, number]} */
function pickNext(s) {
  const cur = /** @type {number[]} */ (s.md.hill);
  const r = cur[2];
  const { w, h } = s.cfg;
  const far = 0.25 * Math.min(w, h);
  const farEnough = (/** @type {number} */ x, /** @type {number} */ y) => Math.hypot(x - cur[0], y - cur[1]) >= far;
  for (let t = 0; t < 60; t++) {
    const cx = randInt(s, r, w - 1 - r);
    const cy = randInt(s, r, h - 1 - r);
    if (farEnough(cx, cy) && fits(s, cx, cy, r)) return [cx, cy, r];
  }
  return findNear(s, randInt(s, r, w - 1 - r), randInt(s, r, h - 1 - r), r, farEnough) ?? [cur[0], cur[1], r];
}

/** Add one second's points. @param {GameState} s */
function addPoints(s) {
  const [cx, cy, r] = /** @type {number[]} */ (s.md.hill);
  const { w } = s.cfg;
  const r2 = r * r;
  /** @type {Map<number, number>} */
  const gain = new Map();
  for (let y = cy - r; y <= cy + r; y++)
    for (let x = cx - r; x <= cx + r; x++) {
      if ((x - cx) * (x - cx) + (y - cy) * (y - cy) > r2) continue;
      const i = y * w + x;
      const o = s.base[i];
      if (o) gain.set(o, (gain.get(o) ?? 0) + (s.terrain[i] === GOLD ? GOLD_POINTS : 1));
    }
  const ks = keys(s);
  const pts = /** @type {number[]} */ (s.md.pts);
  // Forget bots that are gone (their id may come back as a new bot); humans keep their total, dead or not.
  const present = new Set(s.players.map(keyOf));
  for (let i = ks.length - 1; i >= 0; i--)
    if (ks[i].startsWith('b:') && !present.has(ks[i])) {
      ks.splice(i, 1);
      pts.splice(i, 1);
    }
  for (const p of s.players) {
    const g = gain.get(p.id);
    if (!g) continue;
    const key = keyOf(p);
    let i = ks.indexOf(key);
    if (i < 0) {
      i = ks.length;
      ks.push(key);
      pts.push(0);
    }
    pts[i] += g;
  }
}

/** @param {GameState} s */
function placeHill(s) {
  const r = radiusOf(s);
  const { w, h } = s.cfg;
  s.md.hill = findNear(s, Math.floor(w / 2), Math.floor(h / 2), r, () => true) ?? [Math.floor(w / 2), Math.floor(h / 2), r];
}

/** @type {import('./index.js').ModeDef} */
export default {
  id: 'koth',
  name: 'King of the Hill',
  blurb: 'Score for every tile you hold inside the hill.',
  roundTicks: 5 * 60 * 7,
  respawn: true,
  scoreLabel: 'points',

  setup(s) {
    keys(s);
    placeHill(s);
    delete s.md.nextHill;
    delete s.md.moveAt;
  },

  tick(s) {
    if (!s.md.hill) placeHill(s);
    const rel = s.tick - s.round.start;
    if (rel <= 0) return;
    const phase = rel % MOVE_TICKS;
    if (phase === MOVE_TICKS - WARN_TICKS) {
      s.md.nextHill = pickNext(s);
      s.md.moveAt = s.tick + WARN_TICKS;
    } else if (phase === 0) {
      if (Array.isArray(s.md.nextHill)) s.md.hill = s.md.nextHill;
      delete s.md.nextHill;
      delete s.md.moveAt;
    }
    if (rel % TICKS_PER_POINT === 0) addPoints(s);
  },

  score: points,
  onSpawn(s, p) {
    // A new bot (the id may be reused) starts from nothing.
    if (p.kind !== 'bot' || !Array.isArray(s.md.keys)) return;
    const i = /** @type {string[]} */ (s.md.keys).indexOf(keyOf(p));
    if (i >= 0) {
      /** @type {string[]} */ (s.md.keys).splice(i, 1);
      /** @type {number[]} */ (s.md.pts).splice(i, 1);
    }
  },

  botGoal(s) {
    const hill = s.md.hill;
    return Array.isArray(hill) ? { x: Number(hill[0]), y: Number(hill[1]) } : null;
  },
};
