// @ts-check
// Claim Flag visuals: a ring spreads over the claimed 7×7 and the number of tiles taken floats up.

/** @type {import('./index.js').CardFx} */
export default {
  onEvent(e, r, world) {
    const look = world.roster.get(e.id)?.look ?? e.id;
    r.ring(e.x, e.y, 3.5, r.color(look), 450);
    r.floatText(e.x, e.y, '+' + (e.n ?? 0), '#ffffff');
  },
};
