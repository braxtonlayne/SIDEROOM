// @ts-check
// Rewind visuals: a ring where the head landed and a label. The trail it erased is already gone from the grid.

/** @type {import('./index.js').CardFx} */
export default {
  onEvent(e, r, world) {
    const look = world.roster.get(e.id)?.look ?? e.id;
    r.ring(e.x, e.y, 1.5, r.color(look), 300);
    r.floatText(e.x, e.y, 'Rewind', '#56B4E9');
  },
};
