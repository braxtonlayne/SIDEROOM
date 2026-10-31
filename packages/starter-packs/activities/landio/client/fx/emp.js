// @ts-check
// EMP visuals: a wide ring for the 6-tile reach, an inner ring, and a label at the emitter.

/** @type {import('./index.js').CardFx} */
export default {
  onEvent(e, r) {
    r.ring(e.x, e.y, 6, '#CC79A7', 500);
    r.ring(e.x, e.y, 3, '#CC79A7', 300);
    r.floatText(e.x, e.y, 'EMP', '#CC79A7');
  },
};
