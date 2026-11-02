// @ts-check
// Interpolation of remote players. Peers see other players a few ticks in the past, smoothly, between two
// snapshots they already have, instead of extrapolating from the newest one (which jitters with every
// correction). The delay (in ticks) is the price: 6 ticks = 100 ms at 60 Hz with snapshots every 3 ticks.

/**
 * @typedef {object} Sample
 * @property {number} tick
 * @property {[number, number, number]} p
 * @property {number} yaw
 * @property {number} pitch
 * @property {boolean} crouched
 */

export class Interpolator {
  /** @param {{ delayTicks?: number, keep?: number }} [opt] */
  constructor(opt = {}) {
    this.delayTicks = opt.delayTicks ?? 6;
    this.keep = opt.keep ?? 32;
    /** @type {Map<number, Sample[]>} slot -> samples by tick */
    this.tracks = new Map();
  }

  /** @param {import('../sim/world.js').Snapshot} snap */
  push(snap) {
    const present = new Set();
    for (const p of snap.players) {
      present.add(p.slot);
      let t = this.tracks.get(p.slot);
      if (!t) this.tracks.set(p.slot, (t = []));
      if (t.length && t[t.length - 1].tick >= snap.tick) continue;
      t.push({ tick: snap.tick, p: [p.p[0], p.p[1], p.p[2]], yaw: p.yaw, pitch: p.pitch, crouched: p.crouched });
      if (t.length > this.keep) t.shift();
    }
    for (const slot of [...this.tracks.keys()]) if (!present.has(slot)) this.tracks.delete(slot);
  }

  /**
   * Where a slot is at `tick` minus the delay. Holds the oldest/newest sample outside the buffered range.
   * @param {number} slot @param {number} tick a fractional host tick
   * @returns {Sample | null}
   */
  sample(slot, tick) {
    const t = this.tracks.get(slot);
    if (!t || !t.length) return null;
    const at = tick - this.delayTicks;
    if (at <= t[0].tick) return t[0];
    const last = t[t.length - 1];
    if (at >= last.tick) return last;
    let i = t.length - 1;
    while (i > 0 && t[i - 1].tick > at) i--;
    const a = t[i - 1];
    const b = t[i];
    const k = (at - a.tick) / (b.tick - a.tick);
    // Yaw is an int16 angle: interpolate along the short way round.
    let dy = b.yaw - a.yaw;
    if (dy > 32768) dy -= 65536;
    if (dy < -32768) dy += 65536;
    return {
      tick: at,
      p: [a.p[0] + (b.p[0] - a.p[0]) * k, a.p[1] + (b.p[1] - a.p[1]) * k, a.p[2] + (b.p[2] - a.p[2]) * k],
      yaw: a.yaw + dy * k,
      pitch: a.pitch + (b.pitch - a.pitch) * k,
      crouched: k < 0.5 ? a.crouched : b.crouched,
    };
  }

  /** Slots with at least one sample. */
  slots() {
    return [...this.tracks.keys()].sort((a, b) => a - b);
  }
}
