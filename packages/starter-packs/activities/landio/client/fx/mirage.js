// @ts-check
// Mirage visuals: a ghost of the owner's head at 70% opacity, drawn like a real head (dark rim, colour fill,
// eyes facing its direction), on its own tile with no interpolation. The owner's name floats above it.

import { lookColor } from '../../shared/look.js';
import { DX, DY } from '../../shared/grid.js';

/** Darken (f < 0) a #rrggbb colour, as client/render.js does for head rims. @param {string} hex @param {number} f */
function shade(hex, f) {
  const n = parseInt(hex.slice(1), 16);
  const ch = (/** @type {number} */ v) => Math.round(v * (1 + f));
  return `rgb(${ch(n >> 16)},${ch((n >> 8) & 255)},${ch(n & 255)})`;
}

/** @type {import('./index.js').CardFx} */
export default {
  drawObject(ctx, T, o, _now, world) {
    const info = world.roster.get(o.owner);
    const c = lookColor(info?.look ?? o.owner);
    const px = o.x * T;
    const py = o.y * T;
    const pad = T * 0.06;
    ctx.save();
    ctx.globalAlpha = 0.7;
    ctx.fillStyle = shade(c, -0.35);
    ctx.fillRect(px + pad, py + pad, T - 2 * pad, T - 2 * pad);
    ctx.fillStyle = c;
    ctx.fillRect(px + pad * 3, py + pad * 3, T - 6 * pad, T - 6 * pad);
    // Two eyes, pushed towards the way the ghost runs.
    ctx.fillStyle = '#111';
    const e = Math.max(2, T * 0.12);
    const ex = DX[o.dir] * T * 0.16;
    const ey = DY[o.dir] * T * 0.16;
    const sx = o.dir % 2 === 0 ? T * 0.17 : 0;
    const sy = o.dir % 2 === 1 ? T * 0.17 : 0;
    ctx.fillRect(px + T / 2 + ex - sx - e / 2, py + T / 2 + ey - sy - e / 2, e, e);
    ctx.fillRect(px + T / 2 + ex + sx - e / 2, py + T / 2 + ey + sy - e / 2, e, e);
    ctx.restore();
    if (info?.name) {
      ctx.save();
      ctx.textAlign = 'center';
      ctx.textBaseline = 'bottom';
      ctx.font = `600 ${Math.max(11, Math.round(T * 0.5))}px system-ui, sans-serif`;
      ctx.fillStyle = '#ffffff';
      ctx.fillText(info.name, px + T / 2, py - 2);
      ctx.restore();
    }
  },
  onEvent(e, r) {
    r.floatText(e.x, e.y, 'Mirage', '#ffffff');
  },
};
