// @ts-check
// Second Wind visuals: a white ring and burst where the head comes back, and a label rising from it.

/** @type {import('./index.js').CardFx} */
export default {
  onEvent(e, r) {
    r.ring(e.x, e.y, 2, '#ffffff', 600);
    r.burst(e.x, e.y, '#ffffff', 20);
    r.floatText(e.x, e.y, 'Second Wind', '#ffffff');
  },
};
