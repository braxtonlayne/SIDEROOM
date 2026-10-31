// @ts-check
// Seeded RNG (mulberry32). The state lives in the game state, so a copy of the state replays identically.

/** @param {{rng:number}} s @returns {number} a float in [0, 1) */
export function rand(s) {
  let t = (s.rng = (s.rng + 0x6d2b79f5) >>> 0);
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}

/** An integer in [lo, hi] inclusive. @param {{rng:number}} s @param {number} lo @param {number} hi */
export function randInt(s, lo, hi) {
  return lo + Math.floor(rand(s) * (hi - lo + 1));
}

/** Hash a string seed to a 32-bit RNG state. @param {string|number} seed */
export function seedFrom(seed) {
  const str = String(seed);
  let h = 2166136261;
  for (let i = 0; i < str.length; i++) {
    h ^= str.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}
