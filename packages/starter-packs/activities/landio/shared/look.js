// @ts-check
// Player looks: a colour from the Okabe–Ito colour-blind-safe palette plus a pattern, so colour is never the
// only cue. 8 colours × 4 patterns = 32 distinct looks.

export const COLORS = [
  '#E69F00', // orange
  '#56B4E9', // sky blue
  '#009E73', // bluish green
  '#F0E442', // yellow
  '#0072B2', // blue
  '#D55E00', // vermillion
  '#CC79A7', // reddish purple
  '#F2F2F2', // near white
];
export const PATTERNS = ['solid', 'dots', 'stripes', 'grid'];
export const LOOKS = COLORS.length * PATTERNS.length;
/** Number of colours: look = colour + LOOK_COLORS * pattern. */
export const LOOK_COLORS = COLORS.length;

/** @param {number} look */
export const lookColor = (look) => COLORS[look % COLORS.length];
/** @param {number} look */
export const lookPattern = (look) => PATTERNS[Math.floor(look / COLORS.length) % PATTERNS.length];
