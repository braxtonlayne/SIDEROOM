// @ts-check
// The maps (D-50). A map is a terrain layer built once per arena from the arena's seed: OPEN ground, WALL tiles
// (like the map's edge: a head entering one dies, nobody owns one, one never helps close a loop) and GOLD tiles
// (open ground worth more where a mode scores it). Each map lives in its own file and only builds terrain;
// the rules, bots, network and renderer handle walls and gold the same on every map.

import { OPEN, WALL, GOLD } from '../grid.js';
import { rand, seedFrom } from '../rng.js';
import open from './open.js';
import islands from './islands.js';
import hourglass from './hourglass.js';
import donut from './donut.js';
import maze from './maze.js';
import goldrush from './goldrush.js';
import procedural from './procedural.js';

/**
 * @typedef {object} MapDef
 * @property {string} id      lower case, used in config and URLs
 * @property {string} name    shown to players
 * @property {string} blurb   one line for the map picker
 * @property {(w: number, h: number, rand: () => number) => Uint8Array} build
 *   the terrain, w*h tiles in rows (index y * w + x), each OPEN, WALL or GOLD. Must work for any size from
 *   40x40 to 200x200 and use only `rand` (a seeded float in [0, 1)) for randomness, so a seed always gives the
 *   same map.
 */

/** @type {MapDef[]} */
export const MAPS = [open, islands, hourglass, donut, maze, goldrush, procedural];

/** @type {Record<string, MapDef>} */
export const MAP_BY_ID = Object.fromEntries(MAPS.map((m) => [m.id, m]));

/**
 * The terrain for map `id` (an unknown id is the open field). A map that returns a wrong length or a value
 * that is not OPEN, WALL or GOLD falls back to the open field, so a broken map can never break an arena.
 * @param {string} id @param {number} w @param {number} h @param {string|number} seed
 * @returns {Uint8Array}
 */
export function buildTerrain(id, w, h, seed) {
  const def = MAP_BY_ID[id] ?? open;
  // Its own stream of the seed: the map does not depend on (or disturb) the game's random numbers.
  const st = { rng: seedFrom(`${seed}|map:${def.id}`) };
  const t = def.build(w, h, () => rand(st));
  if (!(t instanceof Uint8Array) || t.length !== w * h || t.some((v) => v !== OPEN && v !== WALL && v !== GOLD))
    return new Uint8Array(w * h);
  return t;
}
