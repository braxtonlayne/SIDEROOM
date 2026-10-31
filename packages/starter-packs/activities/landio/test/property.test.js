// @ts-check
// Property test: 200 bot-only games of 2,000 ticks; the invariants hold after every tick.
//   - no tile has a base and a trail of the same owner;
//   - every live head is on the grid;
//   - a trail is a contiguous path of its owner's trail tiles ending at the owner's head, and the grid has no
//     other trail tiles of that owner;
//   - base counts match the grid and land % sums to at most 100;
//   - players that are gone (dead) own nothing.
// The grid is 60×60 with 12 bots, so 200 × 2,000 full-grid scans stay within a few seconds.
import assert from 'node:assert/strict';
import { createState, landPct, step, takeEvents } from '../shared/rules.js';

/** @typedef {import('../shared/types.js').GameState} GameState */

const GAMES = Number(process.env.LANDIO_GAMES ?? 200);
const TICKS = 2000;

/** @param {GameState} s @returns {string|null} the first broken invariant */
function check(s) {
  const { w, h } = s.cfg;
  const n = w * h;
  /** @type {Map<number, number>} */
  const baseCount = new Map();
  /** @type {Map<number, number>} */
  const trailCount = new Map();
  for (let i = 0; i < n; i++) {
    const b = s.base[i];
    const t = s.trail[i];
    if (b && b === t) return `tile ${i} has base and trail of ${b}`;
    if (b) baseCount.set(b, (baseCount.get(b) ?? 0) + 1);
    if (t) trailCount.set(t, (trailCount.get(t) ?? 0) + 1);
  }
  let pct = 0;
  const ids = new Set();
  for (const p of s.players) {
    ids.add(p.id);
    pct += landPct(s, p);
    if ((baseCount.get(p.id) ?? 0) !== p.land) return `${p.name} land ${p.land} vs grid ${baseCount.get(p.id)}`;
    if ((trailCount.get(p.id) ?? 0) !== p.trail.length) return `${p.name} has trail tiles off its path`;
    if (!p.alive) {
      if (p.land || p.trail.length) return `dead ${p.name} still owns tiles`;
      continue;
    }
    if (p.x < 0 || p.y < 0 || p.x >= w || p.y >= h) return `${p.name} head off the grid`;
    if (p.trail.length) {
      if (p.trail[p.trail.length - 1] !== p.y * w + p.x) return `${p.name} trail does not end at its head`;
      for (let k = 0; k < p.trail.length; k++) {
        const i = p.trail[k];
        if (s.trail[i] !== p.id) return `${p.name} trail tile ${i} is not marked`;
        if (k > 0) {
          const j = p.trail[k - 1];
          const d = Math.abs((i % w) - (j % w)) + Math.abs(Math.floor(i / w) - Math.floor(j / w));
          if (d !== 1) return `${p.name} trail jumps between ${j} and ${i}`;
        }
      }
    } else if (s.base[p.y * w + p.x] !== p.id) return `${p.name} is off its base with no trail`;
  }
  for (const id of baseCount.keys()) if (!ids.has(id)) return `orphan base of ${id}`;
  for (const id of trailCount.keys()) if (!ids.has(id)) return `orphan trail of ${id}`;
  if (pct > 100 + 1e-9) return `land sums to ${pct}%`;
  return null;
}

test(`${GAMES} random bot-only games of ${TICKS} ticks keep every invariant`, () => {
  let kills = 0;
  let captures = 0;
  for (let g = 0; g < GAMES; g++) {
    const s = createState({ w: 60, h: 60, targetHeads: 12 }, `prop-${g}`, { fill: true });
    for (let t = 0; t < TICKS; t++) {
      step(s);
      for (const e of takeEvents(s)) {
        if (e.k === 'kill') kills++;
        if (e.k === 'capture') captures++;
      }
      const bad = check(s);
      if (bad) assert.fail(`game ${g} tick ${s.tick}: ${bad}`);
    }
  }
  // The games were real games, not bots idling.
  assert.ok(kills > GAMES, `kills ${kills}`);
  assert.ok(captures > GAMES * 50, `captures ${captures}`);
});
