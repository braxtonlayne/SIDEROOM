// @ts-check
// Wall visuals: five grey brick tiles across the path, the owner's colour along their top edge, fading in the
// last 1.5 s (10 ticks at 7 ticks/s). Placing one sends a grey ring out from the wall's centre.

import { lookColor } from '../../shared/look.js';

const FADE_TICKS = 10;

/** @type {import('./index.js').CardFx} */
export default {
  drawObject(ctx, T, o, _now, world, tick) {
    const horizontal = o.dir === 0 || o.dir === 2;
    const edge = lookColor(world.roster.get(o.owner)?.look ?? o.owner);
    const left = o.until - tick;
    const alpha = left < FADE_TICKS ? Math.max(0.1, left / FADE_TICKS) : 1;
    const m = Math.max(1, T / 12);
    ctx.save();
    ctx.globalAlpha = alpha;
    for (let k = -2; k <= 2; k++) {
      const x = (horizontal ? o.x + k : o.x) * T;
      const y = (horizontal ? o.y : o.y + k) * T;
      // Two shades alternate, like bricks laid in a course.
      ctx.fillStyle = k & 1 ? '#4b5563' : '#6b7280';
      ctx.fillRect(x, y, T, T);
      // Mortar: a joint across the middle of each brick and a thin outline.
      ctx.fillStyle = '#374151';
      ctx.fillRect(x, y + T / 2 - m / 2, T, m);
      ctx.fillRect(x, y, T, m);
      ctx.fillRect(x, y + T - m, T, m);
      ctx.fillRect(x, y, m, T);
      ctx.fillRect(x + T - m, y, m, T);
      // The owner's colour on the top edge.
      ctx.fillStyle = edge;
      ctx.fillRect(x, y, T, Math.max(2, T / 6));
    }
    ctx.restore();
  },
  onEvent(e, r) {
    r.ring(e.x, e.y, 2, '#9ca3af', 300);
  },
};
