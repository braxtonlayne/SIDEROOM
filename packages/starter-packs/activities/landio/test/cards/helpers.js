// @ts-check
// Test helpers for card tests: a small bot-free arena, humans at fixed spots, scripted inputs, and giving
// cards directly.
import { createState, spawnPlayerAt, step } from '../../shared/rules.js';
import { grant, ACT } from '../../shared/cards.js';

/** @typedef {import('../../shared/types.js').GameState} GameState */
/** @typedef {import('../../shared/types.js').Player} Player */

export const U = 0;
export const R = 1;
export const D = 2;
export const L = 3;
export { ACT };

/** A bot-free arena with cards on. @param {number} [w] @param {number} [h] */
export const arena = (w = 30, h = 30) => createState({ w, h, targetHeads: 0, cards: true }, 'cards-test');

/** @param {GameState} s @param {string} name @param {number} x @param {number} y */
export const human = (s, name, x, y) => spawnPlayerAt(s, { kind: 'human', name }, x, y);

/** Give `p` card `id` (as a draft pick would). @param {GameState} s @param {Player} p @param {string} id */
export const give = (s, p, id) => grant(s, p, id);

/**
 * Run scripts in parallel, one entry per tick per player: a direction (0-3), 'stop', an ACT code (< -1,
 * e.g. ACT.item0), an array of those for several actions in one tick, or null.
 * @param {GameState} s @param {[Player, (number|'stop'|null|(number|'stop')[])[]][]} scripts
 */
export function run(s, scripts) {
  const ticks = Math.max(...scripts.map(([, sc]) => sc.length));
  for (let t = 0; t < ticks; t++) {
    /** @type {import('../../shared/types.js').Input[]} */
    const inputs = [];
    for (const [p, sc] of scripts) {
      const a = sc[t];
      if (a === null || a === undefined) continue;
      const list = Array.isArray(a) ? a : [a];
      /** @type {number[]} */
      const turns = [];
      /** @type {number[]} */
      const cards = [];
      let toggleStop = false;
      for (const x of list) {
        if (x === 'stop') toggleStop = true;
        else if (x >= 0) turns.push(x);
        else cards.push(x);
      }
      inputs.push({ id: p.id, turns, cards, toggleStop });
    }
    step(s, inputs);
  }
}

/** n copies of v. @template T @param {T} v @param {number} n @returns {T[]} */
export const rep = (v, n) => Array(n).fill(v);

/** @param {GameState} s @param {number} x @param {number} y */
export const baseAt = (s, x, y) => s.base[y * s.cfg.w + x];
/** @param {GameState} s @param {number} x @param {number} y */
export const trailAt = (s, x, y) => s.trail[y * s.cfg.w + x];

/** Base and trail tiles of player id on the grid. @param {GameState} s @param {number} id */
export function tilesOf(s, id) {
  let b = 0;
  let t = 0;
  for (let i = 0; i < s.base.length; i++) {
    if (s.base[i] === id) b++;
    if (s.trail[i] === id) t++;
  }
  return { b, t };
}

/**
 * The simulation's invariants (as in test/property.test.js): null when they hold, else what broke.
 * @param {GameState} s
 */
export function broken(s) {
  const { w } = s.cfg;
  for (let i = 0; i < s.base.length; i++) if (s.base[i] && s.base[i] === s.trail[i]) return `tile ${i}: base and trail of ${s.base[i]}`;
  for (const p of s.players) {
    const t = tilesOf(s, p.id);
    if (t.b !== p.land) return `${p.name}: land ${p.land} vs grid ${t.b}`;
    if (t.t !== p.trail.length) return `${p.name}: trail list ${p.trail.length} vs grid ${t.t}`;
    if (!p.alive) continue;
    if (p.trail.length) {
      if (p.trail[p.trail.length - 1] !== p.y * w + p.x) return `${p.name}: trail does not end at the head`;
      for (let k = 1; k < p.trail.length; k++) {
        const a = p.trail[k - 1];
        const b = p.trail[k];
        if (Math.abs((a % w) - (b % w)) + Math.abs(Math.floor(a / w) - Math.floor(b / w)) !== 1) return `${p.name}: trail jumps`;
      }
    } else if (s.base[p.y * w + p.x] !== p.id) return `${p.name}: off its base with no trail`;
  }
  return null;
}
