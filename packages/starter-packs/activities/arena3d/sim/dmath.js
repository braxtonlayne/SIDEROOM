// @ts-check
// Deterministic trigonometry for the simulation. ECMAScript does not pin down Math.sin / Math.cos (each engine
// ships its own libm), so two browsers may turn the same yaw into directions that differ in the last bit, and
// physics amplifies that. +, -, * and / on doubles are exact IEEE-754 everywhere, so these use only those: the
// angle is an integer (1/65536 of a turn), the quadrant is split off exactly, and a fixed polynomial does the
// rest. Accuracy is ~1e-9, far finer than the input grid. Rendering may use Math.* freely; sim/ must use these.

/** Integer angle units per full turn. */
export const TURN = 65536;

/** Taylor coefficients for sin(x), |x| <= pi/4. */
const S3 = -1 / 6;
const S5 = 1 / 120;
const S7 = -1 / 5040;
const S9 = 1 / 362880;
const S11 = -1 / 39916800;
/** Taylor coefficients for cos(x), |x| <= pi/4. */
const C2 = -1 / 2;
const C4 = 1 / 24;
const C6 = -1 / 720;
const C8 = 1 / 40320;
const C10 = -1 / 3628800;
const C12 = 1 / 479001600;
/** pi/2 divided by the units in a quarter turn (a literal, so no engine computes it). */
const RAD_PER_UNIT = 1.5707963267948966 / 16384;

/** @param {number} x radians, |x| <= pi/4 */
function sinPoly(x) {
  const x2 = x * x;
  return x * (1 + x2 * (S3 + x2 * (S5 + x2 * (S7 + x2 * (S9 + x2 * S11)))));
}

/** @param {number} x radians, |x| <= pi/4 */
function cosPoly(x) {
  const x2 = x * x;
  return 1 + x2 * (C2 + x2 * (C4 + x2 * (C6 + x2 * (C8 + x2 * (C10 + x2 * C12)))));
}

/**
 * sin and cos of an integer angle, as [sin, cos].
 * @param {number} units an integer angle, TURN units per turn (any integer; it wraps)
 * @returns {[number, number]}
 */
export function sinCos(units) {
  const a = ((units % TURN) + TURN) % TURN;
  // Split into an eighth-of-a-turn-centred quadrant so the polynomial argument stays within +-pi/4.
  // k is 0..4 (4 is the last eighth of the turn, i.e. quadrant 0 again); the remainder must use k, not k & 3.
  const k = Math.floor((a + 8192) / 16384);
  const q = k & 3;
  const r = (a - k * 16384) * RAD_PER_UNIT;
  const s = sinPoly(r);
  const c = cosPoly(r);
  switch (q) {
    case 0:
      return [s, c];
    case 1:
      return [c, -s];
    case 2:
      return [-s, -c];
    default:
      return [-c, s];
  }
}

/**
 * Round to a fixed grid, as an integer. Used for hashing and the wire, never inside the physics.
 * @param {number} v @param {number} scale
 */
export function quantise(v, scale) {
  return Math.round(v * scale) | 0;
}
