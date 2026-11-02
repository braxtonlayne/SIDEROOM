// @ts-check
// The renderer interface and the backend choice: WebGPU when `navigator.gpu` gives an adapter, WebGL2
// otherwise (headless CI and older browsers usually have no WebGPU). Both backends draw the same scene from
// the same calls, so the game never branches on which one it got.
//
//   const r = await createRenderer(canvas);          // r.kind is 'webgpu' or 'webgl2'
//   const cube = r.upload(boxMesh());
//   r.frame({ view, eye, fovY: 1.2, ... }, [{ mesh: cube, model, tint: [1, 0.5, 0.2] }]);

import { createWebGPURenderer } from './webgpu.js';
import { createWebGL2Renderer } from './webgl2.js';

/** @typedef {import('./meshes.js').MeshData} MeshData */
/** @typedef {import('./math.js').Mat4} Mat4 */
/** @typedef {import('./math.js').Vec3} Vec3 */

/** An uploaded mesh. @typedef {{ readonly count: number, readonly id: number }} MeshHandle */
/** An uploaded texture. @typedef {{ readonly id: number, readonly width: number, readonly height: number }} TextureHandle */

/**
 * @typedef {object} Draw
 * @property {MeshHandle} mesh
 * @property {Float32Array} model             column-major model matrix
 * @property {[number, number, number]} tint  linear RGB multiplier
 * @property {number} [grid]                  0..1, strength of the world-space floor grid
 * @property {TextureHandle | null} [texture] base colour texture (white when absent)
 */

/**
 * @typedef {object} FrameParams
 * @property {Mat4} view
 * @property {Vec3} eye
 * @property {number} fovY        radians
 * @property {number} near
 * @property {number} far
 * @property {Vec3} lightDir      direction the light travels (normalised)
 * @property {Vec3} lightColor
 * @property {Vec3} ambient
 * @property {Vec3} fogColor      also the clear colour
 * @property {number} fogDensity
 */

/**
 * @typedef {object} Renderer
 * @property {'webgpu' | 'webgl2'} kind
 * @property {(mesh: MeshData) => MeshHandle} upload
 * @property {(image: ImageBitmap | HTMLCanvasElement | OffscreenCanvas) => TextureHandle} texture
 * @property {(width: number, height: number) => void} resize   device pixels
 * @property {(params: FrameParams, draws: Draw[]) => void} frame
 * @property {() => void} destroy
 */

/**
 * Which backend to try first. Exported for tests.
 * @param {{ hasGpu: boolean, prefer?: 'auto' | 'webgpu' | 'webgl2' }} env
 * @returns {('webgpu' | 'webgl2')[]}
 */
export function backendOrder(env) {
  if (env.prefer === 'webgl2' || !env.hasGpu) return ['webgl2'];
  return ['webgpu', 'webgl2'];
}

/**
 * @param {HTMLCanvasElement} canvas
 * @param {{ prefer?: 'auto' | 'webgpu' | 'webgl2', nav?: any, onLost?: (why: string) => void }} [opt]
 * @returns {Promise<Renderer>}
 */
export async function createRenderer(canvas, opt = {}) {
  const nav = opt.nav ?? (typeof navigator === 'undefined' ? undefined : navigator);
  const order = backendOrder({ hasGpu: !!nav?.gpu, prefer: opt.prefer });
  /** @type {string[]} */
  const why = [];
  for (const kind of order) {
    try {
      const r = kind === 'webgpu' ? await createWebGPURenderer(canvas, nav.gpu, opt.onLost) : createWebGL2Renderer(canvas);
      if (r) return r;
      why.push(`${kind}: not available`);
    } catch (e) {
      why.push(`${kind}: ${e instanceof Error ? e.message : String(e)}`);
    }
  }
  throw new Error(`No WebGPU or WebGL2 on this device (${why.join('; ')}).`);
}
