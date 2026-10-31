// @ts-check
// Bloodlust visuals: an orange ring spreads from the killer's head and the card name floats up.

/** @type {import('./index.js').CardFx} */
export default {
  onEvent(e, r) {
    r.ring(e.x, e.y, 1.6, '#D55E00', 400);
    r.floatText(e.x, e.y, 'Bloodlust', '#D55E00');
  },
};
