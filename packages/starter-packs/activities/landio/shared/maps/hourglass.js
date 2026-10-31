// @ts-check
// Hourglass: two big halves joined by one short waist, mirror symmetric across the centre line.
// Generation: pick the long axis (wide map: x; tall map: y; square: by rand). Two solid wedges come in from
// the two side edges and are thickest on the centre line, leaving a corridor of ~20% of the cross size (at
// least 7 tiles); each wedge thins linearly and slopes away over 28% of the long axis on each side, giving the
// stepped hourglass silhouette. Then 2-4 small pillars (3x3 or 4x4) are dropped in each half by rejection
// (4+ tiles from any wall), each mirrored into the other half. No gold.
import { OPEN, WALL } from '../grid.js';

/** @type {import('./index.js').MapDef} */
export default {
  id: 'hourglass',
  name: 'Hourglass',
  blurb: 'Two big halves, one narrow waist.',
  build(w, h, rand) {
    const alongX = w === h ? rand() < 0.5 : w > h;
    const L = alongX ? w : h; // long axis
    const C = alongX ? h : w; // cross axis
    const t = new Uint8Array(w * h);
    /** @param {number} a @param {number} c @param {number} v */
    const set = (a, c, v) => {
      if (a < 0 || c < 0 || a >= L || c >= C) return;
      t[alongX ? c * w + a : a * w + c] = v;
    };
    /** @param {number} a @param {number} c */
    const get = (a, c) => (alongX ? t[c * w + a] : t[a * w + c]);

    const cw = Math.max(7, Math.round(0.2 * C));
    const total = C - cw;
    const T0 = Math.floor(total / 2); // wedge thickness on the low side
    const T1 = total - T0;
    const S = Math.max(4, 0.28 * L);
    const mid = (L - 1) / 2;
    for (let a = 0; a < L; a++) {
      const f = 1 - Math.max(0, Math.abs(a - mid) - 0.5) / S; // full thickness on the centre line
      if (f <= 0) continue;
      let n0 = Math.round(T0 * f);
      let n1 = Math.round(T1 * f);
      if (n0 < 3) n0 = 0;
      if (n1 < 3) n1 = 0;
      for (let c = 0; c < n0; c++) set(a, c, WALL);
      for (let c = 0; c < n1; c++) set(a, C - 1 - c, WALL);
    }

    // Pillars, in mirrored pairs (the first half is a < mid).
    const want = 2 + Math.floor(rand() * 3);
    const clear = 4;
    /** Is the box grown by `clear` free of walls and inside the map? @param {number} a0 @param {number} c0 @param {number} a1 @param {number} c1 */
    const free = (a0, c0, a1, c1) => {
      if (a0 - clear < 0 || c0 - clear < 0 || a1 + clear >= L || c1 + clear >= C) return false;
      for (let a = a0 - clear; a <= a1 + clear; a++)
        for (let c = c0 - clear; c <= c1 + clear; c++) if (get(a, c) === WALL) return false;
      return true;
    };
    let placed = 0;
    for (let tries = 0; tries < 80 && placed < want; tries++) {
      const s = rand() < 0.5 ? 3 : 4;
      const a0 = Math.floor(rand() * (mid - s));
      const c0 = Math.floor(rand() * (C - s + 1));
      if (mid - (a0 + s - 1) < 0.1 * L) continue; // keep off the waist itself
      const a1 = a0 + s - 1;
      const c1 = c0 + s - 1;
      const m0 = L - 1 - a1;
      const m1 = L - 1 - a0;
      if (!free(a0, c0, a1, c1) || !free(m0, c0, m1, c1)) continue;
      for (let a = a0; a <= a1; a++)
        for (let c = c0; c <= c1; c++) {
          set(a, c, WALL);
          set(L - 1 - a, c, WALL);
        }
      placed++;
    }
    return t;
  },
};
