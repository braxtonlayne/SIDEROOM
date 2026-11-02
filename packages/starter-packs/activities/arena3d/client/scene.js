// @ts-check
// Turns game state into the renderer's draw list: the level's static blocks and goal pad (built once), the
// boxes, the other players (a capsule and a visor showing where they look) and the glTF beacon. Render-side
// only: Math.sin is fine here, nothing feeds back into the simulation.

import { boxMesh, capsuleMesh } from '../render/meshes.js';
import { mat4, quat, unitsToRad } from '../render/math.js';
import { TUNING } from '../sim/world.js';
import { flattenScene } from '../assets/gltf.js';

/** @typedef {import('../render/renderer.js').Renderer} Renderer */
/** @typedef {import('../render/renderer.js').Draw} Draw */
/** @typedef {import('../sim/level.js').Level} Level */

/** One colour per player slot. */
export const SLOT_COLORS = /** @type {[number, number, number][]} */ ([
  [0.95, 0.45, 0.35], [0.35, 0.65, 0.95], [0.55, 0.9, 0.4], [0.95, 0.8, 0.3],
  [0.75, 0.45, 0.95], [0.3, 0.9, 0.85], [0.95, 0.5, 0.75], [0.9, 0.9, 0.9],
]);

/**
 * @typedef {object} PlayerView
 * @property {number} slot
 * @property {number} x
 * @property {number} y   feet
 * @property {number} z
 * @property {number} yaw  integer units
 * @property {boolean} crouched
 */

/**
 * @param {Renderer} renderer @param {Level} level
 */
export function createScene(renderer, level) {
  const cube = renderer.upload(boxMesh());
  const stand = renderer.upload(capsuleMesh(TUNING.radius, TUNING.standHalf));
  const crouch = renderer.upload(capsuleMesh(TUNING.radius, TUNING.crouchHalf));
  /** @type {Draw[]} */
  const statics = level.statics.map((b, i) => {
    const a = unitsToRad(b.rotX ?? 0);
    return {
      mesh: cube,
      model: mat4.fromTRS(b.pos, [Math.sin(a / 2), 0, 0, Math.cos(a / 2)], b.half),
      tint: b.color,
      grid: i === 0 ? 1 : 0, // the floor gets the metre grid
    };
  });
  const { min, max, color } = level.pad;
  statics.push({
    mesh: cube,
    model: mat4.fromTRS([(min[0] + max[0]) / 2, 0.01, (min[1] + max[1]) / 2], [0, 0, 0, 1], [(max[0] - min[0]) / 2, 0.012, (max[1] - min[1]) / 2]),
    tint: color,
    grid: 0.5,
  });
  /** @type {Draw[]} */
  let beacon = [];
  /** @type {{ p: number[], q: number[], half: number[], color: number[] }[]} */
  const boxScratch = [];

  return {
    boxScratch,
    /**
     * Upload a loaded glTF model and place it (the demo puts the beacon on the goal pad).
     * @param {import('../assets/gltf.js').GltfModel} model @param {[number, number, number]} at
     */
    setBeacon(model, at) {
      const handles = new Map();
      const textures = model.images.map((img) => (img ? renderer.texture(img) : null));
      beacon = flattenScene(model, mat4.fromTRS(at, [0, 0, 0, 1], [1, 1, 1])).map((f) => {
        let h = handles.get(f.primitive.mesh);
        if (!h) handles.set(f.primitive.mesh, (h = renderer.upload(f.primitive.mesh)));
        const mat = model.materials[f.primitive.material];
        return {
          mesh: h,
          model: f.world,
          tint: /** @type {[number, number, number]} */ (mat ? [mat.baseColor[0], mat.baseColor[1], mat.baseColor[2]] : [1, 1, 1]),
          texture: mat && mat.baseColorImage !== null ? textures[mat.baseColorImage] : null,
        };
      });
    },
    /**
     * @param {{ p: number[], q: number[], half: number[], color: number[] }[]} boxes
     * @param {PlayerView[]} players  everyone to draw (not the local player in first person)
     * @returns {Draw[]}
     */
    draws(boxes, players) {
      /** @type {Draw[]} */
      const out = statics.slice();
      for (const b of boxes) out.push({ mesh: cube, model: mat4.fromTRS(b.p, b.q, b.half), tint: /** @type {[number, number, number]} */ (b.color) });
      for (const p of players) {
        const color = SLOT_COLORS[p.slot % SLOT_COLORS.length];
        const yaw = unitsToRad(p.yaw);
        const q = quat.fromAxisAngle([0, 1, 0], yaw);
        out.push({ mesh: p.crouched ? crouch : stand, model: mat4.fromTRS([p.x, p.y, p.z], q, [1, 1, 1]), tint: color });
        const eye = p.crouched ? TUNING.crouchEye : TUNING.standEye;
        // The visor: a dark slab just in front of the face, so you can see where someone looks.
        const fx = -Math.sin(yaw) * 0.3;
        const fz = -Math.cos(yaw) * 0.3;
        out.push({ mesh: cube, model: mat4.fromTRS([p.x + fx, p.y + eye - 0.05, p.z + fz], q, [0.22, 0.07, 0.06]), tint: [0.08, 0.08, 0.12] });
      }
      out.push(...beacon);
      return out;
    },
  };
}
