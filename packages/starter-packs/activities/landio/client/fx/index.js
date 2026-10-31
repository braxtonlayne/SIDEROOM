// @ts-check
// Card visuals, one module per card (client/fx/<card id>.js), looked up by card id. A module may draw its
// world objects every frame and react to its card's 'fx' events (a blast, a claim). Shared player effects
// others can see (shield, phase, dash, stun, second wind, EMP) are drawn by client/render.js from head flags.
//
// Coordinates are in tiles; T is the tile size in pixels and the context is already translated so that tile
// (x, y) starts at pixel (x * T, y * T). The Renderer helpers (ring, burst, floatText, color, shake) are the
// way to add one-off effects; they respect prefers-reduced-motion.

import grenade from './grenade.js';
import dash from './dash.js';
import shield from './shield.js';
import rewind from './rewind.js';
import snare from './snare.js';
import claimflag from './claimflag.js';
import phase from './phase.js';
import wall from './wall.js';
import overgrowth from './overgrowth.js';
import bloodlust from './bloodlust.js';
import mirage from './mirage.js';
import emp from './emp.js';
import secondwind from './secondwind.js';
import haste from './haste.js';

/**
 * @typedef {object} CardFx
 * @property {(ctx: CanvasRenderingContext2D, T: number, o: import('../world.js').Obj, now: number, world: import('../world.js').World, tick: number, reducedMotion: boolean) => void} [drawObject]
 *   draw one world object of this card (o.x/o.y in tiles; tick = the playout tick being drawn, fractional;
 *   reducedMotion = the player prefers no motion: no pulsing or flicker)
 * @property {(e: any, r: import('../render.js').Renderer, world: import('../world.js').World) => void} [onEvent]
 *   a game event {k: 'fx', card, id, x, y, ...} of this card, as it plays out
 */

/** @type {Record<string, CardFx>} */
export const FX = { grenade, dash, shield, rewind, snare, claimflag, phase, wall, overgrowth, bloodlust, mirage, emp, secondwind, haste };
