// @ts-check
// The demo arena as plain data: static blocks, the pushable boxes, spawn points and the goal pad. sim/world.js
// turns it into Jolt bodies and render/ turns it into meshes, so a new game starts by editing this file.
// Units are metres, +Y is up, and every number is a literal (no Math.* at load time), so every client builds
// the identical world. Rotations are integer angles (TURN = 65536 per turn) about the X axis, which is all the
// ramps need. Colours are linear RGB in 0..1.

/**
 * @typedef {object} Block
 * @property {[number, number, number]} pos   centre
 * @property {[number, number, number]} half  half extents
 * @property {number} [rotX]                  integer angle about X (TURN units)
 * @property {[number, number, number]} color
 */

/**
 * @typedef {object} Level
 * @property {string} name
 * @property {Block[]} statics          immovable geometry
 * @property {Block[]} boxes            dynamic boxes placed by hand
 * @property {number} randomBoxes       how many more dynamic boxes the seeded PRNG drops in at the start
 * @property {[number, number]} dropArea half extent (x, z) of the random drop area
 * @property {[number, number, number][]} spawns  one per player slot
 * @property {{ min: [number, number], max: [number, number], color: [number, number, number] }} pad  the goal (x/z)
 * @property {number} killY             anything below this height respawns
 */

const FLOOR = /** @type {[number, number, number]} */ ([0.32, 0.36, 0.42]);
const WALL = /** @type {[number, number, number]} */ ([0.2, 0.23, 0.3]);
const STONE = /** @type {[number, number, number]} */ ([0.45, 0.42, 0.52]);
const CRATE = /** @type {[number, number, number]} */ ([0.85, 0.55, 0.25]);
const CRATE2 = /** @type {[number, number, number]} */ ([0.35, 0.7, 0.85]);

/** @type {Level} */
export const ARENA = {
  name: 'Courtyard',
  statics: [
    // Floor (top face at y = 0) and four walls.
    { pos: [0, -0.5, 0], half: [16, 0.5, 16], color: FLOOR },
    { pos: [0, 1, -16.5], half: [17, 1, 0.5], color: WALL },
    { pos: [0, 1, 16.5], half: [17, 1, 0.5], color: WALL },
    { pos: [-16.5, 1, 0], half: [0.5, 1, 16], color: WALL },
    { pos: [16.5, 1, 0], half: [0.5, 1, 16], color: WALL },
    // A raised platform (top at y = 2) with a ramp down towards +Z. The ramp is tilted ~18.4 degrees, so its
    // far (+Z) end sits on the floor and its near end meets the platform's edge.
    { pos: [8, 1, -8], half: [3, 1, 3], color: STONE },
    { pos: [8, 0.82, -1.9], half: [1.5, 0.25, 3.3], rotX: 3355, color: STONE },
    // Two pillars for cover.
    { pos: [-6, 1.5, -6], half: [0.6, 1.5, 0.6], color: STONE },
    { pos: [6, 1.5, 7], half: [0.6, 1.5, 0.6], color: STONE },
  ],
  boxes: [
    // A pyramid in the middle: the first thing a player runs into.
    { pos: [-1.05, 0.5, 3], half: [0.5, 0.5, 0.5], color: CRATE },
    { pos: [0, 0.5, 3], half: [0.5, 0.5, 0.5], color: CRATE },
    { pos: [1.05, 0.5, 3], half: [0.5, 0.5, 0.5], color: CRATE },
    { pos: [-0.525, 1.5, 3], half: [0.5, 0.5, 0.5], color: CRATE },
    { pos: [0.525, 1.5, 3], half: [0.5, 0.5, 0.5], color: CRATE },
    { pos: [0, 2.5, 3], half: [0.5, 0.5, 0.5], color: CRATE },
    // A small stack on the platform.
    { pos: [8, 2.4, -8], half: [0.4, 0.4, 0.4], color: CRATE2 },
    { pos: [8, 3.2, -8], half: [0.4, 0.4, 0.4], color: CRATE2 },
  ],
  randomBoxes: 6,
  dropArea: [10, 10],
  spawns: [
    [0, 0.05, 8],
    [-6, 0.05, 6],
    [6, 0.05, 2],
    [-8, 0.05, -2],
    [2, 0.05, -4],
    [-3, 0.05, 12],
    [11, 0.05, 11],
    [-11, 0.05, -11],
  ],
  pad: { min: [-14, 9], max: [-9, 14], color: [0.3, 0.85, 0.45] },
  killY: -10,
};
