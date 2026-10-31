// @ts-check
// Shield visuals: a blue ring where a cut was blocked, and "Blocked!" floats up. The bubble while the shield
// is up is drawn by client/render.js from head flag 4.

/** @type {import('./index.js').CardFx} */
export default {
  onEvent(e, r) {
    r.ring(e.x, e.y, 1.4, '#56B4E9', 350);
    r.floatText(e.x, e.y, 'Blocked!', '#56B4E9');
  },
};
