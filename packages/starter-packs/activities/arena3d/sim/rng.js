// @ts-check
// The simulation's only source of randomness: a seeded 32-bit PRNG (mulberry32). Math.random() is never used
// in sim/, because every client must draw the same numbers in the same order. The whole generator state is one
// uint32, so it travels in snapshots and goes into the state hash.

/**
 * A string seed as a uint32 (FNV-1a over its UTF-16 code units), so a room code or instance id can seed a match.
 * @param {string} text
 * @returns {number}
 */
export function seedFromString(text) {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h >>> 0;
}

export class Rng {
  /** @param {number} seed any uint32 */
  constructor(seed) {
    /** The whole generator state. @type {number} */
    this.state = seed >>> 0;
  }

  /** The next uint32. Integer-only arithmetic, so it is identical in every engine. */
  nextU32() {
    this.state = (this.state + 0x6d2b79f5) >>> 0;
    let t = this.state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return (t ^ (t >>> 14)) >>> 0;
  }

  /** An integer in [0, n). @param {number} n a positive integer below 2^32 */
  int(n) {
    return this.nextU32() % n;
  }

  /**
   * A number in [lo, hi) on a 1/1024 grid. Dividing an integer by a power of two is exact, so the result is the
   * same double everywhere.
   * @param {number} lo @param {number} hi
   */
  range(lo, hi) {
    return lo + ((hi - lo) * (this.nextU32() >>> 22)) / 1024;
  }
}
