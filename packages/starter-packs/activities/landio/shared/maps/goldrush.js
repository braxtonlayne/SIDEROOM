// @ts-check
// Gold Rush: an open field with gold fields worth three times as much. One big central gold disc (organic edge from a
// smooth radial wobble) is ringed by satellite fields: 4 below 100 tiles on the short side, 6 up to 119, 8 beyond.
// Everything is placed in mirror pairs through the centre (tile (x, y) matches (w-1-x, h-1-y)), so no side is richer.
// Satellites sit 25-40% of the way from the centre to the edge and never touch each other or the central field. A few
// short low walls (3 thick, 6-10 long) between the fields give cover; they keep 4+ tiles from gold and the edge.
import { OPEN, WALL, GOLD } from '../grid.js';

/**
 * Fill a wobbly disc of gold around (cx, cy), and its point mirror.
 * @param {Uint8Array} t @param {number} w @param {number} h @param {number} cx @param {number} cy @param {number} r
 * @param {number[]} wob
 */
function paintDisc(t, w, h, cx, cy, r, wob) {
  const R = Math.ceil(r * 1.4) + 1;
  for (let y = Math.floor(cy - R); y <= Math.ceil(cy + R); y++)
    for (let x = Math.floor(cx - R); x <= Math.ceil(cx + R); x++) {
      if (x < 0 || y < 0 || x >= w || y >= h) continue;
      const dx = x - cx;
      const dy = y - cy;
      const a = Math.atan2(dy, dx);
      const rr = r * (1 + wob[0] * Math.cos(2 * a + wob[1]) + wob[2] * Math.cos(3 * a + wob[3]));
      if (dx * dx + dy * dy <= rr * rr) {
        t[y * w + x] = GOLD;
        t[(h - 1 - y) * w + (w - 1 - x)] = GOLD;
      }
    }
}

/** @type {import('./index.js').MapDef} */
export default {
  id: 'goldrush',
  name: 'Gold Rush',
  blurb: 'Open ground with gold fields worth three times as much.',
  build(w, h, rand) {
    const t = new Uint8Array(w * h);
    const m = Math.min(w, h);
    const cx = (w - 1) / 2;
    const cy = (h - 1) / 2;
    const wobble = () => [0.05 + rand() * 0.04, rand() * 6.28, 0.03 + rand() * 0.03, rand() * 6.28];
    const cR = m * 0.1;
    /** Fields with their largest possible wobbled radius. */
    const fields = [{ x: cx, y: cy, r: cR * 1.15 }];
    paintDisc(t, w, h, cx, cy, cR, wobble());
    const pairs = m < 100 ? 2 : m < 120 ? 3 : 4;
    const sR = m * 0.055;
    const base = rand() * Math.PI;
    for (let k = 0; k < pairs; k++) {
      for (let tries = 0; tries < 200; tries++) {
        const ang = base + ((k + (tries < 100 ? rand() * 0.4 - 0.2 : rand() - 0.5)) * Math.PI) / pairs;
        const f = tries < 100 ? 0.34 + rand() * 0.08 : 0.3 + rand() * 0.18;
        const x = cx + Math.cos(ang) * f * (m / 2 + (w - m) * 0.3);
        const y = cy + Math.sin(ang) * f * (m / 2 + (h - m) * 0.3);
        const r = sR * (0.9 + rand() * 0.1);
        const er = r * 1.15;
        const mx = w - 1 - x;
        const my = h - 1 - y;
        const apart = fields.every((o) => Math.hypot(o.x - x, o.y - y) > o.r + er + 2 && Math.hypot(o.x - mx, o.y - my) > o.r + er + 2);
        const inside = x - er > 2 && y - er > 2 && x + er < w - 3 && y + er < h - 3;
        if (!apart || !inside || Math.hypot(x - mx, y - my) <= 2 * er + 2) continue;
        fields.push({ x, y, r: er }, { x: mx, y: my, r: er });
        paintDisc(t, w, h, x, y, r, wobble());
        break;
      }
    }
    // Low walls as cover, in mirror pairs, clear of gold, other walls and the edge.
    /** @param {number} x0 @param {number} y0 @param {number} bw @param {number} bh */
    const clear = (x0, y0, bw, bh) => {
      const pad = 4;
      if (x0 - pad < 0 || y0 - pad < 0 || x0 + bw + pad > w || y0 + bh + pad > h) return false;
      for (let y = y0 - pad; y < y0 + bh + pad; y++)
        for (let x = x0 - pad; x < x0 + bw + pad; x++) if (t[y * w + x] !== OPEN) return false;
      return true;
    };
    /** @param {number} x0 @param {number} y0 @param {number} bw @param {number} bh @param {number} v */
    const fill = (x0, y0, bw, bh, v) => {
      for (let y = y0; y < y0 + bh; y++) for (let x = x0; x < x0 + bw; x++) t[y * w + x] = v;
    };
    const bars = w * h >= 6000 && rand() < 0.5 ? 6 : 4;
    for (let placed = 0, tries = 0; placed < bars && tries < 800; tries++) {
      const long = 6 + Math.floor(rand() * 5);
      const horiz = rand() < 0.5;
      const bw = horiz ? long : 3;
      const bh = horiz ? 3 : long;
      const x0 = 4 + Math.floor(rand() * (w - 8 - bw + 1));
      const y0 = 4 + Math.floor(rand() * (h - 8 - bh + 1));
      const mx0 = w - x0 - bw;
      const my0 = h - y0 - bh;
      if (tries < 400 && Math.hypot(x0 + bw / 2 - 0.5 - cx, y0 + bh / 2 - 0.5 - cy) > 0.4 * m) continue;
      if (!clear(x0, y0, bw, bh)) continue;
      fill(x0, y0, bw, bh, WALL);
      if (!clear(mx0, my0, bw, bh)) {
        fill(x0, y0, bw, bh, OPEN);
        continue;
      }
      fill(mx0, my0, bw, bh, WALL);
      placed += 2;
    }
    return t;
  },
};
