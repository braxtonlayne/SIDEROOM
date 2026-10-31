// @ts-check
// Shrinking Arena on screen: the next border as a pulsing red dashed outline with a red tint over the tiles
// that will close, during the 5 s warning; the chip counts down to the next step.

const WARN_TICKS = 35;

/** @param {import('../world.js').World} world */
function state(world) {
  const md = world.round?.md;
  if (!md) return null;
  const zone = /** @type {number[] | undefined} */ (md.zone);
  const next = /** @type {number[] | undefined} */ (md.next);
  const at = /** @type {number | undefined} */ (md.at);
  return { zone, next, at };
}

/** @type {import('./index.js').ModeView} */
export default {
  drawWorld(ctx, world, T, now) {
    const st = state(world);
    if (!st || !st.zone || !st.next || st.at === undefined) return;
    const [zx0, zy0, zx1, zy1] = st.zone;
    const [nx0, ny0, nx1, ny1] = st.next;
    // The tiles about to close: four strips between the two rectangles.
    ctx.fillStyle = 'rgba(255,40,40,0.18)';
    ctx.fillRect(zx0 * T, zy0 * T, (zx1 - zx0) * T, (ny0 - zy0) * T);
    ctx.fillRect(zx0 * T, ny1 * T, (zx1 - zx0) * T, (zy1 - ny1) * T);
    ctx.fillRect(zx0 * T, ny0 * T, (nx0 - zx0) * T, (ny1 - ny0) * T);
    ctx.fillRect(nx1 * T, ny0 * T, (zx1 - nx1) * T, (ny1 - ny0) * T);
    const pulse = 0.55 + 0.45 * Math.sin(now / 150);
    ctx.strokeStyle = `rgba(255,60,60,${pulse.toFixed(3)})`;
    ctx.lineWidth = Math.max(2, T / 5);
    ctx.setLineDash([T * 0.6, T * 0.4]);
    ctx.strokeRect(nx0 * T, ny0 * T, (nx1 - nx0) * T, (ny1 - ny0) * T);
    ctx.setLineDash([]);
  },
  chip(world) {
    const st = state(world);
    if (!st) return '';
    if (st.at === undefined) return 'Final zone';
    const left = Math.max(0, st.at - world.recvTick);
    if (st.next && left <= WARN_TICKS) return `Closing in ${Math.ceil(left / 7)} s`;
    const sec = Math.ceil(left / 7);
    return `Border closes in ${Math.floor(sec / 60)}:${String(sec % 60).padStart(2, '0')}`;
  },
};
