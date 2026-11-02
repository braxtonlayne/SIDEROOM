// @ts-check
// ACT-LND-002 (wave 46): the "tick stays cheap" budget, measured so that machine load cannot fail it and a real
// regression still does. The old tests timed the base mode once and the mode under test once and compared: a single
// busy moment (another test file in the next fork, a build, a virus scan) during the second run added more than the
// whole 0.3 ms budget. Here the two modes run in alternating short batches (base, mode, base, mode, ...), so slow
// drift hits both alike, each batch on a fresh state from the same seed (the same work every time), and the verdict is
// the MEDIAN of the per-pair differences, so one or two disturbed batches out of seven cannot move it. The budget
// itself is unchanged: a mode that really costs 0.3 ms more per tick is over it in most pairs and fails.
import { performance } from 'node:perf_hooks';
import { createState, step } from '../../shared/rules.js';

/** @param {number[]} xs */
export const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};

/**
 * Extra milliseconds per tick of `mode` over `base` at 200x200 with 30 bots.
 * @param {string} mode @param {string} base
 * @param {{ pairs?: number, ticks?: number, warm?: number, seed?: string, measure?: (mode: string) => number }} [opt]
 *   measure: ms per tick of one batch (tests of this helper pass a fake)
 * @returns {{ extra: number, base: number, mode: number, diffs: number[] }}
 */
export function modeOverhead(mode, base, opt = {}) {
  const pairs = opt.pairs ?? 7;
  const ticks = opt.ticks ?? 400;
  const warm = opt.warm ?? 100;
  const seed = opt.seed ?? 'timing';
  /** @param {string} m */
  const real = (m) => {
    const s = createState({ w: 200, h: 200, targetHeads: 30, mode: m }, seed, { fill: true });
    step(s, [], warm);
    const t0 = performance.now();
    step(s, [], ticks);
    return (performance.now() - t0) / ticks;
  };
  const once = opt.measure ?? real;
  // Warm the JIT on both before anything counts.
  once(base);
  once(mode);
  /** @type {number[]} */
  const b = [];
  /** @type {number[]} */
  const m = [];
  for (let i = 0; i < pairs; i++) {
    // Alternate which goes first, so neither always gets the warmer cache.
    if (i % 2) {
      m.push(once(mode));
      b.push(once(base));
    } else {
      b.push(once(base));
      m.push(once(mode));
    }
  }
  const diffs = b.map((x, i) => m[i] - x);
  return { extra: median(diffs), base: median(b), mode: median(m), diffs };
}
