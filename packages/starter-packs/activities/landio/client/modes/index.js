// @ts-check
// What each mode shows on screen beyond the shared round chip, results and leaderboard (D-51). A mode with
// nothing special to show has no entry. Each entry lives in its own file.

import shrink from './shrink.js';
import koth from './koth.js';
import infection from './infection.js';

/**
 * @typedef {object} ModeView
 * @property {(ctx: CanvasRenderingContext2D, world: import('../world.js').World, T: number, now: number) => void} [drawWorld]
 *   drawn over the tiles and under the heads, in world space: tile (x, y) covers [x*T, (x+1)*T)
 * @property {(world: import('../world.js').World, me: number, hz: number) => string} [chip]
 *   extra text for the round chip (after the time left), '' for none
 */

// Last Land Standing's sudden death is the same closing border, drawn and timed the same way.
/** @type {Record<string, ModeView>} */
export const MODE_VIEWS = { shrink, koth, infection, lastland: shrink };
