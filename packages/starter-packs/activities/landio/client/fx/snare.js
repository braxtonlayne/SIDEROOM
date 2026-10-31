// @ts-check
// Snare visuals. A snare is secret: only its owner sees it, as a dashed ring with four teeth in the owner's
// colour. When one catches a head, everyone sees the tile flash and "Snared!" float up.

import { lookColor } from '../../shared/look.js';

/** @type {import('./index.js').CardFx} */
export default {
  drawObject(ctx, T, o, _now, world) {
    if (o.owner !== world.you?.pid) return;
    const cx = (o.x + 0.5) * T;
    const cy = (o.y + 0.5) * T;
    const r = T * 0.3;
    const teeth = T * 0.14;
    ctx.save();
    ctx.strokeStyle = lookColor(world.roster.get(o.owner)?.look ?? o.owner);
    ctx.lineWidth = Math.max(1, T * 0.06);
    ctx.globalAlpha = 0.85;
    ctx.setLineDash([T * 0.09, T * 0.07]);
    ctx.beginPath();
    ctx.arc(cx, cy, r, 0, Math.PI * 2);
    ctx.stroke();
    ctx.setLineDash([]);
    ctx.beginPath();
    for (let k = 0; k < 4; k++) {
      const a = (k * Math.PI) / 2;
      const ax = Math.cos(a);
      const ay = Math.sin(a);
      ctx.moveTo(cx + ax * r, cy + ay * r);
      ctx.lineTo(cx + ax * (r + teeth), cy + ay * (r + teeth));
    }
    ctx.stroke();
    ctx.restore();
  },
  onEvent(e, r) {
    r.ring(e.x, e.y, 1.2, '#F0E442', 300);
    r.floatText(e.x, e.y, 'Snared!', '#F0E442');
  },
};
