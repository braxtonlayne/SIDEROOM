// @ts-check
// koth: draws the hill (and, five seconds before it moves, where it goes next) and the local player's points.

/** A soft gold disc with a ring at tile (cx, cy), radius r tiles. @param {CanvasRenderingContext2D} ctx @param {number[]} hill @param {number} T @param {boolean} dashed */
function disc(ctx, hill, T, dashed) {
  const x = (hill[0] + 0.5) * T;
  const y = (hill[1] + 0.5) * T;
  // The server counts tiles whose centre is within r of the centre tile; draw the disc to match.
  const rad = hill[2] * T;
  ctx.beginPath();
  ctx.arc(x, y, rad, 0, Math.PI * 2);
  if (!dashed) {
    ctx.fillStyle = 'rgba(255, 205, 60, 0.11)';
    ctx.fill();
  }
  ctx.lineWidth = dashed ? 2 : 3;
  ctx.strokeStyle = dashed ? 'rgba(255, 205, 60, 0.75)' : 'rgba(255, 205, 60, 0.6)';
  ctx.setLineDash(dashed ? [8, 6] : []);
  ctx.stroke();
  ctx.setLineDash([]);
}

/** @type {import('./index.js').ModeView} */
export default {
  drawWorld(ctx, world, T) {
    const md = world.round?.md;
    if (!md) return;
    if (Array.isArray(md.hill)) disc(ctx, /** @type {number[]} */ (md.hill), T, false);
    if (Array.isArray(md.nextHill)) disc(ctx, /** @type {number[]} */ (md.nextHill), T, true);
  },

  chip(world, me, hz) {
    const md = world.round?.md;
    if (!md) return '';
    if (Array.isArray(md.nextHill) && typeof md.moveAt === 'number') {
      const left = Math.max(0, Math.ceil((md.moveAt - world.appliedTick) / hz));
      return `Hill moves in ${left} s`;
    }
    const mine = world.stats.get(me);
    if (!mine) return '';
    // The leaderboard's order: more points first, ties by id.
    let rank = 1;
    for (const [id, st] of world.stats)
      if (id !== me && st.alive && (st.score > mine.score || (st.score === mine.score && id < me))) rank++;
    return `You: ${mine.score} pts${mine.alive ? ` (#${rank})` : ''}`;
  },
};
