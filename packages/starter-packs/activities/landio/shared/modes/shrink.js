// @ts-check
// Shrinking Arena: the play area is a rectangle that closes in on the map's centre in steps. Nothing closes
// for the first 30 s, then every 20 s the border moves in one step (announced 5 s ahead) until about 30% of the
// map's width and height is left. Tiles outside the border become walls; a head caught outside dies ('wall').
// Mode data (s.md, rectangles are [x0, y0, x1, y1] with x1, y1 exclusive): zone = the open area now, k = steps
// taken, at = tick of the next step (absent once the last step is done), next = the rectangle after that step
// (only during the 5 s warning).

import { WALL, setTerrain } from '../grid.js';
import { cutTrail, kill } from '../cards/util.js';

/** Ticks before the first step, between steps, and the warning lead (7 ticks per second). */
export const FIRST_STEP = 30 * 7;
export const STEP_EVERY = 20 * 7;
export const WARN_TICKS = 5 * 7;
/** The share of the map's width and height left at the end, and the number of steps aimed for. */
export const FINAL_SHARE = 0.3;
const STEPS = 10;

/**
 * The geometry of the closing for a map: per-axis step size and the largest inset (each side).
 * @param {number} w @param {number} h
 */
export function geometry(w, h) {
  const stepFor = (/** @type {number} */ d) => Math.max(2, Math.ceil((0.35 * d) / STEPS));
  const maxFor = (/** @type {number} */ d) => Math.floor((d - Math.round(FINAL_SHARE * d)) / 2);
  const sx = stepFor(w);
  const sy = stepFor(h);
  const mx = maxFor(w);
  const my = maxFor(h);
  return { sx, sy, mx, my, steps: Math.max(Math.ceil(mx / sx), Math.ceil(my / sy)) };
}

/** The open rectangle after `k` steps. @param {number} w @param {number} h @param {number} k @returns {number[]} */
export function zoneAt(w, h, k) {
  const g = geometry(w, h);
  const ix = Math.min(k * g.sx, g.mx);
  const iy = Math.min(k * g.sy, g.my);
  return [ix, iy, w - ix, h - iy];
}

/** @param {import('../types.js').GameState} s */
function startData(s) {
  const { w, h } = s.cfg;
  s.md = { zone: [0, 0, w, h], k: 0, at: s.round.start + FIRST_STEP };
}

/**
 * Close the border to the zone after step `k`; the next step comes `every` ticks later. Last Land Standing uses this
 * too (its sudden death). @param {import('../types.js').GameState} s @param {number} k @param {number} [every]
 */
export function closeTo(s, k, every = STEP_EVERY) {
  const { w, h } = s.cfg;
  const old = /** @type {number[]} */ (s.md.zone);
  const z = zoneAt(w, h, k);
  const outside = (/** @type {number} */ x, /** @type {number} */ y) => x < z[0] || x >= z[2] || y < z[1] || y >= z[3];
  // Trails cut by the closing are cleared (the owner lives unless the head is outside).
  /** @type {import('../types.js').Player[]} */
  const doomed = [];
  for (const p of s.players) {
    if (!p.alive) continue;
    if (outside(p.x, p.y)) {
      doomed.push(p);
      continue;
    }
    /** @type {Set<number>} */
    const cut = new Set();
    for (const i of p.trail) if (outside(i % w, (i / w) | 0)) cut.add(i);
    if (cut.size) cutTrail(s, p, cut);
  }
  // Only the ring between the old and new rectangle changes.
  for (let y = old[1]; y < old[3]; y++) {
    const rowOut = y < z[1] || y >= z[3];
    for (let x = old[0]; x < old[2]; x++) {
      if (!rowOut && x >= z[0] && x < z[2]) {
        x = z[2] - 1; // skip the inside of the row
        continue;
      }
      setTerrain(s, y * w + x, WALL);
    }
  }
  s.md.zone = z;
  s.md.k = k;
  delete s.md.next;
  if (k < geometry(w, h).steps) s.md.at = Number(s.md.at) + every;
  else delete s.md.at;
  for (const p of doomed) if (p.alive) kill(s, p, 'wall', null);
}

/**
 * Run the closing for this tick: announce the next step WARN_TICKS ahead, take it when due (steps `every` ticks
 * apart). Needs md.zone, md.k and md.at (absent: done). @param {import('../types.js').GameState} s @param {number} every
 */
export function closing(s, every) {
  const at = /** @type {number | undefined} */ (s.md.at);
  if (at === undefined) return;
  const k = /** @type {number} */ (s.md.k);
  if (s.tick >= at) closeTo(s, k + 1, every);
  else if (s.tick >= at - WARN_TICKS && !s.md.next) s.md.next = zoneAt(s.cfg.w, s.cfg.h, k + 1);
}

/** @type {import('./index.js').ModeDef} */
export default {
  id: 'shrink',
  name: 'Shrinking Arena',
  blurb: 'The border closes in. Land outside it is gone.',
  roundTicks: 4 * 60 * 7,
  respawn: true,
  setup: startData,
  tick(s) {
    closing(s, STEP_EVERY);
  },
  botGoal(s, p) {
    if (s.md.at === undefined) return null;
    const n = /** @type {number[] | undefined} */ (s.md.next) ?? zoneAt(s.cfg.w, s.cfg.h, /** @type {number} */ (s.md.k) + 1);
    const m = 8;
    if (p.x >= n[0] + m && p.x < n[2] - m && p.y >= n[1] + m && p.y < n[3] - m) return null;
    return { x: Math.floor((n[0] + n[2]) / 2), y: Math.floor((n[1] + n[3]) / 2) };
  },
};
