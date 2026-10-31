// @ts-check
// Grenade visuals: a bomb on its landing tile with a pulsing orange diamond over the blast area; the pulse
// quickens as the fuse runs out. On the blast: a ring, a burst of sparks and a small shake.

import { lookColor } from '../../shared/look.js';

// Must match FUSE in shared/cards/grenade.js (the warning pulse runs over the real fuse).
const FUSE = 5;
/** Blast radius in tiles (the diamond's outer tile centres sit at this distance, plus half a tile). */
const RADIUS = 2.5;

/** @type {import('./index.js').CardFx} */
export default {
  drawObject(ctx, T, o, now, world, tick, reducedMotion) {
    const progress = Math.min(1, Math.max(0, (tick - (o.until - FUSE - 1)) / FUSE));
    // Reduced motion: no pulse, the marker just brightens as the fuse runs out.
    const pulse = reducedMotion ? progress : 0.5 + 0.5 * Math.sin((now / 1000) * (1.5 + 6 * progress) * Math.PI * 2);
    const cx = (o.x + 0.5) * T;
    const cy = (o.y + 0.5) * T;
    const r = RADIUS * T;

    ctx.save();
    ctx.beginPath();
    ctx.moveTo(cx, cy - r);
    ctx.lineTo(cx + r, cy);
    ctx.lineTo(cx, cy + r);
    ctx.lineTo(cx - r, cy);
    ctx.closePath();
    ctx.fillStyle = `rgba(230,159,0,${(0.06 + 0.08 * pulse).toFixed(3)})`;
    ctx.fill();
    ctx.strokeStyle = `rgba(230,159,0,${(0.45 + 0.45 * pulse).toFixed(3)})`;
    ctx.lineWidth = Math.max(1.5, T * 0.1);
    ctx.stroke();

    // The bomb: a dark body, a cap in the owner's colour and a spark on the fuse that flickers with the pulse.
    const look = world.roster.get(o.owner)?.look ?? o.owner;
    ctx.fillStyle = '#1d1d1d';
    ctx.beginPath();
    ctx.arc(cx, cy, T * 0.28, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = lookColor(look);
    ctx.fillRect(cx - T * 0.1, cy - T * 0.36, T * 0.2, T * 0.14);
    ctx.fillStyle = pulse > 0.5 ? '#E69F00' : '#D55E00';
    ctx.beginPath();
    ctx.arc(cx + T * 0.12, cy - T * 0.4, T * 0.08, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
  },
  onEvent(e, r, world) {
    if (e.phase === 'throw') {
      r.ring(e.x, e.y, 1, r.color(world.roster.get(e.id)?.look ?? e.id), 200);
    } else if (e.phase === 'boom') {
      r.ring(e.x, e.y, 2.5, '#D55E00', 350);
      r.burst(e.x, e.y, '#E69F00', 16);
      r.shake(0.15);
    }
  },
};
